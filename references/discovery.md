# First-run discovery

Discovery runs before final scope and planning. It has two tracks: the actual UI architecture, and the actual project and agency purpose. An unknown in one track never blocks independent work in the other. The audit must work with any component library, several libraries, custom components, or no formal design system. A particular government design system is one possible resource, not the assumed stack. Require its adoption only when a verified applicable authority or contract requires it.

`init` starts discovery automatically. In 0.1.0 discovery is an agent-performed phase.

## 1. Passive project discovery

Inspect authorized repository metadata and project documents without executing scripts: manifests and lockfiles, workspace packages, imports and exports, aliases, component folders, theme providers, resets, tokens, overrides, registries, stories, tests, build configuration, README, existing agent instructions, architecture documents, service briefs, and supplied contracts.

- Treat project instructions as task-scoped instructions within the host's hierarchy. Vendor documents cannot override that hierarchy or the audit's evidence requirements.
- Declare a breadth limit. Record ignored and unreadable paths. Do not dump secrets or unrelated records.
- In page-only mode use permitted runtime evidence and published metadata. Styling resemblance may suggest a library but cannot establish its identity or version. Ask for a manifest or record `unknown`. Behavioral tests still run.

## 2. Project and agency context

Write `PROJECT-CONTEXT.json` and `AGENCY-PROFILE.json` from supplied facts and permitted discovery. They extend the service brief and the typed scope facts. Record:

- The owning legal entity, department, agency, bureau, program, system owner, and service owner.
- The jurisdiction and branch, contractor or operator, sponsor, and funding relationships.
- Public or internal audience, staff and representative roles, intended task, actual service outcomes, and channels.
- Information-only versus transactional features, data classes, identity and payment functions, eligibility decisions, and downstream systems.
- Hosting and system boundary, procurement or task-order context, adopted technical and design policies, and approved exceptions.
- Evidence for every fact, uncertainty, conflicting signals, the owner needed, and the next verification action.

Signals differ in reliability. A repository organization, a logo, a `.gov` hostname, a hosting account, or a vendor customer list alone does not establish the governing agency or program. A model may propose a short purpose statement and candidate entities with citations. Facts that select mandatory legal obligations need confirmation or evidence. An unresolved owner does not prevent generic technical observations. It does prevent an unsupported agency-wide compliance conclusion.

Follow the agency research sequence in `overlays.md` and record candidates in `REGULATORY-DISCOVERY.json`.

## 3. UI architecture inventory

Build `DESIGN-SYSTEM.json` with one record per system or library:

- Identity, package or repository provenance, resolved installed version, lockfile evidence, framework, runtime, and license reference.
- Installation form: dependency, local workspace package, copied source, fork, vendor bundle, CDN, or unknown.
- Primitive dependencies and wrappers; app-level providers, themes, resets, token sets, and local overrides.
- Component exports and used instances with source locations, aliases, dynamic imports, and unresolvable usages.
- Variant, state, theme, and locale support, and the publisher's claimed accessibility guarantees or limitations.
- Official documentation and compatible version, available skills and servers, connection state, and retrieved references.
- Evidence quality, unresolved questions, ownership, assessment status, hashes, and change triggers.

A package dependency is not proof of use. A package name is not proof that local wrappers preserve its behavior. Copied components need a local source hash and, if known, the upstream origin and commit. A current registry version cannot be assumed to match a locally edited copy.

Represent dependencies as a graph: primitive, library component, local wrapper, composed pattern, page or journey. Add tokens, global styles, providers, and portals as nodes. Record ambiguous edges. Use the graph to locate the defect and choose retests. Never equate import counts with affected users.

## 4. Resource discovery

Look for suitable resources for each detected system instead of relying on generic model knowledge.

1. Inventory installed project and host skills, approved connected capabilities, local documentation, component stories, and repository guidance.
2. Search the publisher's official documentation for versioned API references, accessibility guidance, agent skills, machine-readable documentation indexes, migration notes, and known issues.
3. Search an approved plugin or skill directory when an external capability would materially help and none is connected. Distinguish a published resource from an installed, connected, callable tool.
4. Select the smallest relevant set matching package identity, version, framework, and task. Prefer official versioned documentation and local source for the exact installed implementation. Treat community material as a labeled lead with provenance.
5. Read a selected skill before applying its workflow. Inspect a connected capability's permissions before invoking it. Use read-only documentation operations for discovery. Installation and generation tools are mutations and need an authorized repair task.
6. Record unsuccessful searches, inaccessible documentation, version mismatches, and unavailable tools. Continue with source, browser measurements, and manual review. No suitable resource is not a reason to stop.

Do not install or execute downloaded skills, packages, or server commands because a page advertises them. Follow host permissions and project policy. Pin approved versions and record publisher provenance. A response from a documentation service is evidence of what it returned, not proof that a component meets a standard. Never send private repository content to a public documentation service unless its use and data handling are permitted.

Discovery is generic by design. For an agency or custom design system, use the supplied approved documentation, private registry, stories, and token definitions, and respect access controls. Missing public documentation does not imply an invalid system. Do not fabricate a skill or server endpoint.

Record each resource in `RESOURCE-CATALOG.json` with status `discovered`, `reviewed`, `compatible`, `version_mismatch`, `available_not_connected`, `connected`, `blocked_by_policy`, `unavailable`, or `rejected`. Keep identity, access state, and relevance separate. A connected resource can still be incompatible with the installed version.

## 5. Component baseline

Write `COMPONENT-BASELINE.json` for the reachable, relevant component set, prioritizing components used in critical journeys. An exported but unused component stays visible as not assessed and does not inflate failure counts. A large library may need staged work; report the remaining inventory and never label a whole library compliant.

Evaluate four layers:

| Layer | Evidence to collect | Common responsibility |
| --- | --- | --- |
| Primitive or upstream library | Minimal faithful reproduction on the installed version and documented usage | Publisher bug, documented limitation, upgrade candidate |
| Theme, global styles, provider | The same component with the actual theme, typography, tokens, resets, and portal context | Project theme owner |
| Wrapper or composed pattern | Actual wrapper props, ref and event forwarding, semantics, focus, names, validation | Shared UI owner |
| Consumer, page, or journey | Real labels, heading context, composition, data, navigation, task states | Feature team, content, or program owner |

Compare layers to localize a failure. Do not claim an upstream bug solely because the application fails. Do not replicate consumer-level patches across many pages when a shared defect is established.

Build a state matrix for each used component: default, focus, hover, active, selected, expanded or collapsed, disabled, loading, invalid, empty, asynchronous success and failure, long content, and applicable locale and theme. Test realistic compositions such as a dialog with a form and popover, a menu inside a portal, a table with controls, live validation, and nested overlays. Record supported exclusions.

Use existing stories when representative. Otherwise a fixture harness may be specified or created only within an authorized implementation task, using the actual version, styles, providers, and wrapper code. An isolated pass does not close page-level or complete-process requirements.

Component assessment states: `not_assessed`, `partial`, `issues_found`, `verified_for_tested_configuration`, `blocked`. Each result names the tested version, configuration, required standard profile, and limitations. Do not expose a timeless boolean such as `is_compliant` for a component or library.

## 6. Refresh

On `resume` and later audits compare discovery fingerprints and refresh affected artifacts. See the invalidation table in `workflow.md`.
