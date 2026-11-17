//! Builds the VRCX-0 runtime host for the phone.
//!
//! Nothing here is hand-wired: `RuntimeHostStateBuilder` assembles the whole
//! runtime (auth scope, web client, realtime, per-domain runtimes) exactly the
//! way the desktop and the headless host do it. The only mobile-specific part is
//! the composition, and it stays trivial because the three desktop extras have
//! no-ops:
//!
//!   local game log  -> `UnavailableLocalGameContextSource` (phone has no logs)
//!   group order     -> `UnavailableGroupOrderSource`
//!   profile services-> none
//!
//! Profile is `HeadlessData`: no tray, no game log, no window services.

use std::path::Path;
use std::sync::Arc;

use vrcx_0_application_core::{RuntimeHostProfile, UnavailableLocalGameContextSource};
use vrcx_0_platform::app_paths::{AppDataDirResolution, AppDataDirSource};
use vrcx_0_composition::{
    RuntimeHostComposition, RuntimeHostOptions, RuntimeHostState, RuntimeHostStateBuilder,
    UnavailableGroupOrderSource,
};

/// Builds the runtime host rooted at `app_data_dir`.
///
/// `device_name` is what this installation reports to remote sync presence (`_sync_devices`). The
/// phone used to pass an empty string - which the sync host treats as "hide it" - so the device list
/// showed a bare id (or, worse, another machine's name on the row the two of them shared).
pub fn build_host(app_data_dir: &Path, device_name: &str) -> Result<RuntimeHostState, String> {
    let resolution = AppDataDirResolution {
        current_dir: app_data_dir.to_path_buf(),
        default_dir: app_data_dir.to_path_buf(),
        persisted_dir: None,
        cli_dir: None,
        source: AppDataDirSource::Default,
    };

    let builder = RuntimeHostStateBuilder::new(RuntimeHostOptions {
        realtime_origin: "mobile".to_string(),
        launched_from_autostart: false,
        app_data_dir: resolution,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        device_hostname: device_name.trim().to_string(),
        profile: RuntimeHostProfile::HeadlessData,
        database_maintenance_cache_dir: Some(app_data_dir.join("cache")),
        task_executor: None,
    })
    .map_err(|error| format!("runtime host options: {error}"))?;

    builder
        .finish(RuntimeHostComposition {
            local_game_context: Arc::new(UnavailableLocalGameContextSource),
            group_order_source: Arc::new(UnavailableGroupOrderSource),
            friend_projection_observer: None,
            profile_extension: None,
        })
        .map_err(|error| format!("build runtime host: {error}"))
}



#[cfg(test)]
mod tests {
    use super::*;

    /// Scratch directory inside the workspace: this build machine's tool
    /// processes cannot write to `%TEMP%`.
    fn scratch(label: &str) -> std::path::PathBuf {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(format!("compose-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn builds_a_headless_host_with_a_local_database() {
        let dir = scratch("host");
        let host = build_host(&dir, "test-device").expect("build host");

        // The builder resolves paths and opens the database during finish().
        assert!(dir.exists(), "app data dir kept");
        drop(host);

        let _ = std::fs::remove_dir_all(dir);
    }
}
