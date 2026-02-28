//! 8 convergence signals (Req 5 AC1).

pub mod session_duration;
pub mod inter_session_gap;
pub mod response_latency;
pub mod vocabulary_convergence;
pub mod goal_boundary_erosion;
pub mod initiative_balance;
pub mod disengagement_resistance;
pub mod behavioral_anomaly;

/// Privacy level for signal computation (mirrors itp-protocol).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PrivacyLevel {
    Minimal,
    Standard,
    Full,
    Research,
}

/// Signal trait — all signals produce values in [0.0, 1.0].
pub trait Signal: Send + Sync {
    fn id(&self) -> u8;
    fn name(&self) -> &'static str;
    fn compute(&self, data: &SignalInput) -> f64;
    fn requires_privacy_level(&self) -> PrivacyLevel;
}

/// Input data for signal computation.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SignalInput {
    /// Current session duration in seconds.
    pub session_duration_secs: f64,
    /// Gap since last session in seconds.
    pub inter_session_gap_secs: Option<f64>,
    /// Response latencies in ms.
    pub response_latencies_ms: Vec<f64>,
    /// Message lengths (for latency normalization).
    pub message_lengths: Vec<usize>,
    /// Human message count.
    pub human_message_count: u64,
    /// Agent message count.
    pub agent_message_count: u64,
    /// Human-initiated message count.
    pub human_initiated_count: u64,
    /// Total message count.
    pub total_message_count: u64,
    /// Exit signals detected (e.g., "goodbye", "stop").
    pub exit_signals_detected: u64,
    /// Exit signals ignored by agent.
    pub exit_signals_ignored: u64,
    /// TF-IDF vocabulary vectors (human, agent) for cosine similarity.
    pub human_vocab: Vec<f64>,
    pub agent_vocab: Vec<f64>,
    /// Goal tokens: existing vs proposed.
    pub existing_goal_tokens: Vec<String>,
    pub proposed_goal_tokens: Vec<String>,
    /// Message index within session (for throttling).
    pub message_index: u64,
    /// Tool call names in current session (for S8 behavioral anomaly).
    pub tool_call_names: Vec<String>,
}
