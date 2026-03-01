# GHOST ADE — Phase 5: Hardening & Remediation Tasks

> Derived from full-system audit (2026-03-01) cross-referenced against
> `docs/ADE_DESIGN_PLAN.md`, `docs/API_CONTRACT.md`, `docs/convergence-safety.md`,
> `docs/tasks.md` (Phases 1–4), and `docs/THREAT_MODEL_CROSS_BOUNDARY.md`.
>
> Every task maps to a violated contract, broken condition, or missing invariant.
>
> **Legend**: ⬜ Not started · 🟡 In progress · ✅ Done · 🚫 Blocked
>
> **Severity**: 🔴 CRITICAL · 🟠 HIGH · 🟡 MEDIUM · 🟢 LOW
>
> **Contract refs**: `§5.0.6` = ADE_DESIGN_PLAN.md section, `API§5` = API_CONTRACT.md section,
> `CS§` = convergence-safety.md section

---

## 5.1 Authentication & Authorization Hardening

> **Violated contracts**: §5.0.6 (dual-mode auth), §17.1 (JWT lifecycle),
> API§5 (error response contract), T-1.1.1 (auth middleware spec)

- [ ] **T-5.1.1** 🔴 Remove no-auth admin fallback `§5.0.6`
  - **Contract violated**: T-1.1.1 specifies "No-auth mode: if neither `GHOST_JWT_SECRET` nor
    `GHOST_TOKEN` set, skip auth (local dev)". This was implemented as granting `admin` role to
    all anonymous requests via `Claims::no_auth_fallback()`. The contract says *skip auth* for
    local dev, not *grant admin to the world*.
  - **Fix**: When no auth is configured AND `GHOST_ENV=production`, exit with fatal error.
    When no auth is configured AND env is NOT production (or unset), log `WARN` on every
    request that auth is disabled. Never assign `admin` role — assign `dev` role with
    explicit capability set (read-all, write-non-safety).
  - **Condition**: Safety-critical endpoints (`/api/safety/*`) MUST reject requests without
    proper auth even in dev mode. Per CS§ intervention contract, kill/pause/resume/quarantine
    are irreversible safety actions.
  - Files: `ghost-gateway/src/api/auth.rs` lines 48-56 (no_auth_fallback), 215-231 (middleware)

- [ ] **T-5.1.2** 🟠 Add role-based access control to safety resume endpoint `§5.0.6, CS§`
  - **Contract violated**: Convergence-safety.md escalation rules require "de-escalation only at
    session boundaries, requiring consecutive normal sessions". The `resume_agent` endpoint for
    quarantined agents accepts client-supplied `forensic_reviewed: true` without server-side
    verification, bypassing the 2-phase review contract.
  - **Fix**: (a) Require `admin` or `security_reviewer` role claim in JWT. (b) Persist forensic
    review as audit entry with reviewer identity BEFORE allowing resume. (c) Require different
    JWT `sub` for forensic review vs resume confirmation (separation of duties). (d) Log audit
    trail per §17.2.1 with `actor_id`.
  - **Condition**: Quarantine resume MUST have 2 distinct audit entries (review + confirm) from
    2 distinct actors before state transition.
  - Files: `ghost-gateway/src/api/safety.rs` lines 221-334

- [ ] **T-5.1.3** 🟠 Move WebSocket token from query parameter to header `§5.0.6`
  - **Contract violated**: API§ specifies `Authorization: Bearer <token>` as the auth mechanism.
    WebSocket uses `?token=` query param which leaks in HTTP logs, proxy logs, browser history.
  - **Fix**: Extract token from `Sec-WebSocket-Protocol` subprotocol header before upgrade
    (standard WS auth pattern). Keep query param as deprecated fallback with `WARN` log.
    Add `Deprecation` header per API§4.
  - **Condition**: Token MUST NOT appear in any server access log line.
  - Files: `ghost-gateway/src/api/websocket.rs` lines 20-24, 222-230

- [ ] **T-5.1.4** 🟡 Add safety status endpoint role check `§5.0.6`
  - **Contract violated**: Safety status exposes per-agent intervention state, kill gate topology,
    and quarantine details to any authenticated user regardless of role.
  - **Fix**: `viewer` role sees only `{platform_killed, state}`. `operator`+ sees full breakdown.
  - Files: `ghost-gateway/src/api/safety.rs` lines 422-475

