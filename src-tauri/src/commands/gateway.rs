use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::CommandChild;
use tauri_plugin_shell::ShellExt;
use tokio::sync::Mutex;

use crate::error::GhostDesktopError;

pub struct GatewayProcess(pub Mutex<Option<CommandChild>>);

pub struct GatewayPort(pub Mutex<u16>);

#[derive(Deserialize, Default)]
struct MinimalConfig {
    #[serde(default)]
    gateway: MinimalGateway,
}

#[derive(Deserialize)]
struct MinimalGateway {
    #[serde(default = "default_port")]
    port: u16,
}

impl Default for MinimalGateway {
    fn default() -> Self {
        Self {
            port: default_port(),
        }
    }
}

fn default_port() -> u16 {
    39780
}

fn read_port_from_config(config_path: &str) -> u16 {
    std::fs::read_to_string(config_path)
        .ok()
        .and_then(|s| serde_yaml::from_str::<MinimalConfig>(&s).ok())
        .map(|c| c.gateway.port)
        .unwrap_or_else(default_port)
}

fn resolve_config_path(handle: &AppHandle) -> String {
    let resource_dir = handle.path().resource_dir().ok();
    let candidates = [
        resource_dir.as_ref().map(|d| d.join("_up_/ghost.yml")),
        resource_dir.as_ref().map(|d| d.join("ghost.yml")),
        Some(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ghost.yml")),
        Some(std::path::PathBuf::from("ghost.yml")),
    ];
    let path = candidates
        .into_iter()
        .flatten()
        .find(|p| p.exists())
        .unwrap_or_else(|| std::path::PathBuf::from("ghost.yml"));
    path.canonicalize()
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn resolve_desktop_gateway_token() -> String {
    if let Ok(token) = std::env::var("GHOST_TOKEN") {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    if let Ok(Some(token)) = crate::commands::desktop::load_auth_token() {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    format!("ghost-desktop-{}", uuid::Uuid::now_v7())
}

async fn set_gateway_port(handle: &AppHandle, port: u16) {
    if let Some(state) = handle.try_state::<GatewayPort>() {
        *state.0.lock().await = port;
    }
}

async fn resolved_gateway_port(handle: &AppHandle) -> u16 {
    if let Some(state) = handle.try_state::<GatewayPort>() {
        *state.0.lock().await
    } else {
        default_port()
    }
}

async fn clear_managed_gateway_process(handle: &AppHandle) {
    if let Some(state) = handle.try_state::<GatewayProcess>() {
        let mut guard = state.0.lock().await;
        *guard = None;
    }
}

async fn store_managed_gateway_process(handle: &AppHandle, child: CommandChild) {
    if let Some(state) = handle.try_state::<GatewayProcess>() {
        let mut guard = state.0.lock().await;
        *guard = Some(child);
    }
}

fn health_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_default()
}

pub async fn auto_start(handle: AppHandle) -> Result<(), GhostDesktopError> {
    let config_path = resolve_config_path(&handle);
    let desktop_gateway_token = resolve_desktop_gateway_token();

    crate::commands::desktop::sync_auth_token(&desktop_gateway_token).map_err(|reason| {
        GhostDesktopError::ConfigError {
            reason: format!("failed to persist desktop auth token: {reason}"),
        }
    })?;

    let port = read_port_from_config(&config_path);
    set_gateway_port(&handle, port).await;

    let health_url = format!("http://127.0.0.1:{port}/api/health");
    let client = health_client();
    if let Ok(resp) = client.get(&health_url).send().await {
        if resp.status().is_success() {
            log::info!("Gateway already healthy on port {port}; skipping sidecar launch");
            clear_managed_gateway_process(&handle).await;
            return Ok(());
        }
    }

    let pid_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".ghost/data/gateway.pid");
    if pid_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&pid_path) {
            if let Ok(info) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(pid) = info["pid"].as_u64() {
                    let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
                    if alive {
                        log::warn!("Stale gateway process {pid} found; sending SIGTERM");
                        unsafe {
                            libc::kill(pid as i32, libc::SIGTERM);
                        }
                        for _ in 0..12 {
                            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                            if unsafe { libc::kill(pid as i32, 0) } != 0 {
                                break;
                            }
                        }
                        if unsafe { libc::kill(pid as i32, 0) } == 0 {
                            log::warn!("Gateway process {pid} did not exit; sending SIGKILL");
                            unsafe {
                                libc::kill(pid as i32, libc::SIGKILL);
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    }
                    let _ = std::fs::remove_file(&pid_path);
                }
            }
        }
    }

    let sidecar =
        handle
            .shell()
            .sidecar("ghost")
            .map_err(|e| GhostDesktopError::GatewayStartFailed {
                reason: e.to_string(),
            })?;
    log::info!("Using config: {config_path}");

    let dev_port = port + 1;
    let cors_origins = format!(
        "http://localhost:{dev_port},http://127.0.0.1:{dev_port},http://127.0.0.1:{port},https://tauri.localhost,tauri://localhost,http://tauri.localhost"
    );

    let mut cmd = sidecar
        .args(["serve", "--config", &config_path])
        .env("GHOST_CORS_ORIGINS", &cors_origins);

    for key in &[
        "XAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "GEMINI_API_KEY",
    ] {
        if let Ok(val) = std::env::var(key) {
            cmd = cmd.env(key, &val);
        }
    }

    cmd = cmd.env("GHOST_TOKEN", &desktop_gateway_token);

    let (mut rx, child) = cmd
        .spawn()
        .map_err(|e| GhostDesktopError::GatewayStartFailed {
            reason: e.to_string(),
        })?;
    store_managed_gateway_process(&handle, child).await;

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

    let client = health_client();
    for i in 0..30 {
        if client
            .get(&health_url)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            log::info!("Gateway healthy on port {port} after {}ms", i * 500);
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    Err(GhostDesktopError::HealthCheckFailed {
        reason: format!("Gateway failed to become healthy on port {port} within 15s"),
    })
}

pub async fn auto_stop(handle: AppHandle) {
    if let Some(state) = handle.try_state::<GatewayProcess>() {
        let mut guard = state.0.lock().await;
        if let Some(child) = guard.take() {
            let _ = child.kill();
            log::info!("Gateway sidecar stopped");
        } else {
            log::info!("No sidecar to stop (external gateway or already stopped)");
        }
    }
}

#[tauri::command]
pub async fn start_gateway(handle: AppHandle) -> Result<String, GhostDesktopError> {
    auto_start(handle).await.map(|_| "started".into())
}

#[tauri::command]
pub async fn stop_gateway(handle: AppHandle) -> Result<String, GhostDesktopError> {
    auto_stop(handle).await;
    Ok("stopped".into())
}

#[tauri::command]
pub async fn gateway_status(handle: AppHandle) -> Result<String, GhostDesktopError> {
    let port = resolved_gateway_port(&handle).await;
    let client = health_client();
    match client
        .get(format!("http://127.0.0.1:{port}/api/health"))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => Ok("healthy".into()),
        _ => Ok("unreachable".into()),
    }
}

#[tauri::command]
pub async fn gateway_port(handle: AppHandle) -> u16 {
    resolved_gateway_port(&handle).await
}
