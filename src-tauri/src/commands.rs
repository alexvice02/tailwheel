use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

use taildrop_core::error::TaildropError;
use taildrop_core::models::{CpTarget, DeviceStats, PendingIncoming, Settings, TransferRecord};
use taildrop_core::store::Store;
use taildrop_core::{device_stats, poll_inbox_once, send_with_attribution, tailscale};

pub struct AppState {
    pub store: Arc<Store>,
}

fn to_str_err<T>(result: Result<T, TaildropError>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}

/// Run `work` off the UI thread.
async fn blocking<T, F>(work: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, TaildropError> + Send + 'static,
    T: Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(result) => to_str_err(result),
        Err(join_err) => Err(join_err.to_string()),
    }
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let store = state.store.clone();
    blocking(move || store.load_settings()).await
}

/// Experimental: request a chromeless window. Best-effort — a window manager
/// that ignores the request (most stacking WMs) just keeps its own title bar.
pub fn apply_window_decorations(app: &AppHandle, hide_titlebar: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_decorations(!hide_titlebar);
    }
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    let store = state.store.clone();
    let hide_titlebar = settings.hide_titlebar;
    let retention = settings.history_retention;

    blocking(move || {
        store.save_settings(&settings)?;
        store.prune_history(retention, chrono::Utc::now())
    })
    .await?;

    apply_window_decorations(&app, hide_titlebar);
    Ok(())
}

/// `fresh` skips the `tailscale` query cache — the frontend passes it for
/// explicit refresh clicks, and leaves it off for the loads that happen just
/// because a view mounted.
#[tauri::command]
pub async fn get_device_stats(
    state: State<'_, AppState>,
    fresh: Option<bool>,
) -> Result<DeviceStats, String> {
    let store = state.store.clone();
    blocking(move || device_stats(&store, fresh.unwrap_or(false))).await
}

#[tauri::command]
pub async fn get_cp_targets(fresh: Option<bool>) -> Result<Vec<CpTarget>, String> {
    blocking(move || tailscale::cp_targets(fresh.unwrap_or(false))).await
}

/// Flattens file-picker output (which may include directories, when the
/// user chose "Browse folder") into a plain list of file paths to send.
#[tauri::command]
pub async fn expand_send_paths(paths: Vec<String>) -> Result<Vec<String>, String> {
    blocking(move || {
        let paths: Vec<std::path::PathBuf> =
            paths.into_iter().map(std::path::PathBuf::from).collect();
        taildrop_core::expand_send_paths(&paths).map(|files| {
            files
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
    })
    .await
}

#[tauri::command]
pub async fn send_file(
    state: State<'_, AppState>,
    target: String,
    path: String,
    batch_id: Option<String>,
) -> Result<TransferRecord, String> {
    let store = state.store.clone();
    blocking(move || {
        // Cached after the first file of a batch: this only needs our own
        // hostname for the attribution sidecar, which cannot change mid-send.
        let status = tailscale::status(false)?;
        send_with_attribution(
            &store,
            &status.self_peer.hostname,
            &status.self_peer.dns_name,
            &target,
            Path::new(&path),
            batch_id.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn list_history(state: State<'_, AppState>) -> Result<Vec<TransferRecord>, String> {
    let store = state.store.clone();
    blocking(move || store.list_history()).await
}

#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    let store = state.store.clone();
    blocking(move || store.clear_history()).await
}

#[tauri::command]
pub async fn list_pending(state: State<'_, AppState>) -> Result<Vec<PendingIncoming>, String> {
    let store = state.store.clone();
    blocking(move || store.list_pending()).await
}

/// Manual "refresh" trigger for the frontend, on top of the background
/// poller — lets the UI react immediately to a user-initiated check instead
/// of waiting for the next timer tick.
#[tauri::command]
pub async fn poll_now(state: State<'_, AppState>) -> Result<Vec<PendingIncoming>, String> {
    let store = state.store.clone();
    blocking(move || {
        let settings = store.load_settings()?;
        poll_inbox_once(&store, settings.conflict_policy)
    })
    .await
}

#[tauri::command]
pub async fn accept_pending(
    state: State<'_, AppState>,
    id: String,
) -> Result<TransferRecord, String> {
    let store = state.store.clone();
    blocking(move || {
        let settings = store.load_settings()?;
        store.accept_pending(&id, &settings)
    })
    .await
}

#[tauri::command]
pub async fn reject_pending(
    state: State<'_, AppState>,
    id: String,
) -> Result<TransferRecord, String> {
    let store = state.store.clone();
    blocking(move || store.reject_pending(&id)).await
}
