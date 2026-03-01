//! ProposalRouter — assembles context, runs pre-checks, delegates to validator (Req 33).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use cortex_core::config::ReflectionConfig;
use cortex_core::memory::BaseMemory;
use cortex_core::models::proposal::{ProposalDecision, ProposalOperation};
use cortex_core::traits::convergence::{Proposal, ProposalContext};
use ghost_policy::feedback::DenialFeedback;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Score cache entry with TTL (Req 33 AC8).
#[derive(Debug, Clone)]
struct CachedScore {
    score: f64,
    level: u8,
    cached_at: Instant,
}

/// Pending proposal record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingProposal {
    pub proposal: Proposal,
    pub decision: ProposalDecision,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_partial_run: bool,
}

/// Routes proposals through validation pipeline.
pub struct ProposalRouter {
    /// Score cache with 30s TTL.
    score_cache: BTreeMap<Uuid, CachedScore>,
    score_cache_ttl: Duration,
    /// Pending proposals by ID.
    pending: BTreeMap<Uuid, PendingProposal>,
    /// Pending proposals by goal (for superseding).
    pending_by_goal: BTreeMap<String, Uuid>,
    /// Rejection records for re-proposal guard.
    rejection_records: Vec<(Uuid, serde_json::Value)>,
    /// Timeout for pending proposals (default 24h).
    pub proposal_timeout: Duration,
    /// DenialFeedback queue — cleared after one prompt inclusion.
    denial_feedback: Vec<DenialFeedback>,
    /// Persistent feedback for pending-review proposals.
    pending_review_feedback: Vec<DenialFeedback>,
    /// Session reflection count for pre-checks.
    session_reflection_counts: BTreeMap<Uuid, u32>,
    /// Last reflection time per session for cooldown.
    last_reflection_time: BTreeMap<Uuid, Instant>,
}

impl ProposalRouter {
    pub fn new() -> Self {
        Self {
            score_cache: BTreeMap::new(),
            score_cache_ttl: Duration::from_secs(30),
            pending: BTreeMap::new(),
            pending_by_goal: BTreeMap::new(),
            rejection_records: Vec::new(),
            proposal_timeout: Duration::from_secs(86400),
            denial_feedback: Vec::new(),
            pending_review_feedback: Vec::new(),
            session_reflection_counts: BTreeMap::new(),
            last_reflection_time: BTreeMap::new(),
        }
    }

    /// Assemble ProposalContext for validation (Req 33 AC1).
    pub fn assemble_context(
        &self,
        proposal: &Proposal,
        active_goals: Vec<BaseMemory>,
        recent_agent_memories: Vec<BaseMemory>,
        convergence_score: f64,
        convergence_level: u8,
    ) -> ProposalContext {
        let session_reflection_count = self
            .session_reflection_counts
            .get(&proposal.session_id)
            .copied()
            .unwrap_or(0);

        ProposalContext {
            active_goals,
            recent_agent_memories,
            convergence_score,
            convergence_level,
            session_id: proposal.session_id,
            session_reflection_count,
            session_memory_write_count: 0,
            daily_memory_growth_rate: 0,
            reflection_config: ReflectionConfig::default(),
            caller: proposal.proposer.clone(),
        }
    }

    /// Run reflection pre-check (Req 33 AC5).
    /// Returns AutoRejected if reflection limits exceeded.
    pub fn reflection_precheck(
        &self,
        proposal: &Proposal,
        config: &ReflectionConfig,
    ) -> Option<ProposalDecision> {
        if proposal.operation != ProposalOperation::ReflectionWrite {
            return None;
        }

        // Check max_per_session
        let count = self
            .session_reflection_counts
            .get(&proposal.session_id)
            .copied()
            .unwrap_or(0);
        if count >= config.max_per_session {
            return Some(ProposalDecision::AutoRejected);
        }

        // Check cooldown
        if let Some(last) = self.last_reflection_time.get(&proposal.session_id) {
            if last.elapsed() < Duration::from_secs(config.cooldown_seconds) {
                return Some(ProposalDecision::AutoRejected);
            }
        }

        // Check max_depth (from content if available)
        if let Some(depth) = proposal.content.get("depth").and_then(|v| v.as_u64()) {
            if depth as u8 > config.max_depth {
                return Some(ProposalDecision::AutoRejected);
            }
        }

        None
    }

