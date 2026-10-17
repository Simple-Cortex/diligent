---
name: "diligent-report"
description: "Use when results exist and a readable rendering is wanted, from any state including incomplete. Renders the recorded assessment facts without adding a second verdict."
---

# Diligent: report

Stage `report` of the Diligent chain. The root skill (`../../SKILL.md`) routes; this file only names the contract.

**Required input:** The current artifacts. Run `gate` first so `ASSESSMENT.json` is current.

**Durable output:** `REPORT.md` beginning with target, snapshot, modes, profiles, and assessment status, in the required section order.

**Read, in order:**

1. `../../references/assessment.md` report structure

**Block only when** artifacts are invalid. Report the validation errors instead of a status.
