//! Fallback chain with auth rotation and exponential backoff (Req 21 AC3).
//! Provider circuit breaker (A22.2) — INDEPENDENT from tool circuit breaker.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::Rng;

use crate::provider::{ChatMessage, CompletionResult, LLMError, LLMProvider, ToolSchema};

/// Circuit breaker state for a single provider (A22.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CBState {
    Closed,
    Open,
    HalfOpen,
}

/// Per-provider circuit breaker.
pub struct ProviderCircuitBreaker {
    state: CBState,
    consecutive_failures: u32,
    threshold: u32,
    cooldown: Duration,
    last_failure: Option<Instant>,
}

impl ProviderCircuitBreaker {
    pub fn new(threshold: u32, cooldown: Duration) -> Self {
        Self {
            state: CBState::Closed,
            consecutive_failures: 0,
            threshold,
            cooldown,
            last_failure: None,
        }
    }

    pub fn state(&self) -> CBState {
        self.state
    }

    /// Check if the circuit breaker allows a request.
    pub fn can_attempt(&mut self) -> bool {
        match self.state {
            CBState::Closed => true,
            CBState::Open => {
                if let Some(last) = self.last_failure {
                    if last.elapsed() >= self.cooldown {
                        self.state = CBState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CBState::HalfOpen => true,
        }
    }

    /// Record a successful call.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.state = CBState::Closed;
    }

    /// Record a failed call.
    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        self.last_failure = Some(Instant::now());

        match self.state {
            CBState::Closed => {
                if self.consecutive_failures >= self.threshold {
                    self.state = CBState::Open;
                }
            }
            CBState::HalfOpen => {
                // HalfOpen + failure → Open (cooldown resets)
                self.state = CBState::Open;
            }
            CBState::Open => {}
        }
    }
}

/// An auth profile for a provider (API key + optional org).
#[derive(Debug, Clone)]
pub struct AuthProfile {
    pub api_key: String,
    pub org_id: Option<String>,
}

/// Fallback chain: rotates auth profiles on 401/429, falls back to next
/// provider, exponential backoff + jitter, 30s total retry budget.
pub struct FallbackChain {
    providers: Vec<(
        Arc<dyn LLMProvider>,
        Vec<AuthProfile>,
        ProviderCircuitBreaker,
    )>,
    total_retry_budget: Duration,
}

impl FallbackChain {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            total_retry_budget: Duration::from_secs(30),
        }
    }

    /// Add a provider with its auth profiles.
    pub fn add_provider(&mut self, provider: Arc<dyn LLMProvider>, profiles: Vec<AuthProfile>) {
        let cb = ProviderCircuitBreaker::new(3, Duration::from_secs(300));
        self.providers.push((provider, profiles, cb));
    }

    /// Attempt completion with fallback logic.
    ///
    /// On 401/429: rotate auth profiles for the current provider.
    /// When all profiles exhausted: fall back to next provider.
    /// Exponential backoff + jitter: 1s, 2s, 4s, 8s base delays.
    /// 30s total retry budget.
    pub async fn complete(
        &mut self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
    ) -> Result<CompletionResult, LLMError> {
        let start = Instant::now();

        for (provider, profiles, cb) in &mut self.providers {
            if start.elapsed() >= self.total_retry_budget {
                break;
            }

            if !cb.can_attempt() {
                continue;
            }

            // Track which auth profile we're using for this provider.
            let mut profile_index: usize = 0;

            // Apply initial auth profile if available
            if !profiles.is_empty() {
                let profile = &profiles[profile_index];
                provider.update_auth(&profile.api_key, profile.org_id.as_deref());
                tracing::debug!(
                    provider = provider.name(),
                    profile_index,
                    total_profiles = profiles.len(),
                    "applied initial auth profile"
                );
            } else {
                tracing::debug!(
                    provider = provider.name(),
                    "no auth profiles configured — using provider defaults"
                );
            }

            // Exponential backoff attempts: 1s, 2s, 4s, 8s base delays
            let backoffs = [1u64, 2, 4, 8];
            for (attempt, &delay_secs) in backoffs.iter().enumerate() {
                if start.elapsed() >= self.total_retry_budget {
                    break;
                }

                match provider.complete(messages, tools).await {
                    Ok(result) => {
                        cb.record_success();
                        return Ok(result);
                    }
                    Err(LLMError::AuthFailed(_)) | Err(LLMError::RateLimited { .. }) => {
                        // Rotate to next auth profile for this provider
                        if !profiles.is_empty() {
                            profile_index = (profile_index + 1) % profiles.len();
                            // Apply the rotated auth profile to the provider
                            let profile = &profiles[profile_index];
                            provider.update_auth(&profile.api_key, profile.org_id.as_deref());
                            tracing::warn!(
                                provider = provider.name(),
                                attempt,
                                profile_index,
                                "auth/rate error, rotated to profile {}/{}",
                                profile_index + 1,
                                profiles.len()
                            );
                            // If we've cycled through all profiles, break to next provider
                            if attempt > 0 && profile_index == 0 {
                                cb.record_failure();
                                break;
                            }
                        }
                        cb.record_failure();
                    }
                    Err(e) => {
                        cb.record_failure();
                        tracing::warn!(
                            provider = provider.name(),
                            attempt,
                            error = %e,
                            "provider error"
                        );
                    }
                }

                if attempt < backoffs.len() - 1 {
                    // Exponential backoff with jitter: base_delay ± 25%
                    let base_ms = delay_secs * 1000;
                    let jitter_range = base_ms / 4; // ±25%
                    let jitter = if jitter_range > 0 {
                        let mut rng = rand::thread_rng();
                        rng.gen_range(-(jitter_range as i64)..=(jitter_range as i64))
                    } else {
                        0
                    };
                    let sleep_ms = (base_ms as i64 + jitter).max(100) as u64;
                    tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                }
            }
        }

        Err(LLMError::Unavailable(
            "all providers exhausted within retry budget".into(),
        ))
    }
}

impl Default for FallbackChain {
    fn default() -> Self {
        Self::new()
    }
}

impl FallbackChain {
    /// Get the token pricing from the first available (non-open-circuit) provider.
    /// Falls back to zero pricing if no providers are configured.
    pub fn current_pricing(&self) -> crate::provider::TokenPricing {
        for (provider, _, cb) in &self.providers {
            if cb.state() != CBState::Open {
                return provider.token_pricing();
            }
        }
        // All providers are circuit-broken or none configured.
        crate::provider::TokenPricing {
            input_per_1k: 0.0,
            output_per_1k: 0.0,
        }
    }
}
