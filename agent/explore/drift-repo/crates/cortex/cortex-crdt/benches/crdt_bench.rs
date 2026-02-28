//! Criterion benchmarks for cortex-crdt.
//!
//! Targets (from QG-MA0):
//! - GCounter merge (5 agents) < 0.01ms
//! - LWWRegister merge < 0.001ms
//! - ORSet merge (100 elements) < 0.1ms
//! - ORSet merge (1000 elements) < 1ms
//! - MaxRegister merge < 0.001ms
//! - VectorClock merge (20 agents) < 0.01ms
//! - MemoryCRDT full merge < 0.5ms
//! - Delta computation (50 changed fields) < 0.2ms
//! - DAG CRDT merge (500 edges) < 5ms
//! - DAG CRDT cycle detection (1K edges) < 10ms

use chrono::Utc;
use criterion::{criterion_group, criterion_main, Criterion};

use cortex_crdt::{
    CausalGraphCRDT, GCounter, LWWRegister, MaxRegister, MemoryCRDT, MergeEngine, ORSet,
    VectorClock,
};
use cortex_core::memory::base::{BaseMemory, TypedContent};
use cortex_core::memory::confidence::Confidence;
use cortex_core::memory::importance::Importance;
use cortex_core::memory::types::MemoryType;
use cortex_core::models::agent::AgentId;
use cortex_core::models::namespace::NamespaceId;

/// Helper: create a minimal BaseMemory.
fn make_bench_memory(id: &str) -> BaseMemory {
    let content = TypedContent::Core(cortex_core::memory::types::CoreContent {
        project_name: "bench".to_string(),
        description: format!("Bench memory {id}"),
        metadata: serde_json::Value::Null,
    });
    let content_hash =
        BaseMemory::compute_content_hash(&content).unwrap_or_else(|_| "hash".to_string());

    BaseMemory {
        id: id.to_string(),
        memory_type: MemoryType::Core,
        content,
        summary: format!("Summary {id}"),
        transaction_time: Utc::now(),
        valid_time: Utc::now(),
        valid_until: None,
        confidence: Confidence::new(0.8),
        importance: Importance::Normal,
        last_accessed: Utc::now(),
        access_count: 5,
        linked_patterns: Vec::new(),
        linked_constraints: Vec::new(),
        linked_files: Vec::new(),
        linked_functions: Vec::new(),
        tags: vec!["bench".to_string(), "test".to_string()],
        archived: false,
        superseded_by: None,
        supersedes: None,
        content_hash,
        namespace: NamespaceId::default(),
        source_agent: AgentId::default(),
    }
}

// TMA-BENCH-01: GCounter merge (5 agents) < 0.01ms
fn bench_gcounter_merge(c: &mut Criterion) {
    let mut a = GCounter::new();
    let mut b = GCounter::new();
    for i in 0..5 {
        for _ in 0..100 {
            a.increment(&format!("agent-{i}"));
            b.increment(&format!("agent-{i}"));
        }
    }

    c.bench_function("gcounter_merge_5_agents", |bench| {
        bench.iter(|| {
            let mut local = a.clone();
            local.merge(&b);
        });
    });
}

// TMA-BENCH-02: LWWRegister merge < 0.001ms
fn bench_lww_register_merge(c: &mut Criterion) {
    let now = Utc::now();
    let a = LWWRegister::new("value-a".to_string(), now, "agent-a".to_string());
    let b = LWWRegister::new(
        "value-b".to_string(),
        now + chrono::Duration::seconds(1),
        "agent-b".to_string(),
    );

    c.bench_function("lww_register_merge", |bench| {
        bench.iter(|| {
            let mut local = a.clone();
            local.merge(&b);
        });
    });
}

// TMA-BENCH-03: ORSet merge (100 elements) < 0.1ms
fn bench_or_set_merge_100(c: &mut Criterion) {
    let mut a = ORSet::new();
    let mut b = ORSet::new();
    for i in 0..100 {
        a.add(format!("elem-{i}"), "agent-a", i as u64);
        b.add(format!("elem-{}", i + 50), "agent-b", i as u64);
    }

    c.bench_function("or_set_merge_100_elements", |bench| {
        bench.iter(|| {
            let mut local = a.clone();
            local.merge(&b);
        });
    });
}

// TMA-BENCH-04: ORSet merge (1000 elements) < 1ms
fn bench_or_set_merge_1000(c: &mut Criterion) {
    let mut a = ORSet::new();
    let mut b = ORSet::new();
    for i in 0..1000 {
        a.add(format!("elem-{i}"), "agent-a", i as u64);
        b.add(format!("elem-{}", i + 500), "agent-b", i as u64);
    }

    c.bench_function("or_set_merge_1000_elements", |bench| {
        bench.iter(|| {
            let mut local = a.clone();
            local.merge(&b);
        });
    });
}

