---
name: "diligent-propose"
description: "Use when established findings need repair alternatives that respect the service task, approved wording, and design constraints. Produces proposals; never marks anything verified and does not apply changes unless separately asked."
---

# Diligent: propose

Stage `propose` of the Diligent chain. The root skill (`../../SKILL.md`) routes; this file only names the contract.

**Required input:** Findings with frozen evidence, the service brief, the design-system inventory, and any active project rules.

**Durable output:** `REPAIRS.json`, and where a library issue is established, entries in `PROJECT-RULES.json`, `COMPONENT-GUIDANCE.md`, and `COMPONENT-BACKLOG.json` as drafts.

**Read, in order:**

1. `../../references/remediation.md`
2. `../../references/service-ux.md`
3. `../../references/discovery.md` for the design-system layers

**Block only when** a repair would change approved text, eligibility logic, amounts, consent, retention, or assurance level without an owner decision. Prepare the review request instead.
