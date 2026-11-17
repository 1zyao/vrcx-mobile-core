//! In-process core for the phone app.
//!
//! The phone runs the VRCX-0 core as a shared library: Kotlin starts it once,
//! it opens the local SQLite database, then serves the same
//! `POST /api/invoke` + `GET /api/events` contract the desktop and cloud use, on
//! an ephemeral loopback port. The UI talks HTTP to its own process and never
//! needs a database driver of its own.
//!
//! Splitting it this way keeps the front/back boundary honest: the Kotlin side
//! only needs an HTTP client and the JSON shapes, so the boundary can later be
//! pointed at a remote host without touching the UI.

pub mod host;

use std::path::Path;

use serde::Serialize;
use vrcx_0_command_server::serve_loopback;
use vrcx_0_local_server::generate_token;
use vrcx_0_mobile_host::build_bus;

/// What the UI needs to reach the core.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreEndpoint {
    /// Port on 127.0.0.1 serving `/api/invoke` and `/api/events`.
    pub port: u16,
    /// Shared secret the UI must send, so another app on the device cannot
    /// drive this core even if it guesses the port.
    pub token: String,
}

/// Opens the core against `files_dir` and starts the loopback bus.
///
/// `files_dir` is the platform's app data directory
/// (`/data/user/0/<package>/files` on Android), so the SQLite database lands in
/// the same place the desktop build would put it.
///
/// `device_name` is reported to remote-sync presence, so the other machines see a real name instead
/// of an id.
pub fn start_core(files_dir: &Path, device_name: &str) -> Result<CoreEndpoint, String> {
    // The whole VRCX-0 runtime, assembled the same way the desktop and headless
    // hosts assemble it. Phone-specific extras are no-ops (see `host`).
    let host = host::build_host(files_dir, device_name)?;
    // Installing the data services is what actually starts the background loops - including
    // `remote_sync_host.start_auto()`, VRCX's shared-database sync. The desktop and headless hosts
    // do this inside their runtime start; the phone host had no equivalent, so its core assembled
    // everything and then ran none of it (measured: `_sync_meta.lastCycleAt` stayed at the value
    // copied in from the desktop).
    host.start_data_services();
    let token = generate_token().map_err(|error| format!("generate token: {error}"))?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("start runtime: {error}"))?;

    let bus = build_bus(std::sync::Arc::new(host));
    let port = runtime.block_on(async move {
        let (address, handle) = serve_loopback(bus)
            .await
            .map_err(|error| format!("bind loopback: {error}"))?;
        // The server lives for the whole process; drop the handle, keep the task.
        std::mem::forget(handle);
        Ok::<u16, String>(address.port())
    })?;

    // Keep the runtime alive for as long as the core is in use.
    std::mem::forget(runtime);

    Ok(CoreEndpoint { port, token })
}


