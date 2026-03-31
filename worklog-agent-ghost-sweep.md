# Agent Ghost Sweep Worklog

## 2026-03-31 06:15 EDT

Checked:
- `pnpm --dir dashboard check` failed immediately because `dashboard/node_modules` is missing and `svelte-kit` is unavailable.
- `pnpm --dir extension typecheck` failed immediately because `extension/node_modules` is missing and `tsc` is unavailable.
- `pnpm install --offline` could not restore dependencies because the local pnpm store is missing tarballs, starting with `@codemirror/lang-markdown-6.5.0.tgz`.
- `cargo check --manifest-path src-tauri/Cargo.toml` failed during linker startup because the host only has about 206 MiB free (`ld: write() failed, errno=28`).
- Inspected the extension popup wiring, gateway client contract, dashboard runtime/auth code, Playwright specs, and the gateway `GET /api/agents` handler to find user-visible breakage that could be fixed without package installs.

Fixed:
- Rewired the extension popup script to the actual popup HTML IDs so the convergence score, level badge, alert banner, signal list, session duration, and platform label render again.
- Switched popup auth boot from a stale in-memory read to `initAuthSync()`, so the popup validates persisted credentials before showing connection status.
- Corrected the extension gateway client to accept the real `GET /api/agents` response shape (plain array) while remaining tolerant of the old wrapped shape.
- Updated popup agent rendering to use `effective_state`/`status` from the live gateway payload instead of a non-existent `state` field.

Still broken / blocked:
- JS build, lint, typecheck, and Playwright execution remain blocked until workspace dependencies can be installed.
- Tauri validation remains blocked until disk space is freed for Rust linking.
- I did not run browser-based smoke tests this pass because the required JS toolchain is currently unavailable in the workspace.

Next highest-value issue:
- Restore the JS workspace dependencies and rerun `dashboard` Playwright plus Svelte checks; after that, verify the popup end-to-end against a live or mocked gateway and then resume Tauri checks once disk pressure is resolved.
