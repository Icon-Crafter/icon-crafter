//! Tauri 2 desktop shell for Icon Crafter.
//!
//! The React frontend is a presentation layer only: it renders a display list
//! produced by the Rust engine and sends user intent as commands. All document
//! state, hit testing, layout, geometry and file I/O live in Rust crates.

use tauri::Manager;

/// Build and run the desktop application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,iconcrafter=debug")),
        )
        .with_target(true)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                "Icon Crafter desktop starting"
            );
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("Icon Crafter");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Icon Crafter desktop application");
}
