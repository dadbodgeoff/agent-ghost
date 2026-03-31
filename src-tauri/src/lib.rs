mod commands;
pub mod error;
mod menu;
mod tray;

use tauri::Manager;

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![
            commands::desktop::read_keybindings,
            commands::desktop::get_auth_token,
            commands::desktop::set_auth_token,
            commands::desktop::clear_auth_token,
            commands::desktop::get_replay_state,
            commands::desktop::advance_replay_session_epoch,
            commands::desktop::open_terminal_session,
            commands::desktop::write_terminal_input,
            commands::desktop::resize_terminal_session,
            commands::desktop::close_terminal_session,
            commands::gateway::start_gateway,
            commands::gateway::stop_gateway,
            commands::gateway::gateway_status,
            commands::gateway::gateway_port,
        ])
        .setup(|app| {
            // --- Tray (Tauri v2 API: build in setup, NOT .system_tray()) ---
            tray::create(app)?;

            // --- Menu (Tauri v2 API: build in setup, NOT .menu()) ---
            menu::create(app)?;

            app.manage(commands::desktop::DesktopTerminalState::default());

            // --- Auto-start gateway sidecar ---
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = commands::gateway::auto_start(handle.clone()).await {
                    log::error!("Failed to start gateway: {e}");
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!());

    match app {
        Ok(app) => app.run(|app_handle, event| {
            // Kill sidecar on app exit (NOT on window close — tray keeps app alive)
            if let tauri::RunEvent::Exit = event {
                tauri::async_runtime::block_on(async {
                    commands::gateway::auto_stop(app_handle.clone()).await;
                });
            }
        }),
        Err(error) => {
            eprintln!("Failed to build GHOST desktop: {error}");
        }
    }
}