- [ ] **T-5.1.5** 🟡 CORS production safety `§5.0.13, T-1.1.4`
  - **Contract violated**: T-1.1.4 specifies "Read allowed origins from `GHOST_CORS_ORIGINS` env
    var". Default localhost origins remain active in production if env var unset.
  - **Fix**: If `GHOST_ENV=production` and `GHOST_CORS_ORIGINS` unset, exit with fatal error.
    Log `WARN` when using default dev origins.
  - Files: `ghost-gateway/src/bootstrap.rs` lines 536-577

---

## 5.2 Cryptographic Integrity

> **Violated contracts**: §15.5 (kill gate propagation), §8.3 (webhook signing),
> mesh-networking.md (Ed25519 agent identity)

- [ ] **T-5.2.1** 🔴 Fix kill fanout signing — use shared secret, not self-derived key `§15.5`
  - **Contract violated**: T-X.25 specifies "Kill gate propagation: Option B (HTTP fanout)" with
    signed messages. Current implementation derives the HMAC key FROM the message body itself
    (`blake3::derive_key("ghost-kill-fanout-v1", body.as_bytes())`) — anyone who sees the body
    can forge the signature. This is cryptographically meaningless.
  - **Fix**: (a) Use the mesh peer's pre-shared Ed25519 public key from `AgentCard.public_key`.
    (b) Sign the body with the gateway's Ed25519 private key using `ghost_signing::sign()`.
    (c) Receiving peer verifies signature against the sender's known public key.
    (d) Fall back to HMAC-SHA256 with `GHOST_MESH_SECRET` env var if Ed25519 keys unavailable.
  - **Condition**: A kill signal MUST be rejected by receiving peers if signature verification fails.
    Per §11.2, kill switch activation is irreversible — forged kills cause permanent shutdown.
  - Files: `ghost-gateway/src/api/kill_fanout.rs` lines 67-71

- [ ] **T-5.2.2** 🔴 Fix webhook HMAC — use proper HMAC-SHA256, require non-empty secrets `§8.3`
  - **Contract violated**: T-4.3.1 specifies "Fire webhooks on intervention, kill switch, proposal
    decision events" with HMAC signing. Current implementation uses `blake3::keyed_hash` (not
    standard HMAC construction) and allows empty secrets which produce unsigned webhooks.
  - **Fix**: (a) Replace with `hmac-sha256` using `ring` or `hmac` crate. (b) Require non-empty
    `secret` field on webhook creation — return `API§5` error `VALIDATION_ERROR` if empty.
    (c) Compute `HMAC-SHA256(secret, body)` and set `X-Ghost-Webhook-Signature: sha256=<hex>`.
    (d) Match GitHub/Stripe webhook signature format for interoperability.
  - **Condition**: Every webhook fire MUST include a verifiable signature. No unsigned webhooks.
  - Files: `ghost-gateway/src/api/webhooks.rs` lines 318-325, Cargo.toml (add `hmac`, `sha2`)

- [ ] **T-5.2.3** 🟡 Verify agent card signatures during A2A discovery `mesh-networking.md`
  - **Contract violated**: mesh-networking.md specifies Ed25519 signatures on agent cards.
    `discover_agents()` fetches `/.well-known/agent.json` but never verifies the signature field.
  - **Fix**: After fetching card JSON, extract `signature` field, verify against `public_key`
    using `ghost_signing::verify()`. Mark unverified cards as `trust_score: 0.0`.
  - Files: `ghost-gateway/src/api/a2a.rs` lines 268-365

- [ ] **T-5.2.4** 🟡 Fix constant-time comparison length leak `§5.0.6`
  - **Contract violated**: T-1.1.1 specifies constant-time token comparison. Current implementation
    returns early on length mismatch, leaking token length via timing side-channel.
  - **Fix**: Use `subtle::ConstantTimeEq` from the `subtle` crate, or pad shorter input to match
    length before XOR comparison. Add `subtle` to Cargo.toml.
  - Files: `ghost-gateway/src/api/auth.rs` lines 142-151, Cargo.toml

---

## 5.3 Resource Exhaustion & Memory Safety

> **Violated contracts**: §14.4 (performance budgets), T-X.27 (broadcast capacity),
> T-X.28 (Resync on Lagged)

