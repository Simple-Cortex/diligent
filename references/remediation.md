# Design-aware remediation

Every repair is proposed against a failed requirement, an affected task, and the protected constraints of an existing service. Diligent 0.1.0 plans and records repairs. It does not verify them automatically, and applying code changes is a separate requested operation.

## 1. Repair standard

State for every proposal: the failed requirement, the affected task, the smallest reliable change, why it preserves or improves the experience, regression risks, and acceptance evidence.

| Issue | Minimal repair | Broader option when justified | Constraint |
| --- | --- | --- | --- |
| Low-contrast supporting text | Adjust the shared color token and retest states | Rebalance typography and emphasis if the palette cannot support hierarchy | Keep supporting text readable without making every label dominant |
| Dense required-link footer | Add accurate links to the shared identifier or policy area | Separate institutional links from task help and improve mobile grouping | Preserve required access and placement; keep legal navigation out of the primary task area |
| Broken custom dropdown | Repair semantics, focus, and keyboard behavior, or use a suitable native control | Replace the interaction if complexity causes repeated errors | Retain search and multiselect needs and task efficiency |
| Inaccessible legal PDF | Remediate the document and offer accessible HTML | Rebuild the process as a digital flow where authorized | Preserve official wording, signatures, recordkeeping, and approved collection |
| Poor errors | Add actionable inline errors, associated instructions, and a useful summary | Simplify fields and validation timing using task evidence | Preserve correct input after failure; avoid loud announcements on every keystroke |
| Hover-only help | Support focus and applicable hover-content behavior | Put essential instructions inline | Keep optional detail available without hiding what is needed to finish |
| Overly restrictive sign-in | Support password managers and paste, and correct assurance-specific controls | Adopt the required shared sign-in and improve recovery | Keep security, assurance, and accessible alternatives intact |
| Excessive tracking | Remove unnecessary collection and update the actual notices and controls | Use approved aggregate task metrics | Retain useful service measurement without collecting sensitive payloads |
| Mobile table overflow | Constrain two-dimensional scrolling to the table; preserve headers | Add task-appropriate summary and detail views | Do not hide required columns or call every legitimate data table a reflow failure |
| Unnecessary warning banner | Check the actual authority and restrict to the right audience and context | Separate public information from authenticated system notices | Current federal policy cautions against treating general-public services as requiring a system-use warning banner |

Offer alternatives when tradeoffs are real. Do not redesign a whole page for a local defect. For substantial changes, use a task, structure, and contract frame constrained by required language, agency identity, accessibility, and approved product behavior. Test whether a repair introduces new contrast, focus, reading-order, privacy, or abandonment problems.

## 2. Repair proposal contract

A repair in `REPAIRS.json` includes: finding ids, task ids, authority constraints, impacted source, component, and token locations, the minimal change, a structural alternative where useful, reasoning, expected effects, risks, required owner decisions, acceptance tests, and evidence to collect. If the location cannot be mapped to source, give a runtime target and a behavior specification instead of inventing a filename.

Rank options in this order:

1. Satisfy applicable obligations.
2. Preserve security, privacy, and program correctness.
3. Restore the user's task.
4. Preserve approved identity and useful hierarchy.
5. Reduce implementation and maintenance cost.

Prefer the smallest effective change when evidence does not justify a redesign. This ranking never lets aesthetics or convenience offset a legal failure.

**Protected constraints.** Approved text, field meanings, eligibility logic, transaction amounts, consent choices, retention behavior, and assurance levels need the corresponding owner's decision. Preserve that decision as evidence. The audit prepares the review request; it does not send messages to those owners.

## 3. Visual and interaction contract

Before substantial changes capture the design-system and version, semantic color and type tokens, spacing and grouping, primary and secondary actions, responsive behavior, content order, component states, and permitted departures. A missing design system is not permission to replace the brand.

Compare before and after at the same viewport, content fixture, fonts, theme, and state. Use image differences to locate changes, then review their meaning. Antialiasing and dynamic content are not defects by themselves. Retest contrast, semantic order, focus, target size, error handling, labels, layout, and long-content behavior affected by the change. Never publish personal or payment information as design evidence.

## 4. Closure lifecycle

Finding states: `open`, `triaged`, `repair_proposed`, `change_applied`, `retest_pending`, `verified_resolved`, `accepted_risk`, `exception_recorded`, `disputed`. State changes keep history and evidence. `accepted_risk` and `exception_recorded` never erase the original observation.

Close a finding only when all of these hold:

- The failed acceptance condition passes on the changed snapshot.
- Required manual and documentary checks are complete.
- Affected shared instances were addressed.
- Specified regressions were assessed.

