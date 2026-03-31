# Agent Ghost Sweep Worklog

## 2026-03-31

### What I checked
- Inspected the PNPM/Turbo workspace layout for `dashboard/`, `extension/`, and `src-tauri/`.
- Attempted targeted JS checks:
  - `pnpm --dir dashboard check`
  - `pnpm --dir dashboard lint`
  - `pnpm --dir extension typecheck`
  - `pnpm --dir extension lint`
- Attempted `pnpm install` to restore missing workspace dependencies.
- Inspected extension popup, background auth wiring, and Tauri gateway startup state handling.
- Ran `cargo fmt --all` at repo root and `cargo fmt` in `src-tauri/`.
- Attempted `cargo check` in `src-tauri/` twice, once normally and once with `CARGO_TARGET_DIR=/tmp/agent-ghost-src-tauri-target`.

### What I fixed
- Fixed broken popup DOM wiring in `extension/src/popup/popup.ts`:
  - Score and level now target the actual popup elements.
  - Alert banner now uses the popup's real `active`/severity classes.
  - Signal rows now render into the existing `#signalList` container instead of writing into missing IDs.
  - Session duration and platform fields now populate the real popup fields.
  - Popup auth now initializes from storage on load instead of reading the default in-memory auth state.
- Fixed extension startup wiring in `extension/src/background/service-worker.ts`:
  - Auth state now initializes on service worker startup.
  - IndexedDB reconnect sync bootstrap now runs on startup.
- Fixed a desktop crash path in `src-tauri/src/commands/gateway.rs`:
  - Repeated gateway starts no longer attempt to re-register `GatewayPort` / `GatewayProcess` managed state, which could panic in Tauri.
  - Existing gateway process state is replaced in place when startup is re-entered.

### What remains broken or blocked
- JS checks are currently blocked because the repo has no installed `node_modules`, and `pnpm install` cannot reach the npm registry in this environment.
- Tauri compilation is currently blocked by local disk exhaustion:
  - normal `cargo check` failed with `No space left on device` under `src-tauri/target`
  - redirected `CARGO_TARGET_DIR=/tmp/... cargo check` also failed with `No space left on device`
- I could not run dashboard Playwright smoke tests because the JS toolchain is unavailable.

### Next highest-value issue
- Restore enough local disk and JS dependencies to run the dashboard and extension checks for real, then smoke the most user-visible flows:
  - dashboard load/login/session navigation
  - extension popup state after stored auth
  - Tauri desktop startup and repeated gateway start/stop interactions
