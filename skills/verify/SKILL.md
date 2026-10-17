---
name: "diligent-verify"
description: "Use when a repair has been applied and a changed snapshot exists. Runs in a context that did not write the repair, from frozen before and after evidence, and decides closure."
---

# Diligent: verify

Stage `verify` of the Diligent chain. The root skill (`../../SKILL.md`) routes; this file only names the contract.

**Required input:** The repair record, before evidence, the changed snapshot, and the required retests including affected shared instances.

**Durable output:** `VERIFICATION.json` with retest coverage, any new failures, and remaining review. Findings move to `verified_resolved` only under the closure contract.

**Read, in order:**

1. `../../references/remediation.md` closure lifecycle
2. `../../references/evidence.md`

**Block only when** the verifier is the repair author, or no changed snapshot exists. Stop and request an independent context.
