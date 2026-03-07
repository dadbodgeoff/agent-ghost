mod commands;
pub mod error;
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
            commands::gateway::gateway_port,
        ])
        .setup(|app| {
            // --- Tray (Tauri v2 API: build in setup, NOT .system_tray()) ---
            tray::create(app)?;

            // --- Menu (Tauri v2 API: build in setup, NOT .menu()) ---
            menu::create(app)?;

            // --- Auto-start gateway sidecar ---
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = commands::gateway::auto_start(handle.clone()).await {
                    log::error!("Failed to start gateway: {e}");
                }
                // Inject the resolved port into the webview so the dashboard
                // can connect to the correct gateway URL.
                if let Some(port_state) = handle.try_state::<commands::gateway::GatewayPort>() {
                    let port = port_state.0;
                    if let Some(window) = handle.get_webview_window("main") {
                        // Inject gateway port for dashboard API client.
                        let _ = window.eval(&format!(
                            "window.__GHOST_GATEWAY_PORT__ = {};",
                            port
                        ));
                        // WP6-B: Inject tightened CSP now that the gateway port is known.
                        let _ = window.eval(&format!(
                            r#"{{
                                const meta = document.createElement('meta');
                                meta.httpEquiv = 'Content-Security-Policy';
                                meta.content = "default-src 'self'; connect-src 'self' http://127.0.0.1:{port} ws://127.0.0.1:{port}; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'";
                                document.head.appendChild(meta);
                            }}"#,
                            port = port
                        ));
                    }
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
