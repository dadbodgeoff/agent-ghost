# Execution Reliability Package

This package is the implementation dossier for hardening ADE live execution, recovery, replay safety, and cancellation to a near-zero loss / duplication standard.

Reading order:

1. `01-reliability-charter.md`
2. `02-current-state-and-gap-audit.md`
3. `03-target-system-spec.md`
4. `04-delivery-plan.md`
5. `05-verification-and-runbooks.md`
6. `06-agent-build-brief.md`

Package intent:

- Define the reliability target in precise terms.
- Capture the current-system gaps without ambiguity.
- Specify the target execution model, data model, invariants, and failure handling.
- Break the work into shippable implementation phases.
- Provide a final build brief an implementation agent can execute start to finish.

Non-goal:

- This package does not try to make arbitrary external side effects magically exactly-once. It scopes reliable semantics explicitly and requires fail-closed behavior for unsupported side-effect classes.
