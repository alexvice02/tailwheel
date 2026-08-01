use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

use taildrop_core::format::human_size;
use taildrop_core::store::Store;

/// Background loop that periodically drains the Tailscale inbox into
/// staging, matches any `.tdmeta.json` sender sidecars, and surfaces new
/// arrivals to the frontend (event) and the OS (notification). Runs on a
/// plain OS thread rather than async: everything it calls shells out to the
/// `tailscale` binary and blocks on file IO anyway, so there's nothing to
/// gain from tokio here.
pub fn spawn(app_handle: AppHandle, store: Arc<Store>) {
    std::thread::spawn(move || loop {
        let settings = match store.load_settings() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("taildrop: failed to load settings: {e}");
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
        };

        if let Err(e) = store.prune_history(settings.history_retention, chrono::Utc::now()) {
            eprintln!("taildrop: history prune failed: {e}");
        }

        match taildrop_core::poll_inbox_once(&store, settings.conflict_policy) {
            Ok(new_items) if !new_items.is_empty() => {
                if settings.auto_accept {
                    for item in &new_items {
                        match store.accept_pending(&item.id, &settings) {
                            Ok(record) => {
                                let body = format!(
                                    "Received {} ({})",
                                    record.file_name,
                                    human_size(record.size)
                                );
                                let _ = app_handle
                                    .notification()
                                    .builder()
                                    .title("Taildrop")
                                    .body(body)
                                    .show();
                            }
                            Err(e) => eprintln!("taildrop: auto-accept failed: {e}"),
                        }
                    }
                    let _ = app_handle.emit("history-updated", ());
                } else {
                    for item in &new_items {
                        let body = match &item.sender_hostname {
                            Some(sender) => format!("{sender} sent you {}", item.file_name),
                            None => format!("New file received: {}", item.file_name),
                        };
                        let _ = app_handle
                            .notification()
                            .builder()
                            .title("Taildrop")
                            .body(body)
                            .show();
                    }
                    let _ = app_handle.emit("pending-updated", &new_items);
                }
            }
            Ok(_) => {}
            Err(e) => eprintln!("taildrop: inbox poll failed: {e}"),
        }

        std::thread::sleep(Duration::from_secs(settings.poll_interval_secs.max(1)));
    });
}
