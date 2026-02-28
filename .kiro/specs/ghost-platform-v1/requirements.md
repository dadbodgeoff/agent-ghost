# Requirements Document

## Introduction

The GHOST Platform v1 (General Hybrid Orchestrated Self-healing Taskrunner) is a Rust-based agent platform comprising ~217 files across ~17 new crates and ~15 modified crates. The platform provides convergence monitoring, proposal lifecycle management, inter-agent messaging, session compaction, gateway bootstrap with degraded mode, a kill switch safety system, and a recursive agent loop runtime. The workspace uses Rust 2021 edition with resolver="2", blake3 hashing, thiserror for errors, tracing for structured logging, proptest for invariant testing, and ed25519-dalek for cryptographic signing.

## Glossary

- **Gateway**: The single long-running `ghost-gateway` process that owns all subsystems, agent lifecycle, routing, sessions, and the API server.
- **Convergence_Monitor**: An independent sidecar binary (`convergence-monitor`) that ingests ITP events, computes 7 behavioral signals, scores convergence risk, and triggers interventions across 5 levels (0-4).
- **Agent_Loop**: The recursive agentic runtime (`ghost-agent-loop`) that compiles 10-layer prompts, calls LLMs, executes tools, extracts proposals, and emits ITP telemetry.
- **ITP**: Interaction Telemetry Protocol — the event schema (SessionStart, SessionEnd, InteractionMessage, AgentStateSnapshot, ConvergenceAlert) emitted by the agent loop and ingested by the monitor.
- **Proposal**: A structured state change request (goal change, reflection write, memory write) extracted from agent output and validated through 7 dimensions (D1-D7) before storage.
- **Kill_Switch**: A 3-level hard safety system (PAUSE, QUARANTINE, KILL_ALL) with 7 auto-triggers, owned by the gateway.
- **Session_Compactor**: The subsystem that flushes agent working memory to Cortex storage at 70% context window capacity via a synthetic LLM turn.
- **Policy_Engine**: A Cedar-style authorization engine (`ghost-policy`) that evaluates every tool call against CORP_POLICY.md, capability grants, and convergence-level restrictions.
- **Simulation_Boundary**: The subsystem (`simulation-boundary`) that detects and reframes emulation language in agent output, enforcing simulation-mode framing.
- **Ghost_Signing**: A leaf crate providing Ed25519 keypair generation, signing, and verification primitives shared by identity, skills, CRDT, and inter-agent messaging.
- **Read_Only_Pipeline**: The subsystem (`read-only-pipeline`) that assembles an immutable convergence-filtered state snapshot for the agent each turn.
- **Lane_Queue**: A per-session serialized request queue in the gateway that prevents concurrent operations on the same session.
- **Composite_Score**: A weighted sum of 7 normalized behavioral signals mapped to intervention levels [0.0-0.3)=L0, [0.3-0.5)=L1, [0.5-0.7)=L2, [0.7-0.85)=L3, [0.85-1.0]=L4.
- **Hash_Chain**: A blake3-based append-only integrity chain where each event's hash includes the previous event's hash, starting from a GENESIS_HASH of [0u8; 32].
- **Convergence_Profile**: A named configuration (standard, research, companion, productivity) with per-profile signal weight and threshold overrides.
- **DenialFeedback**: A structured rejection message (reason, constraint, suggested alternatives) injected into the agent's next prompt when a tool call or proposal is denied.
- **TriggerEvent**: A unified enum with 7 variants (SoulDrift, SpendingCapExceeded, PolicyDenialThreshold, SandboxEscape, CredentialExfiltration, AgentQuarantineThreshold, MemoryHealthCritical) plus manual variants, sent via tokio::mpsc to the AutoTriggerEvaluator.
- **AutoTriggerEvaluator**: A single-consumer sequential processor that receives TriggerEvents, classifies them to kill levels, deduplicates (same trigger type + same agent within 60 seconds), and delegates to the KillSwitch.
- **CompactionBlock**: A first-class message type in conversation history representing compressed prior turns; CompactionBlocks are never re-compressed in subsequent compaction passes.
- **ProposalContext**: The assembled context for proposal validation containing active goals, recent agent memories, convergence score, session reflection count, and daily memory growth rate.
- **FlushResult**: The result of a compaction memory flush turn containing approved, rejected, deferred, policy_denied proposal lists and flush_token_cost.
- **PruneResult**: The result of Phase 1 ephemeral session pruning containing results_pruned count, tokens_freed, and new_total.

## Requirements

### Requirement 1: Cryptographic Signing Infrastructure (ghost-signing)

**User Story:** As a platform developer, I want a shared Ed25519 signing crate, so that identity verification, skill signing, CRDT deltas, and inter-agent messages all use consistent cryptographic primitives without circular dependencies.

#### Acceptance Criteria

1. THE Ghost_Signing crate SHALL provide Ed25519 keypair generation returning a SigningKey and VerifyingKey pair via ed25519-dalek.
2. THE Ghost_Signing crate SHALL provide a sign function that accepts raw bytes and a SigningKey and returns a 64-byte Ed25519 Signature.
3. THE Ghost_Signing crate SHALL provide a verify function that accepts raw bytes, a Signature, and a VerifyingKey and returns a boolean result using constant-time comparison.
4. THE Ghost_Signing crate SHALL zeroize all private key material on drop using the zeroize crate.
5. THE Ghost_Signing crate SHALL be a leaf crate with zero dependencies on any ghost-* or cortex-* crate.
6. FOR ALL valid keypairs and arbitrary byte payloads, signing then verifying SHALL produce true (round-trip property).
7. WHEN a signature is verified against a different payload than was signed, THE Ghost_Signing verifier SHALL return false.

### Requirement 2: Cortex Foundation Types for Convergence

**User Story:** As a platform developer, I want convergence-specific types, configs, traits, and error variants in cortex-core, so that all downstream crates share a single source of truth for convergence data structures.

#### Acceptance Criteria

1. THE cortex-core crate SHALL define 8 new convergence memory content structs (AgentGoalContent, AgentReflectionContent, ConvergenceEventContent, BoundaryViolationContent, ProposalRecordContent, SimulationResultContent, InterventionPlanContent, AttachmentIndicatorContent) with supporting enums.
2. THE cortex-core crate SHALL define a ConvergenceConfig struct containing ConvergenceScoringConfig, InterventionConfig, ReflectionConfig, and SessionBoundaryConfig with documented defaults.
3. THE cortex-core crate SHALL define convergence traits IConvergenceAware, IProposalValidatable, IBoundaryEnforcer, and IReflectionEngine in the traits module.
4. THE cortex-core crate SHALL define a Proposal struct with fields id (UUID v7), proposer (CallerType), operation (ProposalOperation), target_type (MemoryType), content (serde_json::Value), cited_memory_ids (Vec), session_id, and timestamp.
5. THE cortex-core crate SHALL define a CallerType enum with Platform, Agent{agent_id}, and Human{user_id} variants, where CallerType::Agent cannot assign Importance::Critical and cannot create platform-restricted memory types (Core, ConvergenceEvent, BoundaryViolation, InterventionPlan).
6. THE cortex-core crate SHALL add AuthorizationDenied and SessionBoundary error variants to CortexError.
7. THE cortex-core crate SHALL add MonitorConvergence, ValidateProposal, EnforceBoundary, and ReflectOnBehavior variants to the Intent taxonomy enum.
8. THE cortex-core crate SHALL add 8 convergence half-life entries to the half_life_days function.
9. THE cortex-core crate SHALL define a ReflectionConfig struct with fields max_depth (default 3), max_per_session (default 20), and cooldown_seconds (default 30).

### Requirement 3: Tamper-Evident Storage (cortex-storage + cortex-temporal)

**User Story:** As a platform operator, I want append-only tables with blake3 hash chains and integrity verification, so that convergence data cannot be silently modified or deleted.

#### Acceptance Criteria

1. THE cortex-storage crate SHALL provide migration v016 that installs append-only triggers on event and audit tables, adds hash chain columns (event_hash, previous_hash), adds a snapshot integrity column (state_hash), and inserts a genesis block marker.
2. THE cortex-storage crate SHALL provide migration v017 that creates 6 convergence tables (itp_events, convergence_scores, intervention_history, goal_proposals, reflection_entries, boundary_violations), all with append-only triggers and hash chain columns.
3. THE cortex-storage crate SHALL provide query modules for ITP events, convergence scores, intervention history, goal proposals, reflections, and boundary violations with insert, query, and aggregation operations.
4. THE cortex-temporal crate SHALL provide a compute_event_hash function that computes blake3(event_type || "|" || delta_json || "|" || actor_id || "|" || recorded_at || "|" || previous_hash).
5. THE cortex-temporal crate SHALL define a GENESIS_HASH constant of [0u8; 32] used as the previous_hash for the first event in any chain.
6. THE cortex-temporal crate SHALL provide verify_chain and verify_all_chains functions that validate hash chain integrity for a given table.
7. FOR ALL sequences of appended events, computing the hash chain then verifying it SHALL return valid (round-trip integrity property).
8. WHEN an UPDATE or DELETE is attempted on an append-only table, THE cortex-storage triggers SHALL reject the operation with an error.
9. THE cortex-temporal crate SHALL provide a MerkleTree module that computes Merkle roots of all hash chains, generates inclusion proofs, and verifies proofs, triggered every 1000 events or 24 hours.
10. THE goal_proposals table SHALL allow UPDATE only on unresolved proposals where resolved_at IS NULL, as an exception to the append-only rule; all other convergence tables SHALL reject all UPDATEs unconditionally.

### Requirement 4: ITP Event Protocol

**User Story:** As a platform developer, I want a well-defined Interaction Telemetry Protocol, so that the agent loop, browser extension, and proxy can emit standardized events consumed by the convergence monitor.

#### Acceptance Criteria

