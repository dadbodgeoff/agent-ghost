use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Memory importance level. Affects decay rate, compression priority, and retrieval ranking.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Importance {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

impl Importance {
    /// Weight multiplier used in decay and ranking calculations.
    pub fn weight(self) -> f64 {
        match self {
            Self::Low => 0.8,
            Self::Normal => 1.0,
            Self::High => 1.5,
            Self::Critical => 2.0,
        }
    }
}

impl PartialOrd for Importance {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Importance {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let rank = |i: &Importance| -> u8 {
            match i {
                Importance::Low => 0,
                Importance::Normal => 1,
                Importance::High => 2,
                Importance::Critical => 3,
            }
        };
        rank(self).cmp(&rank(other))
    }
}
