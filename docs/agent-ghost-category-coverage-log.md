# Agent Ghost Category Coverage Log

## Sweep Status

- Active category: build and typecheck health
- Status: in progress
- Last updated: 2026-03-31
- Next category: dashboard UI

## Category History

### Build and typecheck health

- Status: in progress
- Scope checked this run:
  - `dashboard/` startup/runtime handling and dashboard home recovery flow
  - `extension/` popup rendering, auth initialization, content script session wiring, and service worker boot behavior
  - Validation environment blockers for `pnpm --dir dashboard check`, `pnpm --dir dashboard build`, `pnpm --dir extension typecheck`, `pnpm --dir extension build`, and `cargo check -p ghost-gateway`
- Fixes completed this run:
  - Added storage guards to the web runtime so browser-only APIs do not explode in SSR-like or pre-hydration execution paths
  - Reworked dashboard home loading into a reusable retryable flow with deterministic fallback state on failure
  - Added cleanup for `online`, `offline`, and `beforeinstallprompt` listeners in the root dashboard layout
  - Typed the deferred install prompt and guarded push subscription on service worker support
  - Initialized extension auth state before popup gateway calls
  - Fixed popup DOM bindings to match the actual HTML ids
  - Rendered signal rows into the real popup container instead of writing to missing nodes
  - Fixed alert banner targeting and hide/show behavior in the popup
  - Sanitized popup agent list HTML before injection
  - Added immediate and recurring session-duration updates in the popup
  - Added active-tab platform detection in the popup
  - Switched popup score loading to promise-based `chrome.runtime.sendMessage`
  - Initialized auth state in the extension background worker on boot
  - Normalized background message handling to one response per request path
  - Reused a stable content-script session id per page session instead of re-reading it per send
  - Reduced extension content-script noise by removing always-on info logging on supported/unsupported pages
  - Stopped sending full page URLs as the extension platform identifier and used hostname instead
  - Added non-fatal send guards for extension teardown/reload windows
- Blockers encountered:
  - Frontend package verification is blocked because the worktree has no installed JS dependencies (`vite`, `svelte-kit`, `tsc` not present)
  - Rust verification is blocked by local disk exhaustion during `cargo check` (`No space left on device` under `target/debug`)
