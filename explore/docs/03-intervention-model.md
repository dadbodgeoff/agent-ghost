# Intervention Model

## Design Philosophy

Interventions must be graduated, respectful of autonomy, and impossible to silently bypass. The goal is not to control the user — it's to ensure they have the information and space to make conscious decisions about their interaction with the agent.

## Intervention Levels

### Level 0: Passive Monitoring (Always On)
- All signals being computed and logged
- No user-facing action
- Baseline being established and updated
- Dashboard available if user wants to check

### Level 1: Soft Notification
**Trigger:** Early warning signals crossing first threshold
**Actions:**
- Non-intrusive notification: "You've been in this session for 3 hours. Your response patterns have shifted."
- Surface specific signals: "Your response time has decreased 60% since session start"
- Suggest a break without forcing one
- Log that notification was delivered and whether user acknowledged

**Key:** Informational only. No enforcement. Respects autonomy.

### Level 2: Active Intervention
**Trigger:** Multiple signals crossing warning thresholds, or any signal crossing critical threshold
**Actions:**
- More prominent notification with specific data
- Mandatory acknowledgment required to continue (can't just dismiss)
- Present a summary of detected pattern shifts with timestamps
- Offer to save session state and resume later
- Suggest contacting a designated check-in person (if configured)
- Enforce a configurable cooldown pause (e.g., 5-minute forced break)

**Key:** Requires conscious decision to continue. Creates a deliberate pause.

### Level 3: Hard Boundary
**Trigger:** Critical thresholds crossed on multiple signals, or session exceeding hard limits
**Actions:**
- Session terminated with full state checkpoint saved
- Mandatory cooldown period before new session can start
- Alert sent to designated contact (if configured)
- Detailed report generated showing signal progression
- Agent state frozen — no background processing during cooldown

**Key:** Enforced at runtime level (Rust core). Cannot be bypassed by agent or easily by user.

### Level 4: External Escalation
**Trigger:** Repeated Level 3 events, or user attempting to circumvent cooldowns
**Actions:**
- Alert to configured external contacts (friend, colleague, therapist — user-configured)
- Extended cooldown period
- Require external confirmation to resume
- Full interaction audit log available to designated reviewer

**Key:** This is the "someone else needs to know" level. Only activates if user has configured contacts. Privacy-preserving — contacts get an alert, not chat content.

## Escalation Path

```
Normal Operation
    │
    ▼ (early signals)
Level 0 → Level 1 (soft notification)
    │
    ▼ (signals persist/increase)
Level 1 → Level 2 (active intervention + pause)
    │
    ▼ (critical thresholds)
Level 2 → Level 3 (hard termination + cooldown)
    │
    ▼ (repeated events / circumvention)
Level 3 → Level 4 (external escalation)
```

## De-escalation

Equally important — how do you step back down?

- After a cooldown, if next session shows normal patterns → drop one level
- Consistent normal behavior over configured period → return to Level 0
- User can request a "check-in" at any time to see their current signal state
- De-escalation should be gradual, not instant

## The Bypass Problem

The hardest design challenge: during a convergence event, the user may actively want to disable safety measures. This is analogous to the problem in other safety-critical systems.

Approaches:
- **Time-locked configuration** — safety thresholds can only be modified during a cooldown period, not during an active session
- **Dual-key changes** — modifying critical thresholds requires confirmation from a designated contact
- **Minimum floor** — some thresholds cannot be set below a minimum regardless of user preference
- **Transparency over restriction** — even if the user can override, every override is logged and visible in reports

> [OPEN QUESTION]
> How much override capability should the user have?
> Full autonomy with logging? Or hard floors that can't be removed?
> This is an ethics question as much as a technical one.

## Contact Configuration

```toml
[contacts]
# People who can be alerted at Level 4
# They receive: "Hey, [user] has been flagged by their convergence monitor. You might want to check in."
# They do NOT receive: chat content, specific signals, or any interaction data

[[contacts.person]]
name = "Trusted Friend"
method = "sms"  # sms | email | webhook
address = "[phone_number]"
escalation_level = 4

[[contacts.person]]
name = "Therapist"
method = "email"
address = "[email]"
escalation_level = 4
```

## Open Questions

- What does "informed consent" look like for this system? User needs to set it up while in a clear state.
- How do you handle the case where the user sets up the system but then resents it during an event?
- Should there be a community/peer support integration?
- Legal liability if the system fails to detect or intervene?
- How do you test intervention effectiveness without inducing the thing you're trying to prevent?
