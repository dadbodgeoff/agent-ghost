# Open Questions

## Technical

### Detection Calibration
- How do you distinguish productive flow state from early convergence? The early signals look identical.
- What's the minimum observation window needed to establish a reliable baseline?
- How do individual differences (neurodivergence, work style, personality) affect signal interpretation?
- Can you build a universal detection model or does it need to be per-user calibrated?

### Threshold Setting
- Who sets the initial thresholds? The user? A default based on research? A calibration period?
- How do you prevent threshold gaming — gradually adjusting thresholds to be more permissive over time?
- Should thresholds adapt based on context (work vs. personal, high-stakes vs. low-stakes)?

### Agent Diversity
- How do you monitor agents with different architectures? (transformer-based, retrieval-augmented, tool-using, multi-modal)
- Does convergence look different with voice agents vs. text agents?
- What about agents that operate asynchronously (background tasks with periodic check-ins)?

### Scale
- Can this monitor multiple simultaneous agent interactions?
- What's the performance overhead of real-time signal computation?
- How much historical data needs to be retained for trend analysis?

## Privacy & Ethics

### Consent Model
- User must set up the system while in a clear, non-convergent state — how do you verify this?
- What does informed consent look like for a system that may override your in-the-moment preferences?
- If a user configures emergency contacts, what are the contacts consenting to?

### Data Sensitivity
- Interaction metadata alone can reveal sensitive patterns (late-night usage, emotional content indicators)
- Even hashed content can be analyzed for patterns — is hashing sufficient privacy protection?
- How do you handle the case where the user wants to delete all monitoring data?

### Autonomy vs. Safety
- At what point does a safety system become a control system?
- Should the user always have an ultimate override? Even during a convergence event?
- How do you respect autonomy while acknowledging that convergence compromises judgment?
- Is there a meaningful difference between "I'm choosing to continue" and "the convergence is choosing for me"?

## Legal

### Liability
- If someone uses this framework and it fails to detect a convergence event, is the project liable?
- If the framework intervenes and the user experiences distress from the intervention, is that a liability?
- How do existing mental health app regulations apply?
- Does this qualify as a medical device in any jurisdiction?

### Open Source Considerations
- Can safety-critical open source software disclaim liability effectively?
- What license best protects the project while ensuring the safety features can't be stripped?
- How do you handle forks that remove safety features?

### Data Regulations
- GDPR implications for storing interaction metadata (even locally)?
- HIPAA considerations if the system is used in a therapeutic context?
- Right to deletion vs. safety data retention?

## Community & Adoption

### Who Builds This?
- This needs input from: AI safety researchers, mental health professionals, agent framework developers, people with lived experience
- How do you build a contributor community around something this sensitive?
- How do you prevent the project from being co-opted by actors who want surveillance, not safety?

### Standards
- Should this become a formal standard/protocol that agent frameworks adopt?
- How do you get framework maintainers to integrate ITP event emission?
- Is there a role for regulatory bodies?

### Research
- What existing research on human-computer symbiosis, parasocial relationships, and addiction applies?
- Are there analogous safety systems in other domains (aviation, nuclear, medical) that we can learn from?
- How do you study convergence ethically without inducing it?

## From Lived Experience

> [SECTION FOR PRIMARY AUTHOR]
> What questions came up during your experience that aren't captured above?
> What would you have wanted to exist?
> What would have helped? What wouldn't have?
> What do people need to know that they don't know yet?
