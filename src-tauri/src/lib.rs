mod commands;
mod poller;
mod tray;

use commands::AppState;
use poller::Receiver;
use std::sync::Arc;
use taildrop_core::store::Store;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let receiver = Arc::new(Receiver::default());
    let receiver_for_exit = receiver.clone();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let store = Arc::new(Store::new(None).expect("failed to initialize local store"));
            let settings = store.load_settings().unwrap_or_default();
            // One line per `tailscale` invocation with how long it took, next
            // to the rest of the app's state. Per-call cost is the difference
            // between a snappy app and a frozen one, and it varies enough
            // between platforms that it has to be measured, not assumed.
            taildrop_core::tailscale::set_timing_log(store.root().join("tailwheel.log"));
            commands::apply_window_decorations(app.handle(), settings.hide_titlebar);
            tray::setup(app.handle())?;
            // Ask tailscaled for the peer list now, on a thread of its own, so
            // those round trips overlap with the webview booting instead of
            // starting only once the tailnet view has mounted and asked.
            std::thread::spawn(taildrop_core::tailscale::warm_caches);
            poller::spawn(app.handle().clone(), store.clone(), receiver.clone());
            app.manage(AppState {
                store,
                receiver: receiver.clone(),
            });
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
            commands::expand_send_paths,
            commands::send_file,
            commands::list_history,
            commands::clear_history,
            commands::list_pending,
            commands::poll_now,
            commands::accept_pending,
            commands::reject_pending,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // `build` + `run` rather than plain `run` so there's somewhere to hang the
    // exit hook: the `tailscale file get --loop` child is not reaped with its
    // parent on Windows, and an orphan would keep draining the inbox into a
    // staging directory nothing is watching.
    app.run(move |_app, event| {
        if let tauri::RunEvent::Exit = event {
            receiver_for_exit.shutdown();
        }
    });
}
