use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

pub fn create(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let mut tray = TrayIconBuilder::new().tooltip("GHOST — Gateway Active");
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    } else {
        log::warn!("Desktop tray icon unavailable; continuing without a custom icon");
    }

    tray.on_tray_icon_event(|tray_icon, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let app = tray_icon.app_handle();
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    })
    .build(app)?;
    Ok(())
}