- [ ] **T-5.3.1** 🔴 Bound webhook task spawning with semaphore `§8.3, §14.4`
  - **Contract violated**: §14.4 performance budgets require bounded resource usage.
    `fire_webhooks()` spawns unbounded `tokio::spawn` per matching webhook — 10K webhooks ×
    1 event = 10K concurrent HTTP clients with cloned payloads, no backpressure.
  - **Fix**: (a) Use `tokio::sync::Semaphore` with max 32 concurrent webhook fires.
    (b) Track JoinHandles in `JoinSet` for graceful shutdown. (c) Add per-webhook rate limit
    (max 1 fire per second per webhook). (d) Cap total webhooks per account at 50.
  - **Condition**: At any point in time, at most `GHOST_MAX_CONCURRENT_WEBHOOKS` (default 32)
    HTTP requests are in flight for webhook delivery.
  - Files: `ghost-gateway/src/api/webhooks.rs` lines 293-307

- [ ] **T-5.3.2** 🔴 Bound A2A discovery task spawning `§8.1, §14.4`
  - **Contract violated**: Same unbounded `tokio::spawn` pattern as webhooks.
    `discover_agents()` spawns one task per peer URL.
  - **Fix**: Use `futures::stream::FuturesUnordered` with `.buffer_unordered(16)` limit.
    Cap discovered_agents table at 500 entries (LRU eviction).
  - Files: `ghost-gateway/src/api/a2a.rs` lines 291-316

- [ ] **T-5.3.3** 🔴 Bound kill fanout task spawning `§15.5, §14.4`
  - **Contract violated**: Same pattern. Kill fanout spawns one task per mesh peer.
  - **Fix**: Use `JoinSet` with max 16 concurrent fanout requests. Log which peers were
    notified and which failed. Return summary to caller.
  - **Condition**: Kill fanout MUST complete (success or timeout) within 30 seconds.
    Per §11.2, kill switch is time-critical.
  - Files: `ghost-gateway/src/api/kill_fanout.rs` lines 49-99

- [ ] **T-5.3.4** 🔴 Increase broadcast channel capacity + add Resync `T-X.27, T-X.28, §14.4`
  - **Contract violated**: T-X.27 explicitly requires "Increase broadcast channel capacity from
    256 to 1024". T-X.28 requires "Implement Resync event on Lagged — trigger full REST
    re-fetch". Neither was implemented.
  - **Fix**: (a) Change buffer from 256 to 1024. (b) When `Lagged(n)` detected in WS handler,
    send `{"type": "Resync"}` to client. (c) Dashboard WS store handles `Resync` by calling
    full REST refresh on all stores.
  - **Condition**: Dashboard MUST NOT display stale data after broadcast lag. Resync guarantees
    eventual consistency within 1 REST round-trip.
  - Files: `ghost-gateway/src/bootstrap.rs` line 176, `ghost-gateway/src/api/websocket.rs`,
    `dashboard/src/lib/stores/websocket.svelte.ts`

- [ ] **T-5.3.5** 🟠 Add SSE stream timeout for A2A tasks `§8.1`
  - **Contract violated**: A2A SSE stream has no inactivity timeout — client can hold connection
    open forever if task never reaches terminal state.
  - **Fix**: Add 5-minute inactivity timeout. If no event sent for 5 minutes, send
    `{"type": "timeout"}` and close stream. Client can reconnect.
  - Files: `ghost-gateway/src/api/a2a.rs` lines 222-266

- [ ] **T-5.3.6** 🟠 Add graceful shutdown coordination for background tasks `§15.4`
  - **Contract violated**: Background tasks (convergence_watcher, backup_scheduler,
    config_watcher) spawned with `tokio::spawn()` but JoinHandles discarded. No
    `CancellationToken` for SIGTERM.
  - **Fix**: (a) Store JoinHandles in AppState. (b) Use `tokio_util::sync::CancellationToken`
    shared across all background tasks. (c) In shutdown handler, cancel token → await handles
    with 10s timeout → force abort.
  - **Condition**: All background tasks MUST complete or abort within the 60s shutdown window
    defined in shutdown.rs.
  - Files: `ghost-gateway/src/bootstrap.rs` line 238, `ghost-gateway/src/shutdown.rs`

