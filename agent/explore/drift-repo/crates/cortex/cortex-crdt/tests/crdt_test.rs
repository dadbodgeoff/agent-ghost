//! Unit tests for all CRDT primitives.
//!
//! Covers: VectorClock, GCounter, LWWRegister, MVRegister, ORSet, MaxRegister.
//! Tests TMA-CRDT-01 through TMA-CRDT-22.

use chrono::{Duration, Utc};
use cortex_crdt::{GCounter, LWWRegister, MVRegister, MaxRegister, ORSet, VectorClock};

// =============================================================================
// VectorClock tests (TMA-CRDT-19 through TMA-CRDT-22)
// =============================================================================

#[test]
fn tma_crdt_19_vector_clock_increment() {
    let mut clock = VectorClock::new();
    assert_eq!(clock.get("agent-1"), 0);

    clock.increment("agent-1");
    assert_eq!(clock.get("agent-1"), 1);

    clock.increment("agent-1");
    assert_eq!(clock.get("agent-1"), 2);

    clock.increment("agent-2");
    assert_eq!(clock.get("agent-2"), 1);
    assert_eq!(clock.get("agent-1"), 2);
}

#[test]
fn tma_crdt_20_vector_clock_merge_component_wise_max() {
    let mut a = VectorClock::new();
    a.increment("agent-1");
    a.increment("agent-1");
    a.increment("agent-2");

    let mut b = VectorClock::new();
    b.increment("agent-1");
    b.increment("agent-2");
    b.increment("agent-2");
    b.increment("agent-3");

    a.merge(&b);
    assert_eq!(a.get("agent-1"), 2); // max(2, 1)
    assert_eq!(a.get("agent-2"), 2); // max(1, 2)
    assert_eq!(a.get("agent-3"), 1); // max(0, 1)
}

#[test]
fn tma_crdt_21_vector_clock_happens_before() {
    let mut a = VectorClock::new();
    a.increment("agent-1");

    let mut b = VectorClock::new();
    b.increment("agent-1");
    b.increment("agent-1");
    b.increment("agent-2");

    // a < b: all a entries ≤ b entries, at least one strictly less
    assert!(a.happens_before(&b));
    assert!(!b.happens_before(&a));

    // a does not happen before itself
    assert!(!a.happens_before(&a));
}

#[test]
fn tma_crdt_22_vector_clock_concurrent() {
    let mut a = VectorClock::new();
    a.increment("agent-1");

    let mut b = VectorClock::new();
    b.increment("agent-2");

    // Neither happens before the other
    assert!(a.concurrent_with(&b));
    assert!(b.concurrent_with(&a));

    // Not concurrent with self
    assert!(!a.concurrent_with(&a));
}

#[test]
fn vector_clock_dominates() {
    let mut a = VectorClock::new();
    a.increment("agent-1");
    a.increment("agent-1");
    a.increment("agent-2");

    let mut b = VectorClock::new();
    b.increment("agent-1");

    assert!(a.dominates(&b));
    assert!(!b.dominates(&a));
}

#[test]
fn vector_clock_merge_commutativity() {
    let mut a = VectorClock::new();
    a.increment("agent-1");
    a.increment("agent-1");

    let mut b = VectorClock::new();
    b.increment("agent-2");

    let mut ab = a.clone();
    ab.merge(&b);

    let mut ba = b.clone();
    ba.merge(&a);

    assert_eq!(ab, ba);
}

#[test]
fn vector_clock_merge_idempotency() {
    let mut a = VectorClock::new();
    a.increment("agent-1");
    a.increment("agent-2");

    let before = a.clone();
    a.merge(&before);
    assert_eq!(a, before);
}

// =============================================================================
// GCounter tests (TMA-CRDT-01 through TMA-CRDT-04)
// =============================================================================

#[test]
fn tma_crdt_01_gcounter_increment_and_value() {
    let mut counter = GCounter::new();
    counter.increment("agent-1");
    counter.increment("agent-1");
    counter.increment("agent-2");

    assert_eq!(counter.value(), 3); // 2 + 1
    assert_eq!(counter.agent_value("agent-1"), 2);
    assert_eq!(counter.agent_value("agent-2"), 1);
    assert_eq!(counter.agent_value("agent-3"), 0);
}

#[test]
fn tma_crdt_02_gcounter_merge_commutativity() {
    let mut a = GCounter::new();
    a.increment("agent-1");
    a.increment("agent-1");

    let mut b = GCounter::new();
    b.increment("agent-2");
    b.increment("agent-2");
    b.increment("agent-2");

    let mut ab = a.clone();
    ab.merge(&b);

    let mut ba = b.clone();
    ba.merge(&a);

    assert_eq!(ab, ba);
    assert_eq!(ab.value(), 5);
}