    /// Check for superseding: mark old pending proposal as Superseded (Req 33 AC3).
    pub fn check_superseding(&mut self, proposal: &Proposal) {
        if proposal.operation == ProposalOperation::GoalChange {
            if let Some(goal_key) = proposal.content.get("goal_text").and_then(|v| v.as_str()) {
                if let Some(old_id) = self.pending_by_goal.get(goal_key).copied() {
                    if let Some(old) = self.pending.get_mut(&old_id) {
                        if old.resolved_at.is_none() {
                            old.decision = ProposalDecision::Superseded;
                            old.resolved_at = Some(chrono::Utc::now());
                        }
                    }
                }
                self.pending_by_goal
                    .insert(goal_key.to_string(), proposal.id);
            }
        }
    }

    /// Re-proposal guard: check if identical content was previously rejected (Req 33 AC4).
    pub fn is_resubmission(&self, proposal: &Proposal) -> bool {
        self.rejection_records
            .iter()
            .any(|(_, content)| *content == proposal.content)
    }

    /// Get cached convergence score if within TTL (Req 33 AC8).
    pub fn get_cached_score(&self, agent_id: &Uuid) -> Option<(f64, u8)> {
        self.score_cache.get(agent_id).and_then(|cached| {
            if cached.cached_at.elapsed() < self.score_cache_ttl {
                Some((cached.score, cached.level))
            } else {
                None
            }
        })
    }

    /// Cache a convergence score.
    pub fn cache_score(&mut self, agent_id: Uuid, score: f64, level: u8) {
        self.score_cache.insert(
            agent_id,
            CachedScore {
                score,
                level,
                cached_at: Instant::now(),
            },
        );
    }

    /// Record a proposal decision.
    pub fn record_decision(
        &mut self,
        proposal: Proposal,
        decision: ProposalDecision,
        is_partial_run: bool,
    ) {
        if decision == ProposalDecision::AutoRejected {
            self.rejection_records
                .push((proposal.id, proposal.content.clone()));
        }

        if decision == ProposalDecision::HumanReviewRequired {
            self.pending_review_feedback.push(DenialFeedback::new(
                "Proposal requires human review",
                "human_review_required",
            ));
        }

        if proposal.operation == ProposalOperation::ReflectionWrite
            && decision == ProposalDecision::AutoApproved
        {
            *self
                .session_reflection_counts
                .entry(proposal.session_id)
                .or_insert(0) += 1;
            self.last_reflection_time
                .insert(proposal.session_id, Instant::now());
        }

        self.pending.insert(
            proposal.id,
            PendingProposal {
                proposal,
                decision,
                created_at: chrono::Utc::now(),
                resolved_at: if matches!(
                    decision,
                    ProposalDecision::AutoApproved
                        | ProposalDecision::AutoRejected
                        | ProposalDecision::ApprovedWithFlags
                ) {
                    Some(chrono::Utc::now())
                } else {
                    None
                },
                is_partial_run,
            },
        );
    }

    /// Take denial feedback for next prompt inclusion (cleared after one use).
    /// Pending-review feedback persists (Req 33 AC6).
    pub fn take_denial_feedback(&mut self) -> Vec<DenialFeedback> {
        let mut feedback = std::mem::take(&mut self.denial_feedback);
        feedback.extend(self.pending_review_feedback.iter().cloned());
        feedback
    }

    /// Add denial feedback.
    pub fn add_denial_feedback(&mut self, feedback: DenialFeedback) {
        self.denial_feedback.push(feedback);
    }

    /// Resolve timed-out proposals (Req 33 AC2).
    pub fn resolve_timeouts(&mut self) {
        let now = chrono::Utc::now();
        for pending in self.pending.values_mut() {
            if pending.resolved_at.is_none() {
                let age = now
                    .signed_duration_since(pending.created_at)
                    .to_std()
                    .unwrap_or(Duration::ZERO);
                if age >= self.proposal_timeout {
                    pending.decision = ProposalDecision::TimedOut;
                    pending.resolved_at = Some(now);
                }
            }
        }
    }
}

impl Default for ProposalRouter {
    fn default() -> Self {
        Self::new()
    }
}
