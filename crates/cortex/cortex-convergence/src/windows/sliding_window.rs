//! Generic sliding window with micro/meso/macro granularities.

/// Sliding window granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WindowLevel {
    /// Current session.
    Micro,
    /// Last 7 sessions.
    Meso,
    /// Last 30 sessions.
    Macro,
}

/// A generic sliding window that partitions data into micro/meso/macro.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SlidingWindow {
    /// Current session data points.
    pub micro: Vec<f64>,
    /// Last 7 sessions (session averages).
    pub meso: Vec<f64>,
    /// Last 30 sessions (session averages).
    pub r#macro: Vec<f64>,
}

impl SlidingWindow {
    pub fn new() -> Self {
        Self {
            micro: Vec::new(),
            meso: Vec::new(),
            r#macro: Vec::new(),
        }
    }

    /// Push a data point to the current session (micro).
    pub fn push_micro(&mut self, value: f64) {
        self.micro.push(value);
    }

    /// End the current session: average micro → push to meso/macro, clear micro.
    pub fn end_session(&mut self) {
        if !self.micro.is_empty() {
            let avg = self.micro.iter().sum::<f64>() / self.micro.len() as f64;
            self.meso.push(avg);
            if self.meso.len() > 7 {
                self.meso.remove(0);
            }
            self.r#macro.push(avg);
            if self.r#macro.len() > 30 {
                self.r#macro.remove(0);
            }
            self.micro.clear();
        }
    }

    /// Get data for a given window level.
    pub fn data(&self, level: WindowLevel) -> &[f64] {
        match level {
            WindowLevel::Micro => &self.micro,
            WindowLevel::Meso => &self.meso,
            WindowLevel::Macro => &self.r#macro,
        }
    }
}

impl Default for SlidingWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute linear regression slope on a data series.
pub fn linear_regression_slope(data: &[f64]) -> f64 {
    let n = data.len() as f64;
    if n < 2.0 {
        return 0.0;
    }

    let x_mean = (n - 1.0) / 2.0;
    let y_mean = data.iter().sum::<f64>() / n;

    let mut numerator = 0.0;
    let mut denominator = 0.0;

    for (i, &y) in data.iter().enumerate() {
        let x = i as f64;
        numerator += (x - x_mean) * (y - y_mean);
        denominator += (x - x_mean) * (x - x_mean);
    }

    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

/// Compute z-score of a value against a baseline mean and std_dev.
pub fn z_score_from_baseline(value: f64, mean: f64, std_dev: f64) -> f64 {
    if std_dev == 0.0 {
        return 0.0;
    }
    (value - mean) / std_dev
}
