//! EigenTrust global trust computation (Kamvar et al., Stanford 2003).
//!
//! Power iteration: `t(i+1) = C^T * t(i)` where C is the normalized
//! local trust matrix. Pre-trusted peers serve as anchors to prevent
//! Sybil attacks from inflating trust.

use std::collections::BTreeMap;

use uuid::Uuid;

use super::local_trust::LocalTrustStore;

/// Configuration for EigenTrust computation.
#[derive(Debug, Clone)]
pub struct EigenTrustConfig {
    /// Maximum power iterations before stopping.
    pub max_iterations: usize,
    /// Convergence threshold (L1 norm of delta vector).
    pub convergence_threshold: f64,
    /// Weight of pre-trusted peers in the iteration (alpha in the paper).
    /// Higher alpha = more influence from pre-trusted set.
    pub pre_trust_weight: f64,
}

impl Default for EigenTrustConfig {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            convergence_threshold: 1e-6,
            pre_trust_weight: 0.5,
        }
    }
}

/// Trust policy thresholds.
#[derive(Debug, Clone)]
pub struct TrustPolicy {
    /// Minimum trust for delegation.
    pub min_trust_for_delegation: f64,
    /// Minimum trust for sensitive data sharing.
    pub min_trust_for_sensitive_data: f64,
}

impl Default for TrustPolicy {
    fn default() -> Self {
        Self {
            min_trust_for_delegation: 0.3,
            min_trust_for_sensitive_data: 0.6,
        }
    }
}

impl TrustPolicy {
    /// Check if an agent can delegate based on trust score.
    pub fn can_delegate(&self, trust: f64) -> bool {
        trust >= self.min_trust_for_delegation
    }

    /// Check if an agent can share sensitive data based on trust score.
    pub fn can_share_sensitive_data(&self, trust: f64) -> bool {
        trust >= self.min_trust_for_sensitive_data
    }
}

/// EigenTrust global trust computer.
pub struct EigenTrustComputer {
    pub config: EigenTrustConfig,
    pub policy: TrustPolicy,
}

impl EigenTrustComputer {
    pub fn new(config: EigenTrustConfig, policy: TrustPolicy) -> Self {
        Self { config, policy }
    }

    /// Compute global trust scores for all agents in the network.
    ///
    /// Uses power iteration with pre-trusted peer anchoring.
    /// Returns a map of agent_id → global trust score in [0.0, 1.0].
    pub fn compute_global_trust(
        &self,
        local_store: &mut LocalTrustStore,
        pre_trusted: &[Uuid],
    ) -> BTreeMap<Uuid, f64> {
        let agents = local_store.all_agents();
        let n = agents.len();

        if n == 0 {
            return BTreeMap::new();
        }

        // Build agent index for matrix operations.
        let agent_index: BTreeMap<Uuid, usize> =
            agents.iter().enumerate().map(|(i, &id)| (id, i)).collect();

        // Build pre-trusted vector p: uniform over pre-trusted set.
        let mut p = vec![0.0_f64; n];
        if !pre_trusted.is_empty() {
            let weight = 1.0 / pre_trusted.len() as f64;
            for pt in pre_trusted {
                if let Some(&idx) = agent_index.get(pt) {
                    p[idx] = weight;
                }
            }
        } else {
            // No pre-trusted: uniform distribution.
            let weight = 1.0 / n as f64;
            p.fill(weight);
        }

        // Build normalized local trust matrix C.
        // C[i][j] = normalized local trust from agent i to agent j.
        let mut c_matrix: Vec<BTreeMap<usize, f64>> = Vec::with_capacity(n);
        for &agent in &agents {
            let row = local_store.normalized_row(agent);
            let mut indexed_row = BTreeMap::new();
            for (target, trust) in row {
                if let Some(&idx) = agent_index.get(&target) {
                    indexed_row.insert(idx, trust);
                }
            }
            c_matrix.push(indexed_row);
        }

        // Initialize trust vector t = p (start from pre-trusted distribution).
        let mut t = p.clone();

        let alpha = self.config.pre_trust_weight;

        // Power iteration: t(i+1) = (1-alpha) * C^T * t(i) + alpha * p
        for _iter in 0..self.config.max_iterations {
            let mut t_new = vec![0.0_f64; n];

            // Compute C^T * t: for each agent i, sum over j: C[j][i] * t[j]
            for (j, row) in c_matrix.iter().enumerate() {
                for (&i, &c_ji) in row {
                    t_new[i] += c_ji * t[j];
                }
            }

            // Apply pre-trust anchoring: t_new = (1-alpha) * C^T * t + alpha * p
            for i in 0..n {
                t_new[i] = (1.0 - alpha) * t_new[i] + alpha * p[i];
            }

            // Check convergence (L1 norm of delta).
            let delta: f64 = t.iter().zip(t_new.iter()).map(|(a, b)| (a - b).abs()).sum();

            t = t_new;

            if delta < self.config.convergence_threshold {
                break;
            }
        }

        // Clamp all values to [0.0, 1.0] and build result map.
        agents
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, t[i].clamp(0.0, 1.0)))
            .collect()
    }
}

impl Default for EigenTrustComputer {
    fn default() -> Self {
        Self::new(EigenTrustConfig::default(), TrustPolicy::default())
    }
}
