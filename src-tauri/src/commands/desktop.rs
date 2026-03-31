use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use portable_pty::{native_pty_system, Child, ChildKiller, CommandBuilder, PtyPair, PtySize};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutBinding {
    pub key: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

fn ghost_config_path(file_name: &str) -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Unable to resolve home directory".to_string())?;
    Ok(home.join(".ghost").join(file_name))
}

fn resolve_default_shell() -> String {
    if cfg!(target_os = "windows") {
        std::env::var("COMSPEC")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "C:\\Windows\\System32\\cmd.exe".to_string())
    } else {
        std::env::var("SHELL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                if cfg!(target_os = "macos") {
                    "/bin/zsh".to_string()
                } else {
                    "/bin/sh".to_string()
                }
            })
    }
}

const DESKTOP_STATE_FILE: &str = "desktop-state.json";
const TERMINAL_DATA_EVENT_PREFIX: &str = "desktop-terminal-data:";
const TERMINAL_EXIT_EVENT_PREFIX: &str = "desktop-terminal-exit:";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DesktopRuntimeStateFile {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    replay_client_id: Option<String>,
    #[serde(default = "default_session_epoch")]
    replay_session_epoch: u64,
}

fn default_session_epoch() -> u64 {
    1
}

#[derive(Debug, Clone, Serialize)]
pub struct DesktopReplayState {
    pub client_id: String,
    pub session_epoch: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalDataEvent {
    pub session_id: u32,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalExitEvent {
    pub session_id: u32,
    pub exit_code: i32,
}

struct TerminalSession {
    pair: Mutex<PtyPair>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    child_killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    writer: Mutex<Box<dyn Write + Send>>,
}

pub struct DesktopTerminalState {
    next_session_id: AtomicU32,
    sessions: RwLock<BTreeMap<u32, Arc<TerminalSession>>>,
}

impl Default for DesktopTerminalState {
    fn default() -> Self {
        Self {
            next_session_id: AtomicU32::new(1),
            sessions: RwLock::new(BTreeMap::new()),
        }
    }
}

fn desktop_state_path() -> Result<PathBuf, String> {
    ghost_config_path(DESKTOP_STATE_FILE)
}

fn read_desktop_state_file() -> Result<DesktopRuntimeStateFile, String> {
    let path = desktop_state_path()?;
    match fs::read_to_string(&path) {
        Ok(raw) => {
            if raw.trim().is_empty() {
                Ok(DesktopRuntimeStateFile::default())
            } else {
                serde_json::from_str(&raw).map_err(|error| {
                    format!(
                        "Failed to parse desktop runtime state from {}: {error}",
                        path.display()
                    )
                })
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(DesktopRuntimeStateFile::default()),
        Err(error) => Err(format!(
            "Failed to read desktop runtime state from {}: {error}",
            path.display()
        )),
    }
}

fn write_desktop_state_file(state: &DesktopRuntimeStateFile) -> Result<(), String> {
    let path = desktop_state_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create desktop runtime state directory {}: {error}",
                parent.display()
            )
        })?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let raw = serde_json::to_string_pretty(state)
        .map_err(|error| format!("Failed to serialize desktop runtime state: {error}"))?;
    let mut file = fs::File::create(&tmp_path)
        .map_err(|error| format!("Failed to create {}: {error}", tmp_path.display()))?;
    file.write_all(raw.as_bytes())
        .map_err(|error| format!("Failed to write {}: {error}", tmp_path.display()))?;
    file.sync_all()
        .map_err(|error| format!("Failed to fsync {}: {error}", tmp_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("Failed to chmod {}: {error}", tmp_path.display()))?;
    }
    fs::rename(&tmp_path, &path)
        .map_err(|error| format!("Failed to replace {}: {error}", path.display()))?;
    Ok(())
}

fn load_desktop_state() -> Result<DesktopRuntimeStateFile, String> {
    let mut state = read_desktop_state_file()?;
    if state.replay_session_epoch == 0 {
        state.replay_session_epoch = default_session_epoch();
    }
    Ok(state)
}

pub fn load_auth_token() -> Result<Option<String>, String> {
    Ok(load_desktop_state()?.token)
}

fn save_desktop_state(
    mutate: impl FnOnce(&mut DesktopRuntimeStateFile),
) -> Result<DesktopRuntimeStateFile, String> {
    let mut state = load_desktop_state()?;
    mutate(&mut state);
    write_desktop_state_file(&state)?;
    Ok(state)
}

pub fn sync_auth_token(token: &str) -> Result<(), String> {
    if token.trim().is_empty() {
        return Err("auth token must not be empty".to_string());
    }

    save_desktop_state(|state| {
        state.token = Some(token.to_string());
    })?;
    Ok(())
}

fn session_for(
    state: &tauri::State<'_, DesktopTerminalState>,
    session_id: u32,
) -> Result<Arc<TerminalSession>, String> {
    state
        .sessions
        .read()
        .map_err(|_| "terminal session registry poisoned".to_string())?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| format!("terminal session {session_id} not found"))
}

