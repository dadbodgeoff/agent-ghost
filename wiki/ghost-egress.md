# ghost-egress

> Per-agent network egress allowlisting with three enforcement backends — proxy (cross-platform), eBPF (Linux kernel-level), and pf (macOS packet filter). Violations feed into the kill switch trigger chain.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 4 (Infrastructure Services) |
| Type | Library |
| Location | `crates/ghost-egress/` |
| Workspace deps | `cortex-core` (Layer 1) |
| External deps | `regex`, `hyper`, `tokio`, `serde`, `chrono`, `uuid`, `tracing`, `thiserror` |
| Feature flags | `ebpf` (Linux eBPF cgroup filter), `pf` (macOS packet filter) |
| Modules | `config`, `domain_matcher`, `policy` (trait), `proxy_provider`, `ebpf_provider`, `pf_provider`, `error` |
| Public API | `EgressPolicy` trait, `ProxyEgressPolicy`, `EbpfEgressPolicy`, `PfEgressPolicy`, `DomainMatcher`, `AgentEgressConfig` |
| Policy modes | Allowlist, Blocklist, Unrestricted |
| Default allowed | `api.anthropic.com`, `api.openai.com`, `generativelanguage.googleapis.com`, `api.mistral.ai`, `api.groq.com` |
| Test coverage | Unit tests, property tests, domain matcher tests, violation threshold tests, concurrent agent tests |
| Downstream consumers | `ghost-gateway` (applies policies per agent), `ghost-agent-loop` (routes HTTP through proxy) |

---

## Why This Crate Exists

An AI agent with unrestricted network access is a security liability. It could:
- Exfiltrate sensitive data to an attacker-controlled server
- Download malicious payloads
- Communicate with command-and-control infrastructure
- Access internal network services it shouldn't reach

`ghost-egress` enforces per-agent network boundaries. Each agent gets its own egress policy that specifies exactly which domains it can reach. Everything else is blocked.

The key insight is that different operating systems require different enforcement mechanisms, and not all environments have kernel-level access. So `ghost-egress` provides three backends with automatic fallback:

1. **Proxy** (always available) — A localhost HTTP proxy per agent that inspects CONNECT requests. No privileges required. Cross-platform.
2. **eBPF** (Linux, requires `CAP_BPF`) — Kernel-level cgroup filter that blocks packets before they leave the network stack. Zero overhead for allowed traffic.
3. **pf** (macOS, requires root) — Packet filter anchors with IP-based rules. Kernel-level enforcement.

Both eBPF and pf automatically fall back to the proxy backend if they can't load (missing privileges, unsupported kernel, etc.).

### Why Per-Agent, Not Per-Process?

GHOST agents run within the same gateway process. Traditional process-level firewalling (iptables, Windows Firewall) can't distinguish between agents. The proxy backend solves this by giving each agent its own proxy port with its own allowlist. The eBPF backend uses cgroup-level filtering. The pf backend uses per-agent anchors.

---

## Module Breakdown

### `config.rs` — Per-Agent Egress Configuration

```rust
pub struct AgentEgressConfig {
    pub policy: EgressPolicyMode,           // Allowlist | Blocklist | Unrestricted
    pub allowed_domains: Vec<String>,       // For Allowlist mode
    pub blocked_domains: Vec<String>,       // For Blocklist mode
    pub log_violations: bool,               // Default: true
    pub alert_on_violation: bool,           // Emit TriggerEvent
    pub violation_threshold: u32,           // Default: 5
    pub violation_window_minutes: u32,      // Default: 10
}
```

**Three policy modes:**

- **Allowlist** (most restrictive): Only explicitly listed domains are reachable. Default list includes LLM provider APIs. Everything else returns 403.
- **Blocklist**: Everything is reachable except explicitly blocked domains. Useful for agents that need broad internet access but should avoid known-bad destinations.
- **Unrestricted**: No filtering. Backward-compatible default for agents that predate egress controls.

**Violation thresholds:** If an agent triggers `violation_threshold` violations within `violation_window_minutes`, the system emits a `TriggerEvent::NetworkEgressViolation` to the `AutoTriggerEvaluator`. This can escalate to QUARANTINE via the kill switch chain.

**Default allowed domains:** The 5 default domains are the LLM provider APIs that agents need to function. This means a default-configured agent can talk to its LLM but nothing else.

---

### `domain_matcher.rs` — Pattern Matching Engine

The domain matcher compiles domain patterns into regexes for fast matching.

**Supported patterns:**
- Exact: `api.openai.com` — matches only `api.openai.com`
- Wildcard: `*.slack.com` — matches `api.slack.com`, `hooks.slack.com`, but NOT `slack.com` itself and NOT `evil-slack.com`

**Security-critical normalization:**

Before matching, domains are:
1. Trimmed of whitespace
2. Path-stripped (`api.openai.com/../../etc/passwd` → `api.openai.com`)
3. Port-stripped (`api.openai.com:443` → `api.openai.com`)
4. Lowercased (DNS is case-insensitive)
5. Rejected if containing null bytes or spaces

