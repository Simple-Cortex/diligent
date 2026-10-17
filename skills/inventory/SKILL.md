---
name: "diligent-inventory"
description: "Use when scope is resolved and the permitted routes, templates, components, documents, vendors, roles, and states have not been enumerated, or when the deployment changed. Discovers assets without running project code."
---

# Diligent: inventory

Stage `inventory` of the Diligent chain. The root skill (`../../SKILL.md`) routes; this file only names the contract.

**Required input:** Resolved scope, allowed origins and roles, and a repository snapshot or captured environment.

**Durable output:** `INVENTORY.json` listing assets, explicit exclusions, unreachable paths, and discovery limits.

**Read, in order:**

1. `../../references/workflow.md` section for `inventory`
2. `../../references/evidence.md` for allowed origins, budgets, and untrusted-content rules

**Block only when** no allowed origin or readable source tree exists. Never execute install hooks or arbitrary project commands to discover assets.