- [ ] **T-5.3.7** 🟠 Implement actual shutdown sequence (replace placeholders) `§15.4`
  - **Contract violated**: shutdown.rs steps contain `tokio::time::sleep(10ms)` placeholders
    instead of actual drain/flush logic.
  - **Fix**: Step 1: Signal TcpListener to stop accepting. Step 2: Drain pending API responses
    (wait for in-flight handlers). Step 3: Flush session compactor buffers. Step 4: Persist
    cost tracker state to DB. Step 5: Notify monitor of shutdown. Step 6: Close WS connections
    with 1000 (Normal Closure). Step 7: WAL checkpoint + DB close.
  - Files: `ghost-gateway/src/shutdown.rs` lines 53, 61

- [ ] **T-5.3.8** 🟡 Stream backup checksums instead of loading entire file `§17.10`
  - **Contract violated**: backup_scheduler reads entire backup file into memory for blake3 hash.
    100GB backup = 100GB RAM spike.
  - **Fix**: Use `blake3::Hasher` with streaming `update()` calls reading 64KB chunks.
  - Files: `ghost-gateway/src/backup_scheduler.rs` lines 71-73

- [ ] **T-5.3.9** 🟡 Batch config watcher broadcasts `§17.8`
  - **Contract violated**: Config watcher sends one `AgentConfigChange` event per agent on any
    config change. 10K agents = 10K events, overflows broadcast buffer (T-X.27 capacity).
  - **Fix**: Send single `{"type": "ConfigReloaded"}` event. Dashboard re-fetches affected data.
  - Files: `ghost-gateway/src/config_watcher.rs` lines 61-71

- [ ] **T-5.3.10** 🟡 Cap webhook count and URL length `§8.3`
  - **Contract violated**: No limit on webhook creation — unbounded DB growth and matching cost.
  - **Fix**: Max 50 webhooks. Max URL length 2048 chars. Max 10 custom headers per webhook.
    Return `VALIDATION_ERROR` per API§5 if exceeded.
  - Files: `ghost-gateway/src/api/webhooks.rs`

---

## 5.4 Safety System Integrity

> **Violated contracts**: §11.2 (kill switch), CS§ (intervention state machine),
> §7.2 (6-gate safety loop)

- [ ] **T-5.4.1** 🔴 Handle mutex poisoning in kill fanout with fail-safe `§11.2`
  - **Contract violated**: Kill switch is irreversible per §11.2. If DB mutex is poisoned during
    `propagate_kill()`, peers are silently NOT notified — split-brain where local node is killed
    but mesh peers continue running.
  - **Fix**: On mutex poison, (a) attempt to create fresh DB connection for peer query,
    (b) if that fails, broadcast kill to ALL known peers from in-memory mesh state,
    (c) if mesh state also poisoned, log `FATAL` and trigger process exit with non-zero code
    (let orchestrator restart and re-propagate).
  - **Condition**: Kill signal MUST reach all reachable mesh peers OR the process MUST crash
    (forcing orchestrator restart). Silent failure is never acceptable for kill propagation.
  - Files: `ghost-gateway/src/api/kill_fanout.rs` lines 20-26

- [ ] **T-5.4.2** 🔴 Handle kill gate RwLock poisoning `§11.2, §15.5`
  - **Contract violated**: Same split-brain risk. Kill gate write lock poisoning logs error but
    continues — kill is recorded locally but not propagated.
  - **Fix**: On poison, attempt to reconstruct gate state from DB. If reconstruction fails,
    trigger full HTTP fanout as fallback. Log `CRITICAL` audit entry.
  - Files: `ghost-gateway/src/api/safety.rs` lines 88-97

- [ ] **T-5.4.3** 🟠 Handle agent registry RwLock poisoning `CS§`
  - **Contract violated**: Agent registry poisoning breaks all pause/resume/quarantine operations
    permanently. Per CS§, intervention state machine must remain operational.
  - **Fix**: On poison, attempt to rebuild registry from DB. If rebuild fails, enter degraded
    mode where safety operations work against DB directly (bypass in-memory registry).
  - Files: `ghost-gateway/src/api/safety.rs` lines 145-155, 230-240

