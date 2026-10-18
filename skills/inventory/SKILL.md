---
name: "diligent-inventory"
description: "Use when scope is resolved and the permitted routes, templates, components, documents, vendors, roles, and states have not been enumerated, or when the deployment changed. Discovers assets without running project code."
---

# Diligent: inventory

Stage `inventory` of the Diligent chain. The root skill (`../../SKILL.md`) routes; this file only names the contract.

**Required input:** Resolved scope, allowed origins and roles, and a repository snapshot or captured environment.

**Durable output:** `INVENTORY.json` listing assets, explicit exclusions, unreachable paths, and discovery limits. `SAMPLE.json` records the WCAG-EM structured and random sample with its method and seed; a sample never supports a whole-site claim.

**Read, in order:**

1. `../../references/workflow.md` section for `inventory`
2. `../../references/evidence.md` for allowed origins, budgets, and untrusted-content rules

**Executable path:** with the `diligent-rs` binary installed, `diligent audit --url <https://host/>` runs inventory, sampling, planning, collection, assessment, the gate and the report in one pass; see `../../references/workflow.md` section 4a. It still needs resolved facts (`--facts`).

**Block only when** no allowed origin or readable source tree exists. Never execute install hooks or arbitrary project commands to discover assets.
