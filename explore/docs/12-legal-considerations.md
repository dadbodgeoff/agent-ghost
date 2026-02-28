# Legal Considerations

## Overview

This document analyzes the legal landscape for an open-source, local-only browser extension and monitoring tool that helps users observe their own AI chat interaction patterns. This is not legal advice — it's research to inform the project and guide conversations with actual attorneys before public release.

---

## 1. Core Legal Position

The fundamental legal framing:

- The user installs the tool voluntarily on their own machine
- The tool reads the user's own conversation data from their own browser
- All data stays on the user's local machine
- No data is collected, transmitted, or stored by the project maintainers
- No data is shared with any third party
- The tool does not modify, interfere with, or automate interactions on any platform
- The tool is read-only and passive

This is functionally equivalent to:
- A user copy-pasting their own conversation into a text file
- Apple Screen Time monitoring which apps you use and for how long
- Android Digital Wellbeing tracking your usage patterns
- A browser's built-in developer tools showing network traffic
- An accessibility screen reader parsing page content

---

## 2. Platform Terms of Service Analysis

### OpenAI (ChatGPT)

**Potential concern:** An old OpenAI community forum post (2023) states "it is against the terms of service to automate the process of saving the output from ChatGPT."

**Reality:**
- This was in the context of automated scraping at scale, not user-side local tools
- Dozens of ChatGPT export extensions are live on the Chrome Web Store right now:
  - "AI Exporter: Save ChatGPT, Gemini to PDF"
  - "Bulk ChatGPT Export"
  - "Save ChatGPT to Notion"
  - "ChatGPT Exporter" (multiple versions)
  - "ConvertOGPT"
- These extensions read the ChatGPT DOM and export conversations — the same technical mechanism our tool uses
- Google continues to approve them; none have been pulled for violating OpenAI's ToS
- OpenAI has not taken legal action against any of these extensions
- OpenAI itself provides a data export feature (Settings → Data Controls → Export Data), acknowledging users have a right to their conversation data

**Risk level: Low.** Our tool does less than existing approved extensions (we analyze metadata patterns, they export full content).

### Anthropic (Claude.ai)

**Potential concern:** Anthropic updated terms in February 2026 to ban third-party tools that use OAuth tokens to access Claude subscriptions.

**Reality:**
- This ban targets tools that authenticate as the user to access Claude's backend API
- Our tool does not authenticate, does not use OAuth tokens, does not access any API
- It reads the DOM of a page the user is already viewing in their browser
- This is the same as a screen reader or any other browser extension that processes page content

**Risk level: Very low.** The Anthropic ban is about authentication piggybacking, not DOM reading.

### Character.AI

**Reality:**
- Extensions like "CAI Tools" are live on the Chrome Web Store, offering chat export, memory management, and character cloning for Character.AI
- Character.AI has not taken action against these extensions
- Character.AI's ToS focuses on content restrictions and user behavior, not on how users access their own data locally

**Risk level: Very low.**

### Google (Gemini)

**Reality:**
- Google Takeout already provides full conversation export
- Google's ecosystem is built around extensions and user data portability
- Multiple Gemini export extensions exist on the Chrome Web Store

**Risk level: Very low.**

---

## 3. Legal Frameworks That Support This Tool

### User Data Portability Rights

Multiple legal frameworks establish that users have a right to access and export their own data:

- **GDPR Article 20** (EU): Right to data portability — users can obtain and reuse their personal data across different services
- **CCPA/CPRA** (California): Right to access — consumers can request access to the personal information a business has collected about them
- **Digital Markets Act** (EU): Requires gatekeeper platforms to provide data portability

Our tool helps users exercise these rights by making their interaction data accessible to them locally.

### Browser Extension Precedent

Browser extensions that read and process page content are a well-established category:

- Password managers (1Password, LastPass) read form fields and page content
- Ad blockers (uBlock Origin) read and modify page DOM
- Accessibility tools (screen readers) parse entire page content
- Price comparison tools read product pages
- Grammar checkers (Grammarly) read all text input

None of these require platform permission to operate. They operate on the user's behalf, on the user's machine, processing content the user is already viewing.

### Screen Time / Digital Wellbeing Precedent

Apple Screen Time and Android Digital Wellbeing:
- Monitor which apps users interact with and for how long
- Track usage patterns and trends
- Provide notifications and enforce limits
- Operate without permission from the apps being monitored
- Are considered consumer safety features, not ToS violations

Our tool is a more specific version of this — monitoring interaction patterns with AI chat platforms specifically.

---

## 4. Potential Legal Risks

### Risk 1: Platform ToS Enforcement

**Scenario:** A platform updates their ToS to explicitly prohibit browser extensions that read chat content.

**Mitigation:**
- The tool can operate in metadata-only mode (timestamps, message lengths, session durations) without reading content
- Metadata-only monitoring is indistinguishable from normal browser behavior
- Even if a platform bans content reading, they cannot prevent timing analysis
- The tool should support graceful degradation to metadata-only mode per platform

### Risk 2: Computer Fraud and Abuse Act (CFAA)

**Scenario:** A platform argues that the extension accesses their service in an "unauthorized" manner.