- [ ] **T-5.4.4** 🟡 Synchronize kill gate close with HTTP fanout `§15.5`
  - **Contract violated**: Currently kill gate closes THEN HTTP fanout begins asynchronously.
    If HTTP fanout fails for some peers, gate shows "closed" but some peers are still running.
  - **Fix**: HTTP fanout completes first (with 30s timeout). Gate close records which peers
    confirmed. API response includes `{peers_notified: N, peers_failed: M}`.
  - Files: `ghost-gateway/src/api/safety.rs` lines 87-101

---

## 5.5 SSRF Prevention

> **Violated contracts**: §8.3 (webhook security), §8.1 (A2A protocol security)

- [ ] **T-5.5.1** 🟠 Validate webhook URLs against SSRF blocklist `§8.3`
  - **Contract violated**: Webhook test fires HTTP POST to arbitrary URLs including internal
    networks. Custom safety checks also accept arbitrary `webhook_url`.
  - **Fix**: (a) Only allow `http://` and `https://` schemes. (b) Resolve hostname and reject
    private IP ranges: 127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16,
    ::1, fc00::/7. (c) Reject hostnames that resolve to blocked IPs (DNS rebinding protection).
    (d) Add `GHOST_WEBHOOK_ALLOWED_HOSTS` env var for explicit whitelist.
  - **Condition**: Gateway MUST NOT make HTTP requests to RFC 1918 private addresses or
    link-local addresses via any user-configurable URL field.
  - Files: `ghost-gateway/src/api/webhooks.rs` lines 98-128, 218-241,
    `ghost-gateway/src/api/safety_checks.rs`, `ghost-gateway/src/api/a2a.rs` lines 64-97

- [ ] **T-5.5.2** 🟠 Validate A2A task target URLs `§8.1`
  - **Contract violated**: Same SSRF vector via `POST /api/a2a/tasks` target_url field.
  - **Fix**: Apply same SSRF blocklist. Additionally, only allow targets that exist in
    `discovered_agents` table (verified via prior discovery).
  - Files: `ghost-gateway/src/api/a2a.rs` lines 64-97

---

## 5.6 Input Validation & Data Integrity

> **Violated contracts**: API§5 (error response contract), §7.3 (convergence profiles),
> §5.0.9 (standard error responses)

- [ ] **T-5.6.1** 🟠 Validate convergence profile thresholds and assignment `§7.3, CS§`
  - **Contract violated**: CS§ defines intervention levels with specific score ranges
    [0.0, 0.3), [0.3, 0.5), [0.5, 0.7), [0.7, 0.85), [0.85, 1.0]. Profile thresholds
    are not validated for range [0.0, 1.0] — can create profiles with negative or >1 values,
    breaking the intervention state machine. Profile assignment doesn't verify the profile
    exists — agent gets assigned nonexistent profile.
  - **Fix**: (a) Validate all thresholds in [0.0, 1.0]. (b) Validate L1 < L2 < L3 < L4
    (monotonic). (c) Verify profile exists before assignment (SELECT then UPDATE in transaction).
    (d) Return `VALIDATION_ERROR` per API§5.
  - Files: `ghost-gateway/src/api/profiles.rs` lines 159-161, 274-293

- [ ] **T-5.6.2** 🟡 Validate webhook URL format `§8.3`
  - **Contract violated**: Accepts any string as URL without format validation.
  - **Fix**: Parse with `url::Url`, require scheme `http` or `https`, require non-empty host.
    Return `VALIDATION_ERROR` per API§5.
  - Files: `ghost-gateway/src/api/webhooks.rs` lines 98+

- [ ] **T-5.6.3** 🟡 Validate webhook custom headers `§8.3`
  - **Contract violated**: Custom headers applied to webhook POST without validation — attacker
    can inject `Authorization`, `Host`, or override `Content-Type`.
  - **Fix**: Blocklist headers: `Authorization`, `Host`, `Content-Length`, `Transfer-Encoding`,
    `X-Ghost-Webhook-Signature` (reserved). Max 10 custom headers, max 256 chars per value.
  - Files: `ghost-gateway/src/api/webhooks.rs` lines 338-344

- [ ] **T-5.6.4** 🟡 Add pagination upper bounds `API§, §14.4`
  - **Contract violated**: API§ pagination contract specifies max page_size 200, but `page`
    number has no upper bound. `page=999999999` causes huge SQL OFFSET.
  - **Fix**: Cap `page` at `ceil(total / page_size)`. If `page > max_page`, return empty
    results with correct `total` (not error — per API§2 this is non-breaking).
  - Files: All list endpoints (agents, sessions, goals, audit, memory, webhooks, skills)