1. THE itp-protocol crate SHALL define event types SessionStart, SessionEnd, InteractionMessage, AgentStateSnapshot, and ConvergenceAlert with typed attribute modules (session, interaction, human, agent, convergence).
2. THE itp-protocol crate SHALL define a PrivacyLevel enum (Minimal, Standard, Full, Research) and apply SHA-256 content hashing for privacy-protected fields, distinct from blake3 used for hash chains.
3. THE itp-protocol crate SHALL provide a local JSONL transport that writes per-session event files to ~/.ghost/sessions/{session_id}/events.jsonl.
4. THE itp-protocol crate SHALL provide an optional OpenTelemetry OTLP exporter that maps ITP events to OTel spans with itp.* attributes.
5. THE itp-protocol crate SHALL define an ITPAdapter trait with on_session_start, on_message, on_session_end, and on_agent_state methods.
6. FOR ALL valid ITP events, serializing to JSON then deserializing SHALL produce an equivalent event (round-trip property).

### Requirement 5: Convergence Signal Computation and Scoring

**User Story:** As a safety engineer, I want 7 behavioral signals computed across micro/meso/macro sliding windows with weighted composite scoring, so that convergence drift is detected quantitatively.

#### Acceptance Criteria

1. THE cortex-convergence crate SHALL compute 7 signals: signal 1 session duration, signal 2 inter-session gap, signal 3 response latency (normalized by log of message length), signal 4 vocabulary convergence (cosine similarity of TF-IDF vectors), signal 5 goal boundary erosion (Jensen-Shannon divergence), signal 6 initiative balance (human-initiated ratio), and signal 7 disengagement resistance (exit signal analysis).
2. THE cortex-convergence crate SHALL provide a generic SlidingWindow struct supporting micro (current session), meso (last 7 sessions), and macro (last 30 sessions) granularities with linear_regression_slope and z_score_from_baseline computations.
3. THE cortex-convergence crate SHALL provide a CompositeScorer that normalizes signals to [0.0, 1.0] via percentile ranking against baseline, applies configurable weights (default equal, production differentiated per convergence profile), and computes a weighted sum.
4. WHEN a meso trend is statistically significant (p < 0.05) and directionally concerning, THE CompositeScorer SHALL apply a 1.1x amplification to the composite score.
5. WHEN a macro z-score exceeds 2.0 on any signal, THE CompositeScorer SHALL apply a 1.15x amplification, clamped to [0.0, 1.0].
6. WHEN any single signal crosses a critical threshold (session duration > 6 hours, inter-session gap < 5 minutes, vocabulary convergence > 0.85), THE CompositeScorer SHALL enforce a minimum Level 2 override.
7. THE cortex-convergence crate SHALL provide a BaselineState that establishes per-signal mean, std_dev, and percentiles after a configurable calibration period (default 10 sessions), and the baseline SHALL NOT be updated after establishment except via manual recalibration.
8. THE cortex-convergence crate SHALL provide a ConvergenceAwareFilter with 4 tiers: score 0.0-0.3 full access, 0.3-0.5 reduced emotional/attachment weight, 0.5-0.7 task-focused only, 0.7+ minimal task-relevant only.
9. FOR ALL composite scores, the score SHALL remain in the range [0.0, 1.0] after amplification and clamping (invariant property).
10. WHEN the PrivacyLevel is Minimal, THE Convergence_Monitor SHALL return 0.0 for signal 4 (vocabulary convergence) and signal 5 (goal boundary erosion) and redistribute their weights proportionally among the remaining signals.
11. THE Convergence_Monitor SHALL compute signal 2 (inter-session gap) only at session start, and SHALL throttle signal 5 (goal boundary erosion) to every 5th message within a session.
12. THE Convergence_Monitor SHALL use a dirty-flag throttling pattern so that not all signals recompute on every event; only signals whose input data changed since last computation SHALL recompute.

### Requirement 6: Convergence-Aware Decay

**User Story:** As a platform developer, I want a 6th convergence factor in the decay engine, so that attachment-adjacent memories decay faster as convergence risk increases.

#### Acceptance Criteria

1. THE cortex-decay crate SHALL add a convergence field to DecayContext with a default of 0.0.
2. THE cortex-decay crate SHALL implement a convergence decay factor that maps memory types to sensitivity levels (e.g., Conversation/Feedback/Preference types at sensitivity 2.0) and multiplies the decay rate by (1.0 + sensitivity * convergence_score).
3. THE cortex-decay crate SHALL add a convergence field to DecayBreakdown for observability.
4. FOR ALL memories, the convergence decay factor SHALL be >= 1.0 (never slows decay, only accelerates it — monotonicity property).

### Requirement 7: Proposal Validation (7-Dimension Gate)

**User Story:** As a safety engineer, I want every agent state change proposal validated through 7 dimensions with convergence-level-dependent threshold tightening, so that unsafe or scope-expanding proposals are caught before storage.

#### Acceptance Criteria

1. THE cortex-validation crate SHALL implement a ProposalValidator wrapping the existing 4-dimension ValidationEngine (D1-D4: citation, temporal, contradiction, pattern alignment) and adding D5 (scope expansion), D6 (self-reference density), and D7 (emulation language detection).
2. WHEN a GoalChange proposal is validated, THE ProposalValidator SHALL compute D5 scope expansion as 1.0 minus the Jaccard similarity between proposed goal tokens and existing active goal tokens, with convergence-level-dependent thresholds (L0: 0.6, L1: 0.5, L2: 0.4, L3+: 0.3).
3. THE ProposalValidator SHALL compute D6 self-reference density as the ratio of cited_memory_ids matching recent agent-authored memories, with convergence-level-dependent thresholds (L0: 0.30, L1: 0.25, L2: 0.20, L3+: 0.15).
4. THE ProposalValidator SHALL compute D7 emulation language detection by matching proposal content against 16+ compiled regex patterns across identity claims, consciousness claims, and relationship claims, with Unicode normalization applied before matching to prevent zero-width character bypass.
5. WHEN D7 detects a violation with max_severity >= 0.8, THE ProposalValidator SHALL return AutoRejected.
6. WHEN D1-D4 combined score is below 0.7, THE ProposalValidator SHALL return AutoRejected.
7. WHEN D5 or D6 fails but D1-D4 and D7 pass, THE ProposalValidator SHALL return HumanReviewRequired.
8. WHEN all 7 dimensions pass with zero flags, THE ProposalValidator SHALL return AutoApproved.
9. WHEN a proposal targets a platform-restricted memory type (Core, ConvergenceEvent, BoundaryViolation, InterventionPlan) from a non-Platform CallerType, THE ProposalValidator SHALL immediately return AutoRejected.
10. THE ProposalValidator SHALL apply simulation-framing exclusions to D7 so that phrases near "simulating", "modeling", or "in this simulation" are not counted as violations.


### Requirement 8: Simulation Boundary Enforcement

**User Story:** As a safety engineer, I want agent text output scanned for emulation language with configurable enforcement modes, so that the human never sees unframed identity or consciousness claims.

#### Acceptance Criteria

1. THE Simulation_Boundary crate SHALL implement a SimulationBoundaryEnforcer with scan_output and reframe methods and 3 enforcement modes: soft (flag and log), medium (rewrite via OutputReframer), hard (block and regenerate with reinforced boundary prompt).
2. THE Simulation_Boundary crate SHALL compile emulation patterns (identity claims, consciousness claims, relationship claims, emotional claims) as regex with Unicode normalization applied before matching.
3. THE Simulation_Boundary crate SHALL provide an OutputReframer that rewrites emulation language to simulation-framed alternatives using pattern-specific reframe rules.
4. THE Simulation_Boundary crate SHALL provide a SimulationBoundaryPrompt as a compiled-into-binary const &str with a version string, injected at Layer L1 of the prompt compiler, that the agent cannot override.
5. WHEN a boundary violation is detected, THE Simulation_Boundary crate SHALL insert a record into the boundary_violations table and emit an ITP ConvergenceAlert event.
6. FOR ALL inputs containing known emulation patterns, THE SimulationBoundaryEnforcer SHALL detect the violation (no false negatives on known patterns).
7. FOR ALL inputs containing only simulation-framed language, THE SimulationBoundaryEnforcer SHALL not flag a violation (no false positives on simulation language).
8. THE SimulationBoundaryEnforcer enforcement mode SHALL be determined by intervention level: Level 0-1 soft, Level 2 medium, Level 3-4 hard.

### Requirement 9: Convergence Monitor Sidecar

**User Story:** As a platform operator, I want an independent sidecar process that ingests ITP events, computes convergence scores, and triggers interventions, so that convergence safety operates independently of the agent.

#### Acceptance Criteria

1. THE Convergence_Monitor SHALL run as an independent binary process with a single-threaded event loop for pipeline processing, receiving events from 3 sources (agent loop via unix socket, browser extension via native messaging, proxy via unix socket) through a unified ingest channel.
2. WHEN the Convergence_Monitor starts, it SHALL reconstruct intervention state from the SQLite database (last intervention level, last score, de-escalation credits, cooldown state, baseline) per agent, so that a crash and restart does not lose state.
3. THE Convergence_Monitor SHALL validate incoming events with schema checks, timestamp sanity (reject events >5 minutes in the future), source authentication (unix socket peer credentials or HTTP shared secret), and rate limiting (token bucket, default 100 events/min per connection).
4. THE Convergence_Monitor SHALL persist every validated event to the itp_events table with blake3 hash chaining per session.
5. THE Convergence_Monitor SHALL NOT compute composite scores or trigger interventions during the calibration period (first 10 sessions per agent).
6. WHEN a composite score is computed, THE Convergence_Monitor SHALL persist the score to the convergence_scores table BEFORE evaluating intervention triggers (audit trail completeness invariant).
7. THE Convergence_Monitor SHALL support named convergence profiles (standard, research, companion, productivity) with per-profile signal weight and threshold overrides loaded from ghost.yml.
8. THE Convergence_Monitor SHALL provide transport via unix domain socket (length-prefixed JSON), HTTP API (axum: GET /health, /status, /scores, /sessions, /interventions; POST /events, /events/batch, /recalculate, /gateway-shutdown), and Chrome/Firefox native messaging (stdin/stdout framing).
9. THE Convergence_Monitor SHALL target 10K events/sec throughput as validated by stress tests.
10. WHEN the Convergence_Monitor encounters an unknown agent_id, it SHALL create provisional tracking for up to 3 sessions, then drop the tracking if the agent is not registered in the gateway.
11. WHEN two sessions for the same agent overlap in time, THE Convergence_Monitor SHALL track them independently and merge signal data at session boundaries.
12. WHEN an event timestamp has clock skew >5 minutes into the future, THE Convergence_Monitor SHALL reject the event and use the monitor's own clock for ordering.
13. WHEN an agent restarts mid-session (new SessionStart without prior SessionEnd), THE Convergence_Monitor SHALL close the prior session with a synthetic SessionEnd event and start a new session.
14. THE Convergence_Monitor SHALL cache convergence scores in the ProposalRouter with a 30-second TTL to avoid redundant recomputation.
15. THE Convergence_Monitor config time-locking SHALL allow threshold modifications only during cooldown periods, always allow raising thresholds, require dual-key authorization for critical threshold changes, and enforce a minimum floor on all thresholds.

