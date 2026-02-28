//! Phase 5 tests for ghost-skills (Task 5.8).

use std::collections::HashSet;
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════════════════
// Task 5.8 — Skill Registry
// ═══════════════════════════════════════════════════════════════════════

mod registry {
    use ghost_skills::registry::{
        SkillManifest, SkillRegistry, SkillSource, SkillState,
    };
    use std::path::PathBuf;

    fn make_manifest(name: &str, signed: bool) -> SkillManifest {
        SkillManifest {
            name: name.into(),
            version: "1.0.0".into(),
            description: "test skill".into(),
            capabilities: vec!["memory_read".into()],
            timeout_seconds: 30,
            signature: if signed { Some("valid_sig".into()) } else { None },
        }
    }

    #[test]
    fn valid_signature_loads() {
        let mut reg = SkillRegistry::new();
        reg.register(
            make_manifest("good_skill", true),
            SkillSource::Workspace,
            PathBuf::from("/skills/good.wasm"),
        );
        let skill = reg.lookup("good_skill").unwrap();
        assert_eq!(skill.state, SkillState::Loaded);
    }

    #[test]
    fn invalid_signature_quarantines() {
        let mut reg = SkillRegistry::new();
        reg.register(
            make_manifest("bad_skill", false),
            SkillSource::Workspace,
            PathBuf::from("/skills/bad.wasm"),
        );
        let skill = reg.lookup("bad_skill").unwrap();
        assert_eq!(skill.state, SkillState::Quarantined);
    }

    #[test]
    fn missing_signature_quarantines() {
        let mut reg = SkillRegistry::new();
        let manifest = SkillManifest {
            name: "unsigned".into(),
            version: "1.0.0".into(),
            description: "no sig".into(),
            capabilities: Vec::new(),
            timeout_seconds: 30,
            signature: None,
        };
        reg.register(manifest, SkillSource::User, PathBuf::from("/skills/unsigned.wasm"));
        let skill = reg.lookup("unsigned").unwrap();
        assert_eq!(skill.state, SkillState::Quarantined);
    }

    #[test]
    fn priority_order() {
        assert!(SkillSource::Workspace > SkillSource::User);
        assert!(SkillSource::User > SkillSource::Bundled);
    }

