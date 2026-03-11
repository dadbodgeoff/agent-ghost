use std::sync::{Arc, RwLock};

use super::input_validator::ScreenRegion;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcControlPolicySnapshot {
    pub enabled: bool,
    pub allowed_apps: Vec<String>,
    pub safe_zone: Option<ScreenRegion>,
    pub blocked_hotkeys: Vec<String>,
}

impl PcControlPolicySnapshot {
    pub fn from_config(config: &super::config::PcControlConfig) -> Self {
        Self {
            enabled: config.enabled,
            allowed_apps: config.allowed_apps.clone(),
            safe_zone: config.safe_zone.clone(),
            blocked_hotkeys: config.blocked_hotkeys.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PcControlPolicyHandle {
    snapshot: Arc<RwLock<PcControlPolicySnapshot>>,
}

impl PcControlPolicyHandle {
    pub fn new(snapshot: PcControlPolicySnapshot) -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(snapshot)),
        }
    }

    pub fn snapshot(&self) -> PcControlPolicySnapshot {
        self.snapshot
            .read()
            .expect("pc control policy lock poisoned")
            .clone()
    }

    pub fn replace(&self, snapshot: PcControlPolicySnapshot) {
        *self
            .snapshot
            .write()
            .expect("pc control policy lock poisoned") = snapshot;
    }

    pub fn is_enabled(&self) -> bool {
        self.snapshot().enabled
    }
}
