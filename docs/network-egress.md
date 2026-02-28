# Network Egress Control

GHOST enforces per-agent network egress policies via the `ghost-egress` crate.
Agents can only reach domains explicitly allowed by their configuration.

## Policy Modes

### Allowlist (recommended)

Only explicitly listed domains are reachable. Everything else is blocked.

```yaml
# ghost.yml
agents:
  my-agent:
    egress:
      policy: allowlist
      allowed_domains:
        - "api.openai.com"
        - "api.anthropic.com"
        - "*.slack.com"       # Wildcard: any subdomain of slack.com
```

### Blocklist

All domains reachable except explicitly blocked ones.

```yaml
agents:
  my-agent:
    egress:
      policy: blocklist
      blocked_domains:
        - "evil.example.com"
        - "*.malware.net"
```

### Unrestricted

No restrictions. This is the default for backward compatibility but not recommended
for production.

## Backends

`ghost-egress` supports three enforcement backends:

| Backend | Platform | Privileges | How it works |
|---------|----------|-----------|--------------|
| Proxy   | All      | None      | Localhost HTTP proxy intercepts outbound requests |
| eBPF    | Linux    | CAP_BPF   | cgroup socket filter at kernel level |
| pf      | macOS    | root      | Packet filter rules via `/dev/pf` |

The proxy backend is always available as a fallback. eBPF and pf require their
respective feature flags (`ebpf`, `pf`) and platform-specific privileges.

## Violation Handling

When an agent attempts to reach a blocked domain:

1. The request is denied
2. A violation event is logged (if `log_violations: true`)
3. If `alert_on_violation: true`, a `TriggerEvent::NetworkEgressViolation` is emitted
4. If violations exceed `violation_threshold` within `violation_window_minutes`,
   the agent is quarantined

```yaml
agents:
  my-agent:
    egress:
      policy: allowlist
      allowed_domains: ["api.openai.com"]
      log_violations: true
      alert_on_violation: true
      violation_threshold: 5
      violation_window_minutes: 10
```

## Domain Matching

- Matching is case-insensitive: `API.OPENAI.COM` matches `api.openai.com`
- Wildcards: `*.slack.com` matches `hooks.slack.com`, `api.slack.com`, etc.
- Exact match takes priority over wildcard match
- Default allowed domains include common LLM provider APIs