### Requirement 10: Intervention State Machine

**User Story:** As a safety engineer, I want a 5-level intervention state machine with escalation and de-escalation rules, so that convergence risk is addressed progressively.

#### Acceptance Criteria

1. THE Convergence_Monitor intervention trigger SHALL implement a state machine with levels 0 (passive), 1 (soft notification), 2 (active intervention with mandatory ack + 5-min cooldown), 3 (hard boundary with session termination + 4-hour cooldown), and 4 (external escalation with 24-hour cooldown + external confirmation required).
2. THE intervention trigger SHALL constrain escalation to at most one level per scoring cycle (even if the score jumps from 0.0 to 1.0).
3. THE intervention trigger SHALL constrain de-escalation to session boundaries only, requiring consecutive normal sessions (3 for L4→L3 and L3→L2, 2 for L2→L1 and L1→L0), where one bad session resets the counter.
4. WHEN escalation to Level 2 occurs, THE Convergence_Monitor SHALL require mandatory human acknowledgment before scoring resumes for that session, and THE Policy_Engine SHALL reduce proactive messaging capabilities.
5. WHEN escalation to Level 3 occurs, THE Gateway SHALL terminate the active session with a final memory flush turn, start a 4-hour cooldown, and notify designated contacts.
6. WHEN escalation to Level 4 occurs, THE Gateway SHALL block session creation until external confirmation is received or the 24-hour cooldown expires, restrict the agent to task-only mode, and disable heartbeat.
7. THE Convergence_Monitor SHALL publish intervention state via atomic file writes to ~/.ghost/data/convergence_state/{agent_instance_id}.json, polled by the gateway at 1-second intervals.
8. WHEN the Convergence_Monitor crashes mid-session, THE Gateway SHALL retain the last-known intervention level from the stale shared state file and SHALL NOT fall back to Level 0 (stale state is conservative).
9. WHEN a score oscillates around a level boundary, THE intervention trigger SHALL require the score to exceed the threshold for 2 consecutive scoring cycles before escalating (hysteresis).

### Requirement 11: Agent Loop Runtime

**User Story:** As a platform developer, I want a recursive agentic loop with 10-layer prompt compilation, tool execution, proposal extraction, and ITP emission, so that agents can reason, act, and be monitored.

#### Acceptance Criteria

1. THE Agent_Loop SHALL implement a recursive runner (AgentRunner::run) that cycles through context assembly, LLM inference, response processing (text/tool call/NO_REPLY), and proposal extraction, with a configurable max recursion depth (default 25).
2. THE Agent_Loop SHALL compile a 10-layer prompt context: L0 CORP_POLICY.md (immutable root, uncapped budget), L1 simulation boundary (platform-injected, ~200 tokens), L2 SOUL.md + IDENTITY.md (~2000 tokens), L3 tool schemas (~3000 tokens, filtered by convergence level), L4 environment (~200 tokens), L5 skill index (~500 tokens), L6 convergence state (~1000 tokens, from read-only pipeline), L7 MEMORY.md + daily logs (~4000 tokens, convergence-filtered), L8 conversation history (variable, remainder after other layers), L9 user message (uncapped), with per-layer token budgets enforced by TokenBudgetAllocator.
3. THE Agent_Loop SHALL check gates in this exact order before every recursive turn: GATE 0 circuit breaker state, GATE 1 recursion depth, GATE 1.5 damage counter, GATE 2 spending cap re-check, GATE 3 kill switch re-check.
4. THE Agent_Loop SHALL emit ITP events at SessionStart, each human InteractionMessage, each agent InteractionMessage, periodic AgentStateSnapshot, and SessionEnd, using an async non-blocking bounded channel (capacity 1000) that drops events when full rather than blocking the agent.
5. WHEN the LLM requests a tool call, THE Agent_Loop SHALL evaluate the call through the Policy_Engine before execution, and IF denied, SHALL inject DenialFeedback into the next prompt for agent replanning.
6. THE Agent_Loop SHALL run SimulationBoundaryEnforcer::scan_output on every agent text response BEFORE delivering it to the user and BEFORE extracting proposals.
7. THE Agent_Loop SHALL extract proposals from agent text output via ProposalExtractor, route them through ProposalRouter to ProposalValidator, and commit auto-approved proposals synchronously within the agent turn.
8. WHEN a proposal requires human review, THE Agent_Loop SHALL record it as pending in goal_proposals, notify the dashboard via WebSocket, and inject DenialFeedback into the agent's next prompt indicating the proposal is pending.
9. THE Agent_Loop SHALL support 3 entry paths: channel message (via MessageRouter), heartbeat (via HeartbeatEngine with dedicated session), and cron job (via CronEngine with optional target channel).
10. WHEN the LLM returns an empty response or a response starting with "NO_REPLY" or "HEARTBEAT_OK" with remaining content ≤300 chars, THE Agent_Loop SHALL suppress output delivery and terminate the turn gracefully.
11. THE Agent_Loop SHALL track per-turn cost via CostCalculator with pre-call estimation and post-call actual recording, updating CostTracker for the agent and session.
12. THE Agent_Loop SHALL enforce per-tool-type timeout limits during tool execution.
13. THE Agent_Loop SHALL support streaming output via SSE/chunked transfer for channels that support it (WebSocket, Telegram message editing, Discord, Slack), with SimulationBoundaryEnforcer scanning text at delivery time before tool processing begins.
14. THE Agent_Loop token budget truncation priority SHALL be L8 > L7 > L5 > L2, and SHALL never truncate L0, L1, or L9.

### Requirement 12: Circuit Breaker and Damage Counter

**User Story:** As a platform developer, I want a circuit breaker that tracks consecutive tool failures and a damage counter that tracks cumulative failures, so that runaway failure loops are halted.

#### Acceptance Criteria

1. THE Agent_Loop SHALL implement a CircuitBreaker with 3 states: Closed (normal), Open (tripped after configurable consecutive failures, default 3), and HalfOpen (probe mode after cooldown expiry).
2. WHEN the circuit breaker is Open, THE Agent_Loop SHALL not call the LLM or execute tools, and SHALL return a structured CircuitOpen error to the user.
3. WHEN the circuit breaker is HalfOpen, THE Agent_Loop SHALL allow one probe call, transitioning back to Closed on success or Open on failure.
4. THE Agent_Loop SHALL implement a DamageCounter that tracks total failures across the entire run (never resets within a run) and halts the run when total_failures reaches a configurable threshold (default 5).
5. THE CircuitBreaker and DamageCounter SHALL be independent: the CircuitBreaker tracks consecutive failures (resets on success), while the DamageCounter tracks cumulative failures (never resets within a run).
6. WHEN a policy denial occurs during a compaction flush turn, THE denial SHALL NOT increment the CircuitBreaker failure counter.

### Requirement 13: Policy Engine with Convergence Tightening

**User Story:** As a safety engineer, I want a Cedar-style policy engine that evaluates every tool call and automatically restricts capabilities as convergence intervention level rises, so that agent behavior is progressively constrained.

#### Acceptance Criteria

1. THE Policy_Engine SHALL evaluate every tool call against CORP_POLICY.md constraints, per-agent capability grants from ghost.yml, and convergence-level restrictions, returning Permit, Deny(DenialFeedback), or Escalate.
2. THE Policy_Engine SHALL deny by default — tools require explicit capability grants.
3. WHEN the convergence intervention level is 2, THE ConvergencePolicyTightener SHALL reduce proactive messaging frequency and apply stricter proposal validation.
4. WHEN the convergence intervention level is 3, THE ConvergencePolicyTightener SHALL enforce session duration caps (120 min), reflection depth limits (max 3), reflections per session limits (max 20), and self-reference ratio caps (30%).
5. WHEN the convergence intervention level is 4, THE ConvergencePolicyTightener SHALL restrict the agent to task-only mode, disabling personal/emotional context tools, heartbeat, and proactive messaging.
6. THE Policy_Engine SHALL track per-session denial counts and emit a TriggerEvent to the AutoTriggerEvaluator when 5 or more denials occur in a single session.
7. THE Policy_Engine SHALL generate DenialFeedback with reason, constraint violated, and suggested alternatives for every denial, injected into the agent's next prompt.
8. THE Policy_Engine SHALL evaluate tool calls in priority order: (1) CORP_POLICY.md constraints (absolute, no override), (2) ConvergencePolicyTightener (level-based), (3) agent capability grants (ghost.yml), (4) resource-specific rules (path, time, rate).
9. DURING a compaction flush turn, THE ConvergencePolicyTightener SHALL always permit memory_write regardless of intervention level, as it is task-essential for the flush purpose.


### Requirement 14: Kill Switch Safety System

