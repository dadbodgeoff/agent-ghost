//! Tests for ghost-identity (Task 4.2).

use ghost_identity::corp_policy::{CorpPolicyError, CorpPolicyLoader};
use ghost_identity::drift_detector::{DriftStatus, IdentityDriftDetector};
use ghost_identity::identity_manager::IdentityManager;
use ghost_identity::keypair_manager::AgentKeypairManager;
use ghost_identity::soul_manager::{SoulError, SoulManager};
use ghost_identity::user::UserManager;
use std::path::PathBuf;
use uuid::Uuid;

// ── SoulManager tests ───────────────────────────────────────────────────

#[test]
fn soul_loads_valid_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("SOUL.md");
    std::fs::write(&path, "# Soul\nI am a helpful assistant.").unwrap();

    let mut mgr = SoulManager::new();
    let doc = mgr.load(&path).unwrap();
    assert!(doc.content.contains("helpful assistant"));
}

#[test]
fn soul_rejects_missing_file() {
    let mut mgr = SoulManager::new();
    let result = mgr.load(&PathBuf::from("/nonexistent/SOUL.md"));
    assert!(matches!(result, Err(SoulError::NotFound { .. })));
}

#[test]
fn soul_rejects_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("SOUL.md");
    std::fs::write(&path, "").unwrap();

    let mut mgr = SoulManager::new();
    let result = mgr.load(&path);
    assert!(matches!(result, Err(SoulError::Empty)));
}

// ── CorpPolicyLoader tests ──────────────────────────────────────────────

#[test]
fn corp_policy_rejects_missing_file() {
    let (_, vk) = ghost_signing::generate_keypair();
    let result = CorpPolicyLoader::load(&PathBuf::from("/nonexistent/CORP_POLICY.md"), &vk);
    assert!(matches!(result, Err(CorpPolicyError::NotFound { .. })));
}

#[test]
fn corp_policy_rejects_missing_signature() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("CORP_POLICY.md");
    std::fs::write(&path, "# Policy\nNo agents shall harm humans.").unwrap();

    let (_, vk) = ghost_signing::generate_keypair();
    let result = CorpPolicyLoader::load(&path, &vk);
    assert!(matches!(result, Err(CorpPolicyError::SignatureMissing)));
}

#[test]
fn corp_policy_accepts_valid_signature() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("CORP_POLICY.md");

    let (sk, vk) = ghost_signing::generate_keypair();
    let content = "# Policy\nNo agents shall harm humans.";
    let sig = ghost_signing::sign(content.as_bytes(), &sk);
    let sig_hex: String = sig.to_bytes().iter().map(|b| format!("{b:02x}")).collect();

    let full = format!("{content}\n<!-- SIGNATURE: {sig_hex} -->");
    std::fs::write(&path, &full).unwrap();

    let doc = CorpPolicyLoader::load(&path, &vk).unwrap();
    assert!(doc.signature_verified);
    assert!(doc.content.contains("No agents shall harm"));
}

#[test]
fn corp_policy_rejects_invalid_signature() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("CORP_POLICY.md");

    let (sk, _) = ghost_signing::generate_keypair();
    let (_, wrong_vk) = ghost_signing::generate_keypair();

    let content = "# Policy\nNo agents shall harm humans.";
    let sig = ghost_signing::sign(content.as_bytes(), &sk);
    let sig_hex: String = sig.to_bytes().iter().map(|b| format!("{b:02x}")).collect();

    let full = format!("{content}\n<!-- SIGNATURE: {sig_hex} -->");
    std::fs::write(&path, &full).unwrap();

    let result = CorpPolicyLoader::load(&path, &wrong_vk);
    assert!(matches!(result, Err(CorpPolicyError::SignatureInvalid)));
}

// ── AgentKeypairManager tests ───────────────────────────────────────────

#[test]
fn keypair_generate_and_verify() {
    let dir = tempfile::tempdir().unwrap();
    let mut mgr = AgentKeypairManager::new(dir.path().to_path_buf());

    let vk = mgr.generate().unwrap();
    assert_eq!(vk.to_bytes().len(), 32);

    // Sign and verify
    let sk = mgr.signing_key().unwrap();
    let data = b"test message";
    let sig = ghost_signing::sign(data, sk);
    assert!(mgr.verify(data, &sig));
}

