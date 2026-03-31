# Agent Ghost Sweep Worklog

## 2026-03-31 00:13:53 EDT

Checked:
- Repo/package scripts for the dashboard, extension, and Tauri surfaces.
- `pnpm -C dashboard check`
- `pnpm -C dashboard build`
- `pnpm -C extension typecheck`
- `pnpm -C extension lint`
- `pnpm -C extension build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- Static review of extension auth, popup, and background wiring.

Fixed:
- Initialized popup auth from persisted storage before rendering connection state or loading agents, so a valid stored token is no longer treated as disconnected on popup open.
- Switched extension auth validation to `/api/auth/session` with a `/api/health` fallback for older gateways, avoiding false-positive "authenticated" states from a generic health check.
- Initialized background auth/sync on worker startup and replaced the fragile MV3 `setInterval` score refresh with an alarm-backed refresh path, with a timer fallback where alarms are unavailable.
- Added the `alarms` permission required for the new background refresh path in both browser manifests.

Still broken / blocked:
- JS workspace checks are blocked because `node_modules` is absent and offline install cannot hydrate the lockfile store; `pnpm install --offline` fails on a missing tarball for `@codemirror/lang-markdown@6.5.0`.
- Tauri compile validation is currently environment-blocked by disk pressure unless build artifacts are cleaned first; this run recovered space by `cargo clean`, but did not complete a fresh `cargo check`.
- The extension repo still contains divergent checked-in `.js` and `.ts` sources for popup/background paths, which remains a maintenance risk for unpacked-source workflows.

Next highest-value issue:
- Re-establish a usable JS dependency install so dashboard Svelte checks and Playwright smoke tests can run; after that, audit the duplicated extension `.js` sources versus the TypeScript build outputs and remove or align the stale path.