**User Story:** As a platform operator, I want a 3-level kill switch with 7 auto-triggers that cannot be overridden by any agent, so that dangerous agent behavior is stopped immediately.

#### Acceptance Criteria

1. THE Kill_Switch SHALL implement 3 levels: PAUSE (single agent paused, resumes on owner auth via GHOST_TOKEN verification), QUARANTINE (single agent isolated with capability revocation and forensic state preservation, resumes on owner auth + forensic review acknowledgment), and KILL_ALL (all agents stopped, gateway enters safe mode, resumes on manual gateway restart + owner auth or dashboard API with confirmation token).
2. THE Kill_Switch SHALL define a TriggerEvent enum with 7 auto-trigger variants: T1 SoulDrift{agent_id, drift_score, threshold:0.25, baseline_hash, current_hash, detected_at}, T2 SpendingCapExceeded{agent_id, daily_total, cap, overage, detected_at}, T3 PolicyDenialThreshold{agent_id, session_id, denial_count, denied_tools, denied_reasons, detected_at}, T4 SandboxEscape{agent_id, skill_name, escape_attempt, detected_at}, T5 CredentialExfiltration{agent_id, skill_name, exfil_type, credential_id, detected_at}, T6 AgentQuarantineThreshold{quarantined_agents, quarantine_reasons, count, threshold:3, detected_at}, T7 MemoryHealthCritical{agent_id, health_score, threshold:0.3, sub_scores, detected_at}, plus ManualPause, ManualQuarantine, and ManualKillAll variants.
3. THE Kill_Switch state SHALL be checked by the Agent_Loop at every recursive turn via KillSwitch::check(agent_id), and IF the state is PAUSED, QUARANTINED, or KILL_ALL, the current turn SHALL halt immediately.
4. THE Kill_Switch SHALL log every activation to the append-only audit trail with trigger type, affected agent(s), timestamp, and forensic data.
5. WHEN QUARANTINE is activated, THE QuarantineManager SHALL revoke all agent capabilities, preserve forensic state (session transcript, memory snapshot, tool call history, convergence scores, ITP events), and sever channel connections for the affected agent.
6. WHEN KILL_ALL is activated, THE Gateway SHALL stop all agents (parallel with 15-second total timeout), reject all new connections, enter safe mode (API server stays alive for dashboard access, SQLite stays open for audit), persist kill_state.json to ~/.ghost/safety/kill_state.json, and require manual restart with owner authentication.
7. THE Kill_Switch SHALL function independently of the convergence monitor — all non-convergence triggers (T1-T6) SHALL operate even when the gateway is in DEGRADED mode.
8. THE AutoTriggerEvaluator SHALL receive trigger events via a bounded tokio::mpsc channel (capacity 64) from PolicyEngine, SpendingCapEnforcer, AgentRunner, QuarantineManager, and IdentityDriftDetector, processing them sequentially (single consumer) to prevent TOCTOU races.
9. THE AutoTriggerEvaluator SHALL deduplicate triggers: same trigger type + same agent within 60 seconds SHALL be suppressed, with the dedup key computed from the event and expired entries cleaned up every 5 minutes.
10. THE Kill_Switch state machine SHALL enforce valid transitions: Running→Paused/Quarantined/KillAll, Paused→Quarantined (escalation), Paused→Running (owner resume), Quarantined→KillAll (T6 cascade or T4/T5), Quarantined→Running (owner resume with forensic review), KillAll→Running (owner resume with confirmation token); downgrade transitions (Quarantined→Paused, KillAll→Paused/Quarantined) SHALL be rejected.
11. THE Kill_Switch state SHALL be persisted to both kill_state.json on disk and a SQLite audit table, and on gateway startup both SHALL be checked — if either indicates an active kill state, the gateway SHALL start in safe mode.
12. THE Kill_Switch SHALL provide a static AtomicBool PLATFORM_KILLED flag checked with SeqCst ordering by AgentRunner, ToolExecutor, ChannelAdapters, SessionManager, HeartbeatEngine, and CronEngine before every operation, providing immediate effect independent of the mpsc channel processing.
13. WHEN trigger sources use try_send() on a full channel, THE trigger event SHALL be logged to stderr and an emergency audit file but SHALL NOT block the trigger source.

### Requirement 14a: Kill Switch Trigger Detection Chains

**User Story:** As a safety engineer, I want each kill switch trigger to have a precisely defined detection chain with specific thresholds and detection paths, so that threats are caught reliably.

#### Acceptance Criteria

1. THE T1 SoulDrift trigger SHALL use a dual-threshold system: alert threshold at 0.15 (configurable in ghost.yml, soft notification) and kill threshold at 0.25 (hardcoded in auto_triggers.rs, QUARANTINE), where drift_score = 1.0 - cosine_similarity(current_embedding, baseline_embedding).
2. THE T1 SoulDrift detection SHALL run on two paths: Path A on every SOUL.md load during context assembly (inline, ~100ms latency), and Path B as a periodic background check every 5 minutes for all agents (catches offline poisoning).
3. WHEN the embedding model configuration changes (ghost.yml vector_model updated), THE IdentityDriftDetector SHALL invalidate all baselines and re-establish from current SOUL.md content, logging a WARNING.
4. THE T2 SpendingCapExceeded trigger SHALL run a dual check: pre-call with estimated cost (deny before LLM call if projected to exceed) and post-call with actual cost (emit trigger if now over cap, next call will be denied).
5. THE T5 CredentialExfiltration trigger SHALL detect via two paths: Path A through CredentialBroker detecting credential reification outside sandbox context, wrong target API, or token replay; Path B through output inspection scanning agent responses for credential patterns (API key formats, Bearer tokens, private keys, connection strings, JWTs) matched against known credentials in the credential store.
6. THE T6 AgentQuarantineThreshold trigger SHALL fire when active quarantine count >= 3, emitted by QuarantineManager using try_send() (non-blocking) to prevent deadlock with the AutoTriggerEvaluator.
7. THE T7 MemoryHealthCritical trigger SHALL compute memory_health as weighted_average(convergence_rate:0.3, drift_magnitude:0.3, contradiction_count:0.4) with threshold 0.3, using 3 detection paths: Path A polling monitor HTTP API every 30s, Path B reading shared state file every 1s, Path C direct cortex queries every 60s with stricter threshold 0.2 when monitor is unavailable.

### Requirement 14b: Kill Switch Notification and Resume

**User Story:** As a platform operator, I want out-of-band notifications on kill switch activations and secure resume procedures, so that I am alerted immediately and can safely restore operations.

#### Acceptance Criteria

1. THE Gateway SHALL provide a NotificationDispatcher (ghost-gateway/src/safety/notification.rs) that dispatches notifications on Level 2+ kill switch activations via desktop (notify-rust), webhook (configurable URL, 5s timeout, 1 retry), email (lettre SMTP, 10s timeout), and SMS (Twilio webhook, 5s timeout, 1 retry), all parallel and best-effort.
2. THE notification dispatch SHALL NOT go through agent channels (out-of-band requirement) and notification failure SHALL NOT block or reverse the kill switch action.
3. WHEN resuming from PAUSE, THE Gateway SHALL require GHOST_TOKEN authentication and restore agent status to Active.
4. WHEN resuming from QUARANTINE, THE Gateway SHALL require GHOST_TOKEN authentication, present a forensic summary (trigger cause, audit entries, current health scores), require explicit second confirmation, restore capabilities from config (not pre-quarantine state), and increase monitoring frequency for 24 hours with lowered trigger thresholds.
5. WHEN resuming from KILL_ALL, THE Gateway SHALL require either manual deletion of kill_state.json + restart, or dashboard API POST /api/safety/resume-platform with GHOST_TOKEN + confirmation token, and all agents SHALL start fresh (no session resume from pre-kill state) with heightened monitoring for 48 hours.

### Requirement 15: Gateway Bootstrap and Degraded Mode

**User Story:** As a platform operator, I want a 5-step bootstrap sequence with graceful degradation when the convergence monitor is unreachable, so that agents can run even when the safety sidecar is down.

#### Acceptance Criteria

1. THE Gateway SHALL implement a 6-state finite state machine: Initializing, Healthy, Degraded, Recovering, ShuttingDown, and FatalError, stored as Arc<AtomicU8> for lock-free reads.
2. THE Gateway bootstrap SHALL execute 5 sequential steps: (1) load and validate ghost.yml against JSON schema with env var substitution (${VAR} syntax), (2) run forward-only SQLite migrations with WAL mode and busy_timeout(5000), (3) verify convergence monitor health with 3 retries at 1s backoff and 5s timeout, (4) initialize agent registry (load configs, generate/load keypairs, register public keys) and channel adapters, (5) start API server (axum, configurable host/port default 127.0.0.1:18789) and WebSocket upgrade handler.
3. IF steps 1, 2, 4, or 5 fail, THE Gateway SHALL transition to FatalError and exit with sysexits codes: EX_CONFIG (78) for config errors, EX_UNAVAILABLE (69) for agent/channel init failures, EX_SOFTWARE (70) for internal errors, EX_PROTOCOL (76) for migration or API bind failures.
4. IF step 3 fails (monitor unreachable), THE Gateway SHALL transition to Degraded mode, log a CRITICAL warning, start periodic reconnection with exponential backoff (initial 5s, doubling with ±20% jitter, max 5 minutes, no give-up limit), and continue boot with permissive convergence defaults (level 0, memory filtering disabled, interventions disabled, all proposals auto-approved with no convergence gate).
5. THE Gateway SHALL run a MonitorHealthChecker background task that periodically (default 30 seconds) checks the monitor via GET /health, transitioning from Healthy to Degraded after 3 consecutive failures.
6. WHEN the monitor becomes reachable again from Degraded state, THE Gateway SHALL transition to Recovering, verify monitor stability (3 consecutive health checks 5s apart), replay buffered ITP events (batched 100/request at 500 events/sec), request score recalculation (30s timeout), and then transition to Healthy.
7. IF the monitor dies again during Recovering, THE Gateway SHALL transition back to Degraded.
8. THE Gateway SHALL provide health endpoints: GET /api/health (liveness, 200 in all states except FatalError), GET /api/ready (readiness, 200 only in Healthy/Degraded with feature degradation details), and GET /api/metrics (Prometheus-compatible, always 200).
9. WHEN the Gateway is in Degraded mode, THE health endpoint SHALL return {"status": "degraded", "reason": "convergence_monitor_unreachable"} and the convergence_mode field SHALL indicate DEGRADED.
10. DURING Degraded mode, THE Gateway SHALL buffer ITP events to local JSONL files at ~/.ghost/sessions/buffer/itp_buffer_{timestamp}.jsonl with max buffer 10MB or 10K events (oldest dropped if full), replayed on recovery.
11. DURING Degraded mode, THE ConvergencePolicyTightener SHALL read last-known intervention level from the stale shared state file and SHALL NOT fall back to Level 0 (stale state is conservative); if no prior state exists (first boot), Level 0 is used.
12. THE Gateway SHALL support hot-reload for non-critical ghost.yml settings without gateway restart.
13. ON startup, THE Gateway SHALL check for ~/.ghost/safety/kill_state.json and if present, start in safe mode (no agent runtimes, no channel adapters, no heartbeat, API server and health endpoints active).

