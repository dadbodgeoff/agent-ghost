# Agent Ghost Sweep Worklog

## 2026-03-29 10:58 EDT

Checked:
- Root workspace shape and package scripts for `dashboard/`, `extension/`, and `src-tauri/`
- `pnpm --dir dashboard check`
- `pnpm --dir dashboard build`
- `pnpm --dir extension typecheck`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- Static inspection of extension auth/bootstrap wiring after JS and Rust checks were blocked by environment issues

Fixed:
- Restored extension auth hydration across contexts so the popup and gateway client read stored credentials before reporting connection state or fetching agents
- Bootstrapped auth restoration during background service-worker startup to reduce false-disconnected state after browser restart

Files changed:
- `extension/src/background/gateway-client.ts`
- `extension/src/popup/popup.ts`
- `extension/src/background/service-worker.ts`

Still broken or blocked:
- JS package checks cannot run in this environment because `pnpm install` cannot reach `registry.npmjs.org` and there are no existing `node_modules`
- `cargo check` is blocked by local disk exhaustion: `No space left on device (os error 28)` while compiling `objc2-app-kit`

Next highest-value issue:
- Re-run the dashboard and extension checks in an environment with dependencies installed, then smoke the dashboard and popup flows in Playwright/browser context to catch remaining user-visible regressions
