//! Causal Graph CRDT — a novel DAG CRDT with cycle prevention.
//!
//! Maintains the DAG invariant across concurrent modifications from multiple
//! agents. Resolves merge-introduced cycles deterministically by removing
//! the weakest edge (lowest MaxRegister strength).
//!
//! # Design
//!
//! - Edges stored in an `ORSet<CausalEdge>` (add-wins semantics)
//! - Edge strengths stored in `HashMap<(String, String), MaxRegister<f64>>`
//! - Cycle prevention: local check before add, global resolution after merge
//! - Deterministic cycle resolution: weakest edge removed; ties broken by
//!   lexicographically smaller `(source, target)` pair
//!
//! # Examples
//!
//! ```
//! use cortex_crdt::CausalGraphCRDT;
//!
//! let mut graph = CausalGraphCRDT::new();
//! graph.add_edge("A", "B", 0.8, "agent-1", 1).unwrap();
//! graph.add_edge("B", "C", 0.6, "agent-1", 2).unwrap();
//!
//! // This would create a cycle A→B→C→A, so it's rejected:
//! assert!(graph.add_edge("C", "A", 0.5, "agent-1", 3).is_err());
//! ```

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::primitives::max_register::MaxRegister;
use crate::primitives::or_set::ORSet;
use cortex_core::errors::{CortexError, CortexResult, MultiAgentError};

/// A directed edge in the causal graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CausalEdge {
    /// Source memory ID.
    pub source: String,
    /// Target memory ID.
    pub target: String,
}

/// A CRDT for directed acyclic graphs with cycle prevention.
///
/// Edges use OR-Set semantics (add-wins over concurrent remove).
/// Edge strengths use MaxRegister (only increases propagate).
/// Cycles introduced by merge are resolved by removing the weakest edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalGraphCRDT {
    /// Edges with add-wins semantics.
    edges: ORSet<CausalEdge>,
    /// Edge strengths: (source, target) → MaxRegister<f64>.
    strengths: HashMap<(String, String), MaxRegister<f64>>,
    /// Next sequence number per agent for ORSet tags.
    seq_counters: HashMap<String, u64>,
}

impl CausalGraphCRDT {
    /// Create a new empty DAG CRDT.
    pub fn new() -> Self {
        Self {
            edges: ORSet::new(),
            strengths: HashMap::new(),
            seq_counters: HashMap::new(),
        }
    }

    /// Add an edge to the graph.
    ///
    /// Performs a local cycle check before adding. Returns an error if the
    /// edge would create a cycle or is a self-loop.
    pub fn add_edge(
        &mut self,
        source: &str,
        target: &str,
        strength: f64,
        agent_id: &str,
        seq: u64,
    ) -> CortexResult<()> {
        // Reject self-loops
        if source == target {
            return Err(CortexError::MultiAgentError(
                MultiAgentError::CyclicDependency(format!("{source} → {source}")),
            ));
        }

        let edge = CausalEdge {
            source: source.to_string(),
            target: target.to_string(),
        };

        // Check if adding this edge would create a cycle
        if self.would_create_cycle(&edge) {
            return Err(CortexError::MultiAgentError(
                MultiAgentError::CyclicDependency(format!("{source} → {target}")),
            ));
        }

        // Add edge to ORSet
        self.edges.add(edge, agent_id, seq);

        // Track sequence counter
        let counter = self.seq_counters.entry(agent_id.to_string()).or_insert(0);
        *counter = (*counter).max(seq);

        // Initialize or update strength
        let key = (source.to_string(), target.to_string());
        let strength_clamped = strength.clamp(0.0, 1.0);
        self.strengths
            .entry(key)
            .and_modify(|reg| reg.set(strength_clamped))
            .or_insert_with(|| MaxRegister::new(strength_clamped, Utc::now()));

        Ok(())
    }

    /// Remove an edge from the graph (OR-Set remove: tombstone all tags).
    pub fn remove_edge(&mut self, source: &str, target: &str) {
        let edge = CausalEdge {
            source: source.to_string(),
            target: target.to_string(),
        };
        self.edges.remove(&edge);
    }

    /// Update the strength of an edge (MaxRegister: only increases propagate).
    pub fn update_strength(&mut self, source: &str, target: &str, strength: f64) {
        let key = (source.to_string(), target.to_string());
        let strength_clamped = strength.clamp(0.0, 1.0);
        if let Some(reg) = self.strengths.get_mut(&key) {
            reg.set(strength_clamped);
        }
    }

    /// Get the strength of an edge.
    pub fn get_strength(&self, source: &str, target: &str) -> Option<f64> {
        let key = (source.to_string(), target.to_string());
        self.strengths.get(&key).map(|reg| *reg.get())
    }

    /// Merge with another DAG CRDT.
    ///
    /// 1. Merge edges (ORSet merge — add-wins)
    /// 2. Merge strengths (per-edge MaxRegister merge)
    /// 3. Resolve any cycles introduced by the merge
    pub fn merge(&mut self, other: &Self) -> CortexResult<()> {
        // Merge edges (ORSet)
        self.edges.merge(&other.edges);

        // Merge strengths (per-edge MaxRegister)
        for (key, other_reg) in &other.strengths {
            self.strengths
                .entry(key.clone())
                .and_modify(|reg| reg.merge(other_reg))
                .or_insert_with(|| other_reg.clone());
        }

        // Merge sequence counters
        for (agent, &other_seq) in &other.seq_counters {
            let entry = self.seq_counters.entry(agent.clone()).or_insert(0);
            *entry = (*entry).max(other_seq);
        }

        // Resolve any cycles introduced by the merge
        self.resolve_cycles();

        Ok(())
    }

