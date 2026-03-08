# Skill Pipeline Hardening Design

## Document Intent

This document defines the scope, target architecture, and implementation plan
for repairing the current skill pipeline so it becomes truthful, safe, and
operationally coherent.

This is a design document, not a task list and not an implementation note.
Its purpose is to establish the engineering decisions that later flow into
`implementation.md` and then `task.md`.

## Verification Summary

This design is based on direct verification of the live code paths, not only on
earlier architecture docs.

Verified by inspection:

- `crates/ghost-gateway/src/api/skills.rs`
- `crates/ghost-gateway/src/api/skill_execute.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-agent-loop/src/tools/skill_bridge.rs`
- `crates/ghost-skills/src/registry.rs`
- `crates/ghost-skills/src/sandbox/wasm_sandbox.rs`
- `crates/ghost-gateway/src/cli/skill.rs`
- `dashboard/src/routes/skills/+page.svelte`
- `agent/AGENT_ARCHITECTURE.md`
- `wiki/ghost-skills.md`

Verified by test execution:

- `cargo test -p ghost-skills` passed
- `cargo test -p ghost-gateway api::skills` matched zero tests, which confirms
  the gateway skill API currently lacks direct integration coverage

Important correction from the initial audit:

- `POST /api/skills/:name/execute` is not unauthenticated. It is mounted under
  `operator_routes`.
- The real problem is not missing route auth. The real problem is that this
  endpoint bypasses the canonical runtime skill-policy path.

## Current Verified Architecture

The live system is not a file-backed `SKILL.md` runtime today.

The current runtime model is:

1. Gateway bootstrap constructs a single in-memory `HashMap<String, Box<dyn Skill>>`
   containing safety, git, code-analysis, bundled, PC control, and delegation
   skills.
2. Runtime execution exposes those skills to the agent loop through
   `SkillBridge`, which registers one tool per skill.
3. Tool execution goes through the runtime policy engine, capability grants,
   convergence logic, and the tool executor.
4. The REST skill APIs and dashboard skill page do not use the same authority
   model as the runtime tool path.

The documented model is different:

- architecture docs still describe directory-backed `SKILL.md` skills,
  signature verification, registry loading, and a WASM sandboxed path as if
  those are the live operating model
- CLI direct mode expects `manifest.json`
- `ghost-skills` contains registry, sandbox, recorder, proposer, and matcher
  components that are largely isolated from the live gateway execution path

This mismatch is the root governance problem.

## Verified Defects To Fix

### 1. Contract Drift Between Skill APIs and Runtime Truth

`/api/skills` currently reports all bootstrapped skills as installed and always
returns an empty `available` list, while install and uninstall mutate the
`installed_skills` table separately.

Result:

- install state is not authoritative
- the dashboard is told a story that the runtime does not follow
- the SDK and CLI inherit that false contract

### 2. Direct Execute Path Bypasses Canonical Runtime Policy Flow

`/api/skills/:name/execute` looks up the skill in the shared map and calls
`skill.execute(...)` directly.

Result:

- execution bypasses the normal agent tool dispatch path
- per-agent skill exposure rules are skipped
- policy capability checks are skipped
- any future auditing, metering, or preflight added to the canonical skill
  runtime path can drift from this endpoint

### 3. Execute Path Uses Read-Only DB Connections

The execute endpoint acquires a read connection from `DbPool::read()`. Those
connections are opened read-only.

Result:

- write-capable skills cannot execute reliably through the REST skill endpoint
- the endpoint contract implies support it does not actually provide

### 4. Capability Disclosure Is Misleading

The skill APIs synthesize `skill:<name>` as the only capability shown to the
user, and the dashboard uses that value as the capability review surface.

Result:

- the UI does not disclose the actual sensitivity of a skill
- install review is not meaningful for operator safety
- the contract conflates policy routing capability with real-world privilege

### 5. Agent Skill Allowlist and Policy Grants Are Not Aligned

`agent.skills` acts as an allowlist for which skills are registered into the
runner, but the runtime policy engine separately requires capability grants.
Skill tools use capability names of the form `skill:<name>`.

Result:

- a skill can be "available" at registration time and still be denied later
- exposure semantics are split across two mechanisms with no guaranteed
  synchronization

### 6. Registry, Signing, Discovery, and WASM Isolation Are Mostly Designed But Not Live

`SkillRegistry`, signature verification, the WASM sandbox, workflow recorder,
skill proposer, and skill matcher exist, but they are not the authority for the
live gateway skill system.

Result:

- documentation overstates the security and lifecycle properties of the system
- unsigned or quarantined file-backed skills are not a meaningful runtime
  concept yet
- safety claims about sandboxing are ahead of implementation

### 7. CLI and Docs Describe Different Skill Packaging Models

