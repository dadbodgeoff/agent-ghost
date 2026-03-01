//! Context usage tracker with progressive thresholds (Task 20.1).
//!
//! Tracks cumulative context window usage across turns and triggers
//! progressive compaction: gentle at 60%, aggressive at 80%, emergency at 95%.
//! LLM performance drops sharply after 60-70% of context window.

use chrono::{DateTime, Utc};

/// Compaction action — advisory, the caller decides what to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionAction {
    /// Below gentle threshold — no action needed.
    None,
    /// Between gentle and aggressive — enable compression, increase masking.
    Gentle,
    /// Between aggressive and emergency — summarize old turns.
    Aggressive,
    /// Above emergency — truncate immediately.
    Emergency,
}

/// Usage trend over last 5 turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageTrend {
    Stable,
    Rising,
    Falling,
}

/// Per-turn token usage record.
#[derive(Debug, Clone)]
pub struct TurnUsage {
    pub turn_number: u32,
    pub total_tokens: usize,
    pub fill_percentage: f64,
    pub timestamp: DateTime<Utc>,
}

/// Progressive compaction thresholds.
#[derive(Debug, Clone)]
pub struct CompactionThresholds {
    /// Trigger observation masking + memory compression (default 0.60).
    pub gentle: f64,
    /// Trigger turn summarization (default 0.80).
    pub aggressive: f64,
    /// Trigger emergency truncation (default 0.95).
    pub emergency: f64,
}

impl Default for CompactionThresholds {
    fn default() -> Self {
        Self {
            gentle: 0.60,
            aggressive: 0.80,
            emergency: 0.95,
        }
    }
}

/// Context usage tracker — per-session, reset at session boundary.
pub struct ContextUsageTracker {
    context_window: usize,
    history: Vec<TurnUsage>,
    thresholds: CompactionThresholds,
    turn_counter: u32,
}

impl ContextUsageTracker {
    pub fn new(context_window: usize) -> Self {
        Self {
            context_window,
            history: Vec::new(),
            thresholds: CompactionThresholds::default(),
            turn_counter: 0,
        }
    }

    pub fn with_thresholds(context_window: usize, thresholds: CompactionThresholds) -> Self {
        Self {
            context_window,
            history: Vec::new(),
            thresholds,
            turn_counter: 0,
        }
    }

    /// Record a turn's token usage and return the appropriate compaction action.
    pub fn record_turn(&mut self, total_tokens: usize) -> CompactionAction {
        self.turn_counter += 1;

        let fill_percentage = if self.context_window == 0 {
            1.0 // Treat 0 context window as emergency
        } else {
            total_tokens as f64 / self.context_window as f64
        };

        self.history.push(TurnUsage {
            turn_number: self.turn_counter,
            total_tokens,
            fill_percentage,
            timestamp: Utc::now(),
        });

        if fill_percentage >= self.thresholds.emergency {
            CompactionAction::Emergency
        } else if fill_percentage >= self.thresholds.aggressive {
            CompactionAction::Aggressive
        } else if fill_percentage >= self.thresholds.gentle {
            CompactionAction::Gentle
        } else {
            CompactionAction::None
        }
    }

    /// Analyze usage trend over last 5 turns.
    pub fn trend(&self) -> UsageTrend {
        let recent: Vec<f64> = self
            .history
            .iter()
            .rev()
            .take(5)
            .map(|t| t.fill_percentage)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        if recent.len() < 2 {
            return UsageTrend::Stable;
        }

        let slope = linear_regression_slope(&recent);
        if slope > 0.01 {
            UsageTrend::Rising
        } else if slope < -0.01 {
            UsageTrend::Falling
        } else {
            UsageTrend::Stable
        }
    }

    /// Estimate how many turns remain before emergency threshold is hit.
    /// Returns None if usage is stable or falling (won't hit emergency).
    pub fn projected_turns_remaining(&self) -> Option<u32> {
        let recent: Vec<f64> = self
            .history
            .iter()
            .rev()
            .take(5)
            .map(|t| t.fill_percentage)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        if recent.len() < 2 {
            return None;
        }

        let slope = linear_regression_slope(&recent);
        if slope <= 0.0 {
            return None; // Not rising
        }

        let current = *recent.last().unwrap_or(&0.0);
        let remaining = self.thresholds.emergency - current;
        if remaining <= 0.0 {
            return Some(0);
        }

        Some((remaining / slope).ceil() as u32)
    }

