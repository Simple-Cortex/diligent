---
name: "diligent-collect"
description: "Use when a plan exists and evidence must be gathered from the repository, the running service, documents, or the owner, within stated budgets, origins, and data limits. Records observations; it does not judge them."
---

# Diligent: collect

Stage `collect` of the Diligent chain. The root skill (`../../SKILL.md`) routes; this file only names the contract.

**Required input:** `AUDIT-PLAN.json`, permission and budget limits, and safe fixtures or test accounts when journeys are in scope.

**Durable output:** Appended `EVIDENCE.jsonl` records with provenance, hashes, trust level, and redaction state.

**Read, in order:**

1. `../../references/evidence.md`
2. `../../references/workflow.md` section for `collect`

**Executable path:** `diligent audit --url <https://host/>` collects each sampled page with headless Chrome inside the permitted origin and keeps the payloads under `captures/`; see `../../references/workflow.md` section 4a. Chrome must be installed (`DILIGENT_CHROME`).

**Block only when** a needed origin, role, or fixture is not permitted. Record the item as `blocked` in the plan and continue. Never submit real applications, payments, or notifications.
