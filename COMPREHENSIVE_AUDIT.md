# Agent Ghost Sweep Worklog

## 2026-03-29

- Checked:
  - `package.json`, `dashboard/package.json`, `extension/package.json`, `src-tauri/Cargo.toml`
  - `pnpm --filter ghost-dashboard check`
  - `pnpm --filter ghost-convergence-extension typecheck`
  - `pnpm install`
  - `cargo check --manifest-path src-tauri/Cargo.toml`
  - Static inspection of the extension popup, gateway client, and dashboard root route
  - `git diff --check`

- Fixed:
  - Rewired the extension popup to the DOM it actually renders in `extension/src/popup/popup.ts`.
  - The popup now initializes stored auth before reading connection state, renders its signal rows, updates the real score/level/alert/session elements, and escapes agent names/states before injecting markup.
  - Normalized extension agent loading in `extension/src/background/gateway-client.ts` so `/api/agents` works whether it returns a plain array or an `{ agents }` wrapper, and it derives state from `effective_state`/`status`.

- Remains broken / blocked:
  - JS workspace checks cannot currently run because dependencies are not installed and `pnpm install` fails with `ENOSPC`.
  - Tauri validation cannot currently run because `cargo check` fails creating target output with `No space left on device`.
  - Restoring tracked generated/cache artifacts after space cleanup was also blocked by the same disk-pressure condition.

- Next highest-value issue:
  - Free workspace disk space, then rerun extension build/typecheck and dashboard checks. After that, prioritize a browser smoke test of the extension popup and dashboard landing flow.
