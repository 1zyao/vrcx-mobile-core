mod app;
pub mod bindings_export;
mod bootstrap;
mod commands;
mod desktop_notification_activation;
mod error;
mod localization;
#[cfg(target_os = "macos")]
mod macos_menu;
mod single_instance_gate;
mod state;

pub use app::run as run_app;

/// Tauri's mobile runtime looks for the symbols this macro generates; without it
/// `tauri android build` rejects the produced library with
/// "does not include required runtime symbols".
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app::run();
}