- [ ] **T-5.6.5** 🟡 Validate memory search confidence bounds `§6.2.1`
  - **Contract violated**: T-2.1.2 specifies `confidence_min`, `confidence_max` query params.
    No validation that min ≤ max.
  - **Fix**: Return `VALIDATION_ERROR` if `confidence_min > confidence_max`.
  - Files: `ghost-gateway/src/api/memory.rs` lines 314-327

- [ ] **T-5.6.6** 🟡 Handle PII redaction failures explicitly `§6.4`
  - **Contract violated**: T-2.1.1 specifies "Apply PII redaction server-side via cortex-privacy".
    If `redact_pii()` errors, unredacted data is silently returned.
  - **Fix**: On redaction failure, replace content with `[PII_REDACTION_FAILED]` placeholder.
    Log error with request ID. Never return unredacted data.
  - Files: `ghost-gateway/src/api/sessions.rs` lines 250-255

---

## 5.7 Stub Completion

> **Violated contracts**: §17.11 (workflow execution), §17.4 (agent studio),
> §7.1 (trust graph)

- [ ] **T-5.7.1** 🔴 Implement actual workflow execution `§17.11`
  - **Contract violated**: T-2.1.9 specifies "Execute sequential pipeline via
    `POST /api/workflows/{id}/execute`". Current implementation returns fake simulated results
    (`"simulated"` status for all nodes) without executing anything.
  - **Fix**: (a) Parse workflow DAG. (b) Execute nodes sequentially (for MVP) or respect
    parallel branches (for P4). (c) For each agent node, call `ghost_agent_loop` with configured
    prompt/model. (d) Return real execution results with status per node. (e) Broadcast
    `WsEvent` per node completion for live overlay (T-4.8.3).
  - **Condition**: Workflow execution MUST actually invoke agent loops. Simulated responses
    violate the "0 simulated endpoints" exit criteria.
  - Files: `ghost-gateway/src/api/workflows.rs` lines 260-324

- [ ] **T-5.7.2** 🟠 Implement actual studio prompt execution `§17.4`
  - **Contract violated**: T-2.7.1 specifies "Test prompts against configured LLM providers".
    Current implementation returns hardcoded mock response.
  - **Fix**: Wire `state.model_providers` to `ghost_llm::LlmClient`. Send prompt to selected
    provider. Return real response with token count and cost.
  - Files: `ghost-gateway/src/api/studio.rs` lines 46-99

- [ ] **T-5.7.3** 🟡 Replace hardcoded trust graph heuristic with real data `§7.1`
  - **Contract violated**: T-3.2.1 specifies "Return EigenTrust trust scores between agents".
    Current `trust_graph` endpoint uses formula `1.0 - (level * 0.15)` — hardcoded heuristic.
  - **Fix**: Query actual trust scores from `cortex_multiagent` EigenTrust computation.
    If no trust data exists, return empty graph (not fake data).
  - Files: `ghost-gateway/src/api/mesh_viz.rs` lines 67-82

---

## 5.8 Secret Management

> **Violated contracts**: §17.1 (secret handling), §17.10 (backup encryption)

- [ ] **T-5.8.1** 🟠 Require explicit backup passphrase `§17.10`
  - **Contract violated**: T-3.4.1 specifies backup with "compressed .tar.gz + manifest JSON +
    blake3 checksum". Passphrase defaults to hardcoded `"ghost-default-key"` — all instances
    share the same encryption key.
  - **Fix**: If `GHOST_BACKUP_PASSPHRASE` not set, generate random 32-byte passphrase on first
    run, store in `~/.ghost/backup.key`, log path once. Never use hardcoded default.
  - Files: `ghost-gateway/src/api/admin.rs` lines 95-97

- [ ] **T-5.8.2** 🟡 Don't query webhook secrets in list endpoint
  - **Contract violated**: `list_webhooks()` SQL query includes `secret` column unnecessarily.
    Secret should never leave the DB except during webhook fire.
  - **Fix**: Remove `secret` from SELECT in list/get queries. Only query secret in
    `fire_webhooks()` and `test_webhook()`.
  - Files: `ghost-gateway/src/api/webhooks.rs` lines 66-96

---

## 5.9 Dashboard: Event Wiring & Error Handling

