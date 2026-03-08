# ghost-skills

> Truthful current-state reference for the live skill pipeline.

## What is live today

The production skill system is a compiled, gateway-owned catalog.

- Skills are compiled Rust implementations shipped in the workspace, primarily from [crates/ghost-skills](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-skills) and [crates/ghost-pc-control](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-pc-control).
- The canonical catalog is built in [crates/ghost-gateway/src/skill_catalog](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/skill_catalog).
- The gateway persists install state in the database and exposes the truthful view through `GET /api/skills`.
- Runtime exposure is resolved per agent from the catalog plus the agent skill allowlist.
- Skill execution is native and in-process through the runtime `SkillBridge` and the canonical execution seam used by `POST /api/skills/{name}/execute`.

## What the API means

Each skill exposed by `/api/skills` is described by:

- `source`: where the skill came from. The live system currently returns `compiled`.
- `state`: `always_on`, `installed`, or `available` for the compiled catalog. `disabled` and `quarantined` remain reserved states in the contract but are not used for file-backed community skills because that pipeline is not live.
- `policy_capability`: the runtime capability grant paired with that skill, for example `skill:note_take`.
- `privileges`: operator-facing statements of what the skill can actually do.
- `removable` / `installable`: whether install or uninstall actions are valid.
- `execution_mode`: the live compiled catalog uses `native`.

Install and uninstall actions are not cosmetic. They change the persisted catalog state and therefore change runtime-visible behavior.

## Runtime behavior

The gateway resolves runtime skills in this order:

1. Start from the compiled catalog.
2. Keep all `always_on` skills.
3. Keep installable skills only when they are currently installed.
4. Apply the agent skill allowlist to non-always-on skills.
5. Register only the resolved skills in the tool registry.
6. Grant matching `skill:<name>` policy capabilities for the same resolved set.

This is the contract enforced by the gateway tests.

## What is not live

The repository still contains design or experimental code for a broader skill platform, but these are not production features today:

- live `SKILL.md` discovery from workspace or user directories
- manifest-backed installation from `~/.ghost/skills`
- signature verification as a runtime enforcement step
- quarantine as an operator-visible file-backed install state
- untrusted WASM execution for third-party skills
- community marketplace skill loading

If any of those features are implemented in the future, they must be wired into the canonical gateway catalog and documented as a separate production phase.

## Where to look in code

- Catalog definitions: [crates/ghost-gateway/src/skill_catalog/definitions.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/skill_catalog/definitions.rs)
- Catalog service and install-state resolution: [crates/ghost-gateway/src/skill_catalog/service.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/skill_catalog/service.rs)
- Canonical execution seam: [crates/ghost-gateway/src/skill_catalog/executor.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/skill_catalog/executor.rs)
- Gateway skill API: [crates/ghost-gateway/src/api/skills.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/skills.rs)
- Runtime skill registration: [crates/ghost-gateway/src/runtime_safety.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/runtime_safety.rs)
- Tool bridge: [crates/ghost-agent-loop/src/tools/skill_bridge.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-agent-loop/src/tools/skill_bridge.rs)

## Guidance for future work

Do not add new public claims around signing, quarantine, file-backed discovery, or sandboxing until the gateway catalog, persistence model, runtime resolution, and operator surfaces all reflect those behaviors truthfully.
