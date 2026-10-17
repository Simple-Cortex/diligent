---
name: "diligent-scope"
description: "Use when discovery is recorded but applicability has not been resolved for the selected profile, or when facts, authorities, or the profile have changed since the last scope. Resolves facts against a frozen authority lock and emits the questions that remain."
---

# Diligent: scope

Stage `scope` of the Diligent chain. The root skill (`../../SKILL.md`) routes; this file only names the contract.

**Required input:** `SCOPE.json`, the discovery artifacts, and the selected profile.

**Durable output:** `AUTHORITIES.lock.json`, `RULES.lock.json`, `APPLICABILITY.json` with a predicate trace for every selected obligation, updated `SCOPE.json`, and a ranked question list with owners and effects.

**Read, in order:**

1. `../../references/applicability.md`
2. `../../references/overlays.md`
3. `../../references/workflow.md` section for `scope`

**Block only when** two jurisdictions or authorities are equally supported and the choice would change which obligations apply. Ask one precise question and keep independent checks moving.
