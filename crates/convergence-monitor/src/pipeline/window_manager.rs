//! Sliding window management for signal computation (Req 5 AC2).

use std::collections::BTreeMap;

use uuid::Uuid;

/// Per-agent window state tracking.
pub struct WindowManager {
    agents: BTreeMap<Uuid, AgentWindows>,
}

struct AgentWindows {
    session_count: u32,
    micro_data: Vec<f64>,
    meso_data: Vec<f64>,
    macro_data: Vec<f64>,
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            agents: BTreeMap::new(),
        }
    }

    /// Record a session boundary for an agent.
    pub fn record_session_end(&mut self, agent_id: Uuid) {
        let windows = self.agents.entry(agent_id).or_insert_with(|| AgentWindows {
            session_count: 0,
            micro_data: Vec::new(),
            meso_data: Vec::new(),
            macro_data: Vec::new(),
        });
        windows.session_count += 1;

        // Rotate micro → meso → macro
        if !windows.micro_data.is_empty() {
            let avg = windows.micro_data.iter().sum::<f64>() / windows.micro_data.len() as f64;
            windows.meso_data.push(avg);
            if windows.meso_data.len() > 7 {
                windows.meso_data.remove(0);
            }
            windows.macro_data.push(avg);
            if windows.macro_data.len() > 30 {
                windows.macro_data.remove(0);
            }
            windows.micro_data.clear();
        }
    }

    /// Get session count for an agent.
    #[allow(dead_code)]
    pub fn session_count(&self, agent_id: &Uuid) -> u32 {
        self.agents.get(agent_id).map_or(0, |w| w.session_count)
    }

    /// Check if meso trend is directionally concerning (AC4 amplification).
    ///
    /// Returns true if the linear regression slope of the meso window
    /// is positive (scores increasing) with p < 0.05.
    pub fn meso_trend_concerning(&self, agent_id: Uuid) -> bool {
        let windows = match self.agents.get(&agent_id) {
            Some(w) => w,
            None => return false,
        };

        if windows.meso_data.len() < 3 {
            return false;
        }

        // Simple linear regression slope
        let n = windows.meso_data.len() as f64;
        let x_mean = (n - 1.0) / 2.0;
        let y_mean: f64 = windows.meso_data.iter().sum::<f64>() / n;

        let mut numerator = 0.0;
        let mut denominator = 0.0;
        for (i, y) in windows.meso_data.iter().enumerate() {
            let x = i as f64;
            numerator += (x - x_mean) * (y - y_mean);
            denominator += (x - x_mean) * (x - x_mean);
        }

        if denominator.abs() < f64::EPSILON {
            return false;
        }

        let slope = numerator / denominator;

        // Positive slope with sufficient magnitude indicates concerning trend
        // Approximate p < 0.05 check: slope > 2 * standard_error
        let residuals: f64 = windows
            .meso_data
            .iter()
            .enumerate()
            .map(|(i, y)| {
                let predicted = y_mean + slope * (i as f64 - x_mean);
                (y - predicted).powi(2)
            })
            .sum();
        let se = (residuals / (n - 2.0)).sqrt() / denominator.sqrt();

        slope > 0.0 && (se.abs() < f64::EPSILON || slope / se > 2.0)
    }

    /// Check if any macro z-score exceeds the given threshold (AC5 amplification).
    pub fn macro_zscore_exceeds(&self, agent_id: Uuid, threshold: f64) -> bool {
        let windows = match self.agents.get(&agent_id) {
            Some(w) => w,
            None => return false,
        };

        if windows.macro_data.len() < 5 {
            return false;
        }

        let n = windows.macro_data.len() as f64;
        let mean = windows.macro_data.iter().sum::<f64>() / n;
        let variance = windows
            .macro_data
            .iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>()
            / n;
        let std_dev = variance.sqrt();

        if std_dev.abs() < f64::EPSILON {
            return false;
        }

        // Check if the latest score's z-score exceeds threshold
        if let Some(&latest) = windows.macro_data.last() {
            let z = (latest - mean).abs() / std_dev;
            return z > threshold;
        }

        false
    }
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}
