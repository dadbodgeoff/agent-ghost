# Orchestration Build Brief

Status: March 11, 2026
Purpose: one-page implementation launch brief for a delivery agent.

This is the shortest usable entry point. It points back to the full package for authority.

## Objective

Rebuild ADE orchestration so trust graph, consensus, sybil posture, and A2A execution form one truthful, realtime-coherent control-plane surface.

## Authority

Follow, in order:

1. `docs/orchestration-remediation-package/ORCHESTRATION_MASTER_REMEDIATION_SPEC.md`
2. `docs/orchestration-remediation-package/ORCHESTRATION_ARCHITECTURE_AND_CONTRACTS.md`
3. `docs/orchestration-remediation-package/ORCHESTRATION_EXECUTION_PLAN.md`
4. `docs/orchestration-remediation-package/ORCHESTRATION_VERIFICATION_PLAN.md`
5. `docs/orchestration-remediation-package/ORCHESTRATION_AGENT_HANDOFF.md`

## Immediate Priorities

1. Fix backend truth before UI work.
2. Align SDK/OpenAPI types to backend truth.
3. Introduce one orchestration dashboard store.
4. Remove duplicate state ownership in orchestration UI.
5. Add blocking tests for backend, SDK, dashboard, and resync behavior.

## Success Condition

An operator can open the orchestration tab and trust it as the live ADE control-plane surface without having to guess whether any panel is placeholder, stale, or semantically wrong.
