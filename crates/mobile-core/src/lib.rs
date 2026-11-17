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
    // The sync identity belongs to this installation, not to whatever database happens to sit here.
    // Copying a data directory copies `_sync_meta.sync.deviceId` too, and `_sync_devices` is keyed by
    // it, so two machines would share one presence row. Adopting the installation's own id wipes the
    // local sync markers, and the engine then bootstraps from scratch - which is what a database this
    // device did not write deserves.
    let install_id = install_device_id(files_dir)?;
    let adopted = host
        .remote_sync()
        .adopt_install_identity(&install_id)
        .map_err(|error| format!("adopt sync identity: {error}"))?;
    if adopted {
        eprintln!("sync identity adopted for this installation: {install_id}");
    }
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

/// The sync identity of this installation, kept next to the database but outside it.
///
/// Deliberately a file: `_sync_meta` travels with a copied data directory, a file in the app's own
/// data directory is recreated by a fresh install, so the two cases stay distinguishable. Missing
/// file means "this install never had an identity" - either a first run or a database copied from
/// another machine - and both get a new one.
fn install_device_id(files_dir: &Path) -> Result<String, String> {
    let path = files_dir.join("device-id");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    std::fs::write(&path, &id).map_err(|error| format!("write device-id: {error}"))?;
    Ok(id)
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
    device_name: jni::objects::JString,
) -> jni::sys::jstring {
    let result = (|| -> Result<String, String> {
        let files_dir: String = env
            .get_string(&files_dir)
            .map_err(|error| format!("read filesDir: {error}"))?
            .into();
        let device_name: String = env
            .get_string(&device_name)
            .map_err(|error| format!("read deviceName: {error}"))?
            .into();
        let endpoint = start_core(Path::new(&files_dir), &device_name)?;
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
        let endpoint = start_core(&dir, "test-device").expect("start core");

        assert!(endpoint.port > 0, "ephemeral port assigned");
        assert!(endpoint.token.len() >= 43, "token is high entropy");
        assert!(dir.join("VRCX-0.sqlite3").exists(), "sqlite opened");
        assert!(dir.join("device-id").exists(), "install identity written");

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
}



