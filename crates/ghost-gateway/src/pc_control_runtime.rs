use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use ghost_pc_control::safety::circuit_breaker::CircuitBreakerSettings;
use ghost_pc_control::safety::{
    PcControlCircuitBreaker, PcControlConfig, PcControlPolicyHandle, PcControlPolicySnapshot,
    ScreenRegion,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcControlRuntimeSnapshot {
    pub revision: u64,
    pub enabled: bool,
    pub activation_state: String,
    pub effective_allowed_apps: Vec<String>,
    pub effective_blocked_hotkeys: Vec<String>,
    pub effective_safe_zone: Option<ScreenRegion>,
    pub last_applied_at: String,
    pub last_apply_source: String,
}

#[derive(Clone)]
pub struct PcControlRuntimeService {
    policy_handle: PcControlPolicyHandle,
    circuit_breaker: Arc<std::sync::Mutex<PcControlCircuitBreaker>>,
    snapshot: Arc<RwLock<PcControlRuntimeSnapshot>>,
    revision: Arc<AtomicU64>,
}

#[derive(Debug, Clone)]
pub struct PcControlRuntimeApplyResult {
    pub snapshot: PcControlRuntimeSnapshot,
    pub changed: bool,
}

impl PcControlRuntimeService {
    pub fn new(config: &PcControlConfig, source: &str) -> Self {
        let policy_snapshot = PcControlPolicySnapshot::from_config(config);
        let policy_handle = PcControlPolicyHandle::new(policy_snapshot.clone());
        let circuit_breaker = config.circuit_breaker();
        let snapshot = Arc::new(RwLock::new(Self::build_snapshot(
            1,
            source,
            policy_snapshot,
        )));

        Self {
            policy_handle,
            circuit_breaker,
            snapshot,
            revision: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn policy_handle(&self) -> PcControlPolicyHandle {
        self.policy_handle.clone()
    }

    pub fn circuit_breaker(&self) -> Arc<std::sync::Mutex<PcControlCircuitBreaker>> {
        Arc::clone(&self.circuit_breaker)
    }

    pub fn snapshot(&self) -> PcControlRuntimeSnapshot {
        self.snapshot
            .read()
            .expect("pc control runtime snapshot lock poisoned")
            .clone()
    }

    pub fn apply_config(
        &self,
        config: &PcControlConfig,
        source: &str,
    ) -> PcControlRuntimeApplyResult {
        let policy_snapshot = PcControlPolicySnapshot::from_config(config);
        let current_policy = self.policy_handle.snapshot();
        let target_breaker_settings = CircuitBreakerSettings {
            rate_limit: config.circuit_breaker.max_actions_per_second,
            failure_threshold: config.circuit_breaker.failure_threshold,
            cooldown: std::time::Duration::from_secs(config.circuit_breaker.cooldown_seconds),
        };

        {
            let breaker = self
                .circuit_breaker
                .lock()
                .expect("pc control circuit breaker lock poisoned");
            if current_policy == policy_snapshot && breaker.settings() == target_breaker_settings {
                return PcControlRuntimeApplyResult {
                    snapshot: self.snapshot(),
                    changed: false,
                };
            }
        }

        self.policy_handle.replace(policy_snapshot.clone());

        {
            let mut breaker = self
                .circuit_breaker
                .lock()
                .expect("pc control circuit breaker lock poisoned");
            breaker.reconfigure(
                target_breaker_settings.rate_limit,
                target_breaker_settings.failure_threshold,
                target_breaker_settings.cooldown,
            );
            if !config.enabled {
                breaker.reset();
            }
        }

        let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        let snapshot = Self::build_snapshot(revision, source, policy_snapshot);
        *self
            .snapshot
            .write()
            .expect("pc control runtime snapshot lock poisoned") = snapshot.clone();
        PcControlRuntimeApplyResult {
            snapshot,
            changed: true,
        }
    }

    fn build_snapshot(
        revision: u64,
        source: &str,
        policy_snapshot: PcControlPolicySnapshot,
    ) -> PcControlRuntimeSnapshot {
        PcControlRuntimeSnapshot {
            revision,
            enabled: policy_snapshot.enabled,
            activation_state: if policy_snapshot.enabled {
                "active".to_string()
            } else {
                "disabled".to_string()
            },
            effective_allowed_apps: policy_snapshot.allowed_apps,
            effective_blocked_hotkeys: policy_snapshot.blocked_hotkeys,
            effective_safe_zone: policy_snapshot.safe_zone,
            last_applied_at: chrono::Utc::now().to_rfc3339(),
            last_apply_source: source.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_runtime_apply_is_a_no_op() {
        let config = PcControlConfig::default();
        let runtime = PcControlRuntimeService::new(&config, "bootstrap");

        let first = runtime.snapshot();
        let applied = runtime.apply_config(&config, "watcher");

        assert!(!applied.changed);
        assert_eq!(applied.snapshot, first);
        assert_eq!(applied.snapshot.last_apply_source, "bootstrap");
    }
}