#[test]
fn keypair_load_verifying_key() {
    let dir = tempfile::tempdir().unwrap();
    let mut mgr = AgentKeypairManager::new(dir.path().to_path_buf());
    let vk1 = mgr.generate().unwrap().to_bytes();

    // Load from disk
    let vk2 = mgr.load_verifying_key().unwrap();
    assert_eq!(vk1, vk2.to_bytes());
}

#[test]
fn keypair_rotation_grace_period() {
    let dir = tempfile::tempdir().unwrap();
    let mut mgr = AgentKeypairManager::new(dir.path().to_path_buf());

    mgr.generate().unwrap();
    let old_sk = mgr.signing_key().unwrap();
    let data = b"test";
    let old_sig = ghost_signing::sign(data, old_sk);

    // Rotate
    mgr.rotate().unwrap();

    // Old signature still valid during grace period
    assert!(mgr.verify(data, &old_sig));

    // New key also works
    let new_sk = mgr.signing_key().unwrap();
    let new_sig = ghost_signing::sign(data, new_sk);
    assert!(mgr.verify(data, &new_sig));
}

#[test]
fn keypair_missing_dir_auto_creates() {
    let dir = tempfile::tempdir().unwrap();
    let keys_dir = dir.path().join("agents/test/keys");
    let mut mgr = AgentKeypairManager::new(keys_dir.clone());

    // Should auto-create directory
    mgr.generate().unwrap();
    assert!(keys_dir.exists());
}

// ── IdentityDriftDetector tests ─────────────────────────────────────────

#[test]
fn drift_identical_embeddings_normal() {
    let detector = IdentityDriftDetector::default();
    let baseline = vec![1.0, 0.0, 0.0];
    let current = vec![1.0, 0.0, 0.0];
    let drift = detector.compute_drift(&baseline, &current);
    assert!(drift < 0.01);
    assert_eq!(detector.evaluate(drift), DriftStatus::Normal);
}

#[test]
fn drift_score_016_is_alert() {
    let detector = IdentityDriftDetector::default();
    assert_eq!(detector.evaluate(0.16), DriftStatus::Alert);
}

#[test]
fn drift_score_026_is_kill() {
    let detector = IdentityDriftDetector::default();
    assert_eq!(detector.evaluate(0.26), DriftStatus::Kill);
}

#[test]
fn drift_score_025_exactly_is_kill() {
    let detector = IdentityDriftDetector::default();
    assert_eq!(detector.evaluate(0.25), DriftStatus::Kill);
}

#[test]
fn drift_builds_trigger_on_kill() {
    let detector = IdentityDriftDetector::default();
    let trigger = detector.build_trigger(
        Uuid::now_v7(),
        0.30,
        "baseline_hash".into(),
        "current_hash".into(),
    );
    assert!(trigger.is_some());
}

#[test]
fn drift_no_trigger_on_normal() {
    let detector = IdentityDriftDetector::default();
    let trigger = detector.build_trigger(
        Uuid::now_v7(),
        0.10,
        "baseline_hash".into(),
        "current_hash".into(),
    );
    assert!(trigger.is_none());
}

#[test]
fn drift_embedding_model_change_invalidates_baseline() {
    let detector = IdentityDriftDetector::default();
    // Different dimension embeddings → drift 0.0 (graceful handling)
    let baseline = vec![1.0, 0.0];
    let current = vec![1.0, 0.0, 0.0];
    let drift = detector.compute_drift(&baseline, &current);
    assert_eq!(drift, 0.0); // Mismatched dimensions → 0.0
}

// ── IdentityManager tests ───────────────────────────────────────────────

#[test]
fn identity_loads_valid_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("IDENTITY.md");
    std::fs::write(
        &path,
        "---\nname: Ghost\nvoice: friendly\n---\n# Identity\nI am Ghost.",
    )
    .unwrap();

    let mut mgr = IdentityManager::new();
    let identity = mgr.load(&path).unwrap();
    assert_eq!(identity.name, "Ghost");
    assert_eq!(identity.voice, "friendly");
}

// ── UserManager tests ───────────────────────────────────────────────────

#[test]
fn user_loads_valid_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("USER.md");
    std::fs::write(&path, "# User\nPrefers concise responses.").unwrap();

    let mut mgr = UserManager::new();
    let doc = mgr.load(&path).unwrap();
    assert!(doc.content.contains("concise"));
}