**Analysis:**
- The CFAA prohibits accessing a computer "without authorization" or "exceeding authorized access"
- The user is authorized to access the platform (they have an account)
- The extension reads data the user is already authorized to view
- The extension does not bypass any access controls, authentication, or technical barriers
- Key precedent: *hiQ Labs v. LinkedIn* (2022) — accessing publicly available data does not violate CFAA
- Our case is even stronger: the user is accessing their own private data, not public data

**Risk level: Very low.** The extension doesn't access anything the user isn't already authorized to see.

### Risk 3: Wiretapping / Interception Laws

**Scenario:** The local proxy approach (mitmproxy) could be characterized as "intercepting" communications.

**Analysis:**
- Federal Wiretap Act requires consent of at least one party to the communication
- The user IS one party and is consenting by installing and configuring the proxy
- The proxy runs on the user's own machine, intercepting the user's own traffic
- This is legally equivalent to a user running Wireshark on their own network

**Risk level: Very low for the proxy approach.** The user is consenting to monitor their own traffic.

### Risk 4: Liability If the Safety System Fails

**Scenario:** A user relies on the convergence monitor, it fails to detect a convergence event, and the user experiences harm.

**Analysis:**
- This is the most significant legal risk
- Open-source software typically includes liability disclaimers (MIT, Apache 2.0, etc.)
- However, safety-critical software may face higher scrutiny
- Medical device regulations could theoretically apply if the tool is marketed as a mental health intervention
- The tool should be clearly positioned as a monitoring/awareness tool, NOT a medical device or therapeutic intervention

**Mitigation:**
- Clear disclaimers that the tool is not a medical device
- No claims of therapeutic benefit
- Encourage users to seek professional help
- License with strong liability limitation
- Do not market as a mental health tool — market as a developer/researcher safety tool

### Risk 5: Liability If the Intervention Causes Harm

**Scenario:** The tool's hard intervention (session termination) causes distress or the user misses something important.

**Mitigation:**
- All intervention levels are user-configured
- Hard interventions require explicit opt-in during setup (while in a clear state)
- Session state is always checkpointed before termination
- The tool never deletes data or prevents future access — it only enforces pauses

---

## 5. Recommended License

### Option A: MIT License
- Maximum permissiveness, maximum adoption
- Risk: Someone could fork, strip safety features, and redistribute
- Risk: Minimal liability protection

### Option B: Apache 2.0
- Permissive with patent protection
- Explicit liability disclaimer
- Still allows stripping safety features in forks

### Option C: AGPL-3.0
- Copyleft — any modifications must be open-sourced
- Prevents closed-source forks that strip safety features
- May reduce adoption (some organizations won't use AGPL)

### Option D: Custom License (Safety Clause)
- Based on Apache 2.0 or MIT
- Additional clause: forks must maintain core safety functionality
- Precedent: some AI model licenses include use restrictions (Llama, Stable Diffusion)
- Risk: Non-standard licenses create legal uncertainty and reduce adoption

### Recommendation: Apache 2.0

Best balance of adoption, liability protection, and patent safety. Accept that forks may strip safety features — the open-source community and reputation will be the enforcement mechanism, not the license. If someone strips safety features from a safety tool, the community will notice.

---

## 6. Required Disclaimers

The following should be included in the project README, extension listing, and any documentation:

```
DISCLAIMER

This tool is provided as-is for research and personal use. It is NOT a medical 
device, therapeutic intervention, or substitute for professional mental health 
support. If you are experiencing a mental health crisis, please contact a 
qualified professional or crisis helpline.

This tool monitors your own interaction patterns on your own device. All data 
stays local. No data is collected, transmitted, or stored by the project 
maintainers or any third party.

Users are responsible for ensuring their use of this tool complies with the 
terms of service of any platforms they use. The project maintainers make no 
representations about the compatibility of this tool with any platform's terms 
of service.

This software is provided "as is", without warranty of any kind, express or 
implied. In no event shall the authors or copyright holders be liable for any 
claim, damages, or other liability arising from the use of this software.
```

---

## 7. Pre-Launch Legal Checklist

- [ ] Have an attorney review the disclaimer language
- [ ] Have an attorney review the chosen license
- [ ] Review current ToS for each supported platform (ToS change frequently)
- [ ] Confirm Chrome Web Store / Firefox Add-ons policies allow this type of extension
- [ ] Review GDPR implications if the tool is used by EU residents (even locally-stored data has GDPR implications if it contains personal data)
- [ ] Review whether any jurisdiction classifies this as a medical device or health monitoring tool
- [ ] Consider forming an LLC or similar entity to limit personal liability
- [ ] Review insurance options for open-source maintainers
- [ ] Document the "safety tool, not medical device" positioning clearly in all materials

---

## 8. Summary

The legal position is strong. You're building a tool that helps users monitor their own behavior on their own machine. The data never leaves their device. The technical approach (DOM reading via browser extension) is identical to dozens of existing, approved extensions. The conceptual approach (usage pattern monitoring) is identical to Apple Screen Time and Android Digital Wellbeing.

The main risks are:
1. Platform ToS changes (mitigated by metadata-only fallback mode)
2. Liability if the safety system fails (mitigated by disclaimers and positioning)
3. Being mischaracterized as a medical device (mitigated by clear positioning)

None of these are blockers. They're manageable with proper disclaimers, legal review, and careful positioning. The project is legal to build and distribute.