#[test]
fn tma_crdt_03_gcounter_merge_associativity() {
    let mut a = GCounter::new();
    a.increment("agent-1");

    let mut b = GCounter::new();
    b.increment("agent-2");

    let mut c = GCounter::new();
    c.increment("agent-3");

    // merge(A, merge(B, C))
    let mut bc = b.clone();
    bc.merge(&c);
    let mut a_bc = a.clone();
    a_bc.merge(&bc);

    // merge(merge(A, B), C)
    let mut ab = a.clone();
    ab.merge(&b);
    ab.merge(&c);

    assert_eq!(a_bc, ab);
}

#[test]
fn tma_crdt_04_gcounter_merge_idempotency() {
    let mut a = GCounter::new();
    a.increment("agent-1");
    a.increment("agent-2");

    let before = a.clone();
    a.merge(&before);
    assert_eq!(a, before);
}

// =============================================================================
// LWWRegister tests (TMA-CRDT-05 through TMA-CRDT-08)
// =============================================================================

#[test]
fn tma_crdt_05_lww_register_set_and_get() {
    let now = Utc::now();
    let reg = LWWRegister::new("hello".to_string(), now, "agent-1".to_string());
    assert_eq!(reg.get(), "hello");
    assert_eq!(reg.agent_id(), "agent-1");
}

#[test]
fn tma_crdt_06_lww_register_merge_keeps_newer() {
    let t1 = Utc::now();
    let t2 = t1 + Duration::seconds(1);

    let mut a = LWWRegister::new("old".to_string(), t1, "agent-1".to_string());
    let b = LWWRegister::new("new".to_string(), t2, "agent-2".to_string());

    a.merge(&b);
    assert_eq!(a.get(), "new");
}

#[test]
fn tma_crdt_07_lww_register_tie_break_by_agent_id() {
    let t = Utc::now();

    let mut a = LWWRegister::new("from-a".to_string(), t, "agent-a".to_string());
    let b = LWWRegister::new("from-b".to_string(), t, "agent-b".to_string());

    // Same timestamp — lexicographically greater agent_id wins
    a.merge(&b);
    assert_eq!(a.get(), "from-b"); // "agent-b" > "agent-a"
}

#[test]
fn tma_crdt_08_lww_register_merge_commutativity() {
    let t1 = Utc::now();
    let t2 = t1 + Duration::seconds(1);

    let a = LWWRegister::new("old".to_string(), t1, "agent-1".to_string());
    let b = LWWRegister::new("new".to_string(), t2, "agent-2".to_string());

    let mut ab = a.clone();
    ab.merge(&b);

    let mut ba = b.clone();
    ba.merge(&a);

    assert_eq!(ab, ba);
}

// =============================================================================
// MVRegister tests (TMA-CRDT-09 through TMA-CRDT-11)
// =============================================================================

#[test]
fn tma_crdt_09_mv_register_concurrent_values() {
    let mut reg = MVRegister::new();

    let mut clock_a = VectorClock::new();
    clock_a.increment("agent-a");
    reg.set("value-a".to_string(), &clock_a);

    let mut reg_b = MVRegister::new();
    let mut clock_b = VectorClock::new();
    clock_b.increment("agent-b");
    reg_b.set("value-b".to_string(), &clock_b);

    reg.merge(&reg_b);

    let values = reg.get();
    assert_eq!(values.len(), 2);
    assert!(values.contains(&&"value-a".to_string()));
    assert!(values.contains(&&"value-b".to_string()));
}

#[test]
fn tma_crdt_10_mv_register_is_conflicted() {
    let mut reg = MVRegister::new();

    let mut clock_a = VectorClock::new();
    clock_a.increment("agent-a");
    reg.set("value-a".to_string(), &clock_a);

    assert!(!reg.is_conflicted()); // Single value

    let mut reg_b = MVRegister::new();
    let mut clock_b = VectorClock::new();
    clock_b.increment("agent-b");
    reg_b.set("value-b".to_string(), &clock_b);

    reg.merge(&reg_b);
    assert!(reg.is_conflicted()); // Two concurrent values
}

#[test]
fn tma_crdt_11_mv_register_resolve_collapses() {
    let mut reg = MVRegister::new();

    let mut clock_a = VectorClock::new();
    clock_a.increment("agent-a");
    reg.set("value-a".to_string(), &clock_a);

    let mut reg_b = MVRegister::new();
    let mut clock_b = VectorClock::new();
    clock_b.increment("agent-b");
    reg_b.set("value-b".to_string(), &clock_b);

    reg.merge(&reg_b);
    assert!(reg.is_conflicted());

    reg.resolve("resolved".to_string());
    assert!(!reg.is_conflicted());
    assert_eq!(reg.get(), vec![&"resolved".to_string()]);
}

// =============================================================================
// ORSet tests (TMA-CRDT-12 through TMA-CRDT-16)
// =============================================================================

