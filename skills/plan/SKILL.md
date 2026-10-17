---
name: "diligent-plan"
description: "Use when inventory exists and the required obligations, check instances, sampling rationale, manual work, and review queue have not been laid out. Builds the coverage matrix and the denominators."
---

# Diligent: plan

Stage `plan` of the Diligent chain. The root skill (`../../SKILL.md`) routes; this file only names the contract.

**Required input:** `APPLICABILITY.json`, `INVENTORY.json`, and the selected rule packs.

**Durable output:** `AUDIT-PLAN.json` and `REVIEW-QUEUE.json`. Unsupported required checks stay in the plan as `not_tested`.

**Read, in order:**

1. `../../references/workflow.md` section for `plan`
2. `../../references/accessibility.md`
3. `../../references/federal-web.md`
4. `../../references/service-ux.md`
5. `../../references/evidence.md` for sampling and coverage

**Block only when** applicability is unresolved for every selected obligation. A partial plan is still produced for the resolved ones.
