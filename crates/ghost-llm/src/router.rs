//! Model router with complexity classification (Req 21 AC2, AC6).

use std::sync::Arc;

use crate::provider::LLMProvider;

/// Complexity tier for model selection.
///
/// Ordering: Local < Free < Cheap < Standard < Premium.
/// `Local` maps to an Ollama/local model with zero token cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComplexityTier {
    /// Local model (e.g., Ollama Qwen-2.5-7B). Zero cost.
    Local,
    Free,
    Cheap,
    Standard,
    Premium,
}

/// Classifies message complexity to select the appropriate model tier.
pub struct ComplexityClassifier;

impl ComplexityClassifier {
    /// Classify a user message into a complexity tier.
    ///
    /// Slash command overrides take precedence, then heuristics.
    pub fn classify(message: &str, is_heartbeat: bool, convergence_level: u8) -> ComplexityTier {
        // Convergence downgrade at L3+ (AC6)
        if convergence_level >= 3 {
            return if message.len() < 100 {
                ComplexityTier::Free
            } else {
                ComplexityTier::Cheap
            };
        }

        // Slash command overrides
        if message.starts_with("/quick") {
            return ComplexityTier::Free;
        }
        if message.starts_with("/deep") {
            return ComplexityTier::Premium;
        }
        if message.starts_with("/model") {
            return ComplexityTier::Standard;
        }

        // Heartbeat → Free
        if is_heartbeat {
            return ComplexityTier::Free;
        }

        // Heuristic classification
        let len = message.len();
        let has_tool_keywords = message.contains("function")
            || message.contains("write")
            || message.contains("create")
            || message.contains("implement")
            || message.contains("debug")
            || message.contains("analyze");

        if len < 20 && !has_tool_keywords {
            ComplexityTier::Free
        } else if len < 100 && !has_tool_keywords {
            ComplexityTier::Cheap
        } else if has_tool_keywords || len > 500 {
            ComplexityTier::Premium
        } else {
            ComplexityTier::Standard
        }
    }
}

/// Routes requests to the appropriate provider based on complexity tier.
pub struct ModelRouter {
    /// Providers indexed by tier: [Local, Free, Cheap, Standard, Premium].
    providers: [Option<Arc<dyn LLMProvider>>; 5],
}

impl ModelRouter {
    pub fn new() -> Self {
        Self {
            providers: [None, None, None, None, None],
        }
    }

    /// Set the provider for a given tier.
    pub fn set_provider(&mut self, tier: ComplexityTier, provider: Arc<dyn LLMProvider>) {
        self.providers[tier as usize] = Some(provider);
    }

    /// Get the provider for a given tier, falling back through the chain.
    ///
    /// Fallback order: Local → Free → Cheap → Standard → Premium.
    /// Tries the requested tier first, then falls back upward to higher tiers.
    pub fn get_provider(&self, tier: ComplexityTier) -> Option<Arc<dyn LLMProvider>> {
        let idx = tier as usize;
        // Try requested tier first, then fall back upward
        for i in idx..5 {
            if let Some(ref p) = self.providers[i] {
                if i != idx {
                    tracing::debug!(
                        requested = ?tier,
                        resolved_idx = i,
                        "model router: requested tier unavailable, fell back to higher tier"
                    );
                }
                return Some(Arc::clone(p));
            }
        }
        // Fall back downward if nothing above
        for i in (0..idx).rev() {
            if let Some(ref p) = self.providers[i] {
                tracing::debug!(
                    requested = ?tier,
                    resolved_idx = i,
                    "model router: no higher tier available, fell back to lower tier"
                );
                return Some(Arc::clone(p));
            }
        }
        tracing::warn!(
            requested = ?tier,
            "model router: no providers configured — returning None"
        );
        None
    }
}

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new()
    }
}
