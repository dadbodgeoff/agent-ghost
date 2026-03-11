# ADE Agent Surface Documentation Packet

Status: March 11, 2026

This packet is the documentation set for remediating the ADE agent surface.

## Reading Order

1. `ADE_AGENT_SURFACE_REMEDIATION_SPEC.md`
   - authoritative design and system rules

2. `ADE_AGENT_SURFACE_IMPLEMENTATION_PLAN.md`
   - exact build order and file-level execution plan

3. `ADE_AGENT_SURFACE_VERIFICATION_PLAN.md`
   - required tests, gates, and acceptance checks

4. `ADE_AGENT_SURFACE_TASKS.md`
   - execution tracker and done definition

5. `ADE_AGENT_SURFACE_AGENT_HANDOFF.md`
   - final execution brief to hand directly to an implementation agent

## Intended Use

For design review:

- start with the remediation spec

For project management:

- use the tasks document and implementation plan together

For implementation:

- read the first four documents
- then execute from `ADE_AGENT_SURFACE_AGENT_HANDOFF.md`

## Authority

If any ambiguity appears during implementation:

- `ADE_AGENT_SURFACE_REMEDIATION_SPEC.md` is the source of truth for behavior
- `ADE_AGENT_SURFACE_AGENT_HANDOFF.md` is the source of truth for execution order and delivery expectations
