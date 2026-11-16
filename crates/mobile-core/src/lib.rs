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
use vrcx_0_mobile_host::{build_bus, MobileRuntimeHostState};

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
pub fn start_core(files_dir: &Path) -> Result<CoreEndpoint, String> {
    let host = MobileRuntimeHostState::open(files_dir).map_err(|error| error.message)?;
    let token = generate_token().map_err(|error| format!("generate token: {error}"));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("start runtime: {error}"))?;

    let bus = build_bus(host);
    let port = runtime.block_on(async move {
        let (address, handle) = serve_loopback(bus)
            .await
            .map_err(|error| format!("bind loopback: {error}"))?;
        // The server runs for the lifetime of the process; keep the task alive
        // by leaking it into the runtime rather than dropping the handle.
        std::mem::forget(handle);
        Ok::<u16, String>(address.port())
    })?;

    // Keep the runtime alive for as long as the core is in use.
    std::mem::forget(runtime);

    Ok(CoreEndpoint { port, token: token? })
}

/// JNI entry point for the Android app.
///
/// Provisional symbol: it matches a `NativeCore` object in the
/// `io.github.vrcmteam.vrcm.core` package on the Kotlin side. Rename the class
/// and this must be renamed with it.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_io_github_vrcmteam_vrcm_core_NativeCore_start(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    files_dir: jni::objects::JString,
) -> jni::sys::jstring {
    let result = (|| -> Result<String, String> {
        let files_dir: String = env
            .get_string(&files_dir)
            .map_err(|error| format!("read filesDir: {error}"))?
            .into();
        let endpoint = start_core(Path::new(&files_dir))?;
        serde_json::to_string(&endpoint).map_err(|error| error.to_string())
    })();

    let reply = match result {
        Ok(json) => json,
        Err(message) => serde_json::json!({ "error": message }).to_string(),
    };
    env.new_string(reply)
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
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
        let endpoint = start_core(&dir).expect("start core");

        assert!(endpoint.port > 0, "ephemeral port assigned");
        assert!(endpoint.token.len() >= 43, "token is high entropy");
        assert!(dir.join("VRCX-0.sqlite3").exists(), "sqlite opened");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_endpoint_is_served_over_http() {
        let dir = scratch_dir("http");
        let endpoint = start_core(&dir).expect("start core");

        // Blocking HTTP from a plain std socket: the runtime is already serving
        // on its own threads.
        let body = r#"{"cmd":"app__feed_rows_query","args":{"userId":"usr_test","mode":"lookup","maxEntries":5}}"#;
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
}