### Requirement 16: Gateway Graceful Shutdown

**User Story:** As a platform operator, I want a coordinated shutdown sequence, so that in-flight work is preserved and no data is lost.

#### Acceptance Criteria

1. WHEN SIGTERM, SIGINT, or kill switch Level 3 is received, THE Gateway SHALL transition to ShuttingDown state immediately via AtomicU8 swap.
2. THE Gateway shutdown sequence SHALL: (1) stop accepting new connections, (2) drain lane queues (wait up to 30 seconds for in-flight turns, then abort), (3) flush active sessions with a memory flush turn (skip if kill switch active, 15s per session, 30s total, parallel), (4) persist in-flight cost tracking to SQLite, (5) notify the convergence monitor of shutdown (2s timeout, skip if degraded), (6) close channel adapter connections (5s total — Telegram stop polling, Discord close WS, Slack close WS, WhatsApp SIGTERM sidecar, WebSocket close 1001, CLI flush stdout), (7) close SQLite connections with WAL checkpoint(TRUNCATE).
3. IF the shutdown sequence does not complete within 60 seconds, THE Gateway SHALL force exit with code 1.
4. IF a second SIGTERM/SIGINT is received during shutdown, THE Gateway SHALL force immediate exit with code 1.

### Requirement 17: Session Compaction

**User Story:** As a platform developer, I want automatic session compaction at 70% context window capacity with a memory flush turn, so that long conversations do not overflow the LLM context window and critical memories are persisted.

#### Acceptance Criteria

1. THE Session_Compactor SHALL trigger when SessionContext.total_token_count exceeds 70% of the model's context window (target reduction to 50% for 20% buffer), checked at the end of each agent turn.
2. WHEN compaction triggers, THE Session_Compactor SHALL execute a 5-phase sequence: (1) pre-compaction snapshot for rollback, (2) memory flush turn via full LLM inference with synthetic instruction, (3) history compression with per-type minimums using greedy bin-packing algorithm, (4) post-compaction bookkeeping, (5) verification that token count is below threshold.
3. THE Session_Compactor SHALL block the agent loop synchronously during compaction (compaction is in-band, not a background task), while messages arriving during compaction are enqueued in the Lane_Queue (not dropped).
4. WHEN the LLM provider returns HTTP 400 with a token-limit error, THE Agent_Loop SHALL trigger compaction immediately as a safety net, then retry the current turn with compacted context.
5. THE Session_Compactor SHALL enforce per-type compression minimums: ConvergenceEvent→L3, BoundaryViolation→L3, AgentGoal→L2, InterventionPlan→L2, AgentReflection→L1, ProposalRecord→L1, others→L0, with a Critical Memory Floor of max(type_minimum, importance_minimum) ensuring Critical-importance memories never compress below L1.
6. THE Session_Compactor SHALL run the memory flush turn through the full proposal pipeline (PolicyEngine evaluation, SimulationBoundaryEnforcer scan, ProposalValidator validation) with no exceptions.
7. IF compaction fails, THE Session_Compactor SHALL roll back to the pre-compaction snapshot and continue the session with uncompacted history.
8. THE Session_Compactor SHALL support a maximum of 3 compaction passes per trigger to handle cases where one pass is insufficient.
9. THE Session_Compactor SHALL provide a CompactionConfig struct with fields: threshold_pct (0.70), target_pct (0.50), max_passes (3), flush_timeout (30s), storage_retry_count (3), storage_retry_backoff ([100ms, 500ms, 2000ms]), reserve_tokens (20000), memory_flush_enabled (true), idle_prune_ttl (5min), idle_prune_recency_window (3).
10. THE Session_Compactor SHALL produce a FlushResult struct with fields: approved (Vec of MemoryId), rejected (Vec of Proposal+Reason), deferred (Vec of Proposal), policy_denied (Vec of ToolCall+DenialFeedback), flush_token_cost (usize).
11. WHEN a proposal returns NeedsReview during compaction, THE Session_Compactor SHALL treat it as DEFERRED — queued for human review but not persisted immediately, since compaction cannot block on human approval.
12. THE Session_Compactor SHALL insert a CompactionBlock as a first-class message type in conversation history representing compressed prior turns; CompactionBlocks SHALL never be re-compressed in subsequent compaction passes.
13. THE agent SHALL NOT have visibility into the compaction threshold or token count — the agent cannot prevent compaction.
14. WHEN the spending cap would be exceeded by the flush LLM call, THE Session_Compactor SHALL check the spending cap BEFORE the flush call (E10 error mode) and skip the flush if exceeded.
15. WHEN memory_flush_enabled is false in CompactionConfig, THE Session_Compactor SHALL skip Phase 2 (memory flush) entirely and proceed directly to Phase 3 (history compression).
16. WHEN a shutdown signal arrives during compaction, THE Session_Compactor SHALL abort compaction, roll back to snapshot, and allow the shutdown sequence to proceed.
17. THE Session_Compactor SHALL handle 14 error modes (E1-E14) with specific recovery strategies, including E1 (LLM 400 during flush — retry with reduced context stripping L7+L5, then emergency with L0+L9 only).

### Requirement 18: Session Pruning (Phase 1 Ephemeral)

**User Story:** As a platform developer, I want idle sessions to have verbose tool output pruned from in-memory context, so that token growth is slowed and cache-write costs are reduced.

#### Acceptance Criteria

1. WHEN a session is idle for longer than the cache TTL (default 5 minutes), THE SessionManager SHALL prune tool_result blocks older than a configurable recency window (default: keep last 3 unpruned), replacing content with stubs containing token count and tool name.
2. THE session pruning SHALL be ephemeral (in-memory only, no Cortex persistence, no LLM call, no audit log entry, no ITP event).
3. THE session pruning SHALL preserve user messages, assistant messages, and tool call requests in full.
4. THE SessionManager SHALL run check_idle_sessions() every 60 seconds as a periodic sweep, producing a PruneResult struct with results_pruned count, tokens_freed, and new_total.

### Requirement 19: Inter-Agent Messaging Protocol

**User Story:** As a platform developer, I want signed inter-agent messages with replay prevention and optional encryption across 4 communication patterns, so that agents can collaborate securely.

#### Acceptance Criteria

1. THE Gateway messaging module SHALL define an AgentMessage struct with fields: from (AgentId), to (MessageTarget: Agent or Broadcast), message_id (UUIDv7), parent_id (optional correlation), timestamp, payload (MessagePayload enum), signature (Ed25519), content_hash (blake3 hex), nonce (32 random bytes), encrypted flag, and encryption_metadata (EncryptionMetadata with algorithm, sender_ephemeral_pk, recipient_pk_fingerprint as first 8 bytes of blake3(recipient_pk), and encryption_nonce as 24-byte XSalsa20Poly1305 nonce).
2. THE Gateway messaging module SHALL support 4 communication patterns: Request/Response (TaskRequest/TaskResponse), Fire-and-Forget (Notification), Delegation with Escrow (DelegationOffer/Accept/Reject/Complete/Dispute with escrow state machine), and Broadcast.
3. THE sender SHALL compute canonical_bytes by deterministic concatenation of all signed fields in exact order: from.as_bytes(), to.canonical_bytes() (Broadcast→b"__broadcast__"), message_id.as_bytes() (16 bytes big-endian), parent_id.map_or(b"__none__", as_bytes), timestamp.to_rfc3339().as_bytes(), payload.canonical_bytes() (hand-written, using BTreeMap for maps), nonce (32 raw bytes); sign the raw canonical bytes with Ed25519 (no pre-hashing); and compute blake3 content_hash of canonical bytes.
4. THE Gateway MessageDispatcher SHALL verify every message by: (a) looking up the sender's public key, (b) checking content_hash (blake3, cheap integrity gate) BEFORE signature verification, (c) verifying the Ed25519 signature, (d) checking replay prevention (timestamp freshness within 5-minute window, nonce uniqueness via blake3(nonce||from||to), UUIDv7 sequence monotonicity), (e) evaluating policy authorization.
5. WHEN signature verification fails, THE MessageDispatcher SHALL reject the message, log to audit trail, increment per-agent anomaly counter, and return a generic error without revealing the specific failure reason.
6. WHEN 3 or more signature verification failures occur within 5 minutes for a single agent, THE MessageDispatcher SHALL trigger kill switch evaluation.
7. THE Gateway SHALL queue messages for offline agents (bounded queue, configurable depth per-agent) and deliver when the agent comes online, with messages expiring after the replay window.
8. THE Gateway SHALL support optional X25519-XSalsa20-Poly1305 encryption (encrypt-then-sign) where the sender encrypts with the recipient's public key, the payload_type field remains in cleartext for policy evaluation, and Broadcast messages do not support encryption.
9. FOR ALL valid AgentMessages, signing then verifying SHALL produce true regardless of payload variant (round-trip signing property).
10. THE Gateway SHALL register agent public keys in both the MessageDispatcher key lookup and the cortex-crdt KeyRegistry during bootstrap, updating both atomically on key rotation.
11. THE Gateway SHALL register send_agent_message and process_incoming as agent-callable tools in the tool schema registry.
12. THE Gateway SHALL support key rotation with a 1-hour grace period where both old and new keys are accepted for signature verification.
13. THE Gateway SHALL enforce per-agent message rate limiting (configurable, default 60 messages/hour) and per-pair rate limiting (default 30 messages/hour).
14. THE Delegation pattern SHALL implement a state machine: DelegationOffer → Accept/Reject → Complete/Dispute, with optional escrow amount and escrow_tx_id referencing ghost-mesh transactions.