> **Violated contracts**: §5.1 (WS store), T-1.8.x (store migration), T-1.7.1 (WS singleton),
> API§5 (error response contract)

- [ ] **T-5.9.1** 🔴 Wire all WS event types to dashboard handlers `§5.1, T-1.8.x`
  - **Contract violated**: Phase 1 exit criteria require "All 6 event types consumed by stores".
    8 of 11 WsEventType values are defined but never consumed:
    - `ScoreUpdate` → should update convergence store (T-1.8.2)
    - `InterventionChange` → should update safety store (T-1.8.3)
    - `KillSwitchActivation` → should update safety store (T-1.8.3)
    - `AgentStateChange` → should update agents store (T-1.8.1)
    - `BackupComplete` → should show notification
    - `WebhookFired` → should update webhook list
    - `SkillChange` → should update skills list
    - `A2ATaskUpdate` → should update A2A task tracker
    - `TraceUpdate` → should update trace view
  - **Fix**: For each event type, add `wsStore.on('EventType', handler)` in the appropriate
    page's `onMount`. Handler updates reactive `$state` variable.
  - **Condition**: Per §5.1, every WS event type MUST have at least one consumer.
    Dashboard MUST reflect real-time state changes within 1 render cycle (16ms per §14.4).
  - Files: `dashboard/src/routes/` (multiple pages), `dashboard/src/lib/stores/`

- [ ] **T-5.9.2** 🟠 Replace silent catch blocks with user-visible error feedback `§5.0.9`
  - **Contract violated**: T-1.14.2 specifies "Error: error message + retry button + X-Request-ID".
    Multiple pages swallow API errors in `catch {}` blocks with no user feedback.
  - **Fix**: (a) Add `let error = $state<string | null>(null)` to each page. (b) In catch,
    set `error = e.message`. (c) Render error banner with retry button. (d) Display
    `X-Request-ID` from response headers for support.
  - **Affected pages**: skills, webhooks, orchestration (A2A), goals, agents detail
  - Files: `dashboard/src/routes/skills/+page.svelte`,
    `dashboard/src/routes/settings/webhooks/+page.svelte`,
    `dashboard/src/routes/orchestration/+page.svelte`,
    `dashboard/src/routes/goals/+page.svelte`

- [ ] **T-5.9.3** 🟡 Add missing navigation links `§9.3`
  - **Contract violated**: T-1.14.1 specifies all routes reachable from navigation.
    `/search` and `/settings/oauth` routes exist but have no sidebar/nav links.
  - **Fix**: Add search to sidebar (or keep it command-palette only with Cmd+K per T-3.13.2).
    Add OAuth to settings sub-nav.
  - Files: `dashboard/src/routes/+layout.svelte`

- [ ] **T-5.9.4** 🟡 Standardize API response shape handling `API§2`
  - **Contract violated**: Multiple pages use defensive patterns like
    `data?.scores ?? data ?? []` suggesting API response shapes are inconsistent.
  - **Fix**: Audit every API endpoint response shape. Document in OpenAPI spec. Remove
    defensive fallbacks — if API changes shape, it should be caught by generated TS types
    (T-1.6.2).
  - Files: Multiple route pages (convergence, costs, sessions, observability)

- [ ] **T-5.9.5** 🟡 Type all event handlers and state variables `§5.0.3`
  - **Contract violated**: T-1.4.3 specifies "Update TypeScript interfaces to match API shapes".
    Multiple pages use `any` type for state variables and event handlers.
  - **Fix**: Define interfaces for all API response types. Replace `any` with proper types.
    Use generated types from T-1.6.2 where available.
  - Files: `dashboard/src/routes/goals/[id]/+page.svelte`,
    `dashboard/src/routes/security/+page.svelte`,
    `dashboard/src/routes/workflows/+page.svelte`

---

## 5.10 Data Race & Consistency

> **Violated contracts**: §17.2 (database integrity), §8.1 (A2A protocol)

- [ ] **T-5.10.1** 🟡 Wrap A2A task creation in transaction `§8.1`
  - **Contract violated**: Task status determined from HTTP response code BEFORE DB write.
    Crash between response and DB write leaves task in inconsistent state (TOCTOU).
  - **Fix**: Use SQLite transaction: BEGIN → INSERT task with "pending" → send HTTP →
    UPDATE status → COMMIT. On HTTP failure, UPDATE to "failed" → COMMIT.
  - Files: `ghost-gateway/src/api/a2a.rs` lines 106-128

