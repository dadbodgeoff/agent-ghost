# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in the GHOST Platform, please report it responsibly.

### Disclosure Process

1. **Do not** open a public GitHub issue for security vulnerabilities.
2. Email security findings to `security@ghost-platform.dev` with:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact assessment
   - Suggested fix (if any)
3. You will receive an acknowledgment within 48 hours.
4. We will work with you to understand and address the issue before any public disclosure.
5. A fix will be developed and released, with credit given to the reporter (unless anonymity is requested).

### Scope

The following are in scope for security reports:

- Simulation boundary bypass (emulation pattern evasion)
- Kill switch circumvention
- Sandbox escape in WASM skill execution
- Credential exfiltration via agent output
- Hash chain tampering or integrity bypass
- Convergence score manipulation
- Authentication/authorization bypass in the API
- Inter-agent message forgery or replay attacks

### Out of Scope

- Denial of service via resource exhaustion (covered by rate limiting)
- Issues in third-party dependencies (report upstream)
- Social engineering attacks

## Security Architecture

The GHOST Platform implements defense-in-depth:

- Ed25519 signatures on all inter-agent messages and CRDT deltas
- Blake3 hash chains for tamper-evident event logs
- 5-level intervention state machine with monotonic escalation
- 3-level kill switch (PAUSE, QUARANTINE, KILL_ALL) with atomic enforcement
- WASM sandbox with capability-scoped imports for skill execution
- Simulation boundary enforcement with Unicode normalization
- 7-dimension proposal validation before any state change