// TMA-BENCH-05: MaxRegister merge < 0.001ms
fn bench_max_register_merge(c: &mut Criterion) {
    let now = Utc::now();
    let a = MaxRegister::new(0.5_f64, now);
    let b = MaxRegister::new(0.8_f64, now);

    c.bench_function("max_register_merge", |bench| {
        bench.iter(|| {
            let mut local = a.clone();
            local.merge(&b);
        });
    });
}

// TMA-BENCH-06: VectorClock merge (20 agents) < 0.01ms
fn bench_vector_clock_merge(c: &mut Criterion) {
    let mut a = VectorClock::new();
    let mut b = VectorClock::new();
    for i in 0..20 {
        for _ in 0..10 {
            a.increment(&format!("agent-{i}"));
            b.increment(&format!("agent-{i}"));
        }
    }

    c.bench_function("vector_clock_merge_20_agents", |bench| {
        bench.iter(|| {
            let mut local = a.clone();
            local.merge(&b);
        });
    });
}

// TMA-BENCH-07: MemoryCRDT full merge < 0.5ms
fn bench_memory_crdt_merge(c: &mut Criterion) {
    let memory = make_bench_memory("bench-001");
    let crdt_a = MemoryCRDT::from_base_memory(&memory, "agent-a");
    let crdt_b = MemoryCRDT::from_base_memory(&memory, "agent-b");

    c.bench_function("memory_crdt_full_merge", |bench| {
        bench.iter(|| {
            let mut local = crdt_a.clone();
            local.merge(&crdt_b);
        });
    });
}

// TMA-BENCH-08: Delta computation < 0.2ms
fn bench_delta_computation(c: &mut Criterion) {
    let memory = make_bench_memory("bench-002");
    let crdt = MemoryCRDT::from_base_memory(&memory, "agent-1");
    let remote_clock = VectorClock::new();

    c.bench_function("delta_computation", |bench| {
        bench.iter(|| {
            MergeEngine::compute_delta(&crdt, &remote_clock, "agent-1");
        });
    });
}

// TMA-BENCH-09: DAG CRDT merge (500 edges) < 5ms
fn bench_dag_crdt_merge_500(c: &mut Criterion) {
    // Build two DAGs with ~250 edges each (forward-only to avoid cycles)
    let mut graph_a = CausalGraphCRDT::new();
    let mut graph_b = CausalGraphCRDT::new();

    let n = 100;
    let mut seq_a = 0u64;
    let mut seq_b = 0u64;
    for i in 0..n {
        for j in (i + 1)..(i + 6).min(n) {
            let src = format!("n{i}");
            let tgt = format!("n{j}");
            if seq_a < 250 {
                let _ = graph_a.add_edge(&src, &tgt, 0.7, "agent-a", seq_a + 1);
                seq_a += 1;
            }
            if seq_b < 250 {
                let _ = graph_b.add_edge(&src, &tgt, 0.6, "agent-b", seq_b + 1);
                seq_b += 1;
            }
        }
    }

    c.bench_function("dag_crdt_merge_500_edges", |bench| {
        bench.iter(|| {
            let mut local = graph_a.clone();
            let _ = local.merge(&graph_b);
        });
    });
}

// TMA-BENCH-10: DAG CRDT cycle detection (1K edges) < 10ms
fn bench_dag_crdt_cycle_detection(c: &mut Criterion) {
    let mut graph = CausalGraphCRDT::new();
    let n = 200;
    let mut seq = 0u64;
    for i in 0..n {
        for j in (i + 1)..(i + 6).min(n) {
            let src = format!("n{i}");
            let tgt = format!("n{j}");
            let _ = graph.add_edge(&src, &tgt, 0.7, "agent-1", seq + 1);
            seq += 1;
            if seq >= 1000 {
                break;
            }
        }
        if seq >= 1000 {
            break;
        }
    }

    c.bench_function("dag_crdt_cycle_detection_1k_edges", |bench| {
        bench.iter(|| {
            graph.detect_cycle();
        });
    });
}

criterion_group!(
    benches,
    bench_gcounter_merge,
    bench_lww_register_merge,
    bench_or_set_merge_100,
    bench_or_set_merge_1000,
    bench_max_register_merge,
    bench_vector_clock_merge,
    bench_memory_crdt_merge,
    bench_delta_computation,
    bench_dag_crdt_merge_500,
    bench_dag_crdt_cycle_detection,
);
criterion_main!(benches);
