//! IMultiAgentEngine — the multi-agent memory orchestration trait.
//!
//! Defines the complete interface for agent registration, namespace management,
//! memory sharing, projections, provenance tracking, trust scoring, and sync.

use crate::errors::CortexResult;
use crate::models::agent::{AgentId, AgentRegistration, AgentStatus};
use crate::models::cross_agent::AgentTrust;
use crate::models::namespace::{
    MemoryProjection, NamespaceId, NamespacePermission,
};
use crate::models::provenance::ProvenanceRecord;

/// Multi-agent memory engine trait.
///
/// Provides the full interface for multi-agent operations. Implementations
/// coordinate agent lifecycle, namespace isolation, memory sharing, provenance
/// tracking, trust scoring, and delta synchronization.
///
/// Phase A defines the trait. Phase B provides the first implementation.
#[allow(async_fn_in_trait)]
pub trait IMultiAgentEngine: Send + Sync {
    /// Register a new agent with the given name and capabilities.
    async fn register_agent(
        &self,
        name: &str,
        capabilities: Vec<String>,
    ) -> CortexResult<AgentRegistration>;

    /// Deregister an agent, archiving its namespace and preserving provenance.
    async fn deregister_agent(&self, agent_id: &AgentId) -> CortexResult<()>;

    /// Look up an agent by ID.
    async fn get_agent(
        &self,
        agent_id: &AgentId,
    ) -> CortexResult<Option<AgentRegistration>>;

    /// List agents, optionally filtered by status.
    async fn list_agents(
        &self,
        filter: Option<AgentStatus>,
    ) -> CortexResult<Vec<AgentRegistration>>;

    /// Create a new namespace with the given scope.
    async fn create_namespace(
        &self,
        namespace: NamespaceId,
        owner: &AgentId,
    ) -> CortexResult<NamespaceId>;

    /// Check whether an agent has a specific permission on a namespace.
    async fn check_permission(
        &self,
        namespace: &NamespaceId,
        agent_id: &AgentId,
        permission: NamespacePermission,
    ) -> CortexResult<bool>;

    /// Share a memory from one namespace to another (one-time copy with provenance).
    async fn share_memory(
        &self,
        memory_id: &str,
        target_namespace: &NamespaceId,
        agent_id: &AgentId,
    ) -> CortexResult<()>;

    /// Create a projection from one namespace to another.
    async fn create_projection(
        &self,
        projection: MemoryProjection,
    ) -> CortexResult<String>;

    /// Synchronize memory state between two agents via delta sync.
    async fn sync_with(
        &self,
        source_agent: &AgentId,
        target_agent: &AgentId,
    ) -> CortexResult<()>;

    /// Get the full provenance record for a memory.
    async fn get_provenance(
        &self,
        memory_id: &str,
    ) -> CortexResult<Option<ProvenanceRecord>>;

    /// Get the trust relationship from one agent toward another.
    async fn get_trust(
        &self,
        agent_id: &AgentId,
        target_agent: &AgentId,
    ) -> CortexResult<AgentTrust>;

    /// Detect consensus across agents for memories in a namespace.
    async fn detect_consensus(
        &self,
        namespace: &NamespaceId,
    ) -> CortexResult<Vec<(Vec<String>, f64)>>;
}