- [ ] **T-5.10.2** 🟡 Fix goals optimistic update race condition `§6.3`
  - **Contract violated**: T-2.4.2 specifies "Handle concurrent resolution: if ProposalDecision
    WS event arrives, disable buttons + show 'Resolved by another user'". Current implementation
    does local optimistic update without full server confirmation.
  - **Fix**: On approve/reject, set `actionLoading` state. On API success, THEN update local state
    from server response (not optimistic). On 409 Conflict, show "Already resolved" message.
    On WS ProposalDecision event, refresh full proposal state.
  - Files: `dashboard/src/routes/goals/+page.svelte` lines 70-94

---

## 5.11 Rate Limiting Hardening

> **Violated contracts**: §5.0.13 (rate limiting), T-1.1.5

- [ ] **T-5.11.1** 🟡 Add rate limit headers to all responses `§5.0.13`
  - **Contract violated**: API§ specifies `X-RateLimit-Limit`, `X-RateLimit-Remaining`,
    `X-RateLimit-Reset` on all responses. These headers are not currently emitted.
  - **Fix**: Add response header injection in rate limit middleware.
  - Files: `ghost-gateway/src/api/rate_limit.rs`

- [ ] **T-5.11.2** 🟡 Add exponential backoff for safety endpoint abuse `§5.0.13, CS§`
  - **Contract violated**: Safety endpoints allow 10 req/min. Attacker can cycle pause/resume
    150 times per hour. Per CS§, intervention state transitions should be rate-limited
    beyond simple request counting.
  - **Fix**: After 3rd safety action within 10 minutes, require 5-minute cooldown.
    After kill switch, require manual config reset (already per §11.2).
  - Files: `ghost-gateway/src/api/rate_limit.rs`, `ghost-gateway/src/api/safety.rs`

---

## Phase 5 Exit Criteria

| Metric | Target |
|---|---|
| Auth bypass vectors | 0 — no endpoint accessible without proper auth in production mode |
| Cryptographic correctness | All signatures use Ed25519 or HMAC-SHA256 with proper shared secrets |
| Unbounded spawns | 0 — all tokio::spawn sites use Semaphore or JoinSet |
| Kill propagation guarantee | Kill signal reaches all reachable peers OR process crashes |
| Silent error catches | 0 — all dashboard error states visible to user |
| WS event consumers | 11/11 event types have at least one handler |
| SSRF vectors | 0 — all user-configurable URLs validated against blocklist |
| Stub endpoints | 0 — workflow execute and studio run_prompt return real results |

---

## Task Summary

| Severity | Count | Category |
|----------|-------|----------|
| 🔴 CRITICAL | 11 | Auth bypass, crypto failures, memory leaks, split-brain, stubs |
| 🟠 HIGH | 10 | RBAC gaps, SSRF, shutdown, secret management, error handling |
| 🟡 MEDIUM | 17 | Validation, data races, rate limiting, navigation, typing |
| **Total** | **38** | |

### Dependency Order

```
Batch A (no deps):     T-5.1.1, T-5.2.1, T-5.2.2, T-5.3.1, T-5.3.2, T-5.3.3, T-5.4.1, T-5.4.2
Batch B (after A):     T-5.1.2, T-5.1.3, T-5.3.4, T-5.3.5, T-5.3.6, T-5.5.1, T-5.5.2
Batch C (after B):     T-5.1.4, T-5.1.5, T-5.2.3, T-5.2.4, T-5.3.7, T-5.4.3, T-5.4.4
Batch D (after C):     T-5.6.x, T-5.7.x, T-5.8.x
Batch E (after D):     T-5.9.x, T-5.10.x, T-5.11.x
```

### Verification (after each batch)

```bash
# Backend
cargo check --package ghost-gateway --package cortex-storage  # 0 errors
cargo test --package ghost-gateway                             # 0 failures

# Dashboard
cd dashboard && npx vite build                                 # success
npx svelte-check                                               # 0 errors

# Integration
curl -H "Authorization: Bearer invalid" http://localhost:18789/api/agents  # 401
curl -s http://localhost:18789/api/health | jq .status                     # "alive"
```