fn remove_terminal_session(state: &DesktopTerminalState, session_id: u32) -> Result<(), String> {
    state
        .sessions
        .write()
        .map_err(|_| "terminal session registry poisoned".to_string())?
        .remove(&session_id);
    Ok(())
}

fn normalized_terminal_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

#[tauri::command]
pub async fn read_keybindings() -> Result<Vec<ShortcutBinding>, String> {
    let path = ghost_config_path("keybindings.json")?;
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "Failed to read keybindings from {}: {error}",
                path.display()
            ))
        }
    };

    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str::<Vec<ShortcutBinding>>(&raw).map_err(|error| {
        format!(
            "Failed to parse keybindings from {}: {error}",
            path.display()
        )
    })
}

#[tauri::command]
pub async fn get_auth_token() -> Result<Option<String>, String> {
    load_auth_token()
}

#[tauri::command]
pub async fn set_auth_token(token: String) -> Result<(), String> {
    sync_auth_token(&token)
}

#[tauri::command]
pub async fn clear_auth_token() -> Result<(), String> {
    save_desktop_state(|state| {
        state.token = None;
    })?;
    Ok(())
}

#[tauri::command]
pub async fn get_replay_state() -> Result<DesktopReplayState, String> {
    let state = save_desktop_state(|state| {
        if state.replay_client_id.is_none() {
            state.replay_client_id = Some(uuid::Uuid::now_v7().to_string());
        }
        if state.replay_session_epoch == 0 {
            state.replay_session_epoch = default_session_epoch();
        }
    })?;

    Ok(DesktopReplayState {
        client_id: state
            .replay_client_id
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string()),
        session_epoch: state.replay_session_epoch,
    })
}

#[tauri::command]
pub async fn advance_replay_session_epoch() -> Result<u64, String> {
    let state = save_desktop_state(|state| {
        state.replay_session_epoch = state.replay_session_epoch.saturating_add(1).max(1);
    })?;
    Ok(state.replay_session_epoch)
}

#[tauri::command]
pub async fn open_terminal_session<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    terminal_state: tauri::State<'_, DesktopTerminalState>,
    cols: u16,
    rows: u16,
) -> Result<u32, String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(normalized_terminal_size(cols, rows))
        .map_err(|error| format!("Failed to create PTY: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("Failed to open PTY writer: {error}"))?;
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("Failed to open PTY reader: {error}"))?;

    let mut command = CommandBuilder::new(resolve_default_shell());
    if let Some(home) = dirs::home_dir() {
        command.cwd(home);
    }
    command.env("TERM", "xterm-256color");

    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("Failed to spawn shell: {error}"))?;
    let child_killer = child.clone_killer();

    let session_id = terminal_state.next_session_id.fetch_add(1, Ordering::Relaxed);
    let session = Arc::new(TerminalSession {
        pair: Mutex::new(pair),
        child: Mutex::new(child),
        child_killer: Mutex::new(child_killer),
        writer: Mutex::new(writer),
    });
    terminal_state
        .sessions
        .write()
        .map_err(|_| "terminal session registry poisoned".to_string())?
        .insert(session_id, Arc::clone(&session));

    let app_handle_clone = app_handle.clone();
    std::thread::spawn(move || {
        let data_event = format!("{TERMINAL_DATA_EVENT_PREFIX}{session_id}");
        let exit_event = format!("{TERMINAL_EXIT_EVENT_PREFIX}{session_id}");
        let mut buffer = [0u8; 4096];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(bytes_read) => {
                    let payload = TerminalDataEvent {
                        session_id,
                        data: String::from_utf8_lossy(&buffer[..bytes_read]).into_owned(),
                    };
                    let _ = app_handle_clone.emit(&data_event, payload);
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }

        let exit_code = session
            .child
            .lock()
            .map(|mut child| child.wait().map(|status| status.exit_code()).unwrap_or(1))
            .unwrap_or(1);
        let _ = app_handle_clone.emit(
            &exit_event,
            TerminalExitEvent {
                session_id,
                exit_code: exit_code as i32,
            },
        );

        let state = app_handle_clone.state::<DesktopTerminalState>();
        let _ = remove_terminal_session(&state, session_id);
    });

    Ok(session_id)
}