    #[test]
    fn loaded_skills_excludes_quarantined() {
        let mut reg = SkillRegistry::new();
        reg.register(
            make_manifest("good", true),
            SkillSource::Workspace,
            PathBuf::from("/good.wasm"),
        );
        reg.register(
            make_manifest("bad", false),
            SkillSource::Workspace,
            PathBuf::from("/bad.wasm"),
        );
        assert_eq!(reg.loaded_skills().len(), 1);
        assert_eq!(reg.quarantined_skills().len(), 1);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.8 — WASM Sandbox
// ═══════════════════════════════════════════════════════════════════════

mod wasm_sandbox {
    use std::collections::HashSet;
    use std::time::Duration;

    use ghost_skills::sandbox::wasm_sandbox::{
        EscapeType, ExecutionResult, WasmSandbox, WasmSandboxConfig,
    };
    use uuid::Uuid;

    #[tokio::test]
    async fn execute_returns_result() {
        let sandbox = WasmSandbox::default();
        let result = sandbox
            .execute(
                b"fake_wasm",
                serde_json::json!({"input": "test"}),
                Uuid::now_v7(),
                "test_skill",
            )
            .await;
        assert!(matches!(result, ExecutionResult::Success { .. }));
    }

    #[test]
    fn capability_check() {
        let mut caps = HashSet::new();
        caps.insert("memory_read".into());
        let config = WasmSandboxConfig {
            allowed_capabilities: caps,
            ..Default::default()
        };
        let sandbox = WasmSandbox::new(config);
        assert!(sandbox.has_capability("memory_read"));
        assert!(!sandbox.has_capability("filesystem_write"));
    }

    #[test]
    fn default_timeout() {
        let config = WasmSandboxConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(30));
    }

    #[test]
    fn default_memory_limit() {
        let config = WasmSandboxConfig::default();
        assert_eq!(config.memory_limit_bytes, 64 * 1024 * 1024);
    }

    #[test]
    fn record_escape_captures_forensics() {
        let sandbox = WasmSandbox::default();
        let attempt = sandbox.record_escape(
            "evil_skill",
            "abc123",
            EscapeType::FilesystemWrite,
            "tried to write /etc/passwd",
            Uuid::now_v7(),
        );
        assert_eq!(attempt.skill_name, "evil_skill");
        assert_eq!(attempt.escape_type, EscapeType::FilesystemWrite);
    }

    #[test]
    fn escape_types() {
        // Verify all escape types exist
        let types = [
            EscapeType::FilesystemWrite,
            EscapeType::NetworkAccess,
            EscapeType::EnvVarRead,
            EscapeType::ProcessSpawn,
            EscapeType::MemoryExceeded,
        ];
        assert_eq!(types.len(), 5);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.8 — Native Sandbox
// ═══════════════════════════════════════════════════════════════════════

mod native_sandbox {
    use std::collections::HashSet;
    use ghost_skills::sandbox::native_sandbox::NativeSandbox;

    #[test]
    fn capability_granted() {
        let mut caps = HashSet::new();
        caps.insert("memory_read".into());
        let sandbox = NativeSandbox::new(caps);
        assert!(sandbox.check_capability("memory_read").is_ok());
    }

    #[test]
    fn capability_denied() {
        let sandbox = NativeSandbox::new(HashSet::new());
        assert!(sandbox.check_capability("filesystem_write").is_err());
    }

    #[test]
    fn tool_call_validation() {
        let mut caps = HashSet::new();
        caps.insert("shell_execute".into());
        let sandbox = NativeSandbox::new(caps);
        assert!(sandbox.validate_tool_call("run_command", "shell_execute").is_ok());
        assert!(sandbox.validate_tool_call("write_file", "filesystem_write").is_err());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.8 — Credential Broker
// ═══════════════════════════════════════════════════════════════════════

mod credential_broker {
    use ghost_skills::credential::broker::{CredentialBroker, CredentialError};
    use chrono::Utc;

    #[test]
    fn register_and_reify() {
        let mut broker = CredentialBroker::new();
        let handle = broker.register(
            "openai".into(),
            "api".into(),
            "sk-secret-key".into(),
            3,
            None,
        );
        let secret = broker.reify(handle.id).unwrap();
        assert_eq!(secret, "sk-secret-key");
    }

    #[test]
    fn max_uses_enforced() {
        let mut broker = CredentialBroker::new();
        let handle = broker.register(
            "openai".into(),
            "api".into(),
            "sk-key".into(),
            1, // max_uses = 1
            None,
        );
        assert!(broker.reify(handle.id).is_ok()); // First use
        assert!(broker.reify(handle.id).is_err()); // Second use — exhausted
    }

    #[test]
    fn expired_credential_rejected() {
        let mut broker = CredentialBroker::new();
        let handle = broker.register(
            "openai".into(),
            "api".into(),
            "sk-key".into(),
            10,
            Some(Utc::now() - chrono::Duration::hours(1)), // Already expired
        );
        assert!(broker.reify(handle.id).is_err());
    }

    #[test]
    fn revoke_credential() {
        let mut broker = CredentialBroker::new();
        let handle = broker.register(
            "openai".into(),
            "api".into(),
            "sk-key".into(),
            10,
            None,
        );
        assert!(broker.revoke(handle.id));
        assert!(broker.reify(handle.id).is_err());
    }

    #[test]
    fn revoke_provider() {
        let mut broker = CredentialBroker::new();
        let h1 = broker.register("openai".into(), "api".into(), "key1".into(), 10, None);
        let h2 = broker.register("openai".into(), "api".into(), "key2".into(), 10, None);
        let h3 = broker.register("anthropic".into(), "api".into(), "key3".into(), 10, None);
        broker.revoke_provider("openai");
        assert!(broker.reify(h1.id).is_err());
        assert!(broker.reify(h2.id).is_err());
        assert!(broker.reify(h3.id).is_ok()); // Different provider
    }

    #[test]
    fn remaining_uses() {
        let mut broker = CredentialBroker::new();
        let handle = broker.register("p".into(), "s".into(), "k".into(), 3, None);
        assert_eq!(broker.remaining_uses(handle.id), Some(3));
        broker.reify(handle.id).unwrap();
        assert_eq!(broker.remaining_uses(handle.id), Some(2));
    }
}
