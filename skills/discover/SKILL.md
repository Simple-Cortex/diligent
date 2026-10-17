---
name: "diligent-discover"
description: "Use when an audit has been initialized but project purpose, governing agency, the UI component libraries in use, documentation resources, or candidate authorities have not been established. Runs before final scope and planning."
---

# Diligent: discover

Stage `discover` of the Diligent chain. The root skill (`../../SKILL.md`) routes; this file only names the contract.

**Required input:** The initialized run, authorized repository metadata or published project metadata, and any supplied project or contract documents.

**Durable output:** `PROJECT-CONTEXT.json`, `AGENCY-PROFILE.json`, `REGULATORY-DISCOVERY.json`, `DESIGN-SYSTEM.json`, and `RESOURCE-CATALOG.json`, each with evidence references and unresolved questions.

**Read, in order:**

1. `../../references/discovery.md`
2. `../../references/overlays.md` for the agency research sequence and activation gates
3. `../../references/evidence.md` for what may be read and what must be redacted

**Block only when** repository access is denied and no page metadata is available. An unknown in one discovery track never blocks the other.