A pull request, a proposed patch, or a successful build is not resolution evidence. Independent verification means a separate assessment step with frozen before and after evidence and no reliance on the repair author's claim of success. A deterministic verifier or a qualified human can meet it. A model reviewing its own draft cannot support a consequential conformance claim.

## 5. Project repair rules and usage guardrails

A project-local rule pack comes from verified findings and accepted constraints, with stable ids and version history.

- **Repair rules** describe existing defects and the exact conditions for resolving them.
- **Usage guardrails** describe supported ways to consume components so the defect does not recur: required props, approved wrappers, forbidden combinations, token constraints, and contextual responsibilities.

Store authoritative rules as structured data in `PROJECT-RULES.json`. Generate human and agent instructions in `COMPONENT-GUIDANCE.md` from those records. Keep findings in `COMPONENT-BACKLOG.json`. Do not silently overwrite existing agent instructions, skills, editor instructions, or project policies. Generated guidance is derived and cannot override higher-priority instructions, law, or approved project constraints.

Each rule carries:

| Field | Meaning |
| --- | --- |
| Identity | Rule id, version, type, finding refs, component ids, owner |
| Scope | Installed version range or exact source hashes, wrapper and theme scope, variants, usage predicates |
| Basis | Verified observation, exact requirement, supporting library documentation, task impact |
| Root cause | Upstream, theme or token, wrapper, consumer, mixed, or unresolved, with evidence |
| Instruction | Desired behavior, allowed repair locations, protected APIs, content, and design constraints |
| Agent procedure | Inspect docs and source, reproduce, implement in allowed scope, run the exact tests, report outcomes |
| Preconditions | Supported configuration, fixture, needed review or authorization, blocked dependencies |
| Acceptance | Deterministic tests, runtime scenarios, manual or assistive-technology review, visual and task regression checks |
| Spread | Known affected instances and dependency edges, with explicit limits on inferred impact |
| Lifecycle | Draft, validated, active, superseded, retired; review and invalidation triggers |

Distinguish the normative basis from implementation advice. A requirement for an accessible name does not mandate one attribute; the name might come from visible text or a label relationship. Support approved alternatives rather than forcing one brittle implementation. A draft with an unresolved binding or missing evidence cannot become an active enforcement rule.

### Work loop

1. Load active project rules, the design-system identity, relevant official documentation, and the frozen finding evidence.
2. Reproduce the issue in the actual supported configuration. If reproduction fails, record the mismatch instead of guessing.
3. Identify the owning layer and the affected dependency graph. Escalate an unresolved root cause or a protected-requirement change.
4. Propose or apply the repair according to the authorized mode, preserving product behavior and visual constraints.
5. Run component tests and affected page and journey checks, including overlays, providers, and alternate states.
6. Obtain required semantic, manual, and documentary review. Mark `verified_resolved` only under the closure contract.
7. Promote supported preventive instructions to active guardrails and update the backlog and fixtures. Reopen related findings if the retest reveals a broader defect.

Evaluate configuration or usage changes, theme and token fixes, shared wrapper fixes, a verified compatible upstream patch, and replacement or forking only when necessary. This is not a rigid order; choose the smallest maintainable fix at the actual root cause. A major version upgrade needs explicit migration and regression planning. A new release existing is not proof it fixes the defect.

Never edit installed dependency directories as the persistent fix. If patching is needed, use a reviewed reproducible patch mechanism or a maintained fork with provenance, owner, and an upgrade or removal plan. For copied components, repair the local source and keep upstream lineage. Do not force migration to any specific design system merely to satisfy the audit.

A generated rule must not lower a legal threshold, remove an exception check, weaken an assertion, or alter the baseline merely to make a patch pass. Rule changes need their own rationale and review, distinct from the code repair. Model suggestions do not automatically become persistent policy.

### Incremental evaluation

On later runs compare library versions, lockfiles, local source hashes, theme and tokens, provider settings, component usage, documentation versions, active authority packs, and critical task definitions. Reevaluate changed nodes and their dependents.

- A token change invalidates dependent color and spacing checks.
- A wrapper event or ref change invalidates relevant interaction tests.
- A library upgrade invalidates version-bound guardrails until compatibility is checked.
- An unchanged dependency version does not preserve evidence if local source or global styles changed.
- A legal or profile change can invalidate required tests even when component code is unchanged.

Keep resolved findings and successful tests as history. Never convert historic evidence into a fresh run timestamp. Components added after the baseline start as `not_assessed`. If impact cannot be bounded, broaden the retest. Prioritize by mandatory failure, task consequence, shared reach, dependency order, and fix risk, and keep all known mandatory failures visible even when repair is staged.
