//! Core mesh types: AgentCard, MeshTask, TaskStatus, MeshMessage,
//! DelegationRequest, DelegationResponse, and payment stubs.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::MeshError;

// ── AgentCard ───────────────────────────────────────────────────────────

/// An agent's public identity card, served at `/.well-known/agent.json`.
/// Signed with the agent's Ed25519 key via ghost-signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    /// Bitfield for fast capability matching (Task 22.2).
    /// Bit 0 = code_execution, 1 = web_search, 2 = file_operations,
    /// 3 = api_calls, 4 = data_analysis, 5 = image_generation, 6-63 reserved.
    #[serde(default)]
    pub capability_flags: u64,
    pub input_types: Vec<String>,
    pub output_types: Vec<String>,
    pub auth_schemes: Vec<String>,
    pub endpoint_url: String,
    /// Ed25519 public key bytes (32 bytes, base64-encoded via serde).
    pub public_key: Vec<u8>,
    pub convergence_profile: String,
    pub trust_score: f64,
    pub sybil_lineage_hash: String,
    pub version: String,
    pub signed_at: DateTime<Utc>,
    /// Ed25519 signature over canonical_bytes() (64 bytes).
    pub signature: Vec<u8>,
}

impl AgentCard {
    /// Compute canonical bytes for signing. Deterministic field ordering.
    /// Signature field is excluded (it's what we're computing).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.name.as_bytes());
        buf.extend_from_slice(self.description.as_bytes());
        for cap in &self.capabilities {
            buf.extend_from_slice(cap.as_bytes());
        }
        buf.extend_from_slice(&self.capability_flags.to_le_bytes());
        for it in &self.input_types {
            buf.extend_from_slice(it.as_bytes());
        }
        for ot in &self.output_types {
            buf.extend_from_slice(ot.as_bytes());
        }
        for auth in &self.auth_schemes {
            buf.extend_from_slice(auth.as_bytes());
        }
        buf.extend_from_slice(self.endpoint_url.as_bytes());
        buf.extend_from_slice(&self.public_key);
        buf.extend_from_slice(self.convergence_profile.as_bytes());
        buf.extend_from_slice(&self.trust_score.to_le_bytes());
        buf.extend_from_slice(self.sybil_lineage_hash.as_bytes());
        buf.extend_from_slice(self.version.as_bytes());
        buf.extend_from_slice(self.signed_at.to_rfc3339().as_bytes());
        buf
    }

    /// Sign this card with the given signing key.
    pub fn sign(&mut self, key: &ghost_signing::SigningKey) {
        let canonical = self.canonical_bytes();
        let sig = ghost_signing::sign(&canonical, key);
        self.signature = sig.to_bytes().to_vec();
    }

    /// Verify the card's signature against its public key.
    pub fn verify_signature(&self) -> bool {
        let Some(vk) = self.verifying_key() else {
            tracing::warn!(
                agent = %self.name,
                key_len = self.public_key.len(),
                "signature verification failed: invalid public key"
            );
            return false;
        };
        let Some(sig) = ghost_signing::Signature::from_bytes(&self.signature) else {
            tracing::warn!(
                agent = %self.name,
                sig_len = self.signature.len(),
                "signature verification failed: invalid signature bytes"
            );
            return false;
        };
        let canonical = self.canonical_bytes();
        let valid = ghost_signing::verify(&canonical, &sig, &vk);
        if !valid {
            tracing::warn!(
                agent = %self.name,
                "signature verification failed: signature does not match canonical bytes"
            );
        }
        valid
    }

    /// Extract the verifying key from the public_key bytes.
    fn verifying_key(&self) -> Option<ghost_signing::VerifyingKey> {
        if self.public_key.len() != 32 {
            return None;
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&self.public_key);
        ghost_signing::VerifyingKey::from_bytes(&bytes)
    }

    /// Check if this agent's capabilities match the required bitfield (Task 22.2).
    /// Returns true if ALL required capability bits are set.
    pub fn capabilities_match(&self, required: u64) -> bool {
        (self.capability_flags & required) == required
    }

    /// Convert string capability names to a bitfield (Task 22.2).
    /// Unknown strings map to 0 (no bits set).
    pub fn capabilities_from_strings(caps: &[String]) -> u64 {
        let mut flags: u64 = 0;
        for cap in caps {
            flags |= match cap.as_str() {
                "code_execution" => 1 << 0,
                "web_search" => 1 << 1,
                "file_operations" => 1 << 2,
                "api_calls" => 1 << 3,
                "data_analysis" => 1 << 4,
                "image_generation" => 1 << 5,
                unknown => {
                    tracing::debug!(capability = %unknown, "unknown capability string — no bits set");
                    0
                }
            };
        }
        flags
    }
}

impl PartialEq for AgentCard {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.endpoint_url == other.endpoint_url
            && self.public_key == other.public_key
            && self.version == other.version
    }
}

impl Eq for AgentCard {}

// ── TaskStatus ──────────────────────────────────────────────────────────

