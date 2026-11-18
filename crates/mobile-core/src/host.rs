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

/// Installs the process-level rustls crypto provider before anything opens a TLS connection.
///
/// Without this the app aborts a few seconds after launch on iOS:
///
/// ```text
/// abort() called
/// core::option::expect              <- could not determine the process-level CryptoProvider
/// rustls::client::...
/// tokio_tungstenite::...
/// vrcx_0_outbound_...
/// ```
///
/// `tokio-tungstenite` builds its own `rustls::ClientConfig` through `ClientConfig::builder()`, which
/// needs a default provider. `Cargo.toml` enables `ring` for the workspace while tungstenite pulls
/// rustls in with its default features, so *both* providers are compiled in - and rustls refuses to
/// guess between them. `headless` installs `aws_lc_rs` for the same reason; the phone installs `ring`,
/// which is the provider `mobile_tls_client_config` already uses for HTTPS.
///
/// Idempotent: an already-installed provider makes this return `Err`, which is ignored.
fn install_tls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
/// Builds the runtime host rooted at `app_data_dir`.
///
/// `device_name` is what this installation reports to remote sync presence (`_sync_devices`). The
/// phone used to pass an empty string - which the sync host treats as "hide it" - so the device list
/// showed a bare id (or, worse, another machine's name on the row the two of them shared).
pub fn build_host(app_data_dir: &Path, device_name: &str) -> Result<RuntimeHostState, String> {
    install_tls_crypto_provider();

    // Kotlin hands over `Context.getFilesDir()`, but VRCX keeps its own folder inside the platform's
    // data directory - the desktop resolves `%APPDATA%\VRCX-0`, not `%APPDATA%`. Appending the same
    // name here makes the two layouts identical (database, image cache and secrets key all under
    // `.../VRCX-0/`), which is also where the secrets key already lived: `platform` resolved
    // `default_app_data_dir()` by appending it, while the database was written one level up.
    let app_data_dir = &app_data_dir.join("VRCX-0");
    migrate_legacy_layout(app_data_dir)?;

    // Tell `platform` where this app keeps its data. Android has no usable `HOME`, so the desktop
    // resolution (`dirs::config_dir()`) cannot produce this path; Kotlin passes `filesDir` in and the
    // override carries it to the code that needs it (error log, secrets key).
    vrcx_0_platform::app_paths::set_app_data_dir_override(app_data_dir.to_path_buf());

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

/// Moves a database written by an earlier build (`<files>/VRCX-0.sqlite3`) into the app folder now
/// that the layout matches the desktop (`<files>/VRCX-0/VRCX-0.sqlite3`).
///
/// Runs on every start but does nothing once the new file exists, so a crash halfway through is
/// retried instead of silently starting from an empty database. The WAL and shared-memory files move
/// with it: leaving the WAL behind would silently drop the tail of the log.
fn migrate_legacy_layout(app_data_dir: &Path) -> Result<(), String> {
    const DB_FILE: &str = "VRCX-0.sqlite3";
    let Some(parent) = app_data_dir.parent() else {
        return Ok(());
    };
    let legacy = parent.join(DB_FILE);
    let target = app_data_dir.join(DB_FILE);
    if !legacy.is_file() || target.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(app_data_dir)
        .map_err(|error| format!("create {}: {error}", app_data_dir.display()))?;
    for suffix in ["", "-wal", "-shm"] {
        let from = parent.join(format!("{DB_FILE}{suffix}"));
        if !from.is_file() {
            continue;
        }
        let to = app_data_dir.join(format!("{DB_FILE}{suffix}"));
        std::fs::rename(&from, &to).map_err(|error| {
            format!(
                "move {} to {}: {error}",
                from.display(),
                to.display()
            )
        })?;
    }
    Ok(())
}