**Wildcard safety:** The wildcard regex `^[a-z0-9]([a-z0-9\-]*[a-z0-9])?\.{base}$` ensures:
- Only proper subdomains match (not `evil-slack.com` for `*.slack.com`)
- Subdomain labels must start and end with alphanumeric characters
- No Unicode characters in subdomain labels (prevents homograph attacks)

**Unicode homograph defense:** The regex only allows `[a-z0-9]` in subdomain labels. Cyrillic `а` (U+0430) looks identical to Latin `a` but won't match. This is explicitly tested: `аpi.openai.com` (Cyrillic а) does NOT match `api.openai.com`.

---

### `policy.rs` — The `EgressPolicy` Trait

```rust
pub trait EgressPolicy: Send + Sync {
    fn apply(&self, agent_id: &Uuid, config: &AgentEgressConfig) -> Result<(), EgressError>;
    fn check_domain(&self, agent_id: &Uuid, domain: &str) -> Result<bool, EgressError>;
    fn remove(&self, agent_id: &Uuid) -> Result<(), EgressError>;
    fn log_violation(&self, agent_id: &Uuid, domain: &str, action: &str);
}
```

All three backends implement this trait. The gateway doesn't need to know which backend is active — it calls `apply()`, `check_domain()`, and `remove()` uniformly.

---

### `proxy_provider.rs` — Cross-Platform Proxy Backend

The proxy backend is always available. It starts a localhost HTTP proxy per agent on a dynamically allocated port.

**How it works:**
1. `apply()` binds to `127.0.0.1:0` (OS assigns a free port)
2. The proxy intercepts CONNECT requests (HTTPS tunneling)
3. Extracts the target domain from the CONNECT request
4. Checks against the agent's `DomainMatcher`
5. Allowed: forwards the connection. Blocked: returns 403.

**Per-agent isolation:** Each agent gets its own port. Agent A's proxy on port 49152 has a different allowlist than Agent B's proxy on port 49153. The agent's HTTP client (reqwest) is configured to route through its assigned proxy.

**Violation tracking:** The proxy maintains a sliding window of violation timestamps per agent. When the count within the window exceeds `violation_threshold`, it emits a `TriggerEvent::NetworkEgressViolation`. Old violations outside the window are pruned on each new violation.

**Reapply semantics:** Calling `apply()` on an agent that already has a policy shuts down the old proxy and starts a new one. This allows live policy updates without restarting the agent.

---

### `ebpf_provider.rs` — Linux Kernel-Level Enforcement

Feature-gated behind `#[cfg(all(target_os = "linux", feature = "ebpf"))]`.

**How it works:**
1. Resolves allowed domains to IP addresses via DNS
2. Loads a compiled eBPF program (CgroupSkb type)
3. Attaches the program to the agent's cgroup
4. Populates an eBPF HashMap with allowed IPs
5. The kernel drops packets to non-allowed IPs before they leave the network stack

**DNS re-resolution:** IP addresses change (CDN rotation, failover). A background task re-resolves all allowed domains every 5 minutes and updates the eBPF map. This is cancellable via a oneshot channel when the policy is removed.

**Automatic fallback:** If eBPF loading fails (missing `CAP_BPF`, unsupported kernel), the provider automatically falls back to `ProxyEgressPolicy`. The `using_proxy_fallback` flag tracks which mode each agent is using.

**Perf event buffer:** In production, violation events are read from an eBPF perf event array. Each event contains the destination IP, protocol, and port. The userspace handler reverse-resolves the IP and feeds the violation into the threshold counter.

---

### `pf_provider.rs` — macOS Packet Filter Enforcement

Feature-gated behind `#[cfg(all(target_os = "macos", feature = "pf"))]`.

**How it works:**
1. Resolves allowed domains to IP addresses
2. Creates a pf anchor named `ghost/{agent_id}`
3. Generates rules: `pass out proto tcp to {ip}` for each allowed IP, then `block out all`
4. Applies rules via `pfctl -a ghost/{agent_id} -f -`

**Rule generation:**
```
pass out proto tcp to 1.2.3.4
pass out proto udp to 1.2.3.4
pass out proto tcp to 5.6.7.8
pass out proto udp to 5.6.7.8
block out all
```

Both TCP and UDP are allowed for each IP (DNS resolution may use UDP). The final `block out all` catches everything else.

**Cleanup:** `remove()` flushes the anchor via `pfctl -a ghost/{agent_id} -F all`.

**Same fallback pattern as eBPF:** If `pfctl` fails with "Permission denied" or "Operation not permitted", falls back to proxy.

---

## Security Properties

### Defense in Depth

Three enforcement layers, each with automatic fallback:
- eBPF/pf: kernel-level, zero-overhead for allowed traffic, requires privileges
- Proxy: userspace, always available, no privileges needed

An agent can't bypass the proxy because its HTTP client is configured to route through it. It can't bypass eBPF/pf because those operate at the kernel level.

### Homograph Attack Defense

The domain matcher rejects Unicode characters in subdomain labels. `аpi.openai.com` (Cyrillic а) does not match `api.openai.com` (Latin a). This prevents an attacker from registering a lookalike domain that passes the allowlist.

