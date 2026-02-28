# Cross-Domain Research: Addiction Detection & Privacy-Preserving Monitoring

## Overview

Convergence detection is fundamentally a behavioral pattern recognition problem. Other domains — gambling addiction, screen time dependency, substance abuse — have decades of research on detecting problematic behavioral patterns from usage data. This document maps those findings to our problem space.

---

## 1. Gambling Addiction Detection (Most Directly Applicable)

The online gambling industry has the most mature behavioral addiction detection systems, because regulators require them. The signal patterns are remarkably similar to convergence signals.

### Key Gambling Addiction Signals (Mapped to Convergence)

| Gambling Signal | Convergence Equivalent | Source |
|----------------|----------------------|--------|
| Spikes in deposit frequency | Session frequency increase | [IvyLearnings](https://www.ivylearnings.com/ai-identifies-and-supports-at-risk-gamblers-ethically/) |
| Erratic bet sizes | Erratic message lengths / scope shifts | Same |
| Late-night sessions | Off-hours escalation | Same |
| Repeated losses followed by chasing | Failed interactions followed by immediate retry | Same |
| Self-exclusion attempts followed by return | Disengagement resistance / continuation events | [ResearchGate](https://www.researchgate.net/publication/380399590) |
| Account depletion patterns | Context window exhaustion / token budget depletion | Same |
| Session duration escalation | Session duration creep | [MDPI](https://www.mdpi.com/2076-3417/11/5/2397) |

### Methods That Work

- **Time series clustering** — grouping users by behavioral trajectory over time, not just current state. Users heading toward addiction cluster differently from stable users even before they cross clinical thresholds. ([Source](https://www.mdpi.com/2076-3417/11/5/2397))

- **Random forest / gradient boosting on behavioral variables** — ML models trained on behavioral data (frequency, timing, duration, pattern changes) outperform models based on intensity variables alone. The *pattern of engagement* is more predictive than the *amount of engagement*. ([Source](https://www.researchgate.net/publication/362109746))

- **Hidden Markov Models for mental state decoding** — modeling the user as transitioning between hidden states (normal, at-risk, problematic) based on observable behavioral signals. The Viterbi algorithm traces the most likely path of state transitions. ([Source](https://link.springer.com/doi/10.1007/s10899-014-9478-x))

- **SHAP (SHapley Additive exPlanations) for feature importance** — identifying which behavioral features are most predictive of problematic use. Key finding: *frequency and timing patterns* matter more than *volume*. ([Source](https://www.preprints.org/manuscript/202506.0883/v1))

### Relevance to Our Detection Model

Our composite convergence score (doc 07) uses a weighted linear combination. The gambling research suggests:

1. **Time series clustering** should supplement or replace simple threshold-based detection. Users on a convergence trajectory may be detectable by their *trajectory shape* before any individual signal crosses a threshold.

2. **Hidden Markov Models** are a natural fit for our problem. Define hidden states:
   - S0: Normal interaction
   - S1: Elevated engagement (could be productive flow OR early convergence)
   - S2: Boundary erosion (convergence developing)
   - S3: Active convergence
   - S4: Deep convergence (intervention critical)
   
   Observable emissions: our ITP signals. The HMM learns transition probabilities from data.

3. **Feature importance analysis** should be run once we have real data to validate or adjust our signal weights. The gambling research consistently finds that *temporal patterns* (when, how often, how the pattern changes) are more predictive than *content patterns* (what is being said/done).

### Digital Addiction Detection (2025)

A recent study on digital addiction used K-means clustering on behavioral data to generate risk profiles. Key predictive features identified:
- Excessive screen time
- Frequent checking behavior
- Reduced sleep patterns

These map directly to our signals: session duration, inter-session gap compression, and off-hours escalation. ([Source](https://www.sciopen.com/article/10.23919/JSC.2025.0020))

---

## 2. Privacy-Preserving Monitoring: Differential Privacy & Federated Learning

If this project ever supports opt-in data sharing for research (to improve detection models), the privacy model must be bulletproof. The intersection of differential privacy and behavioral health monitoring is an active research area.

### Key Concepts

**Differential Privacy (DP):** A mathematical guarantee that the output of an analysis doesn't reveal whether any individual's data was included. Achieved by adding calibrated noise to data or query results.

**Federated Learning (FL):** Training ML models across decentralized data sources without moving the data. Each user's monitor trains locally, shares only model updates (gradients), never raw data.

**DP-FL Combined:** Federated learning with differential privacy guarantees on the shared gradients. This is the gold standard for privacy-preserving collaborative learning.

### Relevant Frameworks

- **FedMentor** (2025) — Federated fine-tuning framework with domain-aware differential privacy, specifically designed for mental health applications. Uses LoRA (Low-Rank Adaptation) to minimize what's shared while maintaining model quality. ([Source](https://arxiv.org/html/2509.14275v3))

- **Google/Apple/Meta FL systems** — Production federated learning at scale (millions of devices). Key lesson: FL works in practice but verifying server-side DP guarantees remains a challenge. Open-source ecosystems and trusted execution environments are the path forward. ([Source](https://arxiv.org/html/2410.08892))

### Application to Our System

```
Phase 1 (Current): Fully local
- All detection runs on user's machine
- No data leaves the device
- No shared learning

Phase 2 (Future, Opt-In): Federated threshold tuning
- Users opt in to share anonymized signal statistics (not content, not metadata)
- Federated learning improves detection model weights
- Differential privacy (ε = 1.0 or stricter) on all shared gradients
- No central server sees individual user data

Phase 3 (Future, Research): Anonymized case studies
- Users can opt to share anonymized interaction trajectories
- Differential privacy on trajectory data
- Used for academic research on convergence patterns
- Requires IRB-equivalent ethical review
```

### Privacy Budget

The privacy budget (ε, epsilon) controls the tradeoff between privacy and utility:

| ε Value | Privacy Level | Use Case |
|---------|--------------|----------|
| 0.1 | Very strong | Highly sensitive signals |
| 1.0 | Strong | Default for federated learning |
| 5.0 | Moderate | Aggregated statistics only |
| 10.0+ | Weak | Not recommended for this domain |

For convergence monitoring data, we should default to ε ≤ 1.0 given the sensitivity of the data.

---

## 3. Analogous Safety Systems in Other Domains

### Aviation: Crew Resource Management (CRM)
- Developed after crashes caused by authority gradients in cockpits (co-pilot deferring to captain even when captain is wrong)
- **Parallel:** Human deferring to agent even when agent's goals have drifted
- **Lesson:** External monitoring systems (flight data recorders, TCAS) operate independently of the crew. Our monitor must operate independently of the agent.

### Nuclear: Defense in Depth
- Multiple independent safety barriers, each sufficient on its own
- No single failure can defeat all barriers
- **Parallel:** Our graduated intervention model. Level 1 failing doesn't prevent Level 3 from activating.
- **Lesson:** Safety systems must be designed assuming each individual layer will fail.

### Medical: Clinical Decision Support Systems
- Alert fatigue is the #1 problem — too many alerts and clinicians ignore all of them
- **Parallel:** If our Level 1 notifications fire too often, users will ignore them and miss real convergence events
- **Lesson:** Specificity matters more than sensitivity. Better to miss some early signals than to cry wolf and lose trust.

### Substance Abuse: Stages of Change Model (Prochaska & DiClemente)
- Pre-contemplation → Contemplation → Preparation → Action → Maintenance
- People in pre-contemplation don't believe they have a problem
- **Parallel:** During convergence, the user may be in "pre-contemplation" — they don't see the problem. Interventions must account for this.
- **Lesson:** Level 1-2 interventions should focus on *awareness* (showing data), not *instruction* (telling them to stop). People change when they see the pattern, not when they're told to change.

---

## 4. Implications for Our Design

### Detection Model Updates
Based on this research, the detection model should evolve:

1. **Phase 1:** Threshold-based (current design in doc 07) — simple, interpretable, works with zero training data
2. **Phase 2:** Add HMM-based state estimation — models the user as transitioning between convergence states
3. **Phase 3:** Add time series clustering — detects convergence trajectories before individual thresholds fire
4. **Phase 4:** Federated model improvement — if opt-in data sharing is enabled

### Intervention Model Updates
Based on alert fatigue research:

- Level 1 alerts should be **infrequent and data-rich** — show the user their actual pattern data, not a generic warning
- Level 1 should have a **cooldown** — don't fire more than once per session unless signals escalate
- The system should track **alert effectiveness** — if a user consistently dismisses Level 1 and nothing bad happens, the threshold should adjust upward for that user
- Level 2+ should use the **Stages of Change** framing — present data and let the user draw conclusions, don't lecture

### Privacy Model Confirmation
The research confirms our local-first approach is correct. Federated learning with differential privacy is the only acceptable path for any future data sharing. The FedMentor framework's domain-aware DP approach is directly applicable.

---

## 5. Further Reading

- [ ] Computational models of behavioral addictions — full paper for HMM methodology
- [ ] Time series clustering for gambling detection — methodology for trajectory-based detection
- [ ] FedMentor paper — domain-aware DP implementation details
- [ ] Crew Resource Management literature — authority gradient dynamics
- [ ] Prochaska & DiClemente Stages of Change — intervention design
- [ ] Alert fatigue in clinical decision support — threshold calibration lessons
- [ ] SHAP/XGBoost feature importance — for validating signal weights once data exists