The docs describe YAML frontmatter and `SKILL.md`. Direct CLI mode reads
`manifest.json`.

Result:

- contributor extension points are unclear
- future file-backed skill work has no truthful operator-facing contract

## Scope

### In Scope

- make the live skill contract truthful across gateway, SDK, CLI, and dashboard
- establish one authoritative skill state model
- unify all skill execution through one canonical policy-enforced path
- align agent skill exposure with policy capability grants
- clearly separate live functionality from deferred functionality
- either wire discovery, registry, and sandbox flows into production or demote
  them to explicit future work
- add tests and release gates for these invariants

### Out of Scope For This Design Slice

- adding a public skill marketplace
- implementing autonomous skill proposal approval UX
- shipping third-party community skill execution before the registry and sandbox
  are real
- broad UI redesign beyond contract-truth changes
- replacing the core `Skill` trait model

## Non-Negotiable Invariants

### Invariant 1: One Source of Truth for Skill State

The system must have one canonical authority for:

- what skills exist
- which are installable
- which are installed or always-on
- which are quarantined
- which are available to a specific agent

### Invariant 2: One Source of Truth for Skill Execution

Every externally reachable skill execution path must pass through the same:

- policy evaluation
- capability check
- convergence guard behavior
- auditing and metering seam

### Invariant 3: Policy Capabilities and Human-Readable Privileges Must Be Separate

`skill:<name>` is a routing capability, not an operator-facing privilege model.
The system must expose both:

- policy capability used by the runtime
- declared operational privileges shown to humans

### Invariant 4: Installed State Must Affect Behavior

If an API says a skill is installed, uninstalled, enabled, disabled, available,
or quarantined, that state must change runtime behavior or visibility in a
deterministic way.

### Invariant 5: Docs Must Not Claim Sandboxing or Signing That Is Not Enforced

If file-backed registry loading, signature validation, or WASM sandboxing are
not active in production, the docs, CLI, and UI must say so explicitly.

## Target Design

### 1. Introduce a Canonical Skill Service

Create a single gateway-owned service, referred to here as `SkillCatalogService`.
The exact type name can change, but the boundary must exist.

This service owns:

- the static definition catalog for compiled skills
- discovered file-backed skills when that path is enabled
- install and enablement state
- quarantine state
- per-agent exposure resolution
- API-facing DTO generation

It becomes the only source used by:

- `/api/skills`
- install and uninstall endpoints
- the dashboard skills page
- CLI skill commands
- runtime runner construction

### 2. Separate Four Different Concepts

The current system mixes these together. They must become distinct.

### Skill Definition

Immutable metadata about a skill:

- id
- name
- version
- description
- source
- removability
- execution mode
- real operational privileges
- policy capability name

### Skill State

Mutable lifecycle state:

- always_on
- installed
- available
- quarantined
- disabled

### Agent Exposure

Whether a given agent is allowed to see and call the skill:

- inherited by policy
- allowed by agent config
- blocked by install state
- blocked by quarantine

### Skill Execution

The actual invocation path that must go through runtime policy and auditing.

### 3. Normalize Runtime Skill Exposure

Agent runtime construction should resolve skills in this order:

1. Start from all defined skills
2. Drop quarantined skills
3. Keep always-on platform safety skills regardless of install state
4. Keep other skills only if installed or explicitly enabled by the chosen
   product rule
5. Apply per-agent skill allowlist if present
6. Grant matching `skill:<name>` policy capabilities for the final resolved set
7. Register only that resolved set into the `SkillBridge`

This removes the current split-brain behavior where skill registration and
policy grants can disagree.

### 4. Replace Direct Skill Execution with Canonical Dispatch

`POST /api/skills/:name/execute` must not call `skill.execute(...)` directly.

Instead it should:

1. resolve the skill through the canonical catalog service
2. enforce install, quarantine, removability, and exposure policy
3. execute through the same runtime dispatch seam used by the agent loop, or
   through a shared lower-level executor that both routes call
4. use a write-capable connection when the skill can mutate state
5. record the same audit and error semantics as canonical tool execution

If this endpoint cannot be made semantically honest in the current phase, it
should be removed or explicitly marked internal-only until it can.

### 5. Make the Public Skill DTO Truthful

The API contract should stop overloading one field for multiple concerns.

Recommended DTO shape:

- `id`
- `name`
- `version`
- `description`
- `source`
- `removable`
- `execution_mode`: `native` or `wasm`
- `policy_capability`: `skill:<name>`
- `privileges`: list of human-readable operational privileges
- `state`: `always_on`, `installed`, `available`, `quarantined`, `disabled`
- `quarantine_reason`: nullable
- `installable`: boolean
- `enabled_for_agent`: optional in agent-scoped views

The dashboard install review should render `privileges`, not only
`policy_capability`.

### 6. Decide the Production Story for File-Backed Skills

