use chrono::{Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};

/// Signals derived from temporal context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalSignals {
    /// Hour of day (0–23).
    pub hour_of_day: u32,
    /// Day of week (Mon=1 .. Sun=7, ISO).
    pub day_of_week: u32,
    /// How long the current session has been active, in seconds.
    pub session_duration_secs: u64,
}

impl TemporalSignals {
    /// Gather temporal signals from the current time and session start.
    pub fn gather(session_start: chrono::DateTime<Utc>) -> Self {
        let now = Utc::now();
        let duration = (now - session_start).num_seconds().max(0) as u64;
        Self {
            hour_of_day: now.hour(),
            day_of_week: now.weekday().number_from_monday(),
            session_duration_secs: duration,
        }
    }

    /// Returns a time-of-day bucket for pattern matching.
    /// morning (6-12), afternoon (12-18), evening (18-24), night (0-6).
    pub fn time_bucket(&self) -> &'static str {
        match self.hour_of_day {
            6..=11 => "morning",
            12..=17 => "afternoon",
            18..=23 => "evening",
            _ => "night",
        }
    }
}

impl Default for TemporalSignals {
    fn default() -> Self {
        Self {
            hour_of_day: Utc::now().hour(),
            day_of_week: Utc::now().weekday().number_from_monday(),
            session_duration_secs: 0,
        }
    }
}