#[tauri::command]
pub async fn write_terminal_input(
    terminal_state: tauri::State<'_, DesktopTerminalState>,
    session_id: u32,
    data: String,
) -> Result<(), String> {
    let session = session_for(&terminal_state, session_id)?;
    let mut writer = session
        .writer
        .lock()
        .map_err(|_| "terminal writer poisoned".to_string())?;
    writer
        .write_all(data.as_bytes())
        .map_err(|error| format!("Failed to write terminal input: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("Failed to flush terminal input: {error}"))?;
    Ok(())
}

#[tauri::command]
pub async fn resize_terminal_session(
    terminal_state: tauri::State<'_, DesktopTerminalState>,
    session_id: u32,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let session = session_for(&terminal_state, session_id)?;
    session
        .pair
        .lock()
        .map_err(|_| "terminal pair poisoned".to_string())?
        .master
        .resize(normalized_terminal_size(cols, rows))
        .map_err(|error| format!("Failed to resize terminal: {error}"))?;
    Ok(())
}

#[tauri::command]
pub async fn close_terminal_session(
    terminal_state: tauri::State<'_, DesktopTerminalState>,
    session_id: u32,
) -> Result<(), String> {
    let session = session_for(&terminal_state, session_id)?;
    session
        .child_killer
        .lock()
        .map_err(|_| "terminal killer poisoned".to_string())?
        .kill()
        .map_err(|error| format!("Failed to close terminal session: {error}"))?;
    remove_terminal_session(&terminal_state, session_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }

        fn remove(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(ref value) = self.previous {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    #[tokio::test]
    async fn read_keybindings_returns_empty_when_file_is_missing() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("create temp home");
        let _home = EnvVarGuard::set("HOME", temp.path().to_string_lossy().as_ref());

        let bindings = read_keybindings().await.expect("missing file should not fail");

        assert!(bindings.is_empty());
    }

    #[tokio::test]
    async fn read_keybindings_reads_user_file_from_ghost_config_dir() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("create temp home");
        let _home = EnvVarGuard::set("HOME", temp.path().to_string_lossy().as_ref());

        let config_dir = temp.path().join(".ghost");
        fs::create_dir_all(&config_dir).expect("create config dir");
        fs::write(
            config_dir.join("keybindings.json"),
            r#"[{"key":"mod+k","command":"search.global"},{"key":"mod+shift+t","command":"theme.toggle","when":"editorFocus"}]"#,
        )
        .expect("write keybindings");

        let bindings = read_keybindings().await.expect("valid keybindings should load");

        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].key, "mod+k");
        assert_eq!(bindings[0].command, "search.global");
        assert_eq!(bindings[0].when, None);
        assert_eq!(bindings[1].when.as_deref(), Some("editorFocus"));
    }

    #[tokio::test]
    async fn read_keybindings_returns_parse_error_for_invalid_json() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("create temp home");
        let _home = EnvVarGuard::set("HOME", temp.path().to_string_lossy().as_ref());

        let config_dir = temp.path().join(".ghost");
        fs::create_dir_all(&config_dir).expect("create config dir");
        fs::write(config_dir.join("keybindings.json"), "{not-json").expect("write malformed keybindings");

        let error = read_keybindings()
            .await
            .expect_err("invalid keybindings should fail loudly");

        assert!(error.contains("Failed to parse keybindings"));
        assert!(error.contains("keybindings.json"));
    }

    #[tokio::test]
    async fn read_keybindings_returns_read_error_when_keybindings_path_is_not_a_file() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("create temp home");
        let _home = EnvVarGuard::set("HOME", temp.path().to_string_lossy().as_ref());

        let config_dir = temp.path().join(".ghost");
        fs::create_dir_all(config_dir.join("keybindings.json"))
            .expect("create invalid keybindings directory");

        let error = read_keybindings()
            .await
            .expect_err("directory path should fail loudly");

        assert!(error.contains("Failed to read keybindings"));
        assert!(error.contains("keybindings.json"));
    }

    #[test]
    fn resolve_default_shell_prefers_shell_env_when_present() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _shell = EnvVarGuard::set("SHELL", "/tmp/ghost-shell");

        assert_eq!(resolve_default_shell(), "/tmp/ghost-shell");
    }

    #[test]
    fn resolve_default_shell_uses_platform_fallback_when_shell_is_blank() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _shell = EnvVarGuard::set("SHELL", "   ");

        let expected = if cfg!(target_os = "macos") {
            "/bin/zsh"
        } else {
            "/bin/sh"
        };

        assert_eq!(resolve_default_shell(), expected);
    }

    #[test]
    fn resolve_default_shell_uses_platform_fallback_when_shell_missing() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _shell = EnvVarGuard::remove("SHELL");

        let expected = if cfg!(target_os = "macos") {
            "/bin/zsh"
        } else {
            "/bin/sh"
        };

        assert_eq!(resolve_default_shell(), expected);
    }

    #[test]
    fn desktop_terminal_state_starts_session_ids_at_one() {
        let state = DesktopTerminalState::default();

        assert_eq!(state.next_session_id.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn normalized_terminal_size_clamps_zero_dimensions() {
        let size = normalized_terminal_size(0, 0);

        assert_eq!(size.cols, 1);
        assert_eq!(size.rows, 1);
    }

    #[test]
    fn normalized_terminal_size_preserves_non_zero_dimensions() {
        let size = normalized_terminal_size(132, 48);

        assert_eq!(size.cols, 132);
        assert_eq!(size.rows, 48);
    }

    #[tokio::test]
    async fn desktop_runtime_state_round_trips_token_and_replay_metadata() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("create temp home");
        let _home = EnvVarGuard::set("HOME", temp.path().to_string_lossy().as_ref());

        assert_eq!(get_auth_token().await.unwrap(), None);

        set_auth_token("secret-token".to_string()).await.unwrap();
        assert_eq!(get_auth_token().await.unwrap().as_deref(), Some("secret-token"));

        let replay_state = get_replay_state().await.unwrap();
        assert_eq!(replay_state.session_epoch, 1);
        assert!(!replay_state.client_id.is_empty());

        let next_epoch = advance_replay_session_epoch().await.unwrap();
        assert_eq!(next_epoch, 2);
        let updated = get_replay_state().await.unwrap();
        assert_eq!(updated.session_epoch, 2);
        assert_eq!(updated.client_id, replay_state.client_id);

        clear_auth_token().await.unwrap();
        assert_eq!(get_auth_token().await.unwrap(), None);
    }

    #[tokio::test]
    async fn desktop_runtime_state_recovers_from_empty_file() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("create temp home");
        let _home = EnvVarGuard::set("HOME", temp.path().to_string_lossy().as_ref());

        let config_dir = temp.path().join(".ghost");
        fs::create_dir_all(&config_dir).expect("create config dir");
        fs::write(config_dir.join(DESKTOP_STATE_FILE), "").expect("write empty desktop state");

        let replay_state = get_replay_state().await.expect("empty file should self-heal");
        assert_eq!(replay_state.session_epoch, 1);
    }
}
