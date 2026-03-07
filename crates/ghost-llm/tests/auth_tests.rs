//! Tests for AuthProfileManager (Task 10.4).
//!
//! Covers: backward compat with EnvProvider, mock SecretProvider,
//! credential rotation, graceful degradation, and tracing safety.

use ghost_llm::auth::AuthProfileManager;
use ghost_llm::provider::LLMError;
use ghost_secrets::{ExposeSecret, SecretProvider, SecretString, SecretsError};
use std::collections::HashMap;
use std::sync::Mutex;

// ─── Mock SecretProvider ─────────────────────────────────────────────────

struct MockSecretProvider {
    secrets: Mutex<HashMap<String, String>>,
}

impl MockSecretProvider {
    fn new(entries: Vec<(&str, &str)>) -> Self {
        let map: HashMap<String, String> = entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Self {
            secrets: Mutex::new(map),
        }
    }
}

impl SecretProvider for MockSecretProvider {
    fn get_secret(&self, key: &str) -> Result<SecretString, SecretsError> {
        let secrets = self.secrets.lock().unwrap();
        match secrets.get(key) {
            Some(val) => Ok(SecretString::from(val.clone())),
            None => Err(SecretsError::NotFound(key.to_string())),
        }
    }

    fn set_secret(&self, key: &str, value: &str) -> Result<(), SecretsError> {
        let mut secrets = self.secrets.lock().unwrap();
        secrets.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete_secret(&self, key: &str) -> Result<(), SecretsError> {
        let mut secrets = self.secrets.lock().unwrap();
        secrets.remove(key);
        Ok(())
    }

    fn has_secret(&self, key: &str) -> bool {
        let secrets = self.secrets.lock().unwrap();
        secrets.contains_key(key)
    }
}

/// A provider that always returns errors.
struct FailingSecretProvider;

impl SecretProvider for FailingSecretProvider {
    fn get_secret(&self, _key: &str) -> Result<SecretString, SecretsError> {
        Err(SecretsError::ProviderError("backend unavailable".into()))
    }
    fn set_secret(&self, _key: &str, _value: &str) -> Result<(), SecretsError> {
        Err(SecretsError::StorageUnavailable(
            "backend unavailable".into(),
        ))
    }
    fn delete_secret(&self, _key: &str) -> Result<(), SecretsError> {
        Err(SecretsError::StorageUnavailable(
            "backend unavailable".into(),
        ))
    }
    fn has_secret(&self, _key: &str) -> bool {
        false
    }
}

// ─── Backward compatibility: EnvProvider ─────────────────────────────────

#[test]
fn auth_profile_manager_with_env_reads_env_vars() {
    let key = "ANTHROPIC_API_KEY";
    std::env::set_var(key, "sk-ant-test-key-123");

    let manager = AuthProfileManager::with_env("anthropic");
    let cred = manager.get_credential().expect("should find env var");
    assert_eq!(cred.expose_secret(), "sk-ant-test-key-123");

    std::env::remove_var(key);
}

// ─── Mock SecretProvider: retrieves correct keys ─────────────────────────

#[test]
fn auth_profile_manager_with_mock_retrieves_correct_key() {
    let mock = MockSecretProvider::new(vec![
        ("anthropic-api-key", "sk-mock-primary"),
        ("anthropic-api-key-2", "sk-mock-secondary"),
        ("anthropic-api-key-3", "sk-mock-tertiary"),
    ]);

    let manager = AuthProfileManager::new(Box::new(mock), "anthropic", 3);
    let cred = manager.get_credential().unwrap();
    assert_eq!(cred.expose_secret(), "sk-mock-primary");
}

// ─── Credential rotation on 401 ─────────────────────────────────────────

#[test]
fn auth_profile_manager_rotation_cycles_through_profiles() {
    let mock = MockSecretProvider::new(vec![
        ("anthropic-api-key", "sk-primary"),
        ("anthropic-api-key-2", "sk-secondary"),
        ("anthropic-api-key-3", "sk-tertiary"),
    ]);

    let mut manager = AuthProfileManager::new(Box::new(mock), "anthropic", 3);

    // Profile 0
    let cred = manager.get_credential().unwrap();
    assert_eq!(cred.expose_secret(), "sk-primary");

    // Rotate to profile 1
    assert!(manager.rotate());
    let cred = manager.get_credential().unwrap();
    assert_eq!(cred.expose_secret(), "sk-secondary");

    // Rotate to profile 2
    assert!(manager.rotate());
    let cred = manager.get_credential().unwrap();
    assert_eq!(cred.expose_secret(), "sk-tertiary");

    // No more profiles
    assert!(!manager.rotate());
}

#[test]
fn auth_profile_manager_reset_returns_to_first_profile() {
    let mock = MockSecretProvider::new(vec![
        ("anthropic-api-key", "sk-primary"),
        ("anthropic-api-key-2", "sk-secondary"),
    ]);

    let mut manager = AuthProfileManager::new(Box::new(mock), "anthropic", 2);
    manager.rotate();
    assert_eq!(manager.current_index(), 1);

    manager.reset();
    assert_eq!(manager.current_index(), 0);
    let cred = manager.get_credential().unwrap();
    assert_eq!(cred.expose_secret(), "sk-primary");
}

// ─── Legacy env var fallback ─────────────────────────────────────────────

#[test]
fn auth_profile_manager_falls_back_to_legacy_env_key() {
    // The new-style key doesn't exist, but the legacy UPPER_CASE one does
    let key = "OPENAI_API_KEY";
    std::env::set_var(key, "sk-openai-legacy");

    let manager = AuthProfileManager::with_env("openai");
    let cred = manager.get_credential().unwrap();
    assert_eq!(cred.expose_secret(), "sk-openai-legacy");

    std::env::remove_var(key);
}

// ─── Adversarial: SecretProvider returns error ───────────────────────────

#[test]
fn auth_profile_manager_provider_error_returns_auth_failed() {
    let manager = AuthProfileManager::new(Box::new(FailingSecretProvider), "anthropic", 3);
    let result = manager.get_credential();
    assert!(matches!(result, Err(LLMError::AuthFailed(_))));
}

// ─── Full FallbackChain with mock provider ───────────────────────────────

#[test]
fn auth_profile_manager_full_rotation_through_3_profiles() {
    let mock = MockSecretProvider::new(vec![
        ("gemini-api-key", "key-1"),
        ("gemini-api-key-2", "key-2"),
        ("gemini-api-key-3", "key-3"),
    ]);

    let mut manager = AuthProfileManager::new(Box::new(mock), "gemini", 3);

    let mut collected = Vec::new();
    loop {
        let cred = manager.get_credential().unwrap();
        collected.push(cred.expose_secret().to_string());
        if !manager.rotate() {
            break;
        }
    }

    assert_eq!(collected, vec!["key-1", "key-2", "key-3"]);
}

// ─── Missing credential returns AuthFailed ───────────────────────────────

#[test]
fn auth_profile_manager_missing_credential_returns_auth_failed() {
    let mock = MockSecretProvider::new(vec![]); // empty
    let manager = AuthProfileManager::new(Box::new(mock), "anthropic", 3);
    let result = manager.get_credential();
    assert!(matches!(result, Err(LLMError::AuthFailed(_))));
}

// ─── Tracing safety: SecretString never appears in logs ──────────────────

#[test]
fn auth_profile_manager_secret_not_in_tracing_output() {
    use std::sync::{Arc, Mutex};

    // Capture tracing output
    let captured = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_clone = Arc::clone(&captured);

    // Simple layer that captures formatted events
    struct CapturingLayer {
        captured: Arc<Mutex<Vec<String>>>,
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CapturingLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut visitor = StringVisitor(String::new());
            event.record(&mut visitor);
            self.captured.lock().unwrap().push(visitor.0);
        }
    }

    struct StringVisitor(String);
    impl tracing::field::Visit for StringVisitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            use std::fmt::Write;
            let _ = write!(self.0, "{}={:?} ", field.name(), value);
        }
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            use std::fmt::Write;
            let _ = write!(self.0, "{}={} ", field.name(), value);
        }
    }

    use tracing_subscriber::layer::SubscriberExt;
    let subscriber = tracing_subscriber::registry().with(CapturingLayer {
        captured: captured_clone,
    });

    let secret_value = "sk-super-secret-key-12345";
    let mock = MockSecretProvider::new(vec![("test-api-key", secret_value)]);

    tracing::subscriber::with_default(subscriber, || {
        let manager = AuthProfileManager::new(Box::new(mock), "test", 3);
        let _cred = manager.get_credential().unwrap();
    });

    let logs = captured.lock().unwrap();
    for log_line in logs.iter() {
        assert!(
            !log_line.contains(secret_value),
            "SECRET LEAKED IN TRACING OUTPUT: {log_line}"
        );
    }
}
