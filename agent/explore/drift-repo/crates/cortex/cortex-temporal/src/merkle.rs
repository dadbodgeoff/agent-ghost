//! Merkle audit tree over the append-only event log.
//! Provides O(log n) proof that a specific event exists in the log
//! and that no events have been modified or deleted.
//! Uses blake3 for all hashing (workspace standard).

use serde::{Deserialize, Serialize};

/// A node in the Merkle tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleNode {
    pub hash: String,
    pub left: Option<Box<MerkleNode>>,
    pub right: Option<Box<MerkleNode>>,
    /// Event ID if this is a leaf node.
    pub event_id: Option<i64>,
}

/// Merkle proof for a single event — path from leaf to root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    pub event_id: i64,
    pub leaf_hash: String,
    pub path: Vec<ProofStep>,
    pub root_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofStep {
    pub hash: String,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Position { Left, Right }

/// Build a Merkle tree from a list of event hashes.
pub fn build_tree(event_hashes: &[(i64, String)]) -> Option<MerkleNode> {
    if event_hashes.is_empty() {
        return None;
    }

    // Create leaf nodes
    let mut nodes: Vec<MerkleNode> = event_hashes
        .iter()
        .map(|(id, hash)| MerkleNode {
            hash: hash.clone(),
            left: None,
            right: None,
            event_id: Some(*id),
        })
        .collect();

    // Build tree bottom-up
    while nodes.len() > 1 {
        let mut next_level = Vec::new();
        for chunk in nodes.chunks(2) {
            if chunk.len() == 2 {
                let combined = format!("{}{}", chunk[0].hash, chunk[1].hash);
                let parent_hash = blake3::hash(combined.as_bytes()).to_hex().to_string();
                next_level.push(MerkleNode {
                    hash: parent_hash,
                    left: Some(Box::new(chunk[0].clone())),
                    right: Some(Box::new(chunk[1].clone())),
                    event_id: None,
                });
            } else {
                // Odd node — promote directly
                next_level.push(chunk[0].clone());
            }
        }
        nodes = next_level;
    }

    nodes.into_iter().next()
}

/// Generate a Merkle proof for a specific event.
pub fn generate_proof(root: &MerkleNode, target_event_id: i64) -> Option<MerkleProof> {
    let mut path = Vec::new();
    if !find_path(root, target_event_id, &mut path) {
        return None;
    }

    let leaf_hash = find_leaf_hash(root, target_event_id)?;

    Some(MerkleProof {
        event_id: target_event_id,
        leaf_hash,
        path,
        root_hash: root.hash.clone(),
    })
}

/// Verify a Merkle proof — recompute root from leaf + path.
pub fn verify_proof(proof: &MerkleProof) -> bool {
    let mut current_hash = proof.leaf_hash.clone();
    for step in &proof.path {
        let combined = match step.position {
            Position::Left => format!("{}{}", step.hash, current_hash),
            Position::Right => format!("{}{}", current_hash, step.hash),
        };
        current_hash = blake3::hash(combined.as_bytes()).to_hex().to_string();
    }
    current_hash == proof.root_hash
}

fn find_path(node: &MerkleNode, target: i64, path: &mut Vec<ProofStep>) -> bool {
    if let Some(id) = node.event_id {
        return id == target;
    }
    if let (Some(left), Some(right)) = (&node.left, &node.right) {
        if find_path(left, target, path) {
            path.push(ProofStep { hash: right.hash.clone(), position: Position::Right });
            return true;
        }
        if find_path(right, target, path) {
            path.push(ProofStep { hash: left.hash.clone(), position: Position::Left });
            return true;
        }
    }
    false
}

fn find_leaf_hash(node: &MerkleNode, target: i64) -> Option<String> {
    if let Some(id) = node.event_id {
        if id == target { return Some(node.hash.clone()); }
        return None;
    }
    if let Some(left) = &node.left {
        if let Some(h) = find_leaf_hash(left, target) { return Some(h); }
    }
    if let Some(right) = &node.right {
        if let Some(h) = find_leaf_hash(right, target) { return Some(h); }
    }
    None
}
