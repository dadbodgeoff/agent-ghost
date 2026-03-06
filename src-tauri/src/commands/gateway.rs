use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandChild;

pub struct GatewayProcess(pub Mutex<Option<CommandChild>>);

pub async fn auto_start(handle: AppHandle) -> Result<(), String> {
    let sidecar = handle.shell().sidecar("binaries/ghost").map_err(|e| e.to_string())?;
    // Resolve ghost.yml: check next to sidecar binary first, then project root
    let config_path = {
        let app_dir = handle.path().resource_dir().ok();
        let candidates = [
            app_dir.as_ref().map(|d| d.join("ghost.yml")),
            Some(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ghost.yml")),
            Some(std::path::PathBuf::from("ghost.yml")),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|p| p.exists())
            .unwrap_or_else(|| std::path::PathBuf::from("ghost.yml"))
    };

    let (mut rx, child) = sidecar
        .args(["serve", "--config", &config_path.to_string_lossy()])
        .env(
            "GHOST_CORS_ORIGINS",
            "http://localhost:39781,http://127.0.0.1:39781,http://127.0.0.1:39780,https://tauri.localhost",
        )
        .spawn()
        .map_err(|e| e.to_string())?;

    // Store child handle for shutdown
    handle.manage(GatewayProcess(Mutex::new(Some(child))));

    // Log sidecar stdout/stderr
    tauri::async_runtime::spawn(async move {
        use tauri_plugin_shell::process::CommandEvent;
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line) => {
                    log::info!("[gateway] {}", String::from_utf8_lossy(&line));
                }
                CommandEvent::Stderr(line) => {
                    log::warn!("[gateway] {}", String::from_utf8_lossy(&line));
                }
                CommandEvent::Terminated(status) => {
                    log::info!("[gateway] exited: {status:?}");
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for health (poll every 500ms, up to 15s)
    for i in 0..30 {
        if reqwest::get("http://127.0.0.1:39780/api/health")
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            log::info!("Gateway healthy after {}ms", i * 500);
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err("Gateway failed to become healthy within 15s".into())
}

pub async fn auto_stop(handle: AppHandle) {
    if let Some(state) = handle.try_state::<GatewayProcess>() {
        if let Ok(mut guard) = state.0.lock() {
            if let Some(child) = guard.take() {
                let _ = child.kill();
                log::info!("Gateway sidecar stopped");
            }
        }
    }
}

#[tauri::command]
pub async fn start_gateway(handle: AppHandle) -> Result<String, String> {
    auto_start(handle).await.map(|_| "started".into())
}

#[tauri::command]
pub async fn stop_gateway(handle: AppHandle) -> Result<String, String> {
    auto_stop(handle).await;
    Ok("stopped".into())
}

#[tauri::command]
pub async fn gateway_status() -> Result<String, String> {
    match reqwest::get("http://127.0.0.1:39780/api/health").await {
        Ok(r) if r.status().is_success() => Ok("healthy".into()),
        _ => Ok("unreachable".into()),
    }
}