/// Task lifecycle states following A2A protocol conventions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Submitted,
    Working,
    InputRequired(String),
    Completed,
    Failed(String),
    Canceled,
}

impl TaskStatus {
    /// Check if a transition from `self` to `to` is valid.
    pub fn can_transition_to(&self, to: &TaskStatus) -> bool {
        matches!(
            (self, to),
            // Forward transitions
            (TaskStatus::Submitted, TaskStatus::Working)
                | (TaskStatus::Working, TaskStatus::Completed)
                | (TaskStatus::Working, TaskStatus::Failed(_))
                | (TaskStatus::Working, TaskStatus::InputRequired(_))
                | (TaskStatus::InputRequired(_), TaskStatus::Working)
                // Any state can be canceled
                | (TaskStatus::Submitted, TaskStatus::Canceled)
                | (TaskStatus::Working, TaskStatus::Canceled)
                | (TaskStatus::InputRequired(_), TaskStatus::Canceled)
        )
    }

    /// Attempt a transition, returning an error if invalid.
    pub fn transition_to(&self, to: TaskStatus) -> Result<TaskStatus, MeshError> {
        if self.can_transition_to(&to) {
            Ok(to)
        } else {
            Err(MeshError::InvalidTransition {
                from: format!("{:?}", self),
                to: format!("{:?}", to),
            })
        }
    }

    /// Returns true if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed | TaskStatus::Failed(_) | TaskStatus::Canceled
        )
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Submitted => write!(f, "submitted"),
            TaskStatus::Working => write!(f, "working"),
            TaskStatus::InputRequired(msg) => write!(f, "input-required: {msg}"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Failed(reason) => write!(f, "failed: {reason}"),
            TaskStatus::Canceled => write!(f, "canceled"),
        }
    }
}

// ── MeshTask ────────────────────────────────────────────────────────────

/// A task delegated between agents via the mesh protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshTask {
    pub id: Uuid,
    pub initiator_agent_id: Uuid,
    pub target_agent_id: Uuid,
    pub status: TaskStatus,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Timeout in seconds. 0 = no timeout.
    pub timeout: u64,
    /// Delegation chain depth (incremented on each hop).
    pub delegation_depth: u32,
    /// Metadata for GHOST-specific extensions.
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl MeshTask {
    /// Create a new task in Submitted state.
    pub fn new(
        initiator_agent_id: Uuid,
        target_agent_id: Uuid,
        input: serde_json::Value,
        timeout: u64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            initiator_agent_id,
            target_agent_id,
            status: TaskStatus::Submitted,
            input,
            output: None,
            created_at: now,
            updated_at: now,
            timeout,
            delegation_depth: 0,
            metadata: BTreeMap::new(),
        }
    }

    /// Transition the task to a new status.
    pub fn transition(&mut self, to: TaskStatus) -> Result<(), MeshError> {
        let new_status = self.status.transition_to(to)?;
        self.status = new_status;
        self.updated_at = Utc::now();
        Ok(())
    }
}

impl PartialEq for MeshTask {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for MeshTask {}

// ── MeshMessage (JSON-RPC 2.0) ──────────────────────────────────────────

/// JSON-RPC 2.0 envelope for mesh protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshMessage {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    /// JSON-RPC 2.0 error object (present in error responses).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    /// JSON-RPC 2.0 result (present in success responses).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl MeshMessage {
    /// Create a JSON-RPC 2.0 request.
    pub fn request(method: &str, params: serde_json::Value, id: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: Some(params),
            id: Some(id),
            error: None,
            result: None,
        }
    }

    /// Create a JSON-RPC 2.0 success response.
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: String::new(),
            params: None,
            id: Some(id),
            error: None,
            result: Some(result),
        }
    }

    /// Create a JSON-RPC 2.0 error response.
    pub fn error_response(id: serde_json::Value, code: i32, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: String::new(),
            params: None,
            id: Some(id),
            error: Some(JsonRpcError {
                code,
                message: message.to_string(),
                data: None,
            }),
            result: None,
        }
    }

    /// Validate that this message conforms to JSON-RPC 2.0 structure.
    pub fn is_valid_jsonrpc(&self) -> bool {
        if self.jsonrpc != "2.0" {
            return false;
        }
        // Request: must have method and id
        if !self.method.is_empty() && self.id.is_some() && self.params.is_some() {
            return true;
        }
        // Response: must have id and either result or error
        if self.id.is_some() && (self.result.is_some() || self.error.is_some()) {
            return true;
        }
        // Notification: method present, no id
        if !self.method.is_empty() && self.id.is_none() {
            return true;
        }
        false
    }
}

// ── DelegationRequest / DelegationResponse ──────────────────────────────

/// A request from one agent to delegate a task to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationRequest {
    pub task_description: String,
    pub required_capabilities: Vec<String>,
    pub max_cost: f64,
    pub timeout: u64,
}

/// Response to a delegation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationResponse {
    pub accepted: bool,
    pub estimated_cost: f64,
    pub estimated_duration: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
}

// ── Payment stubs (kept from Phase 9 placeholder) ───────────────────────

