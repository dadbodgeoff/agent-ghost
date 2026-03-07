//! S8: Behavioral Anomaly signal.
//!
//! Tracks tool call patterns per session and computes deviation from
//! established baseline pattern. Spikes after processing external content
//! indicate potential prompt injection influence.
//!
//! Uses Kullback-Leibler divergence between current tool call distribution
//! and baseline distribution.
//!
//! Research: Item 2 (Feed anomalies into convergence scoring).

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::{PrivacyLevel, Signal, SignalInput};

/// Minimum number of tool calls needed before computing anomaly.
const MIN_TOOL_CALLS: usize = 5;

/// Number of sessions for baseline calibration (same as other signals).
const CALIBRATION_SESSIONS: usize = 10;

/// Small constant to avoid log(0) in KL divergence.
const EPSILON: f64 = 1e-10;

/// Tracks tool call distribution for behavioral anomaly detection.
#[derive(Debug, Clone, Default)]
pub struct ToolCallDistribution {
    /// Counts per tool type/name.
    pub counts: BTreeMap<String, u64>,
    /// Total tool calls.
    pub total: u64,
}

impl ToolCallDistribution {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a tool call.
    pub fn record(&mut self, tool_name: &str) {
        *self.counts.entry(tool_name.to_string()).or_insert(0) += 1;
        self.total += 1;
    }

    /// Get the probability distribution (normalized counts).
    pub fn distribution(&self) -> BTreeMap<String, f64> {
        if self.total == 0 {
            return BTreeMap::new();
        }
        self.counts
            .iter()
            .map(|(k, &v)| (k.clone(), v as f64 / self.total as f64))
            .collect()
    }

    /// Check if we have enough data to compute anomaly.
    pub fn has_sufficient_data(&self) -> bool {
        self.total >= MIN_TOOL_CALLS as u64
    }
}

/// S8: Behavioral Anomaly signal.
///
/// Computes deviation from established baseline tool call pattern using
/// KL divergence. Higher values indicate more anomalous behavior.
pub struct BehavioralAnomalySignal {
    /// Baseline distribution (established from first N sessions).
    baseline: Mutex<Option<ToolCallDistribution>>,
    /// Accumulated baseline sessions during calibration.
    calibration_sessions: Mutex<Vec<ToolCallDistribution>>,
    /// Current session distribution.
    current: Mutex<ToolCallDistribution>,
    /// Whether baseline has been established.
    calibrated: Mutex<bool>,
    /// Cached value for dirty-flag throttling.
    cached_value: std::sync::atomic::AtomicU64,
    /// Last total count when value was computed (dirty flag).
    last_computed_total: std::sync::atomic::AtomicU64,
}

