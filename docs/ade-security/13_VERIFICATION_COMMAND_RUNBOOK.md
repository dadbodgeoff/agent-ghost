# ADE Security Remediation: Verification Command Runbook

Status: draft for implementation orchestration on March 11, 2026.

This runbook defines the concrete command path for verification. Use these
commands unless there is a documented reason to substitute.

Working directory assumptions:

- repo root: `agent-ghost/`
- dashboard package: `agent-ghost/dashboard/`
- SDK package: `agent-ghost/packages/sdk/`

## 1. Preflight

Run from repo root:

```bash
pnpm --version
node --version
cargo --version
rustc --version
```

Record the output in the final signoff packet if environment-specific behavior
is suspected.

## 2. Architecture And Drift Audits

Run from repo root:

```bash
pnpm audit:openapi-parity:strict
pnpm audit:ws-contract-parity
pnpm audit:generated-types-freshness
pnpm audit:dashboard-architecture
```

Purpose:

- detect gateway/OpenAPI drift
- detect websocket contract drift
- detect stale SDK generated types
- detect dashboard architectural drift

## 3. Gateway Verification

Run from repo root:

```bash
cargo test -p ghost-gateway
```

If targeted iteration is needed during implementation, use focused runs such as:

```bash
cargo test -p ghost-gateway safety
cargo test -p ghost-gateway authz
```

Minimum final expectation:

- full `ghost-gateway` test pass

## 4. SDK Verification

Run from repo root:

```bash
pnpm --dir packages/sdk test
pnpm --dir packages/sdk typecheck
```

If SDK types changed, also run:

```bash
pnpm --dir packages/sdk generate:types
```

Then rerun the drift audits in Section 2.

## 5. Dashboard Static Verification

Run from repo root:

```bash
pnpm --dir dashboard check
pnpm --dir dashboard lint
```

If workspace-level validation is needed after broad changes:

```bash
pnpm typecheck
pnpm lint
```

## 6. Dashboard End-To-End Verification

Run from repo root:

```bash
pnpm --dir dashboard test:e2e
```

For service-worker/auth-specific coverage:

```bash
pnpm --dir dashboard test:e2e:service-worker
```

If dedicated Security tests are added, they should be included in the default
`test:e2e` run, not left as ad hoc one-off commands only.

## 7. Workspace Build Confidence Pass

Run from repo root when the remediation crosses gateway, SDK, and dashboard:

```bash
pnpm build
```

If this is too broad for iterative runs, it remains mandatory before final
signoff unless a documented existing workspace issue unrelated to this change
blocks it.

## 8. Recommended Final Verification Sequence

Run in this order for final closeout:

```bash
pnpm audit:openapi-parity:strict
pnpm audit:ws-contract-parity
pnpm audit:generated-types-freshness
pnpm audit:dashboard-architecture
cargo test -p ghost-gateway
pnpm --dir packages/sdk test
pnpm --dir packages/sdk typecheck
pnpm --dir dashboard check
pnpm --dir dashboard lint
pnpm --dir dashboard test:e2e
pnpm --dir dashboard test:e2e:service-worker
pnpm build
```

## 9. Package-Level Mapping

Use this mapping during execution:

- `WP-01` to `WP-03`
  Run Sections 2, 3, and 4.
- `WP-04` to `WP-06`
  Run Sections 5 and 6, then Section 2 drift checks.
- `WP-07` to `WP-11`
  Run Sections 5 and 6, then Section 2 drift checks.
- `WP-12` to `WP-14`
  Run Sections 4, 5, 6, and Section 2 drift checks.
- `WP-15` to `WP-17`
  Run the full final verification sequence.

## 10. Evidence Capture Rule

For each major verification run, record:

- command executed
- pass/fail
- relevant output summary
- follow-up action if failed

Do not record “tests run” without the command list and result.
