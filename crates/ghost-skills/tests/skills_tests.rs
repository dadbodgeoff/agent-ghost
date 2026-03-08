//! Phase 5 tests for ghost-skills (Task 5.8).

// ═══════════════════════════════════════════════════════════════════════
// Task 5.8 — Skill Registry
// ═══════════════════════════════════════════════════════════════════════

mod registry {
    use ghost_skills::registry::{SkillManifest, SkillRegistry, SkillSource, SkillState};
    use std::path::PathBuf;

    fn make_manifest(name: &str, signed: bool) -> SkillManifest {
        SkillManifest {
            name: name.into(),
            version: "1.0.0".into(),
            description: "test skill".into(),
            capabilities: vec!["memory_read".into()],
            timeout_seconds: 30,
            signature: if signed {
                Some("valid_sig".into())
            } else {
                None
            },
        }
    }

    #[test]
    fn configured_verifier_can_admit_preverified_entries() {
        let mut reg = SkillRegistry::with_manifest_verifier(|manifest| {
            manifest.signature.as_deref() == Some("valid_sig")
        });
        reg.register(
            make_manifest("good_skill", true),
            SkillSource::Workspace,
            PathBuf::from("/skills/good.wasm"),
        );
        let skill = reg.lookup("good_skill").unwrap();
        assert_eq!(skill.state, SkillState::Loaded);
    }

    #[test]
    fn default_registry_fails_closed_without_real_verifier() {
        let mut reg = SkillRegistry::new();
        reg.register(
            make_manifest("signed_but_untrusted", true),
            SkillSource::Workspace,
            PathBuf::from("/skills/signed.wasm"),
        );
        let skill = reg.lookup("signed_but_untrusted").unwrap();
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
        reg.register(
            manifest,
            SkillSource::User,
            PathBuf::from("/skills/unsigned.wasm"),
        );
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
        let mut reg = SkillRegistry::with_manifest_verifier(|manifest| {
            manifest.signature.as_deref() == Some("valid_sig")
        });
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
    use wat::parse_str;

    #[test]
    fn execute_runs_a_minimal_pure_wasm_module() {
        let sandbox = WasmSandbox::default();
        let result = sandbox.execute(
            &echo_module(),
            serde_json::json!({"input": "test"}),
            Uuid::now_v7(),
            "test_skill",
        );
        match result {
            ExecutionResult::Success { output, .. } => {
                assert_eq!(output, serde_json::json!({"input": "test"}));
            }
            other => panic!("expected success, got {other:?}"),
        }
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
            EscapeType::HiddenImport,
        ];
        assert_eq!(types.len(), 6);
    }

    #[test]
    fn hidden_import_probe_is_detected_and_classified() {
        let sandbox = WasmSandbox::default();
        let result = sandbox.execute(
            &env_import_module(),
            serde_json::json!({}),
            Uuid::now_v7(),
            "env_probe",
        );

        match result {
            ExecutionResult::EscapeDetected(attempt) => {
                assert_eq!(attempt.escape_type, EscapeType::EnvVarRead);
                assert!(attempt.details.contains("environ_get"));
            }
            other => panic!("expected escape detection, got {other:?}"),
        }
    }

    #[test]
    fn filesystem_network_and_process_imports_fail_closed() {
        let sandbox = WasmSandbox::default();

        let fs = sandbox.execute(
            &filesystem_import_module(),
            serde_json::json!({}),
            Uuid::now_v7(),
            "fs_probe",
        );
        let net = sandbox.execute(
            &network_import_module(),
            serde_json::json!({}),
            Uuid::now_v7(),
            "net_probe",
        );
        let process = sandbox.execute(
            &process_import_module(),
            serde_json::json!({}),
            Uuid::now_v7(),
            "proc_probe",
        );

        assert!(matches!(
            fs,
            ExecutionResult::EscapeDetected(attempt)
                if attempt.escape_type == EscapeType::FilesystemWrite
        ));
        assert!(matches!(
            net,
            ExecutionResult::EscapeDetected(attempt)
                if attempt.escape_type == EscapeType::NetworkAccess
        ));
        assert!(matches!(
            process,
            ExecutionResult::EscapeDetected(attempt)
                if attempt.escape_type == EscapeType::ProcessSpawn
        ));
    }

    #[test]
    fn infinite_loop_times_out() {
        let sandbox = WasmSandbox::new(WasmSandboxConfig {
            timeout: Duration::from_millis(20),
            fuel_limit: u64::MAX / 4,
            ..Default::default()
        });
        let result = sandbox.execute(
            &infinite_loop_module(),
            serde_json::json!({}),
            Uuid::now_v7(),
            "spin",
        );
        assert!(matches!(result, ExecutionResult::Timeout { .. }));
    }

