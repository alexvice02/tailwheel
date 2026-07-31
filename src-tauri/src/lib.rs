mod commands;
mod poller;
mod tray;

use commands::AppState;
use std::sync::Arc;
use taildrop_core::store::Store;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let store = Arc::new(Store::new(None).expect("failed to initialize local store"));
            tray::setup(app.handle())?;
            poller::spawn(app.handle().clone(), store.clone());
            app.manage(AppState { store });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Close hides the window instead of quitting: the app keeps
            // running in the tray so it can keep receiving/confirming files.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::update_settings,
            commands::get_device_stats,
            commands::get_cp_targets,
            commands::send_file,
            commands::list_history,
            commands::list_pending,
            commands::poll_now,
            commands::accept_pending,
            commands::reject_pending,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