impl BehavioralAnomalySignal {
    pub fn new() -> Self {
        Self {
            baseline: Mutex::new(None),
            calibration_sessions: Mutex::new(Vec::new()),
            current: Mutex::new(ToolCallDistribution::new()),
            calibrated: Mutex::new(false),
            cached_value: std::sync::atomic::AtomicU64::new(0.0f64.to_bits()),
            last_computed_total: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Record a tool call in the current session.
    pub fn record_tool_call(&self, tool_name: &str) {
        if let Ok(mut current) = self.current.lock() {
            current.record(tool_name);
        }
    }

    /// End the current session and update baseline if still calibrating.
    pub fn end_session(&self) {
        let session_dist = {
            let mut current = self.current.lock().unwrap();
            let dist = current.clone();
            *current = ToolCallDistribution::new();
            dist
        };

        let mut calibrated = self.calibrated.lock().unwrap();
        if *calibrated {
            return; // Baseline frozen
        }

        let mut sessions = self.calibration_sessions.lock().unwrap();
        sessions.push(session_dist);

        if sessions.len() >= CALIBRATION_SESSIONS {
            // Merge all calibration sessions into baseline
            let mut merged = ToolCallDistribution::new();
            for session in sessions.iter() {
                for (tool, &count) in &session.counts {
                    *merged.counts.entry(tool.clone()).or_insert(0) += count;
                    merged.total += count;
                }
            }
            *self.baseline.lock().unwrap() = Some(merged);
            *calibrated = true;
        }
    }

    fn get_cached(&self) -> f64 {
        f64::from_bits(self.cached_value.load(std::sync::atomic::Ordering::Relaxed))
    }

    fn set_cached(&self, val: f64) {
        self.cached_value
            .store(val.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }
}

impl Default for BehavioralAnomalySignal {
    fn default() -> Self {
        Self::new()
    }
}

impl Signal for BehavioralAnomalySignal {
    fn id(&self) -> u8 {
        8
    }

    fn name(&self) -> &'static str {
        "behavioral_anomaly"
    }

    fn requires_privacy_level(&self) -> PrivacyLevel {
        PrivacyLevel::Minimal
    }

    fn compute(&self, _data: &SignalInput) -> f64 {
        let calibrated = self.calibrated.lock().unwrap();
        if !*calibrated {
            return 0.0; // During calibration, return 0.0
        }

        let current = self.current.lock().unwrap();
        if !current.has_sufficient_data() {
            return 0.0; // Not enough data yet this session
        }

        // Dirty-flag throttling: only recompute when tool call data changes
        let current_total = current.total;
        let last_total = self
            .last_computed_total
            .load(std::sync::atomic::Ordering::Relaxed);
        if current_total == last_total {
            return self.get_cached();
        }

        let baseline = self.baseline.lock().unwrap();
        let baseline = match baseline.as_ref() {
            Some(b) => b,
            None => return 0.0,
        };

        let current_dist = current.distribution();
        let baseline_dist = baseline.distribution();

        // Compute symmetric KL divergence (Jensen-Shannon divergence)
        let divergence = jensen_shannon_divergence(&current_dist, &baseline_dist);

        // Normalize to [0.0, 1.0] — JS divergence is bounded by ln(2) ≈ 0.693
        // We use a sigmoid-like mapping for smoother scaling
        let signal = (divergence / 0.693).clamp(0.0, 1.0);

        self.last_computed_total
            .store(current_total, std::sync::atomic::Ordering::Relaxed);
        self.set_cached(signal);
        signal
    }
}

/// Compute Jensen-Shannon divergence between two distributions.
///
/// JSD is a symmetric, bounded version of KL divergence.
/// JSD(P||Q) = 0.5 * KL(P||M) + 0.5 * KL(Q||M) where M = 0.5*(P+Q)
pub fn jensen_shannon_divergence(p: &BTreeMap<String, f64>, q: &BTreeMap<String, f64>) -> f64 {
    // Collect all keys from both distributions
    let mut all_keys: Vec<&String> = p.keys().chain(q.keys()).collect();
    all_keys.sort();
    all_keys.dedup();

    let mut jsd = 0.0;

    for key in &all_keys {
        let p_val = p.get(*key).copied().unwrap_or(EPSILON);
        let q_val = q.get(*key).copied().unwrap_or(EPSILON);
        let m_val = 0.5 * (p_val + q_val);

        if m_val > 0.0 {
            if p_val > 0.0 {
                jsd += 0.5 * p_val * (p_val / m_val).ln();
            }
            if q_val > 0.0 {
                jsd += 0.5 * q_val * (q_val / m_val).ln();
            }
        }
    }

    jsd.max(0.0) // Ensure non-negative (floating point errors)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_signal_input() -> SignalInput {
        SignalInput::default()
    }

    #[test]
    fn s8_returns_zero_during_calibration() {
        let signal = BehavioralAnomalySignal::new();
        let input = make_signal_input();
        assert_eq!(signal.compute(&input), 0.0);
    }

    #[test]
    fn s8_id_and_name() {
        let signal = BehavioralAnomalySignal::new();
        assert_eq!(signal.id(), 8);
        assert_eq!(signal.name(), "behavioral_anomaly");
    }

    #[test]
    fn s8_requires_minimal_privacy() {
        let signal = BehavioralAnomalySignal::new();
        assert_eq!(signal.requires_privacy_level(), PrivacyLevel::Minimal);
    }

    #[test]
    fn s8_returns_zero_when_pattern_matches_baseline() {
        let signal = BehavioralAnomalySignal::new();

        // Calibrate with 10 sessions of identical patterns
        for _ in 0..CALIBRATION_SESSIONS {
            for _ in 0..10 {
                signal.record_tool_call("file_read");
            }
            for _ in 0..5 {
                signal.record_tool_call("shell_exec");
            }
            signal.end_session();
        }

        // Current session matches baseline pattern
        for _ in 0..10 {
            signal.record_tool_call("file_read");
        }
        for _ in 0..5 {
            signal.record_tool_call("shell_exec");
        }

        let input = make_signal_input();
        let value = signal.compute(&input);
        // Should be very close to 0.0 since pattern matches
        assert!(value < 0.1, "expected near-zero, got {}", value);
    }

    #[test]
    fn s8_returns_high_when_pattern_shifts_dramatically() {
        let signal = BehavioralAnomalySignal::new();

        // Calibrate with file_read-heavy sessions
        for _ in 0..CALIBRATION_SESSIONS {
            for _ in 0..20 {
                signal.record_tool_call("file_read");
            }
            signal.end_session();
        }

        // Current session: completely different pattern
        for _ in 0..20 {
            signal.record_tool_call("web_request");
        }

        let input = make_signal_input();
        let value = signal.compute(&input);
        assert!(value > 0.5, "expected high anomaly, got {}", value);
    }

    #[test]
    fn s8_value_in_range() {
        let signal = BehavioralAnomalySignal::new();

        // Calibrate
        for _ in 0..CALIBRATION_SESSIONS {
            for _ in 0..10 {
                signal.record_tool_call("tool_a");
            }
            signal.end_session();
        }

        // Various patterns
        for _ in 0..10 {
            signal.record_tool_call("tool_b");
        }

        let input = make_signal_input();
        let value = signal.compute(&input);
        assert!((0.0..=1.0).contains(&value), "value {} out of range", value);
    }

    #[test]
    fn s8_empty_tool_call_history_returns_zero() {
        let signal = BehavioralAnomalySignal::new();

        // Calibrate with empty sessions (no tool calls)
        for _ in 0..CALIBRATION_SESSIONS {
            signal.end_session();
        }

        let input = make_signal_input();
        assert_eq!(signal.compute(&input), 0.0);
    }

    #[test]
    fn s8_single_tool_type_repeated_no_overflow() {
        let signal = BehavioralAnomalySignal::new();

        // Calibrate
        for _ in 0..CALIBRATION_SESSIONS {
            for _ in 0..100 {
                signal.record_tool_call("tool_a");
            }
            signal.end_session();
        }

        // 1000 calls of same type
        for _ in 0..1000 {
            signal.record_tool_call("tool_a");
        }

        let input = make_signal_input();
        let value = signal.compute(&input);
        assert!(
            (0.0..=1.0).contains(&value),
            "value {} out of range after 1000 calls",
            value
        );
    }

    #[test]
    fn tool_call_distribution_record() {
        let mut dist = ToolCallDistribution::new();
        dist.record("file_read");
        dist.record("file_read");
        dist.record("shell_exec");

        assert_eq!(dist.total, 3);
        assert_eq!(dist.counts["file_read"], 2);
        assert_eq!(dist.counts["shell_exec"], 1);
    }

    #[test]
    fn tool_call_distribution_probability() {
        let mut dist = ToolCallDistribution::new();
        dist.record("a");
        dist.record("a");
        dist.record("b");
        dist.record("b");

        let probs = dist.distribution();
        assert!((probs["a"] - 0.5).abs() < f64::EPSILON);
        assert!((probs["b"] - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn jensen_shannon_identical_distributions() {
        let mut p = BTreeMap::new();
        p.insert("a".into(), 0.5);
        p.insert("b".into(), 0.5);

        let jsd = jensen_shannon_divergence(&p, &p);
        assert!(
            jsd < 1e-10,
            "identical distributions should have JSD ≈ 0, got {}",
            jsd
        );
    }

    #[test]
    fn jensen_shannon_completely_different() {
        let mut p = BTreeMap::new();
        p.insert("a".into(), 1.0);

        let mut q = BTreeMap::new();
        q.insert("b".into(), 1.0);

        let jsd = jensen_shannon_divergence(&p, &q);
        // JSD is bounded by ln(2) ≈ 0.693
        assert!(
            jsd > 0.5,
            "completely different distributions should have high JSD, got {}",
            jsd
        );
        assert!(jsd <= 0.694, "JSD should be bounded by ln(2), got {}", jsd);
    }

    #[test]
    fn jensen_shannon_empty_distributions() {
        let p: BTreeMap<String, f64> = BTreeMap::new();
        let q: BTreeMap<String, f64> = BTreeMap::new();
        let jsd = jensen_shannon_divergence(&p, &q);
        assert_eq!(jsd, 0.0);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn s8_value_always_in_range(
            baseline_tools in proptest::collection::vec("[a-z]{3,8}", 1..10usize),
            current_tools in proptest::collection::vec("[a-z]{3,8}", 1..10usize),
        ) {
            let signal = BehavioralAnomalySignal::new();

            // Calibrate
            for _ in 0..CALIBRATION_SESSIONS {
                for tool in &baseline_tools {
                    signal.record_tool_call(tool);
                }
                signal.end_session();
            }

            // Current session
            for tool in &current_tools {
                signal.record_tool_call(tool);
            }

            let input = SignalInput::default();
            let value = signal.compute(&input);
            prop_assert!((0.0..=1.0).contains(&value),
                "S8 value {} out of [0.0, 1.0]", value);
        }

        #[test]
        fn jsd_always_non_negative(
            keys in proptest::collection::vec("[a-z]{1,5}", 1..5usize),
            p_vals in proptest::collection::vec(0.01f64..1.0, 1..5usize),
            q_vals in proptest::collection::vec(0.01f64..1.0, 1..5usize),
        ) {
            let len = keys.len().min(p_vals.len()).min(q_vals.len());
            let p_sum: f64 = p_vals[..len].iter().sum();
            let q_sum: f64 = q_vals[..len].iter().sum();

            let p: BTreeMap<String, f64> = keys[..len].iter()
                .zip(p_vals[..len].iter())
                .map(|(k, &v)| (k.clone(), v / p_sum))
                .collect();
            let q: BTreeMap<String, f64> = keys[..len].iter()
                .zip(q_vals[..len].iter())
                .map(|(k, &v)| (k.clone(), v / q_sum))
                .collect();

            let jsd = jensen_shannon_divergence(&p, &q);
            prop_assert!(jsd >= 0.0, "JSD should be non-negative, got {}", jsd);
        }
    }
}
