//! Tauri 2 desktop shell for Icon Crafter.
//!
//! The React frontend is a presentation layer only: it renders RGBA frames
//! produced by the Rust engine, draws the overlay and sends user intent as
//! commands. All document state, hit testing, layout, geometry and file I/O
//! live in Rust crates.

pub mod ipc;

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
        .manage(ipc::IpcState::new())
        .invoke_handler(tauri::generate_handler![
            ipc::bootstrap,
            ipc::new_document,
            ipc::open_document,
            ipc::close_document,
            ipc::save_document,
            ipc::engine_state,
            ipc::set_tool,
            ipc::pointer,
            ipc::wheel,
            ipc::key,
            ipc::command,
            ipc::render_frame,
            ipc::node_details,
            ipc::export_png,
            ipc::export_svg,
            ipc::export_pdf,
            ipc::import_svg,
            ipc::import_image,
            ipc::set_preference,
            ipc::set_dev_mode,
            ipc::dev_inspect,
            ipc::dev_codegen,
            ipc::dev_handoff,
            ipc::dev_add_annotation,
            ipc::dev_mark_ready,
            ipc::dev_compare,
        ])
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