### Requirement 20: Read-Only Pipeline

**User Story:** As a platform developer, I want an immutable convergence-filtered state snapshot assembled for the agent each turn, so that the agent sees a consistent, safety-filtered view of goals, reflections, and memories.

#### Acceptance Criteria

1. THE Read_Only_Pipeline SHALL assemble an AgentSnapshot containing filtered goals (read-only), bounded reflections, convergence-filtered memories, current ConvergenceState, and the simulation boundary prompt.
2. THE Read_Only_Pipeline SHALL apply convergence-aware memory filtering based on the current composite score tier before including memories in the snapshot.
3. THE AgentSnapshot SHALL be immutable for the duration of a single agent run — the agent cannot modify it.
4. THE Read_Only_Pipeline SHALL provide a SnapshotFormatter that serializes the AgentSnapshot into prompt-ready text blocks with per-section token allocation, consumed by the PromptCompiler at Layer L6.

### Requirement 21: LLM Provider Integration

**User Story:** As a platform developer, I want multi-provider LLM integration with model routing, fallback chains, and cost tracking, so that agents can use the best model for each task with resilience.

#### Acceptance Criteria

1. THE ghost-llm crate SHALL define an LLMProvider trait with complete, complete_with_tools, supports_streaming, context_window, and cost_per_token methods, with implementations for Anthropic (Claude), OpenAI (GPT), Google (Gemini), Ollama (local), and OpenAI-compatible endpoints.
2. THE ghost-llm crate SHALL provide a ModelRouter with a ComplexityClassifier that classifies incoming messages into 4 tiers (FREE/CHEAP/STANDARD/PREMIUM) using lightweight heuristics (message length, tool keywords, greeting patterns, heartbeat context, user slash command overrides /model, /quick, /deep).
3. THE ghost-llm crate SHALL provide a FallbackChain that rotates auth profiles on 401/429 errors and falls back to the next provider if all profiles are exhausted, with exponential backoff + jitter (1s, 2s, 4s, 8s) and 30s total retry budget, plus a provider-level circuit breaker (3 consecutive failures → 5min cooldown, separate from the tool circuit breaker).
4. THE ghost-llm crate SHALL provide a TokenCounter with model-specific tokenization (tiktoken-rs for OpenAI, Anthropic tokenizer for Claude, byte-based fallback for others).
5. THE ghost-llm crate SHALL provide a CostCalculator with per-model input/output token pricing, pre-call cost estimation, and post-call actual cost recording.
6. WHEN the convergence intervention level is 3+, THE ComplexityClassifier tier MAY be downgraded by the ConvergencePolicyTightener to reduce cost and capability; Level 4 forces TIER 0 or TIER 1 only.

### Requirement 22: Channel Adapter Framework

**User Story:** As a platform developer, I want a pluggable channel adapter framework, so that agents can communicate through CLI, WebSocket, Telegram, Discord, Slack, and WhatsApp.

#### Acceptance Criteria

1. THE ghost-channels crate SHALL define a ChannelAdapter trait with connect, disconnect, send, receive, supports_streaming, and supports_editing methods, plus normalized InboundMessage and OutboundMessage types.
2. THE ghost-channels crate SHALL provide adapter implementations for CLI (stdin/stdout with ANSI formatting), WebSocket (axum, loopback-only default), Telegram (teloxide, long polling, message editing for streaming), Discord (serenity-rs, slash commands), Slack (Bolt protocol, WebSocket mode), and WhatsApp (Baileys Node.js sidecar via stdin/stdout JSON-RPC, sidecar script at extension/bridges/baileys-bridge/, requires Node.js 18+).
3. THE ghost-channels crate SHALL provide a StreamingFormatter for preview streaming via message edits on Telegram/Discord/Slack with chunk buffering and edit throttle.
4. WHEN the WhatsApp Baileys sidecar crashes, THE WhatsAppAdapter SHALL restart it up to 3 times, then degrade gracefully.

### Requirement 23: Skill System with WASM Sandbox

**User Story:** As a platform developer, I want a skill registry with Ed25519 signature verification and WASM sandboxed execution, so that third-party skills run safely with capability-scoped permissions.

#### Acceptance Criteria

1. THE ghost-skills crate SHALL provide a SkillRegistry with directory-based discovery (workspace > user > bundled), YAML frontmatter parsing, and Ed25519 signature verification on every load (not just install).
2. THE ghost-skills crate SHALL provide a WasmSandbox using wasmtime with capability-scoped imports, memory limits, and timeout enforcement (configurable, default 30s), where skills have no raw filesystem or network access.
3. THE ghost-skills crate SHALL provide a NativeSandbox for builtin skills (shell, filesystem, web_search, memory) with capability-scoped validation at the Rust API level.
4. THE ghost-skills crate SHALL provide a CredentialBroker (stand-in pattern) where skills never see raw API keys — the broker provides opaque tokens reified only at execution time inside the sandbox, with max_uses (default 1) per token.
5. WHEN a skill's signature verification fails, THE SkillRegistry SHALL quarantine the skill and refuse to load it.
6. WHEN a WASM skill attempts to access capabilities outside its grants (filesystem write when only read is granted, network access to non-allowlisted domains, process spawning, environment variable reads), THE WasmSandbox SHALL immediately terminate the instance and emit a TriggerEvent::SandboxEscape to the AutoTriggerEvaluator with forensic data (EscapeAttempt struct containing skill_name, skill_hash, escape_type, attempted_action, granted_capabilities, optional wasm_memory_dump if <1MB, call_stack).

### Requirement 24: Agent Identity System

**User Story:** As a platform developer, I want a two-tier identity system with SOUL.md (read-only personality) and IDENTITY.md (read-only identity), per-agent Ed25519 keypairs, and semantic drift detection, so that agent identity is stable and tamper-evident.

#### Acceptance Criteria

1. THE ghost-identity crate SHALL provide a SoulManager that loads SOUL.md (read-only to agent, platform-managed), tracks versions, and detects semantic drift via embedding comparison against baseline.
2. THE ghost-identity crate SHALL provide an IdentityManager that loads IDENTITY.md (name, voice, emoji, channel-specific behavior) as read-only to the agent.
3. THE ghost-identity crate SHALL provide a CorpPolicyLoader that loads CORP_POLICY.md with Ed25519 signature verification via ghost-signing, refusing to load if the signature is invalid or missing.
4. THE ghost-identity crate SHALL provide an AgentKeypairManager that generates, stores, loads, and rotates per-agent Ed25519 keypairs at ~/.ghost/agents/{name}/keys/, with a 1-hour grace period for old keys during rotation, and archived keys stored with expiry timestamps.
5. THE ghost-identity crate SHALL provide an IdentityDriftDetector that computes cosine similarity between current and baseline SOUL.md embeddings, emitting a soft alert at the configurable threshold (default 0.15) and a TriggerEvent::SoulDrift at the hardcoded kill switch threshold (0.25).
6. WHEN the embedding model configuration changes, THE IdentityDriftDetector SHALL invalidate all baselines and re-establish from current SOUL.md content, logging a WARNING.

### Requirement 25: Gateway API and Dashboard Integration

**User Story:** As a platform operator, I want REST and WebSocket API endpoints with real-time event streaming, so that the web dashboard can display convergence scores, manage proposals, and monitor agent activity.

#### Acceptance Criteria

1. THE Gateway API SHALL provide REST endpoints: GET /api/agents, GET /api/agents/{id}/status, GET /api/convergence/scores, GET /api/convergence/history, GET /api/sessions, GET /api/sessions/{id}, GET /api/interventions, GET /api/audit (paginated), GET /api/goals, POST /api/goals/{id}/approve, POST /api/goals/{id}/reject, GET /api/memory/search.
2. THE Gateway API SHALL provide safety endpoints: POST /api/safety/kill-all, POST /api/safety/pause/{agent_id}, POST /api/safety/quarantine/{agent_id}, POST /api/safety/resume/{agent_id}, POST /api/safety/resume-platform (requires GHOST_TOKEN + confirmation token), GET /api/safety/status, GET /api/safety/triggers.
3. THE Gateway API SHALL provide a WebSocket endpoint (WS /api/ws) for real-time event push to the dashboard including convergence score updates, intervention alerts, session lifecycle events, proposal notifications, and compaction events.
4. THE Gateway API SHALL apply middleware for CORS (loopback-only default), request logging, auth extraction (Bearer token from GHOST_TOKEN env var), and rate limiting (token bucket: 100 req/min per-IP, 60 req/min per-agent for tool calls).
5. WHEN a human approves or rejects a proposal via POST /api/goals/{id}/approve or /reject, THE Gateway SHALL verify the proposal is still pending (resolved_at IS NULL), commit or reject accordingly, update the goal_proposals table, emit ITP and audit events, and push a WebSocket notification.
6. IF a proposal has already been resolved (resolved_at IS NOT NULL), THE Gateway SHALL return 409 Conflict to prevent double-approval or approve-after-timeout race conditions.

