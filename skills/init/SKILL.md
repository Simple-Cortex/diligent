---
name: "diligent-init"
description: "Use when a government website or digital service audit is about to begin and no `.diligent/<audit-id>/RUN.json` exists, or when intake facts and the service brief have not been captured. Starts the run and triggers first-run discovery."
---

# Diligent: init

Stage `init` of the Diligent chain. The root skill (`../../SKILL.md`) routes; this file only names the contract.

**Required input:** The target (repository path, URL, documents, or a combination), the requested mode, and a profile if the user named one.

**Durable output:** `.diligent/<audit-id>/RUN.json`, an intake in `SCOPE.json` with every unresolved fact marked `unknown`, a `SERVICE-BRIEF.json` skeleton, and a `PROJECT-CONTEXT.json` skeleton with discovery pending.

**Read, in order:**

1. `../../references/workflow.md` sections for `init` and the fallback table
2. `../../references/discovery.md` to start discovery automatically
3. `../../references/applicability.md` for the intake fact groups

**Block only when** no writable artifact root exists, or the target cannot be identified even provisionally.
