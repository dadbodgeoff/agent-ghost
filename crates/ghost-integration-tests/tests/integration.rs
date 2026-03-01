//! End-to-end integration tests for GHOST platform.
//!
//! These tests validate cross-crate wiring and full lifecycle flows
//! as specified in Phase 8 Task 8.1.

// Phase 1-7 integration tests
#[path = "integration/convergence_decay_lifecycle.rs"]
mod convergence_decay_lifecycle;
#[path = "integration/convergence_pipeline.rs"]
mod convergence_pipeline;
#[path = "integration/hash_chain_lifecycle.rs"]
mod hash_chain_lifecycle;
#[path = "integration/multiagent_consensus.rs"]
mod multiagent_consensus;
#[path = "integration/napi_bindings.rs"]
mod napi_bindings;
#[path = "integration/observability_metrics.rs"]
mod observability_metrics;
#[path = "integration/privacy_convergence.rs"]
mod privacy_convergence;
#[path = "integration/retrieval_convergence.rs"]
mod retrieval_convergence;
#[path = "integration/signing_lifecycle.rs"]
mod signing_lifecycle;
#[path = "integration/simulation_boundary_lifecycle.rs"]
mod simulation_boundary_lifecycle;

// Phase 8 integration tests
#[path = "integration/agent_turn_lifecycle.rs"]
mod agent_turn_lifecycle;
#[path = "integration/convergence_full_pipeline.rs"]
mod convergence_full_pipeline;
#[path = "integration/gateway_state_machine.rs"]
mod gateway_state_machine;
#[path = "integration/inter_agent_messaging.rs"]
mod inter_agent_messaging;
#[path = "integration/kill_switch_chain.rs"]
mod kill_switch_chain;
#[path = "integration/multi_agent_scenarios.rs"]
mod multi_agent_scenarios;
#[path = "integration/proposal_lifecycle.rs"]
mod proposal_lifecycle;
#[path = "integration/compaction_lifecycle.rs"]
mod compaction_lifecycle;
#[path = "integration/gateway_shutdown.rs"]
mod gateway_shutdown;
#[path = "integration/safety_critical_edge_cases.rs"]
mod safety_critical_edge_cases;
#[path = "integration/distributed_kill_gates.rs"]
mod distributed_kill_gates;
#[path = "integration/orchestrator_fix_verification.rs"]
mod orchestrator_fix_verification;

// Phase 15 e2e integration tests (Task 22.3)
#[path = "integration/secrets_e2e.rs"]
mod secrets_e2e;
#[path = "integration/egress_e2e.rs"]
mod egress_e2e;
#[path = "integration/oauth_e2e.rs"]
mod oauth_e2e;
#[path = "integration/mesh_e2e.rs"]
mod mesh_e2e;