/// Starts the core through a plain C ABI, for hosts whose runtime has no JNI.
///
/// iOS is the reason this exists: `System.loadLibrary` + `external fun` is a JVM/ART mechanism, so the
/// Android entry above cannot be called from Swift or Kotlin/Native. The two entry points do exactly the
/// same job - start the core once, hand back `{port, token}` - and after that the protocol is loopback
/// HTTP, so nothing else needs to cross the boundary.
///
/// `data_dir` is the platform's app data directory (`NSSearchPathForDirectoriesInDomains(.applicationSupportDirectory, …)`
/// on iOS); the host creates its own `VRCX-0` folder inside it, exactly as on Android.
///
/// Returns a NUL-terminated JSON string that the caller must release with [`vrcx_core_free`]. An
/// `{"error": …}` document means start-up failed - the same shape the Android entry returns, so one
/// parser on the Kotlin/Swift side covers both.
///
/// # Safety
///
/// Both pointers must be valid NUL-terminated strings for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn vrcx_core_start(
    data_dir: *const std::os::raw::c_char,
    device_name: *const std::os::raw::c_char,
) -> *mut std::os::raw::c_char {
    let reply = (|| -> Result<String, String> {
        if data_dir.is_null() {
            return Err("data dir is null".to_string());
        }
        let dir = std::ffi::CStr::from_ptr(data_dir)
            .to_str()
            .map_err(|error| format!("data dir is not UTF-8: {error}"))?
            .to_string();
        let name = if device_name.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(device_name)
                .to_str()
                .map_err(|error| format!("device name is not UTF-8: {error}"))?
                .to_string()
        };
        let endpoint = start_core(Path::new(&dir), &name)?;
        serde_json::to_string(&endpoint).map_err(|error| error.to_string())
    })()
    .unwrap_or_else(|message| serde_json::json!({ "error": message }).to_string());

    // Interior NULs would truncate the string when the caller reads it, so replace them.
    let reply = reply.replace('\0', " ");
    std::ffi::CString::new(reply)
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Releases a string returned by [`vrcx_core_start`].
///
/// # Safety
///
/// `value` must be a pointer returned by `vrcx_core_start` that has not been freed yet, or null.
#[no_mangle]
pub unsafe extern "C" fn vrcx_core_free(value: *mut std::os::raw::c_char) {
    if !value.is_null() {
        drop(std::ffi::CString::from_raw(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scratch directory inside the workspace: `std::env::temp_dir()` is not
    /// writable for this build machine's tool processes.
    fn scratch_dir(label: &str) -> std::path::PathBuf {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(format!("core-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn starts_the_core_and_reports_a_loopback_endpoint() {
        let dir = scratch_dir("start");
        let endpoint = start_core(&dir, "test-device").expect("start core");

        assert!(endpoint.port > 0, "ephemeral port assigned");
        assert!(endpoint.token.len() >= 43, "token is high entropy");
        assert!(
            dir.join("VRCX-0").join("VRCX-0.sqlite3").exists(),
            "sqlite opened in the app folder"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_endpoint_is_served_over_http() {
        let dir = scratch_dir("http");
        let endpoint = start_core(&dir, "test-device").expect("start core");

        // Blocking HTTP from a plain std socket: the runtime is already serving
        // on its own threads.
        // Canonical envelope: the input is wrapped in the command's parameter name
        // (`query`), same as the desktop and cloud frontends send it.
        let body = r#"{"cmd":"app__feed_rows_query","args":{"query":{"userId":"usr_test","mode":"lookup","maxEntries":5}}}"#;
        let request = format!(
            "POST /api/invoke HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            endpoint.port,
            body.len()
        );
        use std::io::{Read, Write};
        let mut stream =
            std::net::TcpStream::connect(("127.0.0.1", endpoint.port)).expect("connect");
        stream.write_all(request.as_bytes()).expect("write");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read");

        assert!(response.contains("\"ok\":true"), "unexpected body: {response}");
        assert!(response.contains("\"result\":[]"), "unexpected body: {response}");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// The C entry point works on this host, which is what iOS will call.
    ///
    /// Exercised here rather than on iOS because the code path is platform-independent: the ABI reads
    /// two C strings, starts the same core and returns the same JSON. If this passes, the only thing
    /// left for iOS is compiling the static library on a Mac.
    #[test]
    fn the_c_entry_point_starts_a_reachable_core() {
        use std::ffi::{CStr, CString};

        let dir = scratch_dir("c-abi");
        let dir_c = CString::new(dir.to_string_lossy().to_string()).unwrap();
        let name_c = CString::new("c-abi-device").unwrap();

        let raw = unsafe { vrcx_core_start(dir_c.as_ptr(), name_c.as_ptr()) };
        assert!(!raw.is_null(), "vrcx_core_start returned null");
        let json = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_string();
        unsafe { vrcx_core_free(raw) };

        assert!(!json.contains("\"error\""), "start failed: {json}");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("json");
        let port = parsed["port"].as_u64().expect("port") as u16;
        assert!(parsed["token"].as_str().is_some_and(|t| t.len() >= 43), "token: {json}");
        assert!(
            dir.join("VRCX-0").join("VRCX-0.sqlite3").exists(),
            "sqlite opened in the app folder"
        );

        // The port it returned must actually be serving.
        std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect to the reported port");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A null data dir must come back as an error document, not a crash.
    #[test]
    fn the_c_entry_point_reports_a_null_data_dir() {
        use std::ffi::{CStr, CString};

        let name_c = CString::new("c-abi-device").unwrap();
        let raw = unsafe { vrcx_core_start(std::ptr::null(), name_c.as_ptr()) };
        assert!(!raw.is_null(), "should still return a string");
        let json = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_string();
        unsafe { vrcx_core_free(raw) };
        assert!(json.contains("\"error\""), "expected error document, got {json}");
    }
}



