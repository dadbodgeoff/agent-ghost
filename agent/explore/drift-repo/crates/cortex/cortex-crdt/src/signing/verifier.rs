//! SignedDeltaVerifier — validates signatures before passing to MergeEngine.
//! This is the safety wrapper that ensures only authenticated deltas are applied.

use ed25519_dalek::{Signature, Verifier};
use cortex_core::errors::{CortexError, CortexResult};

use crate::memory::memory_crdt::MemoryCRDT;
use crate::memory::merge_engine::MergeEngine;
use super::key_registry::KeyRegistry;
use super::signed_delta::SignedDelta;

/// Verifies Ed25519 signatures on deltas before applying them.
/// Does NOT modify MergeEngine — wraps it.
pub struct SignedDeltaVerifier<'a> {
    registry: &'a KeyRegistry,
}

impl<'a> SignedDeltaVerifier<'a> {
    pub fn new(registry: &'a KeyRegistry) -> Self {
        Self { registry }
    }

    /// Verify signature, then delegate to MergeEngine::apply_delta().
    pub fn verify_and_apply(
        &self,
        local: &mut MemoryCRDT,
        signed: &SignedDelta,
    ) -> CortexResult<()> {
        // 1. Look up the agent's public key
        let agent_id = &signed.delta.source_agent;
        let verifying_key = self.registry.get(agent_id).ok_or_else(|| {
            CortexError::MultiAgentError(
                cortex_core::errors::MultiAgentError::PermissionDenied {
                    agent: agent_id.clone(),
                    namespace: String::new(),
                    permission: "no registered key".into(),
                },
            )
        })?;

        // 2. Verify Ed25519 signature over serialized delta
        let payload = signed.signed_bytes();
        let signature = Signature::from_bytes(
            signed.signature.as_slice().try_into().map_err(|_| {
                CortexError::ValidationError("Invalid signature length".into())
            })?,
        );
        verifying_key.verify(&payload, &signature).map_err(|_| {
            CortexError::ValidationError(format!(
                "Signature verification failed for agent {agent_id}"
            ))
        })?;

        // 3. Verify content hash (blake3)
        let computed_hash = blake3::hash(&payload).to_hex().to_string();
        if computed_hash != signed.content_hash {
            return Err(CortexError::ValidationError(
                "Content hash mismatch — delta may have been tampered".into(),
            ));
        }

        // 4. Delegate to stateless MergeEngine
        MergeEngine::apply_delta(local, &signed.delta)
    }
}
