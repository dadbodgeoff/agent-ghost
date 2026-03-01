# ADE Design Plan — Gap & Flaw Audit (v2)

**Date:** 2026-02-28
**Scope:** `docs/ADE_DESIGN_PLAN.md` (1899 lines, 16 sections, 4 phases)
**Method:** Every claim cross-referenced against actual source code (not prior
audit documents, which contain stale findings). SQL schemas, API handlers,
dashboard routes, crate structures verified by reading the code directly.
Technology claims web-verified (OTel GenAI conventions, A2A protocol, Svelte 5).

---

## Meta-Finding: Prior Audits Are Stale

The design plan incorporates findings from `CONNECTIVITY_AUDIT.md` and
`docs/ADE_INTEGRATION_AUDIT.md`. Many of those findings have been fixed
in the codebase since the audits were written. The plan must be reconciled
with the CURRENT code state, not the audit documents.

Stale findings that the plan treats as open (but are actually fixed):

- SQL column mismatches in convergence-monitor — FIXED
- Mesh router never merged — FIXED (build_router line 419)
- Push routes never mounted — FIXED (build_router line 425)
- WsEvent ScoreUpdate/InterventionChange never sent — FIXED (convergence_watcher.rs)
- pause/resume don't broadcast WS events — FIXED (safety.rs)
- OAuth routes are stubs — FIXED (all 5 call state.oauth_broker)
- Handlers silently return 200 on DB errors — FIXED (all return 500)
- memory_snapshots has no write path — FIXED (POST /api/memory)
- goal_proposals never written in production — FIXED (runner.rs:243)

Impact: Phase 1 Week 1 includes no-op tasks. Contract alignment is higher
than the claimed 45%.

---

## Severity Legend

- BLOCKER — Will prevent a phase from starting or completing.
- CRITICAL — Significant gap that will cause rework if not addressed.
- HIGH — Missing detail that will stall implementation.
- MEDIUM — Oversight that should be addressed but won't block progress.
- LOW — Minor inconsistency or nice-to-have.

---

## 1. BLOCKER: REST Endpoints Have No Authentication

Verified: build_router() applies no auth middleware. validate_token() exists
but is only called by the WS handler. Any HTTP request succeeds regardless
of Authorization header.

Additional gap: The WS handler accepts connections with no token param.
The `if let Some(token)` guard means missing tokens bypass auth entirely.
The plan says "WebSocket: Validated" — this is incorrect.

Action: Phase 1 prerequisite must add tower auth middleware for REST AND
fix WS handler to reject missing tokens (when GHOST_TOKEN is set).

---

## 2. BLOCKER: CostTracker Is Never Written To

Verified: CostTracker::new() is called in bootstrap. get_daily_total() and
get_compaction_cost() are read by costs.rs and spending_cap.rs. But no
write method (record) is ever called anywhere in the codebase.

/api/costs always returns zeros. SpendingCapEnforcer reads empty data,
so spending caps are never enforced.

Action: Wire CostTracker.record() into the LLM call path in ghost-agent-loop.

---

## 3. CRITICAL: No Standard Error Response Contract

Verified: Handlers now return proper HTTP status codes, but error response
shapes are inconsistent across handlers. The dashboard needs a consistent
contract to implement error handling. The plan never defines one.

Action: Define a standard error envelope in Phase 1 prerequisites.

---

## 4. CRITICAL: /api/goals vs /api/proposals Path Decision Unmade

Verified: Code uses /api/goals. Plan introduces /api/proposals. The plan
acknowledges the conflict but doesn't resolve it. All UI mockups and
appendices use /proposals while code uses /goals.

Action: Make the decision. Recommendation: keep /api/goals, add missing
features (detail endpoint, filters) to existing path.

---

## 5. CRITICAL: No API Versioning Strategy

Verified: All 33 endpoints under /api/ with no version prefix. Plan adds
22 new endpoints across 4 phases without discussing versioning.

Action: Add backward-compatibility contract: new fields may be added,
existing fields never removed or renamed.

---

## 6. CRITICAL: No PII Redaction Design for Session Replay

Verified: Session Replay (6.4) shows full LLM conversations. Plan mentions
"(with PII redaction)" once with no design. cortex-privacy exists but
integration is not specified.

