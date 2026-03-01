# WhatsApp Integration — Legal Review Gate (T-4.9.5)

**STATUS: GATED**

Do NOT ship WhatsApp integration until legal review is complete.

## Required Reviews

1. WhatsApp Terms of Service compliance
2. End-to-end encryption implications for content observation
3. User consent requirements for passive monitoring
4. Data retention and processing obligations
5. GDPR/CCPA compliance for EU/California users

## Technical Notes

- The `bridges/baileys-bridge/` directory contains a prototype WhatsApp bridge
- It uses the Baileys library for WhatsApp Web protocol
- Content observation is passive (read-only, no message sending)
- No message content is stored — only convergence signals are extracted

## Approval Process

1. Legal team reviews this document
2. Privacy team signs off on data handling
3. Security team reviews bridge implementation
4. Product team approves user-facing disclosures
5. This file is updated with approval signatures

---

**Last updated**: 2026-03-01
**Review requested by**: Engineering
**Review status**: PENDING
