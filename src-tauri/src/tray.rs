use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

/// Tray icon + menu: Open / Send file... / Recent transfers / Quit. The
/// window hides rather than closes (see the `CloseRequested` handler in
/// `lib.rs`), so this tray is the only way back in once the window is
/// dismissed.
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open", "Open taildrop-gui", true, None::<&str>)?;
    let send_item = MenuItem::with_id(app, "send", "Send file...", true, None::<&str>)?;
    let history_item = MenuItem::with_id(app, "history", "Recent transfers", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&open_item, &send_item, &history_item, &separator, &quit_item],
    )?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().unwrap())
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("taildrop-gui")
        .on_menu_event(|app, event| {
            let route = match event.id().as_ref() {
                "open" => Some("/"),
                "send" => Some("/send"),
                "history" => Some("/history"),
                "quit" => {
                    app.exit(0);
                    None
                }
                _ => None,
            };
            if let Some(route) = route {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = window.emit("navigate", route);
                }
            }
        })
        .build(app)?;

    Ok(())
}
