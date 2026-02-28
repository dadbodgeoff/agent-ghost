//! CausalGraphCRDT tests.
//!
//! Tests TMA-CRDT-27 through TMA-CRDT-31.

use cortex_crdt::CausalGraphCRDT;

// =============================================================================
// TMA-CRDT-27: Add edge → edge present
// =============================================================================

#[test]
fn tma_crdt_27_add_edge_present() {
    let mut graph = CausalGraphCRDT::new();
    graph.add_edge("A", "B", 0.8, "agent-1", 1).unwrap();

    let edges = graph.edges();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].source, "A");
    assert_eq!(edges[0].target, "B");
}

// =============================================================================
// TMA-CRDT-28: Self-loop rejected
// =============================================================================

#[test]
fn tma_crdt_28_self_loop_rejected() {
    let mut graph = CausalGraphCRDT::new();
    let result = graph.add_edge("A", "A", 0.5, "agent-1", 1);
    assert!(result.is_err());
}

// =============================================================================
// TMA-CRDT-29: Multi-hop cycle rejected
// =============================================================================

#[test]
fn tma_crdt_29_multi_hop_cycle_rejected() {
    let mut graph = CausalGraphCRDT::new();
    graph.add_edge("A", "B", 0.8, "agent-1", 1).unwrap();
    graph.add_edge("B", "C", 0.6, "agent-1", 2).unwrap();

    // C→A would create A→B→C→A cycle
    let result = graph.add_edge("C", "A", 0.5, "agent-1", 3);
    assert!(result.is_err());
}

// =============================================================================
// TMA-CRDT-30: Merge-introduced cycle resolved
// =============================================================================

#[test]
fn tma_crdt_30_merge_introduced_cycle_resolved() {
    // Agent 1 adds A→B
    let mut graph_1 = CausalGraphCRDT::new();
    graph_1.add_edge("A", "B", 0.8, "agent-1", 1).unwrap();

    // Agent 2 adds B→A (independently, no cycle in their local view)
    let mut graph_2 = CausalGraphCRDT::new();
    graph_2.add_edge("B", "A", 0.3, "agent-2", 1).unwrap();

    // Merge: introduces A→B + B→A cycle
    // resolve_cycles() should remove the weakest edge (B→A at 0.3)
    graph_1.merge(&graph_2).unwrap();

    // Graph should be acyclic
    assert!(graph_1.detect_cycle().is_none());

    // The stronger edge (A→B at 0.8) should survive
    let edges = graph_1.edges();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].source, "A");
    assert_eq!(edges[0].target, "B");
}

// =============================================================================
// TMA-CRDT-31: Strength max-wins
// =============================================================================

#[test]
fn tma_crdt_31_strength_max_wins() {
    let mut graph_1 = CausalGraphCRDT::new();
    graph_1.add_edge("A", "B", 0.5, "agent-1", 1).unwrap();

    let mut graph_2 = CausalGraphCRDT::new();
    graph_2.add_edge("A", "B", 0.9, "agent-2", 1).unwrap();

    graph_1.merge(&graph_2).unwrap();

    let strength = graph_1.get_strength("A", "B").unwrap();
    assert!((strength - 0.9).abs() < f64::EPSILON);
}

// =============================================================================
// Additional DAG CRDT tests
// =============================================================================

#[test]
fn dag_crdt_remove_edge() {
    let mut graph = CausalGraphCRDT::new();
    graph.add_edge("A", "B", 0.8, "agent-1", 1).unwrap();
    assert_eq!(graph.edge_count(), 1);

    graph.remove_edge("A", "B");
    assert_eq!(graph.edge_count(), 0);
}

#[test]
fn dag_crdt_update_strength_only_increases() {
    let mut graph = CausalGraphCRDT::new();
    graph.add_edge("A", "B", 0.8, "agent-1", 1).unwrap();

    // Try to decrease — should be ignored
    graph.update_strength("A", "B", 0.3);
    let strength = graph.get_strength("A", "B").unwrap();
    assert!((strength - 0.8).abs() < f64::EPSILON);

    // Increase — should succeed
    graph.update_strength("A", "B", 0.95);
    let strength = graph.get_strength("A", "B").unwrap();
    assert!((strength - 0.95).abs() < f64::EPSILON);
}

#[test]
fn dag_crdt_merge_commutativity() {
    let mut graph_1 = CausalGraphCRDT::new();
    graph_1.add_edge("A", "B", 0.8, "agent-1", 1).unwrap();
    graph_1.add_edge("B", "C", 0.6, "agent-1", 2).unwrap();

    let mut graph_2 = CausalGraphCRDT::new();
    graph_2.add_edge("C", "D", 0.7, "agent-2", 1).unwrap();
    graph_2.add_edge("D", "E", 0.5, "agent-2", 2).unwrap();

    let mut g12 = graph_1.clone();
    g12.merge(&graph_2).unwrap();

    let mut g21 = graph_2.clone();
    g21.merge(&graph_1).unwrap();

    // Both should have the same edges
    assert_eq!(g12.edge_count(), g21.edge_count());
    assert_eq!(g12.edge_count(), 4);
}

#[test]
fn dag_crdt_complex_cycle_resolution() {
    // Create a more complex scenario: A→B, B→C, C→D
    let mut graph_1 = CausalGraphCRDT::new();
    graph_1.add_edge("A", "B", 0.9, "agent-1", 1).unwrap();
    graph_1.add_edge("B", "C", 0.7, "agent-1", 2).unwrap();
    graph_1.add_edge("C", "D", 0.5, "agent-1", 3).unwrap();

    // Agent 2 adds D→A (creates cycle A→B→C→D→A)
    let mut graph_2 = CausalGraphCRDT::new();
    graph_2.add_edge("D", "A", 0.3, "agent-2", 1).unwrap();

    graph_1.merge(&graph_2).unwrap();

    // Should be acyclic after resolution
    assert!(graph_1.detect_cycle().is_none());

    // The weakest edge (D→A at 0.3) should have been removed
    assert!(graph_1.get_strength("D", "A").is_none()
        || !graph_1.edges().iter().any(|e| e.source == "D" && e.target == "A"));
}

#[test]
fn dag_crdt_nodes_tracking() {
    let mut graph = CausalGraphCRDT::new();
    graph.add_edge("A", "B", 0.8, "agent-1", 1).unwrap();
    graph.add_edge("B", "C", 0.6, "agent-1", 2).unwrap();

    let nodes = graph.nodes();
    assert_eq!(nodes.len(), 3);
    assert!(nodes.contains("A"));
    assert!(nodes.contains("B"));
    assert!(nodes.contains("C"));
}