    #[test]
    fn fuel_exhaustion_fails_closed() {
        let sandbox = WasmSandbox::new(WasmSandboxConfig {
            timeout: Duration::from_secs(2),
            fuel_limit: 10_000,
            ..Default::default()
        });
        let result = sandbox.execute(
            &infinite_loop_module(),
            serde_json::json!({}),
            Uuid::now_v7(),
            "fuel_spin",
        );
        assert!(matches!(result, ExecutionResult::FuelExhausted { .. }));
    }

    #[test]
    fn memory_limit_failures_do_not_crash_execution() {
        let sandbox = WasmSandbox::new(WasmSandboxConfig {
            memory_limit_bytes: 64 * 1024 * 1024,
            ..Default::default()
        });
        let result = sandbox.execute(
            &oversized_memory_module(),
            serde_json::json!({}),
            Uuid::now_v7(),
            "memory_blowup",
        );
        assert!(matches!(result, ExecutionResult::MemoryExceeded { .. }));
    }

    fn echo_module() -> Vec<u8> {
        parse_str(
            r#"
            (module
              (memory (export "memory") 2)
              (global $heap (mut i32) (i32.const 1024))
              (func (export "alloc") (param $len i32) (result i32)
                (local $ptr i32)
                global.get $heap
                local.set $ptr
                global.get $heap
                local.get $len
                i32.add
                global.set $heap
                local.get $ptr)
              (func (export "run") (param $input_ptr i32) (param $input_len i32) (result i64)
                local.get $input_ptr
                i64.extend_i32_u
                i64.const 32
                i64.shl
                local.get $input_len
                i64.extend_i32_u
                i64.or))
            "#,
        )
        .unwrap()
    }

    fn infinite_loop_module() -> Vec<u8> {
        parse_str(
            r#"
            (module
              (memory (export "memory") 1)
              (global $heap (mut i32) (i32.const 1024))
              (func (export "alloc") (param $len i32) (result i32)
                global.get $heap)
              (func (export "run") (param i32 i32) (result i64)
                (loop
                  br 0)
                i64.const 0))
            "#,
        )
        .unwrap()
    }

    fn oversized_memory_module() -> Vec<u8> {
        parse_str(
            r#"
            (module
              (memory (export "memory") 2048)
              (func (export "alloc") (param i32) (result i32)
                i32.const 0)
              (func (export "run") (param i32 i32) (result i64)
                i64.const 0))
            "#,
        )
        .unwrap()
    }

    fn env_import_module() -> Vec<u8> {
        parse_str(
            r#"
            (module
              (import "wasi_snapshot_preview1" "environ_get" (func $environ_get (param i32 i32) (result i32)))
              (memory (export "memory") 1)
              (func (export "alloc") (param i32) (result i32)
                i32.const 0)
              (func (export "run") (param i32 i32) (result i64)
                i64.const 0))
            "#,
        )
        .unwrap()
    }

    fn filesystem_import_module() -> Vec<u8> {
        parse_str(
            r#"
            (module
              (import "wasi_snapshot_preview1" "path_open" (func $path_open))
              (memory (export "memory") 1)
              (func (export "alloc") (param i32) (result i32)
                i32.const 0)
              (func (export "run") (param i32 i32) (result i64)
                i64.const 0))
            "#,
        )
        .unwrap()
    }

    fn network_import_module() -> Vec<u8> {
        parse_str(
            r#"
            (module
              (import "wasi_snapshot_preview1" "sock_open" (func $sock_open))
              (memory (export "memory") 1)
              (func (export "alloc") (param i32) (result i32)
                i32.const 0)
              (func (export "run") (param i32 i32) (result i64)
                i64.const 0))
            "#,
        )
        .unwrap()
    }

    fn process_import_module() -> Vec<u8> {
        parse_str(
            r#"
            (module
              (import "wasi_snapshot_preview1" "proc_exit" (func $proc_exit (param i32)))
              (memory (export "memory") 1)
              (func (export "alloc") (param i32) (result i32)
                i32.const 0)
              (func (export "run") (param i32 i32) (result i64)
                i64.const 0))
            "#,
        )
        .unwrap()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.8 — Native Sandbox
// ═══════════════════════════════════════════════════════════════════════

mod native_sandbox {
    use ghost_skills::sandbox::native_sandbox::{
        NativeContainmentMode, NativeContainmentProfile, NativeSandbox,
    };
    use std::collections::HashSet;

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
        assert!(sandbox
            .validate_tool_call("run_command", "shell_execute")
            .is_ok());
        assert!(sandbox
            .validate_tool_call("write_file", "filesystem_write")
            .is_err());
    }

    #[test]
    fn host_interaction_requires_audited_profile() {
        let profile = NativeContainmentProfile::new(
            NativeContainmentMode::HostInteraction,
            false,
            ["host_interaction".to_string()],
        );
        assert!(NativeSandbox::from_profile(&profile).is_err());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.8 — Credential Broker
// ═══════════════════════════════════════════════════════════════════════

mod credential_broker {
    use chrono::Utc;
    use ghost_skills::credential::broker::CredentialBroker;

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
        let handle = broker.register("openai".into(), "api".into(), "sk-key".into(), 10, None);
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
