# ADE Security Remediation: Independent Review Checklist

Status: draft for implementation orchestration on March 11, 2026.

This checklist is for an external engineer reviewing the completed remediation.
It assumes the reviewer did not implement the work.

## 1. Contract Review

Answer yes/no:

- Is the safety-status contract canonical and explicitly typed?
- Does the dashboard render kill state without string-to-number inference?
- Do SDK types match the gateway fields in current use?
- Are audit query and export semantics aligned?
- Are canonical event types and severities clearly owned?

## 2. Security Affordance Review

Answer yes/no:

- Is `kill-all` unavailable to non-superadmins before invocation?
- Are sandbox review decision controls unavailable to principals lacking the
  required permission?
- Is shell shortcut behavior aligned with page behavior?
- Is command palette behavior aligned with page behavior?

## 3. Failure Semantics Review

Answer yes/no:

- Can each Security section show error independently of empty state?
- Is a sandbox review fetch failure visible as failure rather than “no reviews”?
- Does degraded or partial state remain explicit?

## 4. Evidence Integrity Review

Answer yes/no:

- Do Security filters only expose contract-valid values?
- Does the timeline render all canonical severities correctly?
- Does export reflect active filters rather than a broader dataset?
- Do websocket-driven events refresh the evidence pane?

## 5. Verification Review

Answer yes/no:

- Were the commands in `13_VERIFICATION_COMMAND_RUNBOOK.md` executed?
- Are gateway tests sufficient for contract and auth behavior?
- Are SDK tests sufficient for contract parity?
- Are dashboard tests sufficient for authorized, unauthorized, and degraded
  states?
- Were drift checks rerun after contract-affecting changes?

## 6. Signoff Outcome

Select one:

- `Sign off`
- `Sign off with non-blocking observations`
- `Do not sign off`

If not signing off, identify the exact blocking item by requirement or work
package ID.