### Path Traversal Immunity

Domains are stripped of paths before matching. `api.openai.com/../../etc/passwd` matches as `api.openai.com`. The path component is irrelevant for domain-level filtering.

### Violation Escalation

Repeated violations trigger `TriggerEvent::NetworkEgressViolation`, which feeds into the kill switch chain. A compromised agent that repeatedly tries to reach blocked domains will be quarantined automatically.

### Per-Agent Isolation

Each agent has its own policy, its own proxy port, its own eBPF program, or its own pf anchor. Agent A's policy cannot affect Agent B's traffic.

---

## Downstream Consumer Map

```
ghost-egress (Layer 4)
├── ghost-gateway (Layer 8)
│   └── Applies egress policies per agent on startup
│   └── Removes policies on agent shutdown
│   └── Handles violation threshold escalation
└── ghost-agent-loop (Layer 7)
    └── Routes HTTP requests through assigned proxy URL
    └── Receives 403 for blocked domains
```

---

## Test Strategy

### Domain Matcher Tests (inline in `domain_matcher.rs`)

| Test | What It Verifies |
|------|-----------------|
| `exact_domain_match` | Exact match, case-insensitive |
| `wildcard_matches_subdomain` | `*.slack.com` matches `api.slack.com` |
| `wildcard_does_not_match_bare_domain` | `*.slack.com` does NOT match `slack.com` |
| `wildcard_does_not_match_evil_prefix` | `*.slack.com` does NOT match `evil-slack.com` |
| `strips_port_before_matching` | `api.openai.com:443` matches |
| `strips_path_before_matching` | Path traversal stripped |
| `domain_with_null_byte_returns_false` | Null byte injection rejected |
| `unicode_domain_rejected` | Cyrillic homograph rejected |

### Proxy Provider Tests (inline in `proxy_provider.rs`)

| Test | What It Verifies |
|------|-----------------|
| `allowlist_allows_listed_domain` | Allowed domains pass |
| `allowlist_blocks_unlisted_domain` | Unlisted domains blocked |
| `blocklist_blocks_listed_domain` | Blocked domains rejected |
| `blocklist_allows_unlisted_domain` | Unlisted domains pass in blocklist mode |
| `unrestricted_allows_everything` | No filtering in unrestricted mode |
| `violation_threshold_triggers` | Threshold exceeded after N violations |
| `multiple_agents_independent` | Agent A's policy doesn't affect Agent B |
| `reapply_replaces_existing_policy` | Live policy update works |

### eBPF/pf Provider Tests

| Test | What It Verifies |
|------|-----------------|
| `fallback_to_proxy_on_ebpf_failure` | Graceful degradation when eBPF unavailable |
| `fallback_to_proxy_on_permission_error` | pf falls back without root |
| `anchor_create_command_correct` | pfctl command construction |
| `build_rules_correct` | pf rule generation |

---

## File Map

```
crates/ghost-egress/
├── Cargo.toml                          # Feature flags: ebpf, pf
├── ebpf/
│   └── src/
│       └── main.rs                     # eBPF program source (compiled separately)
├── src/
│   ├── lib.rs                          # Conditional compilation, re-exports
│   ├── config.rs                       # AgentEgressConfig, EgressPolicyMode, defaults
│   ├── domain_matcher.rs               # Regex-compiled domain pattern matching
│   ├── error.rs                        # EgressError enum
│   ├── policy.rs                       # EgressPolicy trait
│   ├── proxy_provider.rs              # Cross-platform proxy backend
│   ├── ebpf_provider.rs              # Linux eBPF cgroup filter (feature-gated)
│   └── pf_provider.rs                # macOS packet filter (feature-gated)
└── tests/
    ├── egress_tests.rs                # Integration tests
    ├── egress_e2e.rs                  # End-to-end tests
    └── property_tests.rs             # Proptest domain matching properties
```

---

## Common Questions

### Why three backends instead of just the proxy?

The proxy works everywhere but has a fundamental limitation: it only controls HTTP/HTTPS traffic. An agent that makes raw TCP connections (unlikely but possible) bypasses the proxy entirely. eBPF and pf operate at the kernel level and control all network traffic regardless of protocol.

### What happens if DNS resolution fails for an allowed domain?

The domain is logged as a warning but the policy is still applied with whatever IPs were successfully resolved. The 5-minute DNS re-resolution task will retry. In the meantime, the agent can't reach the unresolved domain (eBPF/pf mode) or the proxy falls back to domain-name matching (proxy mode).

### Can an agent change its own egress policy?

No. Egress policies are applied by the gateway, not by the agent. The agent has no API to modify its policy. The `EgressPolicy` trait methods require a `&Uuid` agent ID, and the gateway controls which agent ID maps to which policy.

### Why is `Unrestricted` the default mode?

Backward compatibility. Existing agents that predate egress controls should continue working without configuration changes. New agents should explicitly set `Allowlist` mode. The default allowed domains (LLM APIs) are only used when `Allowlist` mode is active.
