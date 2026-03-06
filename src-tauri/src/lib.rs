mod commands;
mod menu;
mod tray;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_pty::init())
        .invoke_handler(tauri::generate_handler![
            commands::gateway::start_gateway,
            commands::gateway::stop_gateway,
            commands::gateway::gateway_status,
        ])
        .setup(|app| {
            // --- Tray (Tauri v2 API: build in setup, NOT .system_tray()) ---
            tray::create(app)?;

            // --- Menu (Tauri v2 API: build in setup, NOT .menu()) ---
            menu::create(app)?;

            // --- Auto-start gateway sidecar ---
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = commands::gateway::auto_start(handle).await {
                    log::error!("Failed to start gateway: {e}");
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building GHOST desktop")
        .run(|app_handle, event| {
            // Kill sidecar on app exit (NOT on window close — tray keeps app alive)
            if let tauri::RunEvent::Exit = event {
                tauri::async_runtime::block_on(async {
                    commands::gateway::auto_stop(app_handle.clone()).await;
                });
            }
        });
}
