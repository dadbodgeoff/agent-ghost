# Agent Ghost Sweep Worklog

## 2026-03-23

### Checked
- `pnpm --dir dashboard check` and `pnpm --dir dashboard lint` both failed immediately because local `node_modules` are missing, so Svelte and ESLint binaries are unavailable.
- `pnpm --dir extension typecheck` failed for the same reason: no local JS dependencies are installed.
- Static extension inspection found a user-visible popup regression and lost auth initialization in the TypeScript service worker.
- `node --check extension/dist/background/auth-sync.js`
- `node --check extension/dist/background/service-worker.js`
- `node --check extension/dist/popup/popup.js`
- `cargo test --manifest-path src-tauri/Cargo.toml` was attempted but failed during dependency compilation because the machine ran out of disk space in `src-tauri/target`.

### Fixed
- Restored extension popup rendering wiring so the checked-in popup script now targets the actual DOM ids used by `extension/src/popup/popup.html`.
- Replaced missing signal-row rendering with dynamic signal list output, so score details are visible instead of silently failing against nonexistent `s1`-`s7` nodes.
- Restored alert banner behavior and session-duration updates against the real popup markup.
- Rehydrated popup auth state by calling `initAuthSync()` before reading gateway auth, preventing the popup from always showing a disconnected/default state on load.
- Reintroduced service-worker auth initialization and auto-sync startup in the TypeScript source, then aligned the checked-in `dist` artifacts to match so the extension bundle is not left broken until a future JS rebuild.
- Added storage change listening in auth sync so token and gateway-url updates refresh in-memory auth state instead of staying stale until reload.

### Still Broken / Blocked
- Dashboard and extension lint/typecheck/build/playwright coverage are still blocked by missing local JS dependencies.
- Tauri tests are currently blocked by local disk exhaustion rather than a code failure.

### Next Highest-Value Issue
- Restore a usable JS dependency install or free enough disk to complete `pnpm` and `cargo` validation, then run dashboard checks and Playwright flows to catch the next user-visible regression.
