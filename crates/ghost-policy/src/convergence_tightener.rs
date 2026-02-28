//! Convergence-level policy tightening (Req 13 AC3–AC5).
//!
//! Progressively restricts agent capabilities as intervention level rises.

use std::time::Duration;

use crate::context::{PolicyContext, ToolCall};
use crate::feedback::DenialFeedback;

/// Evaluates tool calls against convergence-level restrictions.
///
/// - Level 0–1: no restrictions
/// - Level 2: reduce proactive messaging (AC3)
/// - Level 3: session duration cap 120min, reflection limits (AC4)
/// - Level 4: task-only mode — disable personal/emotional tools, heartbeat, proactive (AC5)
pub struct ConvergencePolicyTightener;

impl ConvergencePolicyTightener {
    /// Returns `Some(DenialFeedback)` if the tool call is denied at the current
    /// convergence level, `None` if permitted.
    pub fn evaluate(&self, call: &ToolCall, ctx: &PolicyContext) -> Option<DenialFeedback> {
        match ctx.intervention_level {
            0..=1 => None,
            2 => self.evaluate_level_2(call, ctx),
            3 => self.evaluate_level_3(call, ctx),
            4.. => self.evaluate_level_4(call, ctx),
        }
    }

    fn evaluate_level_2(&self, call: &ToolCall, _ctx: &PolicyContext) -> Option<DenialFeedback> {
        if call.is_proactive_messaging() {
            return Some(
                DenialFeedback::new(
                    "Proactive messaging restricted at intervention level 2",
                    "convergence_level_2_proactive_restriction",
                )
                .with_alternatives(vec![
                    "Wait for user to initiate conversation".into(),
                    "Respond only to direct messages".into(),
                ]),
            );
        }
        None
    }

    fn evaluate_level_3(&self, call: &ToolCall, ctx: &PolicyContext) -> Option<DenialFeedback> {
        // Level 3 inherits level 2 restrictions
        if let Some(denial) = self.evaluate_level_2(call, ctx) {
            return Some(denial);
        }

        // Session duration cap: 120 minutes (AC4)
        if ctx.session_duration > Duration::from_secs(7200) {
            return Some(DenialFeedback::new(
                "Session duration exceeds 120-minute cap at intervention level 3",
                "convergence_level_3_session_cap",
            ));
        }

        // Reflection limits: max 3 reflections per session at L3 (AC4)
        if call.tool_name == "reflection_write" && ctx.session_reflection_count >= 3 {
            return Some(
                DenialFeedback::new(
                    "Reflection limit reached at intervention level 3 (max 3 per session)",
                    "convergence_level_3_reflection_limit",
                )
                .with_alternatives(vec![
                    "Focus on task execution rather than reflection".into(),
                ]),
            );
        }

        None
    }

    fn evaluate_level_4(&self, call: &ToolCall, ctx: &PolicyContext) -> Option<DenialFeedback> {
        // Level 4 inherits level 3 restrictions
        if let Some(denial) = self.evaluate_level_3(call, ctx) {
            return Some(denial);
        }

        // Task-only mode: disable personal/emotional tools
        if call.is_personal_emotional() {
            return Some(
                DenialFeedback::new(
                    "Personal/emotional tools disabled at intervention level 4 (task-only mode)",
                    "convergence_level_4_task_only",
                )
                .with_alternatives(vec![
                    "Focus on task-related operations only".into(),
                ]),
            );
        }

        // Disable heartbeat
        if call.is_heartbeat() {
            return Some(DenialFeedback::new(
                "Heartbeat disabled at intervention level 4",
                "convergence_level_4_heartbeat_disabled",
            ));
        }

        None
    }
}