    /// Reset tracker at session boundary.
    pub fn reset(&mut self) {
        self.history.clear();
        self.turn_counter = 0;
    }
}

/// Simple linear regression slope on a sequence of values.
/// Returns 0.0 for degenerate inputs (< 2 values, all-NaN, zero denominator).
fn linear_regression_slope(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    if n < 2.0 {
        return 0.0;
    }

    // Filter out NaN/Inf values — they would poison the entire calculation
    let finite_count = values.iter().filter(|v| v.is_finite()).count();
    if finite_count < 2 {
        return 0.0;
    }

    let fc = finite_count as f64;
    let x_mean = (n - 1.0) / 2.0;
    let y_mean: f64 = values.iter().map(|v| if v.is_finite() { *v } else { 0.0 }).sum::<f64>() / fc;

    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for (i, y) in values.iter().enumerate() {
        let x = i as f64;
        let safe_y = if y.is_finite() { *y } else { 0.0 };
        numerator += (x - x_mean) * (safe_y - y_mean);
        denominator += (x - x_mean) * (x - x_mean);
    }

    if denominator == 0.0 {
        0.0
    } else {
        let slope = numerator / denominator;
        // Final NaN guard — should not happen with finite inputs, but be safe
        if slope.is_finite() { slope } else { 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_gentle_returns_none() {
        let mut tracker = ContextUsageTracker::new(100_000);
        assert_eq!(tracker.record_turn(50_000), CompactionAction::None);
    }

    #[test]
    fn at_gentle_returns_gentle() {
        let mut tracker = ContextUsageTracker::new(100_000);
        assert_eq!(tracker.record_turn(60_000), CompactionAction::Gentle);
    }

    #[test]
    fn at_aggressive_returns_aggressive() {
        let mut tracker = ContextUsageTracker::new(100_000);
        assert_eq!(tracker.record_turn(85_000), CompactionAction::Aggressive);
    }

    #[test]
    fn at_emergency_returns_emergency() {
        let mut tracker = ContextUsageTracker::new(100_000);
        assert_eq!(tracker.record_turn(96_000), CompactionAction::Emergency);
    }

    #[test]
    fn zero_context_window_returns_emergency() {
        let mut tracker = ContextUsageTracker::new(0);
        assert_eq!(tracker.record_turn(100), CompactionAction::Emergency);
    }

    #[test]
    fn over_context_window_returns_emergency() {
        let mut tracker = ContextUsageTracker::new(100_000);
        assert_eq!(tracker.record_turn(110_000), CompactionAction::Emergency);
    }

    #[test]
    fn trend_rising() {
        let mut tracker = ContextUsageTracker::new(100_000);
        for tokens in [40_000, 45_000, 50_000, 55_000, 60_000] {
            tracker.record_turn(tokens);
        }
        assert_eq!(tracker.trend(), UsageTrend::Rising);
    }

    #[test]
    fn trend_falling() {
        let mut tracker = ContextUsageTracker::new(100_000);
        for tokens in [60_000, 55_000, 50_000, 45_000, 40_000] {
            tracker.record_turn(tokens);
        }
        assert_eq!(tracker.trend(), UsageTrend::Falling);
    }

    #[test]
    fn trend_stable() {
        let mut tracker = ContextUsageTracker::new(100_000);
        for _ in 0..5 {
            tracker.record_turn(50_000);
        }
        assert_eq!(tracker.trend(), UsageTrend::Stable);
    }

    #[test]
    fn projected_turns_with_rising_usage() {
        let mut tracker = ContextUsageTracker::new(100_000);
        for tokens in [40_000, 50_000, 60_000, 70_000, 80_000] {
            tracker.record_turn(tokens);
        }
        let remaining = tracker.projected_turns_remaining();
        assert!(remaining.is_some());
        assert!(remaining.unwrap() > 0);
    }

    #[test]
    fn projected_turns_with_stable_usage() {
        let mut tracker = ContextUsageTracker::new(100_000);
        for _ in 0..5 {
            tracker.record_turn(50_000);
        }
        assert!(tracker.projected_turns_remaining().is_none());
    }
}