#[test]
fn tma_crdt_12_or_set_add_and_contains() {
    let mut set = ORSet::new();
    set.add("hello".to_string(), "agent-1", 1);
    assert!(set.contains(&"hello".to_string()));
    assert!(!set.contains(&"world".to_string()));
}

#[test]
fn tma_crdt_13_or_set_remove_and_contains() {
    let mut set = ORSet::new();
    set.add("hello".to_string(), "agent-1", 1);
    assert!(set.contains(&"hello".to_string()));

    set.remove(&"hello".to_string());
    assert!(!set.contains(&"hello".to_string()));
}

#[test]
fn tma_crdt_14_or_set_add_wins_semantics() {
    // Agent A adds "tag" (tag-1)
    let mut set_a = ORSet::new();
    set_a.add("tag".to_string(), "agent-a", 1);

    // Agent B starts from same state, removes "tag" (tombstones tag-1)
    let mut set_b = set_a.clone();
    set_b.remove(&"tag".to_string());

    // Agent A concurrently adds "tag" again (tag-2)
    set_a.add("tag".to_string(), "agent-a", 2);

    // Merge: tag-2 is NOT tombstoned → element is present (add-wins)
    set_a.merge(&set_b);
    assert!(set_a.contains(&"tag".to_string()));
}

#[test]
fn tma_crdt_15_or_set_merge_commutativity() {
    let mut a = ORSet::new();
    a.add("x".to_string(), "agent-1", 1);
    a.add("y".to_string(), "agent-1", 2);

    let mut b = ORSet::new();
    b.add("y".to_string(), "agent-2", 1);
    b.add("z".to_string(), "agent-2", 2);

    let mut ab = a.clone();
    ab.merge(&b);

    let mut ba = b.clone();
    ba.merge(&a);

    assert_eq!(ab, ba);
}

#[test]
fn tma_crdt_16_or_set_size_bounded() {
    let mut set = ORSet::new();
    set.add("a".to_string(), "agent-1", 1);
    set.add("b".to_string(), "agent-1", 2);
    set.add("c".to_string(), "agent-1", 3);

    // Remove one
    set.remove(&"b".to_string());

    // Size should be ≤ unique adds (3), and specifically 2 after remove
    assert!(set.len() <= 3);
    assert_eq!(set.len(), 2);
}

// =============================================================================
// MaxRegister tests (TMA-CRDT-17 through TMA-CRDT-18)
// =============================================================================

#[test]
fn tma_crdt_17_max_register_only_up() {
    let now = Utc::now();
    let mut reg = MaxRegister::new(0.8_f64, now);

    // Try to set a lower value — should be ignored
    reg.set(0.5);
    assert!((*reg.get() - 0.8).abs() < f64::EPSILON);

    // Set a higher value — should succeed
    reg.set(0.9);
    assert!((*reg.get() - 0.9).abs() < f64::EPSILON);
}

#[test]
fn tma_crdt_18_max_register_merge_keeps_max() {
    let now = Utc::now();
    let mut a = MaxRegister::new(0.5_f64, now);
    let b = MaxRegister::new(0.8_f64, now);

    a.merge(&b);
    assert!((*a.get() - 0.8).abs() < f64::EPSILON);

    // Merge the other way
    let mut c = MaxRegister::new(0.8_f64, now);
    let d = MaxRegister::new(0.5_f64, now);
    c.merge(&d);
    assert!((*c.get() - 0.8).abs() < f64::EPSILON);
}

#[test]
fn max_register_merge_commutativity() {
    let now = Utc::now();
    let a = MaxRegister::new(0.3_f64, now);
    let b = MaxRegister::new(0.7_f64, now);

    let mut ab = a.clone();
    ab.merge(&b);

    let mut ba = b.clone();
    ba.merge(&a);

    assert_eq!(ab, ba);
}

// =============================================================================
// GCounter delta tests
// =============================================================================

#[test]
fn gcounter_delta_since() {
    let mut a = GCounter::new();
    a.increment("agent-1");
    a.increment("agent-1");
    a.increment("agent-2");

    let mut b = GCounter::new();
    b.increment("agent-1");

    let delta = a.delta_since(&b);
    assert_eq!(*delta.counts.get("agent-1").unwrap(), 2);
    assert_eq!(*delta.counts.get("agent-2").unwrap(), 1);
}

// =============================================================================
// LWWRegister set semantics
// =============================================================================

#[test]
fn lww_register_set_ignores_older() {
    let t1 = Utc::now();
    let t2 = t1 + Duration::seconds(1);

    let mut reg = LWWRegister::new("new".to_string(), t2, "agent-1".to_string());
    reg.set("old".to_string(), t1, "agent-2".to_string());

    // Older write should be ignored
    assert_eq!(reg.get(), "new");
}