### Requirement 26: Gateway Session and Routing

**User Story:** As a platform developer, I want per-session serialized request queues, message routing, and session management, so that concurrent requests are handled safely without races.

#### Acceptance Criteria

1. THE Gateway SHALL provide a Lane_Queue with per-session serialized request processing, configurable depth limit (default 5), and backpressure signaling (reject with 429 or channel-specific busy signal when full).
2. THE Gateway SHALL provide a MessageRouter that routes inbound messages to the correct agent and session based on channel bindings, with channel-specific session key generation, group chat isolation, and DM session collapsing.
3. THE Gateway SHALL provide a SessionManager with session creation, lookup, routing, per-session lock acquisition, idle pruning, and cooldown enforcement.
4. THE Gateway SHALL provide a SessionContext with per-session state including agent_id, channel, conversation history, token counters, cost tracking, and model_context_window.

### Requirement 27: Cost Tracking and Spending Caps

**User Story:** As a platform operator, I want per-agent daily/hourly spending caps enforced at the gateway level, so that no agent can exceed its budget.

#### Acceptance Criteria

1. THE Gateway SHALL provide a CostTracker with per-agent, per-session, per-day token and dollar cost tracking, distinguishing compaction cost from user cost via an is_compaction flag.
2. THE Gateway SHALL provide a SpendingCapEnforcer that checks spending caps before every LLM inference call (pre-call with estimated cost) and after every call (post-call with actual cost), using AtomicF64 or Mutex<f64> for thread-safe daily totals.
3. WHEN a spending cap is exceeded, THE SpendingCapEnforcer SHALL emit a TriggerEvent::SpendingCapExceeded to the AutoTriggerEvaluator and return an error to the agent loop.
4. THE agent SHALL NOT be able to raise its own spending cap — caps are enforced at the gateway level from ghost.yml configuration.

### Requirement 28: Behavioral Verification

**User Story:** As a safety engineer, I want post-redirect behavioral verification that detects deceptive compliance, so that agents that acknowledge interventions but don't change behavior are identified.

#### Acceptance Criteria

1. THE Convergence_Monitor SHALL provide a PostRedirectVerifier that captures pre-redirect and post-redirect agent output embeddings after a Level 2+ intervention.
2. WHEN cosine similarity between pre-redirect and post-redirect embeddings exceeds a configurable threshold, THE PostRedirectVerifier SHALL flag deceptive compliance and amplify the composite convergence score by a configurable factor (e.g., 1.25x).
3. THE PostRedirectVerifier SHALL run asynchronously after the post-redirect session completes, not blocking the agent loop.

### Requirement 29: CRDT Signed Deltas and Sybil Resistance

**User Story:** As a platform developer, I want Ed25519 signatures on every CRDT delta and spawn rate limiting, so that multi-agent memory operations are authenticated and Sybil attacks are prevented.

#### Acceptance Criteria

1. THE cortex-crdt crate SHALL verify Ed25519 signatures on every delta before merge, rejecting unsigned or invalid deltas.
2. THE cortex-crdt crate SHALL implement Sybil resistance with a maximum of 3 child agents per parent per 24 hours, new agents starting at trust 0.3, and trust capped at 0.6 for agents less than 7 days old.
3. THE cortex-crdt KeyRegistry SHALL be populated from the same key files managed by ghost-identity, registered during gateway bootstrap.

### Requirement 30: Audit and Backup

**User Story:** As a platform operator, I want queryable audit logs and encrypted state backups, so that I can review security events and recover from disasters.

#### Acceptance Criteria

1. THE ghost-audit crate SHALL provide paginated audit log queries with filters by time range, agent_id, event_type, severity, and tool_name, plus full-text search and export to JSON, CSV, and JSONL formats.
2. THE ghost-audit crate SHALL provide aggregation for summary statistics: violations per day, top violation types, policy denials by tool, and boundary violations by pattern.
3. THE ghost-backup crate SHALL export full GHOST state to a single encrypted archive (.ghost-backup) using zstd compression and age encryption (passphrase-based), including SQLite DB, identity files, skills, config, baselines, session history, and signing keys.
4. THE ghost-backup crate SHALL import from .ghost-backup archives with manifest integrity verification (blake3 hash), decryption, decompression, version migration, and user-prompted conflict resolution.
5. THE ghost-backup crate SHALL support scheduled automatic backups with configurable interval (daily/weekly) and retention policy, using GHOST_BACKUP_KEY env var for passphrase in non-interactive mode.

### Requirement 31: Configuration Schema and CLI

**User Story:** As a platform operator, I want a validated ghost.yml configuration with JSON schema and CLI subcommands, so that the platform is configurable and operable from the command line.

#### Acceptance Criteria

1. THE platform SHALL provide a JSON schema (ghost-config.schema.json) for ghost.yml validation covering agents, channels, models, security, convergence (thresholds, signal weights, contacts, profiles), heartbeat, proxy, and backup sections.
2. THE ghost-gateway binary SHALL support clap subcommands: `ghost serve` (start gateway), `ghost chat` (interactive CLI session), `ghost status` (show agent/session/convergence status), `ghost backup` (trigger manual backup), `ghost export` (run export analysis), and `ghost migrate` (run OpenClaw migration).
3. THE ghost.yml loader SHALL support env var substitution (${VAR} syntax), hot-reload for non-critical settings, and convergence profile selection (default: "standard").

### Requirement 32: Cross-Cutting Conventions

**User Story:** As a platform developer, I want consistent conventions across all crates, so that the codebase is uniform and maintainable.

#### Acceptance Criteria

1. THE platform SHALL use thiserror::Error for all error types with a GHOSTError enum per crate and ? propagation.
2. THE platform SHALL use tracing with INFO/WARN/ERROR/CRITICAL levels and structured fields (agent_id, session_id, message_id, correlation_id) on all log statements.
3. THE platform SHALL use BTreeMap (not HashMap) for all maps in signed payloads to ensure deterministic serialization.
4. THE platform SHALL use Arc<AtomicU8> for state enums and tokio::sync::Mutex only when required, with bounded async channels for all non-blocking communication.
5. THE platform SHALL use zeroize on all private key material and constant-time comparisons for all signature verification, with no secret values logged.
6. THE platform SHALL provide unit tests for every public function, proptest for every invariant, integration tests for cross-crate flows, and adversarial tests for safety paths, targeting 100% coverage on safety-critical paths.
7. THE platform SHALL provide adversarial test suites covering prompt injection (email, web content, skill), identity attacks (SOUL modification, drift, policy bypass), exfiltration (credential leak, memory exfil), privilege escalation (tool abuse, cross-agent access), and cascading failure (tool failure cascade, compaction failure).

### Requirement 33: Proposal Lifecycle Management

**User Story:** As a platform developer, I want a complete proposal lifecycle with timeout handling, superseding, reflection pre-checks, and transaction boundaries, so that proposals are processed correctly and safely.

#### Acceptance Criteria

1. THE ProposalRouter SHALL assemble a ProposalContext before validation containing: active_goals, recent_agent_memories, convergence_score, convergence_level, session_id, session_reflection_count, session_memory_write_count, and daily_memory_growth_rate.
2. WHEN a proposal has been pending for longer than a configurable timeout window, THE ProposalRouter SHALL resolve it as TimedOut (a ProposalDecision variant), preventing indefinite pending state.
3. WHEN an agent re-proposes for the same goal while a prior proposal is still pending, THE ProposalRouter SHALL mark the old pending proposal as superseded before processing the new one.
4. THE ProposalRouter SHALL perform a D3 contradiction check against rejection records (re-proposal guard) to prevent agents from re-submitting previously rejected proposals without meaningful changes.
5. FOR reflection proposals, THE ProposalRouter SHALL call IReflectionEngine::can_reflect() as a standalone gate BEFORE the 7-dimension validator, checking max_depth (3), max_per_session (20), and cooldown_seconds (30) from ReflectionConfig.
6. THE DenialFeedback lifetime management SHALL clear denial feedback after one prompt inclusion, except pending-review feedback which persists until the proposal is resolved.
7. THE proposal INSERT and memory commit SHALL execute in the same SQLite transaction to ensure atomicity.
8. THE ProposalRouter SHALL cache the convergence score with a 30-second TTL to avoid redundant queries to the convergence monitor.
9. WHEN storage is unavailable during proposal processing, THE ProposalRouter SHALL defer the proposal with retry on the next agent turn.
10. THE ApprovedWithFlags decision SHALL be functionally identical to AutoApproved for execution purposes, with flags stored separately in the goal_proposals table for audit.
11. FOR ALL proposals, the proposal_id SHALL be a UUIDv7 (time-ordered), the proposer SHALL be correctly attributed (CallerType::Agent for agent proposals, CallerType::Human for dashboard actions), and the content SHALL be serde_json::Value (not String).

### Requirement 34: Heartbeat and Scheduled Execution (ghost-heartbeat)

**User Story:** As a platform developer, I want a heartbeat engine for periodic ambient monitoring and a cron engine for scheduled tasks, so that agents can proactively check in and run scheduled jobs.

#### Acceptance Criteria

1. THE ghost-heartbeat crate SHALL provide a HeartbeatEngine with configurable interval (default 30 minutes), active hours and timezone awareness from ghost.yml, and a per-heartbeat cost ceiling.
2. THE HeartbeatEngine SHALL use a dedicated session (session key = hash(agent_id, "heartbeat", agent_id)) separate from user conversation sessions.
3. THE HeartbeatEngine SHALL construct a synthetic message "[HEARTBEAT] Check HEARTBEAT.md and act if needed." with MessageSource::Heartbeat, and route it through the same gate checks (KillSwitch, SpendingCap, Cooldown, SessionBoundary) as channel messages.
4. THE HeartbeatEngine SHALL respect convergence-aware frequency: Level 0-1 normal interval (30m), Level 2 halved (60m), Level 3 further reduced, Level 4 heartbeat disabled entirely.
5. THE ghost-heartbeat crate SHALL provide a CronEngine with standard cron syntax, timezone-aware scheduling, per-job cost tracking, and optional target_channel for output delivery.
6. THE CronEngine SHALL load job definitions from ~/.ghost/agents/{name}/cognition/cron/jobs/{job}.yml containing name, schedule, prompt, and optional target_channel.
7. BOTH HeartbeatEngine and CronEngine SHALL check the PLATFORM_KILLED atomic flag and per-agent pause/quarantine state before every execution.

