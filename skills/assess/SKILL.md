---
name: "diligent-assess"
description: "Use when evidence exists and results, findings, and review routing have not been produced, or when evidence or rules changed. Applies supplied observations to rules and routes bounded questions to review."
---

# Diligent: assess

Stage `assess` of the Diligent chain. The root skill (`../../SKILL.md`) routes; this file only names the contract.

**Required input:** Frozen evidence, `APPLICABILITY.json`, `AUDIT-PLAN.json`, and the rule packs.

**Durable output:** Appended `RESULTS.jsonl`, `FINDINGS.json`, an updated `REVIEW-QUEUE.json`, and `ASSESSMENT.json` when the gate is run.

**Read, in order:**

1. `../../references/assessment.md`
2. `../../references/evidence.md`
3. `../../prompts/` for the bounded model tasks

**Block only when** evidence or lock references are invalid. Invalid artifacts produce an execution error and an incomplete assessment, never a pass.
