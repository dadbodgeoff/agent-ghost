# Orchestration Remediation Package

Status: March 11, 2026
Owner: Engineering
Purpose: define the full document package required to rebuild the ADE orchestration surface as a coherent, production-grade subsystem.

This package is designed to end in a single agent-consumable execution brief. The brief is not standalone authority; it inherits all architectural, contract, sequencing, and verification requirements from the documents in this folder.

## Reading Order

1. `ORCHESTRATION_MASTER_REMEDIATION_SPEC.md`
2. `ORCHESTRATION_ARCHITECTURE_AND_CONTRACTS.md`
3. `ORCHESTRATION_EXECUTION_PLAN.md`
4. `ORCHESTRATION_VERIFICATION_PLAN.md`
5. `ORCHESTRATION_AGENT_HANDOFF.md`

## Package Rule

If any document outside this package conflicts with this package on orchestration work, this package wins for orchestration scope.

## What This Package Produces

- one authoritative target architecture for orchestration across gateway, SDK, dashboard, websocket, and persistence
- one implementation sequence that can be executed without inventing requirements mid-flight
- one verification standard that blocks partial completion
- one final handoff brief that an implementation agent can execute start to finish

## Non-Negotiable Standard

- No orchestration panel may present inferred or placeholder data as live truth.
- No UI component may own a shadow copy of critical orchestration state.
- No backend route may silently swallow schema or query drift.
- No trust, consensus, sybil, or A2A metric may be shown unless its derivation is explicit and test-covered.
- No implementation is complete until the orchestration surface stays coherent under realtime change and reconnect/resync.

## Current Audit Summary

This package is based on current code inspection. The key failures that motivated it are:

- trust graph edge queries do not align with the actual delegation schema
- consensus numbers are not backed by a real consensus/vote model
- orchestration dashboard panels do not update coherently with live ADE events
- A2A task lifecycle is only partially wired
- A2A UI state is duplicated across route and child component
- sybil metrics are placeholder-grade rather than decision-grade

Primary implementation targets are documented in the files below.