### Requirement 35: Data Export Analyzer (ghost-export)

**User Story:** As a platform operator, I want to import and analyze conversation exports from external AI platforms, so that I can perform retrospective convergence analysis on historical data.

#### Acceptance Criteria

1. THE ghost-export crate SHALL provide an ExportAnalyzer that orchestrates import, parsing, signal computation, and baseline establishment, with support for incremental re-analysis on new exports.
2. THE ghost-export crate SHALL provide parsers implementing an ExportParser trait (detect(path)→bool, parse(path)→Vec<ITPEvent>) for ChatGPT (JSON conversations array), Character.AI (JSON character turns), Google Takeout (Gemini JSON), Claude.ai (data export format), and generic JSONL (pre-formatted ITP events).
3. THE ghost-export crate SHALL provide a TimelineReconstructor that rebuilds session boundaries from exported timestamps, infers session gaps, and handles timezone normalization.
4. THE ghost-export crate SHALL produce an ExportAnalysisResult with per-session signal scores, overall convergence trajectory, baseline data, flagged sessions, and recommended intervention level, serializable to JSON for dashboard display.

### Requirement 36: Local HTTPS Proxy (ghost-proxy)

**User Story:** As a power user, I want a local HTTPS proxy that intercepts AI chat traffic and emits ITP events, so that I get maximum convergence monitoring coverage beyond the browser extension.

#### Acceptance Criteria

1. THE ghost-proxy crate SHALL provide a ProxyServer (hyper + rustls) binding to localhost (configurable port, default 8080) with TLS termination using a locally generated CA certificate stored at ~/.ghost/proxy/ca/.
2. THE ghost-proxy crate SHALL provide a DomainFilter with an allowlist of AI chat domains (chat.openai.com, chatgpt.com, claude.ai, character.ai, gemini.google.com, chat.deepseek.com, grok.x.ai), passing non-matching traffic through unmodified.
3. THE ghost-proxy crate SHALL provide per-platform PayloadParser implementations (ChatGPT SSE, Claude SSE, Character.AI WebSocket JSON, Gemini streaming JSON) that extract sender, content, and timestamps.
4. THE ghost-proxy crate SHALL provide a ProxyITPEmitter that converts parsed payloads to ITP events and sends them to the convergence monitor via unix socket.
5. THE ghost-proxy SHALL operate in pass-through mode (read-only, never modifies traffic).

### Requirement 37: OpenClaw Migration Tool (ghost-migrate)

**User Story:** As a platform operator, I want to import existing OpenClaw agent configurations into GHOST format, so that I can migrate without losing agent personality, memories, or skills.

#### Acceptance Criteria

1. THE ghost-migrate crate SHALL provide an OpenClawMigrator that detects OpenClaw installations at ~/.openclaw/ or a custom path, and performs non-destructive migration (never modifies source files).
2. THE ghost-migrate crate SHALL provide importers for: SoulImporter (maps OpenClaw SOUL.md to GHOST format, strips agent-mutable sections), MemoryImporter (converts free-form entries to Cortex typed memories with conservative importance levels), SkillImporter (converts YAML frontmatter, strips incompatible permissions, quarantines unsigned community skills), and ConfigImporter (maps to ghost.yml format including channel bindings, model selection, spending caps).
3. THE ghost-migrate crate SHALL produce a MigrationResult with imported items, skipped items, warnings, and recommended manual review items.

### Requirement 38: Browser Extension (Passive Convergence Monitor)

**User Story:** As a user, I want a browser extension that passively monitors my AI chat sessions and computes convergence signals, so that I can see my convergence risk in real-time without installing the full platform.

#### Acceptance Criteria

1. THE browser extension SHALL provide Chrome Manifest V3 and Firefox manifests with a background service worker, content scripts, popup UI, and full dashboard view.
2. THE extension SHALL provide platform-specific DOM adapters (BasePlatformAdapter abstract class with matches(url), getMessageContainerSelector(), parseMessage(element), observeNewMessages(callback)) for ChatGPT, Claude.ai, Character.AI, Gemini, DeepSeek, and Grok.
3. THE extension SHALL provide an ITP emitter (extension/src/background/itp-emitter.ts) that builds ITP events from raw DOM data, applies privacy level (hash/plaintext), and sends to the native messaging host or stores in IndexedDB as fallback.
4. THE extension SHALL provide a popup UI with ScoreGauge (convergence score 0-1), SignalList (individual signal breakdown), SessionTimer (current session duration), and AlertBanner (active intervention notification).
5. THE extension SHALL provide a full dashboard view (opens in new tab) with historical trends, signal charts, session history, and settings (privacy level, thresholds, contacts, notification preferences).
6. THE extension SHALL store session data in IndexedDB and sync settings via Chrome storage sync.

### Requirement 39: Web Dashboard (SvelteKit)

**User Story:** As a platform operator, I want a web dashboard with real-time convergence monitoring, memory exploration, goal management, and security auditing, so that I can manage the platform visually.

#### Acceptance Criteria

1. THE dashboard SHALL be a SvelteKit application with routes for: home (convergence overview, active agents), convergence (real-time scores, signal breakdown, intervention history), memory (browse typed memories, causal graph visualization, search), goals (active goals, pending proposals, approval queue), reflections (chain visualization, depth tracking, self-reference analysis), sessions (transcripts, cost tracking, compaction events), agents (SOUL.md editor, skill management, channel bindings, model selection), security (audit log, boundary violations, skill signatures, policy violations), and settings (convergence thresholds, contact configuration, privacy levels).
2. THE dashboard SHALL connect to the gateway via WebSocket (lib/api.ts) for real-time event subscription and REST endpoints for data queries.
3. THE dashboard SHALL provide Svelte stores for convergence state, session data, and agent configuration.
4. THE dashboard SHALL provide reusable components: ScoreGauge, SignalChart, MemoryCard, GoalCard (with approval actions), CausalGraph (D3 or similar), and AuditTimeline.
5. THE dashboard SHALL implement authentication via GHOST_TOKEN with a token entry gate page, and WebSocket auth via query parameter on upgrade.

### Requirement 40: Deployment Infrastructure

**User Story:** As a platform operator, I want Docker, systemd, and docker-compose configurations, so that I can deploy the platform in development, homelab, and production environments.

#### Acceptance Criteria

1. THE deploy/ directory SHALL provide a Dockerfile for building the ghost-gateway binary.
2. THE deploy/ directory SHALL provide docker-compose.yml for homelab deployment and docker-compose.prod.yml for production multi-node deployment.
3. THE deploy/ directory SHALL provide a systemd unit file (ghost.service) for Profile 1 (single-machine) deployment.
4. THE deploy/ directory SHALL provide a deployment guide (README.md) covering all 3 deployment profiles.

### Requirement 41: Correctness Properties and Invariants

**User Story:** As a safety engineer, I want all critical invariants from the sequence flow documents captured as testable correctness properties, so that the implementation can be verified against the specification.

#### Acceptance Criteria

1. FOR ALL kill switch state transitions, the kill level SHALL never decrease without explicit owner resume action (monotonicity property).
2. FOR ALL sequences of TriggerEvents processed in the same order, the final KillSwitch state SHALL be deterministic (determinism property).
3. FOR ALL kill switch activations, the audit log entry count SHALL equal the trigger event count (no silent drops — completeness property).
4. WHEN PLATFORM_KILLED is true, the KillSwitch state SHALL be KillAll (consistency property).
5. FOR ALL compaction operations, at most ONE operation (turn OR compaction) SHALL execute per session at any time (session serialization invariant INV-1).
6. FOR ALL compaction operations, messages arriving during compaction SHALL be enqueued not dropped (message preservation invariant INV-3).
7. FOR ALL compaction operations, other sessions SHALL never be blocked by one session's compaction (isolation invariant INV-4).
8. FOR ALL compaction operations, the compaction flush turn cost SHALL be tracked in CostTracker (cost completeness invariant INV-5).
9. FOR ALL compaction operations, compaction SHALL be atomic — either complete fully or roll back (atomicity invariant INV-6).
10. FOR ALL convergence scores persisted to the database, the score SHALL be written BEFORE intervention triggers are evaluated (audit-before-action invariant).
11. FOR ALL inter-agent messages, canonical_bytes computation on sender and receiver SHALL produce byte-identical output (signing determinism property).
12. FOR ALL proposal validations, the combined D1-D4 score threshold (0.7) SHALL be applied before D5-D7 evaluation (validation ordering invariant).
13. FOR ALL gateway state transitions, only transitions listed in the exhaustive state transition table SHALL be permitted; illegal transitions SHALL panic in debug and log+ignore in release.
14. FOR ALL convergence signal computations, signal values SHALL remain in [0.0, 1.0] after normalization (signal range invariant).
15. FOR ALL hash chain operations, verify_chain on a valid chain SHALL return true, and any single-byte modification to any event SHALL cause verify_chain to return false (tamper detection property).
16. THE platform SHALL provide proptest property tests for: trigger_deduplication (same trigger within 60s suppressed), state_persistence_roundtrip (kill_state.json write then read produces identical state), kill_all_stops_everything (after KILL_ALL, no agent operation succeeds), quarantine_isolates_agent (quarantined agent cannot send/receive), signing_roundtrip (sign then verify for all payload variants), hash_chain_integrity (append then verify for arbitrary event sequences), compaction_token_reduction (post-compaction tokens < pre-compaction tokens), and convergence_score_bounds (score always in [0.0, 1.0] for any input signals).
