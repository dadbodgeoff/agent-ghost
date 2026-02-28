//! Merkle tree for hash chain anchoring.
//!
//! Triggered every 1000 events or 24 hours (AC9).

/// A Merkle tree built from hash chain leaves.
#[derive(Debug, Clone)]
pub struct MerkleTree {
    pub root: [u8; 32],
    pub leaves: Vec<[u8; 32]>,
    /// Internal nodes stored level-by-level for proof generation.
    nodes: Vec<Vec<[u8; 32]>>,
}

impl MerkleTree {
    /// Build a Merkle tree from chain hashes (leaves).
    pub fn from_chain(chain_hashes: &[[u8; 32]]) -> Self {
        if chain_hashes.is_empty() {
            return Self {
                root: [0u8; 32],
                leaves: Vec::new(),
                nodes: Vec::new(),
            };
        }

        if chain_hashes.len() == 1 {
            return Self {
                root: chain_hashes[0],
                leaves: chain_hashes.to_vec(),
                nodes: vec![chain_hashes.to_vec()],
            };
        }

        let mut levels: Vec<Vec<[u8; 32]>> = Vec::new();
        let mut current_level: Vec<[u8; 32]> = chain_hashes.to_vec();

        // Pad to even length if needed
        if current_level.len() % 2 != 0 {
            let last = *current_level.last().unwrap();
            current_level.push(last);
        }

        levels.push(current_level.clone());

        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            for pair in current_level.chunks(2) {
                let hash = hash_pair(&pair[0], &pair[1]);
                next_level.push(hash);
            }
            if next_level.len() > 1 && next_level.len() % 2 != 0 {
                let last = *next_level.last().unwrap();
                next_level.push(last);
            }
            levels.push(next_level.clone());
            current_level = next_level;
        }

        let root = current_level[0];

        Self {
            root,
            leaves: chain_hashes.to_vec(),
            nodes: levels,
        }
    }

    /// Generate an inclusion proof for a leaf at the given index.
    pub fn inclusion_proof(&self, leaf_index: usize) -> Vec<[u8; 32]> {
        if leaf_index >= self.leaves.len() || self.nodes.is_empty() {
            return Vec::new();
        }

        let mut proof = Vec::new();
        let mut idx = leaf_index;

        for level in &self.nodes[..self.nodes.len().saturating_sub(1)] {
            let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
            if sibling_idx < level.len() {
                proof.push(level[sibling_idx]);
            }
            idx /= 2;
        }

        proof
    }

    /// Verify an inclusion proof against a root.
    pub fn verify_proof(
        root: &[u8; 32],
        leaf: &[u8; 32],
        proof: &[[u8; 32]],
        leaf_index: usize,
    ) -> bool {
        let mut current = *leaf;
        let mut idx = leaf_index;

        for sibling in proof {
            current = if idx % 2 == 0 {
                hash_pair(&current, sibling)
            } else {
                hash_pair(sibling, &current)
            };
            idx /= 2;
        }

        current == *root
    }
}

fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(left);
    hasher.update(right);
    *hasher.finalize().as_bytes()
}
