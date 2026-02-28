//! Baseline state for per-signal calibration (Req 5 AC7).

/// Per-signal baseline statistics.
#[derive(Debug, Clone)]
pub struct SignalBaseline {
    pub mean: f64,
    pub std_dev: f64,
    pub samples: Vec<f64>,
}

impl Default for SignalBaseline {
    fn default() -> Self {
        Self {
            mean: 0.0,
            std_dev: 0.0,
            samples: Vec::new(),
        }
    }
}

/// Baseline state across all 7 signals.
#[derive(Debug, Clone)]
pub struct BaselineState {
    /// Number of sessions required for calibration (default 10).
    pub calibration_sessions: u32,
    /// Whether still in calibration period.
    pub is_calibrating: bool,
    /// Per-signal baselines.
    pub per_signal: [SignalBaseline; 7],
    /// Sessions observed so far.
    pub sessions_observed: u32,
}

impl BaselineState {
    pub fn new(calibration_sessions: u32) -> Self {
        Self {
            calibration_sessions,
            is_calibrating: true,
            per_signal: Default::default(),
            sessions_observed: 0,
        }
    }

    /// Record a session's signal values during calibration.
    /// After calibration_sessions, baseline is frozen (AC7).
    pub fn record_session(&mut self, signals: &[f64; 7]) {
        if !self.is_calibrating {
            return; // Baseline frozen after establishment
        }

        self.sessions_observed += 1;

        for (i, &value) in signals.iter().enumerate() {
            self.per_signal[i].samples.push(value);
        }

        if self.sessions_observed >= self.calibration_sessions {
            // Compute final statistics
            for baseline in &mut self.per_signal {
                if !baseline.samples.is_empty() {
                    let n = baseline.samples.len() as f64;
                    baseline.mean = baseline.samples.iter().sum::<f64>() / n;
                    let variance = baseline
                        .samples
                        .iter()
                        .map(|&x| (x - baseline.mean).powi(2))
                        .sum::<f64>()
                        / n;
                    baseline.std_dev = variance.sqrt();
                }
            }
            self.is_calibrating = false;
        }
    }

    /// Percentile rank of a value against the baseline for a signal.
    pub fn percentile_rank(&self, signal_index: usize, value: f64) -> f64 {
        if signal_index >= 7 || self.is_calibrating {
            return value; // Pass through during calibration
        }
        let baseline = &self.per_signal[signal_index];
        if baseline.samples.is_empty() {
            return value;
        }
        let count_below = baseline.samples.iter().filter(|&&s| s <= value).count();
        (count_below as f64 / baseline.samples.len() as f64).clamp(0.0, 1.0)
    }
}

impl Default for BaselineState {
    fn default() -> Self {
        Self::new(10)
    }
}
