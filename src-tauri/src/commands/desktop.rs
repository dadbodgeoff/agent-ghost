use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
pub async fn default_shell() -> String {
    resolve_default_shell()
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
}