    /// Check if adding an edge would create a cycle.
    ///
    /// DFS from target to source in the current graph. If source is
    /// reachable from target, adding source→target would create a cycle.
    pub fn would_create_cycle(&self, edge: &CausalEdge) -> bool {
        // DFS from edge.target — can we reach edge.source?
        let mut visited = HashSet::new();
        let mut stack = vec![edge.target.clone()];

        while let Some(node) = stack.pop() {
            if node == edge.source {
                return true;
            }
            if visited.contains(&node) {
                continue;
            }
            visited.insert(node.clone());

            // Find all outgoing edges from this node
            for present_edge in self.edges.elements() {
                if present_edge.source == node {
                    stack.push(present_edge.target.clone());
                }
            }
        }

        false
    }

    /// Detect a cycle in the current graph using DFS.
    ///
    /// Returns `Some(cycle_edges)` if a cycle is found, `None` if acyclic.
    pub fn detect_cycle(&self) -> Option<Vec<CausalEdge>> {
        let present_edges = self.edges.elements();

        // Build adjacency list
        let mut adj: HashMap<&str, Vec<&CausalEdge>> = HashMap::new();
        for edge in &present_edges {
            adj.entry(edge.source.as_str())
                .or_default()
                .push(edge);
        }

        // Collect all nodes
        let mut all_nodes: HashSet<&str> = HashSet::new();
        for edge in &present_edges {
            all_nodes.insert(edge.source.as_str());
            all_nodes.insert(edge.target.as_str());
        }

        // DFS-based cycle detection with path tracking
        let mut visited = HashSet::new();
        let mut in_stack = HashSet::new();
        let mut path: Vec<&CausalEdge> = Vec::new();

        for &start in &all_nodes {
            if visited.contains(start) {
                continue;
            }
            if let Some(cycle) =
                Self::dfs_find_cycle(start, &adj, &mut visited, &mut in_stack, &mut path)
            {
                return Some(cycle);
            }
        }

        None
    }

    /// DFS helper for cycle detection.
    fn dfs_find_cycle<'a>(
        node: &'a str,
        adj: &HashMap<&'a str, Vec<&'a CausalEdge>>,
        visited: &mut HashSet<&'a str>,
        in_stack: &mut HashSet<&'a str>,
        path: &mut Vec<&'a CausalEdge>,
    ) -> Option<Vec<CausalEdge>> {
        visited.insert(node);
        in_stack.insert(node);

        if let Some(neighbors) = adj.get(node) {
            for &edge in neighbors {
                if !visited.contains(edge.target.as_str()) {
                    path.push(edge);
                    if let Some(cycle) = Self::dfs_find_cycle(
                        edge.target.as_str(),
                        adj,
                        visited,
                        in_stack,
                        path,
                    ) {
                        return Some(cycle);
                    }
                    path.pop();
                } else if in_stack.contains(edge.target.as_str()) {
                    // Found a cycle — extract the cycle edges
                    let mut cycle_edges = Vec::new();
                    // Find where the cycle starts in the path
                    let cycle_start = edge.target.as_str();
                    let mut found_start = false;
                    for &path_edge in path.iter() {
                        if path_edge.source.as_str() == cycle_start {
                            found_start = true;
                        }
                        if found_start {
                            cycle_edges.push(path_edge.clone());
                        }
                    }
                    cycle_edges.push(edge.clone());
                    return Some(cycle_edges);
                }
            }
        }

        in_stack.remove(node);
        None
    }

    /// Resolve all cycles by removing the weakest edge in each cycle.
    ///
    /// Deterministic: if strengths are equal, the edge with the
    /// lexicographically smaller `(source, target)` pair is removed.
    pub fn resolve_cycles(&mut self) {
        loop {
            let cycle = self.detect_cycle();
            match cycle {
                None => break,
                Some(cycle_edges) => {
                    // Find the weakest edge in the cycle
                    let weakest = cycle_edges
                        .iter()
                        .min_by(|a, b| {
                            let strength_a = self.get_strength(&a.source, &a.target)
                                .unwrap_or(0.0);
                            let strength_b = self.get_strength(&b.source, &b.target)
                                .unwrap_or(0.0);
                            strength_a
                                .partial_cmp(&strength_b)
                                .unwrap_or(std::cmp::Ordering::Equal)
                                .then_with(|| {
                                    // Tie-break: lexicographically smaller (source, target)
                                    (&a.source, &a.target).cmp(&(&b.source, &b.target))
                                })
                        })
                        .cloned();

                    if let Some(edge) = weakest {
                        self.remove_edge(&edge.source, &edge.target);
                    } else {
                        break; // Safety: no edges in cycle (shouldn't happen)
                    }
                }
            }
        }
    }

    /// Get all present edges.
    pub fn edges(&self) -> Vec<&CausalEdge> {
        self.edges.elements()
    }

    /// Get the number of present edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Get all unique node IDs in the graph.
    pub fn nodes(&self) -> HashSet<String> {
        let mut nodes = HashSet::new();
        for edge in self.edges.elements() {
            nodes.insert(edge.source.clone());
            nodes.insert(edge.target.clone());
        }
        nodes
    }
}

impl Default for CausalGraphCRDT {
    fn default() -> Self {
        Self::new()
    }
}