Action: Add PII redaction section to 6.4 with read-time redaction strategy.

---

## 7. CRITICAL: No Accessibility Plan

Verified: Section 9 covers color, typography, layout but never mentions
accessibility. Color-only severity encoding fails WCAG. Complex widgets
(DAG, graph, scrubber) have no keyboard/screen reader alternatives.

Action: Add accessibility section covering ARIA roles, keyboard navigation,
text/icon alternatives for color, screen reader announcements.

---

## 8. CRITICAL: Dashboard Store Migration Strategy Missing

Verified: All 3 stores use Svelte 4 writable(). Plan says migrate to
Svelte 5 runes but specifies no migration order, no import rule changes,
no class-based pattern guidance.

Action: Add incremental migration strategy to 5.1 with class-based pattern.

---

## 9. HIGH: Browser Extension Orphaned From Plan

Verified: Section 4.9 lists extension as scaffolded. Never mentioned again
in any phase deliverable.

Action: Add to Phase 4 or explicitly defer with rationale.

---

## 10. HIGH: OTel Integration Underestimates Complexity

Verified: Section 7.2 allocates OTel as one feature within a 7-week phase
alongside 5 other major features. Trace storage design is completely missing.

Action: Expand 7.2 with trace storage design and realistic effort estimate.

---

## 11. HIGH: Offline Safety Action Queue Is Dangerous

Verified: Section 8.4 says queue safety actions for offline replay. This
contradicts 11.1 monotonic escalation principle.

Action: Safety actions must NOT be queued offline.

---

## 12. HIGH: No Concurrency Design for Proposal Review

Verified: Section 6.3 shows approve/reject but doesn't address simultaneous
review by multiple users. Backend has safeguard but UI behavior is unspecified.

Action: Add concurrency handling to 6.3.

---

## 13. HIGH: No Performance Budget

Verified: Plan says "dozens of values update per second" but specifies no
targets. broadcast::channel(256) silently drops events when full.

Action: Add performance budgets to section 14.

---

## 14. HIGH: Unused Config Fields Not Addressed

Verified: soul_drift_threshold, convergence_profile, model_providers loaded
into AppState but never read. agent.capabilities dropped during registration.

Action: Wire config fields to consumers or document which phase uses each.

---

## 15. MEDIUM: Dashboard Has No Test Infrastructure

Verified: package.json has zero test dependencies. Plan lists tools in 14.1
but no phase includes setup.

Action: Add Phase 1 task to install test infrastructure.

---

## 16. MEDIUM: No Graceful Degradation When Monitor Is Down

Verified: Health endpoint returns monitor status but dashboard designs don't
show degraded state for convergence views.

Action: Add degraded-state UI to 5.5.

---

## 17. MEDIUM: No Multi-Tab WebSocket Handling

Verified: Multiple tabs create multiple WS connections. Offline queue could
replay from multiple tabs.

Action: Add multi-tab coordination to 5.1.

---

## 18. MEDIUM: No Agent Deletion Cascade Design

Verified: Append-only triggers prevent DELETE. Deleted agents leave orphaned
data. Plan doesn't specify soft-delete behavior.

Action: Add soft-delete note to 5.3.

---

## 19. MEDIUM: Risk Register Missing Key Risks

Section 16 has 10 entries but misses: CostTracker dead, PII exposure,
offline safety queue, API versioning skew, multi-tab WS explosion.

---

## 20. MEDIUM: Crate Dependency Claims Inaccurate

Verified: Section 4.8 says 9 deps needed. Several already in Cargo.toml
(just not imported): cortex-temporal, cortex-convergence, ghost-skills,
ghost-channels, ghost-policy.

Action: Distinguish "add to Cargo.toml" from "already dep, needs import."

---

## 21. LOW: Timeline Estimates Are Aggressive

Phase 1 has ~25 deliverables in 3 weeks. No developer count assumption.
No buffer between phases.

Action: Add developer count and buffer weeks.

---

## Summary

| Severity | Count |
|---|---|
| BLOCKER | 2 |
| CRITICAL | 5 |
| HIGH | 6 |
| MEDIUM | 6 |
| LOW | 1 |
| Meta | 1 |
| Total | 21 |