This design does not accept the current ambiguous middle ground.

One of these two options must be chosen in implementation planning:

### Option A: Productionize File-Backed Skills

Wire the following into the live gateway:

- YAML or `SKILL.md` parsing
- registry-backed discovery
- real signature verification via `ghost-signing`
- quarantine surfaced through the API
- WASM execution for untrusted skills

### Option B: Defer File-Backed Skills Explicitly

If that work is not part of the next implementation wave, then:

- docs must describe compiled skills as the live model
- CLI direct mode must stop pretending file-backed manifests are the supported
  operator path
- registry and sandbox modules remain internal future work, not public truth

Recommendation:

- choose Option B for the first remediation wave
- first make the compiled-skill system truthful and safe
- then add file-backed skills as a separate, explicit expansion

That sequencing is more defensible than trying to harden a misleading control
plane and a half-live extension plane at the same time.

### 7. Treat Learning Components as Explicitly Deferred

`WorkflowRecorder`, `SkillProposer`, and `SkillMatcher` should not be described
as live behavior until they are integrated with:

- durable storage
- human approval flow
- runtime skill creation or enablement
- test coverage across the whole proposal lifecycle

Until then, they are internal prototypes.

## Phased Plan

### Phase 1: Contract and Safety Correction

Deliver:

- canonical skill catalog service
- truthful `/api/skills` response
- truthful install and uninstall semantics
- skill execute route moved onto canonical execution path or removed
- dashboard review changed to real privilege disclosure
- SDK and CLI updated to the corrected DTOs

Exit criteria:

- install state changes runtime-visible behavior
- no route executes skills outside the shared execution seam
- no API claims capabilities it does not actually describe

### Phase 2: Runtime Exposure and Policy Alignment

Deliver:

- final resolved skill set derived during runner construction
- automatic grant of `skill:<name>` policy capabilities for resolved skills
- alignment between `agent.skills` and the policy engine
- tests for allowlist, denial, and always-on safety skill behavior

Exit criteria:

- "registered" and "callable" are the same thing
- per-agent allowlist behavior is deterministic and test-backed

### Phase 3: Registry and Packaging Truth

Deliver either:

- real file-backed registry, signing, quarantine, and sandbox integration

or:

- documentation and CLI demotion of that path so the compiled-skill model is
  the only supported truth

Exit criteria:

- extension model is unambiguous to contributors

### Phase 4: Learning Pipeline Activation

Deliver only if explicitly approved later:

- recorder integration
- proposal generation lifecycle
- approval flow
- matcher activation

Exit criteria:

- no auto-learned or proposed skill behavior exists without durable state and
  approval coverage

## Test and Evidence Plan

Required tests for the implementation wave:

- gateway integration tests for `/api/skills`
- gateway integration tests for install and uninstall affecting runtime skill
  exposure
- gateway integration tests for execute route using write-capable skills
- runtime tests proving `agent.skills` and policy capability grants stay aligned
- dashboard tests for truthful privilege review rendering
- CLI tests for the corrected contract shape
- regression tests proving quarantined or disabled skills do not appear
  callable

Required negative-path coverage:

- execute a skill that is not installed
- execute quarantined skill
- execute removable false skill uninstall
- allowlisted skill without policy grant regression
- write skill via read-only path regression
- dashboard install review on empty privileges and on sensitive privileges

## Risks and Mitigations

### Risk 1: Backward Compatibility Drift

Some clients may already depend on the current loose DTO shape.

Mitigation:

- keep a transitional compatibility field only if explicitly marked
  non-canonical
- update SDK first so dashboard and CLI move in lockstep

### Risk 2: Overloading Phase 1 With Marketplace and Sandbox Work

Trying to fix contract truth and ship a real third-party skill platform in the
same wave will dilute quality.

Mitigation:

- fix the compiled-skill control plane first
- gate file-backed skills behind a separate explicit phase

### Risk 3: Hidden Internal Callers of Direct Execution

Some subsystems may depend on the current direct execute route semantics.

Mitigation:

- introduce a shared execution service first
- migrate route and internal callers onto it before removing old logic

## Deliverables For The Next Documents

`implementation.md` should derive from this design and specify:

- the concrete module and type changes
- migration order by file
- DTO changes and compatibility posture
- route changes
- test file changes

`task.md` should derive from `implementation.md` and specify:

- ordered tasks
- acceptance criteria
- test commands
- rollout checkpoints

## Design Decision Summary

The next implementation wave should not attempt to make the current skill
system more clever. It should make it more honest.

The correct sequence is:

1. make the compiled-skill system truthful
2. unify execution and policy enforcement
3. align install state with runtime behavior
4. separate human-readable privileges from routing capabilities
5. only then decide whether file-backed signed and sandboxed skills are part of
   the next real product surface
