use chrono::{Duration, Utc};
use cortex_core::memory::*;
use cortex_decay::{DecayContext, DecayEngine};
use criterion::{criterion_group, criterion_main, Criterion};

fn make_memories(count: usize) -> Vec<BaseMemory> {
    let now = Utc::now();
    (0..count)
        .map(|i| BaseMemory {
            id: format!("mem-{i:04}"),
            memory_type: MemoryType::Tribal,
            content: TypedContent::Tribal(cortex_core::memory::types::TribalContent {
                knowledge: "Test knowledge".to_string(),
                severity: "medium".to_string(),
                warnings: vec![],
                consequences: vec![],
            }),
            summary: "Test".to_string(),
            transaction_time: now,
            valid_time: now,
            valid_until: None,
            confidence: Confidence::new(0.8),
            importance: Importance::Normal,
            last_accessed: now - Duration::days((i % 90) as i64),
            access_count: (i * 3) as u64,
            linked_patterns: vec![],
            linked_constraints: vec![],
            linked_files: vec![],
            linked_functions: vec![],
            tags: vec![],
            archived: false,
            superseded_by: None,
            supersedes: None,
            content_hash: format!("hash-{i}"),
            namespace: Default::default(),
            source_agent: Default::default(),
        })
        .collect()
}

fn decay_benchmarks(c: &mut Criterion) {
    let engine = DecayEngine::new();
    let ctx = DecayContext::default();

    // T4-DEC-11: 1K memories decay < 1ms
    let memories_1k = make_memories(1000);
    c.bench_function("decay_1k_memories", |b| {
        b.iter(|| engine.process_batch(&memories_1k, &ctx))
    });

    let memories_10k = make_memories(10_000);
    c.bench_function("decay_10k_memories", |b| {
        b.iter(|| engine.process_batch(&memories_10k, &ctx))
    });
}

criterion_group!(benches, decay_benchmarks);
criterion_main!(benches);
