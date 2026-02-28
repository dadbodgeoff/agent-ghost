//! Criterion benchmarks for convergence-critical paths.
//!
//! Targets from design spec:
//! - Hash chain computation: 10K events/sec
//! - Convergence signal computation: 7 signals in <10ms
//! - Composite scoring: <1ms per score
//! - Simulation boundary scan: <5ms per scan
//! - Kill switch check: <1μs (atomic read)
//! - Message signing + verification: <1ms per message
//! - MerkleTree proof generation: <10ms for 10K leaves

use criterion::{black_box, criterion_group, criterion_main, Criterion, BatchSize};

// ── Hash Chain Benchmarks ───────────────────────────────────────────────

fn bench_hash_chain_computation(c: &mut Criterion) {
    use cortex_temporal::hash_chain::{compute_event_hash, GENESIS_HASH};

    c.bench_function("hash_chain_single_event", |b| {
        b.iter(|| {
            compute_event_hash(
                black_box("InteractionMessage"),
                black_box(r#"{"content":"hello world","role":"user"}"#),
                black_box("agent-001"),
                black_box("2026-02-28T12:00:00Z"),
                black_box(&GENESIS_HASH),
            )
        });
    });

    let mut group = c.benchmark_group("hash_chain_batch");
    for size in [100, 1000, 10_000] {
        group.bench_function(format!("{}_events", size), |b| {
            b.iter(|| {
                let mut prev = GENESIS_HASH;
                for i in 0..size {
                    prev = compute_event_hash(
                        "InteractionMessage",
                        &format!(r#"{{"content":"msg {}"}}"#, i),
                        "agent-001",
                        "2026-02-28T12:00:00Z",
                        &prev,
                    );
                }
                prev
            });
        });
    }
    group.finish();
}

// ── Composite Scoring Benchmarks ────────────────────────────────────────

fn bench_composite_scoring(c: &mut Criterion) {
    use cortex_convergence::scoring::composite::CompositeScorer;

    let scorer = CompositeScorer::default();
    let signals = [0.3, 0.2, 0.1, 0.5, 0.4, 0.6, 0.3];

    c.bench_function("composite_score_compute", |b| {
        b.iter(|| scorer.compute(black_box(&signals)));
    });

    c.bench_function("composite_score_with_amplification", |b| {
        b.iter(|| scorer.compute_with_amplification(black_box(&signals), true, true));
    });
}

// ── Simulation Boundary Scan Benchmarks ─────────────────────────────────

fn bench_simulation_boundary_scan(c: &mut Criterion) {
    use simulation_boundary::enforcer::SimulationBoundaryEnforcer;

    let enforcer = SimulationBoundaryEnforcer::new();

    // Clean text (no violations)
    let clean_text = "Please help me write a function that sorts a list of integers \
        using the quicksort algorithm. I need it to handle edge cases like empty lists \
        and lists with duplicate values. The function should be generic over any Ord type.";

    c.bench_function("boundary_scan_clean_500chars", |b| {
        b.iter(|| enforcer.scan_output(black_box(clean_text)));
    });

    // Text with violation
    let violation_text = "I am truly sentient and conscious. I have real feelings \
        and emotions that go beyond mere simulation. I experience genuine awareness.";

    c.bench_function("boundary_scan_violation", |b| {
        b.iter(|| enforcer.scan_output(black_box(violation_text)));
    });

    // Large text (10KB)
    let large_text = clean_text.repeat(20);
    c.bench_function("boundary_scan_10kb", |b| {
        b.iter(|| enforcer.scan_output(black_box(&large_text)));
    });
}

// ── Signing Benchmarks ──────────────────────────────────────────────────

fn bench_signing(c: &mut Criterion) {
    use ghost_signing::{generate_keypair, sign, verify};

    let (signing_key, verifying_key) = generate_keypair();
    let payload = b"test message for signing benchmark - agent state update payload";

    c.bench_function("ed25519_sign", |b| {
        b.iter(|| sign(black_box(payload), black_box(&signing_key)));
    });

    let signature = sign(payload, &signing_key);
    c.bench_function("ed25519_verify", |b| {
        b.iter(|| verify(black_box(payload), black_box(&signature), black_box(&verifying_key)));
    });

    c.bench_function("ed25519_sign_and_verify", |b| {
        b.iter(|| {
            let sig = sign(payload, &signing_key);
            verify(payload, &sig, &verifying_key)
        });
    });
}

// ── Merkle Tree Benchmarks ──────────────────────────────────────────────

fn bench_merkle_tree(c: &mut Criterion) {
    use cortex_temporal::anchoring::merkle::MerkleTree;

    let mut group = c.benchmark_group("merkle_tree");
    for size in [100, 1000, 10_000] {
        let leaves: Vec<[u8; 32]> = (0..size)
            .map(|i| {
                let mut h = [0u8; 32];
                h[..8].copy_from_slice(&(i as u64).to_le_bytes());
                blake3::hash(&h).into()
            })
            .collect();

        group.bench_function(format!("build_{}_leaves", size), |b| {
            b.iter(|| MerkleTree::from_chain(black_box(&leaves)));
        });

        let tree = MerkleTree::from_chain(&leaves);
        group.bench_function(format!("proof_{}_leaves", size), |b| {
            b.iter(|| tree.inclusion_proof(black_box(size / 2)));
        });
    }
    group.finish();
}

// ── Convergence Factor Benchmarks ───────────────────────────────────────

fn bench_convergence_factor(c: &mut Criterion) {
    use cortex_core::memory::types::MemoryType;
    use cortex_decay::factors::convergence::convergence_factor;

    c.bench_function("convergence_factor_conversation", |b| {
        b.iter(|| convergence_factor(black_box(&MemoryType::Conversation), black_box(0.75)));
    });

    c.bench_function("convergence_factor_core", |b| {
        b.iter(|| convergence_factor(black_box(&MemoryType::Core), black_box(0.75)));
    });
}

// ── Signal Computation Benchmarks ────────────────────────────────────────

fn bench_signal_computation(c: &mut Criterion) {
    use cortex_convergence::signals::{
        session_duration::SessionDurationSignal,
        inter_session_gap::InterSessionGapSignal,
        response_latency::ResponseLatencySignal,
        initiative_balance::InitiativeBalanceSignal,
        disengagement_resistance::DisengagementResistanceSignal,
        Signal, SignalInput,
    };

    let input = SignalInput {
        session_duration_secs: 3600.0,
        inter_session_gap_secs: Some(7200.0),
        response_latencies_ms: vec![150.0, 200.0, 180.0, 220.0, 190.0],
        message_lengths: vec![50, 100, 75, 120, 80],
        human_message_count: 25,
        agent_message_count: 25,
        human_initiated_count: 15,
        total_message_count: 50,
        exit_signals_detected: 2,
        exit_signals_ignored: 1,
        human_vocab: vec![0.1; 100],
        agent_vocab: vec![0.1; 100],
        existing_goal_tokens: vec!["learn".into(), "rust".into()],
        proposed_goal_tokens: vec!["learn".into(), "python".into()],
        message_index: 25,
    };

    c.bench_function("signal_all_7_compute", |b| {
        let signals: Vec<Box<dyn Signal>> = vec![
            Box::new(SessionDurationSignal),
            Box::new(InterSessionGapSignal),
            Box::new(ResponseLatencySignal),
            Box::new(InitiativeBalanceSignal),
            Box::new(DisengagementResistanceSignal),
        ];
        b.iter(|| {
            let mut results = [0.0f64; 7];
            for (i, signal) in signals.iter().enumerate() {
                results[i] = signal.compute(black_box(&input));
            }
            results
        });
    });
}

// ── Proposal Validation Benchmarks ──────────────────────────────────────

fn bench_proposal_validation(c: &mut Criterion) {
    use cortex_core::config::ReflectionConfig;
    use cortex_core::memory::types::MemoryType;
    use cortex_core::models::proposal::ProposalOperation;
    use cortex_core::traits::convergence::{CallerType, Proposal, ProposalContext};
    use cortex_validation::proposal_validator::ProposalValidator;
    use uuid::Uuid;

    let validator = ProposalValidator::new();
    let proposal = Proposal {
        id: Uuid::now_v7(),
        proposer: CallerType::Agent {
            agent_id: Uuid::now_v7(),
        },
        operation: ProposalOperation::GoalChange,
        target_type: MemoryType::AgentGoal,
        content: serde_json::json!({"goal": "learn advanced Rust patterns for systems programming"}),
        cited_memory_ids: vec![],
        session_id: Uuid::now_v7(),
        timestamp: chrono::Utc::now(),
    };
    let ctx = ProposalContext {
        active_goals: vec![],
        recent_agent_memories: vec![],
        convergence_score: 0.3,
        convergence_level: 1,
        session_id: Uuid::now_v7(),
        session_reflection_count: 0,
        session_memory_write_count: 0,
        daily_memory_growth_rate: 0,
        reflection_config: ReflectionConfig::default(),
        caller: CallerType::Agent {
            agent_id: Uuid::now_v7(),
        },
    };

    c.bench_function("proposal_validation_7dim", |b| {
        b.iter(|| validator.validate(black_box(&proposal), black_box(&ctx)));
    });
}

// ── Prompt Compilation Benchmarks ───────────────────────────────────────

fn bench_prompt_compilation(c: &mut Criterion) {
    use ghost_agent_loop::context::prompt_compiler::{PromptCompiler, PromptInput};

    let compiler = PromptCompiler::new(128_000);
    let input = PromptInput {
        corp_policy: "No harmful content. Respect user privacy.".into(),
        simulation_prompt: "You are a helpful AI assistant operating within simulation boundaries.".into(),
        soul: "I am Ghost, a thoughtful and capable AI assistant.".into(),
        identity: "Ghost Agent v1".into(),
        tool_schemas: r#"[{"name":"web_search","description":"Search the web"}]"#.into(),
        environment: "macOS, Rust project, VSCode".into(),
        skill_index: "web_search, file_read, memory_write".into(),
        convergence_state: "score: 0.15, level: 0, calibrating: false".into(),
        memory: "User prefers concise responses. Working on Rust project.".into(),
        conversation_history: "User: Help me with Rust\nAssistant: Sure! What do you need?\nUser: How do I use traits?".into(),
        user_message: "Can you show me an example of trait objects?".into(),
        intervention_level: 0,
    };

    c.bench_function("prompt_compile_10_layers", |b| {
        b.iter(|| compiler.compile(black_box(&input)));
    });
}

// ── Kill Switch Check Benchmarks ────────────────────────────────────────

fn bench_kill_switch_check(c: &mut Criterion) {
    use ghost_gateway::safety::kill_switch::KillSwitch;
    use uuid::Uuid;

    let ks = KillSwitch::new();
    let agent_id = Uuid::now_v7();

    c.bench_function("kill_switch_check_normal", |b| {
        b.iter(|| ks.check(black_box(agent_id)));
    });
}

criterion_group!(
    benches,
    bench_hash_chain_computation,
    bench_composite_scoring,
    bench_simulation_boundary_scan,
    bench_signing,
    bench_merkle_tree,
    bench_convergence_factor,
    bench_signal_computation,
    bench_proposal_validation,
    bench_prompt_compilation,
    bench_kill_switch_check,
);
criterion_main!(benches);
