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

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    to_str_err(state.store.load_settings())
}

/// Experimental: request a chromeless window. Best-effort — a window manager
/// that ignores the request (most stacking WMs) just keeps its own title bar.
pub fn apply_window_decorations(app: &AppHandle, hide_titlebar: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_decorations(!hide_titlebar);
    }
}

#[tauri::command]
pub fn update_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> Result<(), String> {
    to_str_err(state.store.save_settings(&settings))?;
    apply_window_decorations(&app, settings.hide_titlebar);

    to_str_err(
        state
            .store
            .prune_history(settings.history_retention, chrono::Utc::now()),
    )
}

#[tauri::command]
pub fn get_device_stats(state: State<AppState>) -> Result<DeviceStats, String> {
    to_str_err(device_stats(&state.store))
}

#[tauri::command]
pub fn get_cp_targets() -> Result<Vec<CpTarget>, String> {
    to_str_err(tailscale::cp_targets())
}

/// Flattens file-picker output (which may include directories, when the
/// user chose "Browse folder") into a plain list of file paths to send.
#[tauri::command]
pub fn expand_send_paths(paths: Vec<String>) -> Result<Vec<String>, String> {
    let paths: Vec<std::path::PathBuf> = paths.into_iter().map(std::path::PathBuf::from).collect();
    to_str_err(taildrop_core::expand_send_paths(&paths)).map(|files| {
        files
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    })
}

#[tauri::command]
pub fn send_file(
    state: State<AppState>,
    target: String,
    path: String,
    batch_id: Option<String>,
) -> Result<TransferRecord, String> {
    let status = to_str_err(tailscale::status())?;
    to_str_err(send_with_attribution(
        &state.store,
        &status.self_peer.hostname,
        &status.self_peer.dns_name,
        &target,
        Path::new(&path),
        batch_id.as_deref(),
    ))
}

#[tauri::command]
pub fn list_history(state: State<AppState>) -> Result<Vec<TransferRecord>, String> {
    to_str_err(state.store.list_history())
}

#[tauri::command]
pub fn clear_history(state: State<AppState>) -> Result<(), String> {
    to_str_err(state.store.clear_history())
}

#[tauri::command]
pub fn list_pending(state: State<AppState>) -> Result<Vec<PendingIncoming>, String> {
    to_str_err(state.store.list_pending())
}

/// Manual "refresh" trigger for the frontend, on top of the background
/// poller — lets the UI react immediately to a user-initiated check instead
/// of waiting for the next timer tick.
#[tauri::command]
pub fn poll_now(state: State<AppState>) -> Result<Vec<PendingIncoming>, String> {
    let settings = to_str_err(state.store.load_settings())?;
    to_str_err(poll_inbox_once(&state.store, settings.conflict_policy))
}

#[tauri::command]
pub fn accept_pending(state: State<AppState>, id: String) -> Result<TransferRecord, String> {
    let settings = to_str_err(state.store.load_settings())?;
    to_str_err(state.store.accept_pending(&id, &settings))
}

#[tauri::command]
pub fn reject_pending(state: State<AppState>, id: String) -> Result<TransferRecord, String> {
    to_str_err(state.store.reject_pending(&id))
}
