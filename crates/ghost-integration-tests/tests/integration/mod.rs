//! End-to-end integration tests for GHOST platform.
//!
//! These tests validate cross-crate wiring and full lifecycle flows
//! as specified in Phase 8 Task 8.1.

// Phase 1-7 integration tests
mod convergence_decay_lifecycle;
mod convergence_pipeline;
mod hash_chain_lifecycle;
mod multiagent_consensus;
mod napi_bindings;
mod observability_metrics;
mod privacy_convergence;
mod retrieval_convergence;
mod signing_lifecycle;
mod simulation_boundary_lifecycle;

// Phase 8 integration tests
mod agent_turn_lifecycle;
mod compaction_lifecycle;
mod convergence_full_pipeline;
mod gateway_shutdown;
mod gateway_state_machine;
mod safety_critical_edge_cases;
mod distributed_kill_gates;
mod inter_agent_messaging;
mod kill_switch_chain;
mod multi_agent_scenarios;
mod proposal_lifecycle;
mod orchestrator_fix_verification;

// Phase 15 e2e integration tests (Task 22.3)
mod secrets_e2e;
mod egress_e2e;
mod oauth_e2e;
mod mesh_e2e;