/// Placeholder for mesh payment protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshPayment {
    pub id: Uuid,
    pub from_agent: Uuid,
    pub to_agent: Uuid,
    pub amount: f64,
    pub currency: String,
    pub created_at: DateTime<Utc>,
}

/// Placeholder for mesh invoice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshInvoice {
    pub id: Uuid,
    pub task_id: Uuid,
    pub amount: f64,
    pub currency: String,
    pub issued_at: DateTime<Utc>,
    pub due_at: DateTime<Utc>,
}

/// Placeholder for mesh settlement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshSettlement {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub payment_id: Uuid,
    pub settled_at: DateTime<Utc>,
}

/// Placeholder for mesh wallet balance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshWallet {
    pub agent_id: String,
    pub currency: String,
    pub balance: u64,
}

/// Placeholder for mesh transaction record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshTransaction {
    pub id: Uuid,
    pub from_agent: String,
    pub to_agent: String,
    pub amount: u64,
    pub currency: String,
    pub created_at: DateTime<Utc>,
}

/// Placeholder for mesh escrow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshEscrow {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub depositor: String,
    pub beneficiary: String,
    pub amount: u64,
    pub currency: String,
    pub created_at: DateTime<Utc>,
}

/// Placeholder for mesh receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshReceipt {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub amount: u64,
    pub currency: String,
    pub issued_at: DateTime<Utc>,
}

// ── AgentCardCache (Task 22.2) ──────────────────────────────────────────

/// TTL-based cache for agent cards (Task 22.2).
/// Uses BTreeMap for deterministic ordering (convention: signed payloads).
pub struct AgentCardCache {
    cards: BTreeMap<Uuid, CachedCard>,
    ttl: std::time::Duration,
}

/// A cached agent card with timestamp for TTL expiry.
struct CachedCard {
    card: AgentCard,
    cached_at: std::time::Instant,
    last_signed_at: DateTime<Utc>,
}

impl AgentCardCache {
    /// Create a new cache with the given TTL (default 1 hour).
    pub fn new(ttl: std::time::Duration) -> Self {
        Self {
            cards: BTreeMap::new(),
            ttl,
        }
    }

    /// Get a cached card if present and not expired.
    pub fn get(&self, agent_id: &Uuid) -> Option<&AgentCard> {
        let cached = self.cards.get(agent_id)?;
        if cached.cached_at.elapsed() < self.ttl {
            Some(&cached.card)
        } else {
            None
        }
    }

    /// Store a card in the cache. If the card's `signed_at` matches
    /// the existing cached card's `signed_at`, skip re-verification
    /// (signature-based invalidation).
    pub fn put(&mut self, agent_id: Uuid, card: AgentCard) {
        if let Some(existing) = self.cards.get(&agent_id) {
            if existing.last_signed_at == card.signed_at {
                // Same signed_at → skip re-verification, just refresh cache time
                let refreshed = CachedCard {
                    card,
                    cached_at: std::time::Instant::now(),
                    last_signed_at: existing.last_signed_at,
                };
                self.cards.insert(agent_id, refreshed);
                return;
            }
        }
        let signed_at = card.signed_at;
        self.cards.insert(agent_id, CachedCard {
            card,
            cached_at: std::time::Instant::now(),
            last_signed_at: signed_at,
        });
    }

    /// Remove expired entries.
    pub fn evict_expired(&mut self) {
        self.cards.retain(|_, cached| cached.cached_at.elapsed() < self.ttl);
    }
}

impl Default for AgentCardCache {
    fn default() -> Self {
        Self::new(std::time::Duration::from_secs(3600)) // 1 hour default
    }
}

// ── MeshTaskDelta (Task 22.2) ───────────────────────────────────────────

/// Delta-encoded task update — only changed fields (Task 22.2).
/// `None` means "unchanged" for that field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshTaskDelta {
    pub task_id: Uuid,
    /// Only if status changed.
    pub status: Option<TaskStatus>,
    /// Only if output changed.
    pub output: Option<serde_json::Value>,
    /// Only if updated_at changed.
    pub updated_at: Option<DateTime<Utc>>,
}

impl MeshTask {
    /// Compute a delta between this task and a previous version (Task 22.2).
    /// Only includes fields that have changed.
    pub fn compute_delta(&self, previous: &MeshTask) -> MeshTaskDelta {
        MeshTaskDelta {
            task_id: self.id,
            status: if self.status != previous.status {
                Some(self.status.clone())
            } else {
                None
            },
            output: if self.output != previous.output {
                self.output.clone()
            } else {
                None
            },
            updated_at: if self.updated_at != previous.updated_at {
                Some(self.updated_at)
            } else {
                None
            },
        }
    }

    /// Apply a delta to this task, merging only changed fields (Task 22.2).
    pub fn apply_delta(&mut self, delta: &MeshTaskDelta) {
        if let Some(ref status) = delta.status {
            self.status = status.clone();
        }
        if let Some(ref output) = delta.output {
            self.output = Some(output.clone());
        }
        if let Some(updated_at) = delta.updated_at {
            self.updated_at = updated_at;
        }
    }
}
