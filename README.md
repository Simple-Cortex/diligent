# Diligent

A portable Agent Skill for auditing U.S. government websites and digital services.

Diligent helps an AI coding agent do what an accessibility and compliance reviewer does: work out which rules actually apply, gather evidence, record what passed, failed, and could not be checked, and plan repairs that keep the service usable. Every step leaves a durable artifact, and the final status is computed, not asserted.

Diligent is not a government service, does not issue certifications, and is not affiliated with any agency.

```text
scope and authority -> applicability -> evidence -> results -> findings -> repair plan -> independent verification
```

**Version 0.1.1 is a contracts-and-gate release. It ships zero automated website checks.** It defines the workflow, the artifact formats, the authority registry, the rule packs, and a deterministic gate. It does not find or clear accessibility issues by itself. Read [What 0.1.1 does and does not do](#what-011-does-and-does-not-do) before you rely on it.

---

## Why this exists

Ask a capable coding agent whether a government site is compliant and you get a confident answer. It is usually not an audit. Nobody decided which law applies, which version of the standard is binding, what was actually tested, or what was skipped. The agent produced a plausible checklist; compliance is the reasoning that should have come first.

That reasoning does not survive a chat. It happens implicitly, in one pass, and it is graded by the same agent that produced it. Four failure modes follow:

1. **Wrong rules.** A `.gov` hostname, a city name, or a logo becomes a legal conclusion. Federal, state, local, and program obligations differ, and some are not yet in effect.
2. **Pass by silence.** A check that did not run, a page that did not load, or a timeout is quietly counted as a pass.
3. **Evidence from assertion.** A policy page, a vendor report, or a comment in the code is accepted as proof of how the deployed service behaves.
4. **Self-certified repair.** The agent that wrote the fix declares the problem solved.

Diligent makes the agent work the way a careful reviewer does instead. Resolve scope and applicability against a frozen set of sources. Collect evidence with provenance. Record results in explicit states, with unknown and untested kept visible. Compute the overall status mechanically. Hand repairs to a separate context for verification.

## What using it looks like

You install one skill directory and ask your agent in plain language: "audit this site for Section 508," "is our benefits application compliant," "plan fixes for these findings." The skill routes the request through phases, and each phase writes its artifact before the next may start.

| Phase | What the agent does | Durable output |
| --- | --- | --- |
| `init` | Records the target, the mode, and every fact it does not yet know | `.diligent/<audit-id>/RUN.json`, `SCOPE.json`, `SERVICE-BRIEF.json` |
| `discover` | Works out what the project is, who governs it, which UI libraries it uses, and which resources exist | `PROJECT-CONTEXT.json`, `AGENCY-PROFILE.json`, `DESIGN-SYSTEM.json`, `RESOURCE-CATALOG.json` |
| `scope` | Resolves which obligations apply, with a cited predicate for each, and asks only the questions that matter | `APPLICABILITY.json`, `AUTHORITIES.lock.json`, `RULES.lock.json` |
| `inventory` | Lists the routes, templates, components, documents, and vendors it is allowed to look at | `INVENTORY.json` |
| `plan` | Lays out the checks, the sample, the manual work, and the denominators | `AUDIT-PLAN.json`, `REVIEW-QUEUE.json` |
| `collect` | Gathers observations within stated budgets and origins | `EVIDENCE.jsonl` |
| `assess` | Applies the observations to the rules and routes questions it cannot settle | `RESULTS.jsonl`, `FINDINGS.json` |
| `propose` | Drafts repair options that respect the task and the approved wording | `REPAIRS.json`, `PROJECT-RULES.json` |
| `verify` | A different context retests the change from frozen before and after evidence | `VERIFICATION.json` |
| `report` | Renders the recorded facts, with coverage counts and gaps first | `REPORT.md` |

Two properties distinguish this from asking an agent to "check carefully":

**Missing is not passing.** Every result has one of six states: `pass`, `fail`, `needs_review`, `not_tested`, `blocked`, or `error`. A skipped test is `not_tested`. An unknown applicability keeps the assessment `incomplete`. The overall status is one of `fail`, `incomplete`, `pass_for_declared_scope`, or `not_applicable`, and `incomplete` can never become a pass. An exception is recorded beside a failure; it never rewrites one.

**The verifier is not the author.** `verify` runs in a context that did not write the repair, from frozen evidence. A pull request or a green build is not proof that a barrier is gone.

## Who it is for

**Accessibility and compliance leads.** You need the audit to say what it covered and what it did not. Diligent reports counts with denominators, lists unsupported criteria, and routes the judgment calls to named roles instead of hiding them.

**Developers and product teams on government services.** You need to know where to fix something and what must not change. Findings carry a location, a reproduction, an acceptance test, and the protected constraints, such as required legal wording and approved flows.

**Program, privacy, and security owners.** You need the right questions at the right time. Diligent produces evidence requests that name the exact artifact, the owner role, and the obligation it would settle, and it never treats a missing document as proof that a requirement does not exist.

**What it will not do:** lint a non-government site for generic accessibility, certify anything, grant an authorization to operate, approve a privacy assessment, or decide eligibility. For generic accessibility linting use a general accessibility tool.

## How the system is built

The implementation is small: zero-dependency Node ESM, plain JSON and Markdown, no framework, no build step. Everything an agent needs to act on is a file it can read.

```text
SKILL.md                     the router; the only surface a host must understand
skills/<command>/SKILL.md    ten trigger-only subskills for hosts that discover nested skills
references/                  the teaching layer; workflow.md is the normative protocol
authorities/registry.json    the source register: laws, policies, standards, and guidance
rules/packs/                 seven rule packs at the research lifecycle
profiles/                    jurisdiction profiles that select packs
prompts/                     four bounded, review-only model tasks
schemas/                     JSON Schema for every artifact
bin/diligent.mjs             the command line
docs/                        supported checks, known limitations, changelog policy
```

**Authorities.** Each source record carries an official link, a status, dates only where the source states them, and a review status. Sources that could not be fully retrieved are marked `unverified`. Nothing is `reviewed` until a human reviews it.

**Rule packs.** A rule record carries its authority and exact section, its modality (shall, must, should, may), a safe applicability predicate, temporal rules, supported channels, blind spots, and fixture slots. All rules ship at the `research` lifecycle. None is published.

| Pack | Rules | Scope |
| --- | --- | --- |
| `a11y-wcag20-aa` | 38 | WCAG 2.0 A and AA, the Section 508 baseline |
| `a11y-wcag21-aa` | 12 | WCAG 2.1 additions, the ADA Title II baseline |
| `a11y-wcag22-aa` | 6 | WCAG 2.2 additions, a recommended target |
| `federal-public-web` | 42 | Identity, required content, discoverability, design, pending GSA standards, Section 508 content, support, and procurement |
| `privacy-collection` | 17 | Privacy, information collection, records, open information |
| `infrastructure` | 26 | Security, identity, hosting, operations |
| `ux-service` | 18 | Service UX review questions, not legal obligations |

**Profiles.** `us-federal-public-web` is a draft. `us-state-local-ada-title-ii` is a research profile. State, local, and sector overlays need a specialist before any whole-profile claim.

**The gate.** `diligent gate` recomputes the assessment from the recorded artifacts. Exit 0 means pass for the declared scope, 1 a confirmed mandatory failure, 2 incomplete, and 3 an invalid configuration. It never reads prose and never guesses.

## What 0.1.1 does and does not do

It does:

- Route an audit through ten phases with durable, schema-checked artifacts.
- Ship an authority registry and seven research-lifecycle rule packs with explicit modality, applicability predicates, and blind spots.
- Compute a deterministic overall status from recorded results, and render a report.
- Initialize an audit directory, show its status, and validate its own rule data.
- Guide an agent through discovery, scoping, evidence handling, review routing, and repair planning with bounded, review-only model tasks.

It does not:

- Run any automated check of a website. **There are zero.** Every rule is `unsupported`, `human`, or `documentary`. See [`docs/supported-checks.md`](docs/supported-checks.md).
- Certify, declare compliance, or issue a pass for a site. A clean result in 0.1.1 can only mean that the supplied results show no failure for the declared scope and nothing required was left untested.
- Verify that any rule matches its source text. Rules are cataloged for review, not reviewed.
- Include state, local, tribal, territorial, sector, or agency overlay packs.
- Calibrate any model. Model tasks are review-only.

Known limits are tracked in [`docs/known-limitations.md`](docs/known-limitations.md).

## Requirements and installation

Node 20 or later enables the command line helpers. The skill itself has a manual, host-neutral fallback described in `references/workflow.md`.

### Install with the `skills` CLI (recommended)

```bash
npx skills@latest add Simple-Cortex/diligent
```

Target a specific agent, or install globally instead of into the current project:

```bash
npx skills@latest add Simple-Cortex/diligent --agent <agent>
npx skills@latest add Simple-Cortex/diligent --global
```

By default this installs the root skill, `diligent`, which is the router. To install the ten phase skills as separately triggerable entries, ask for the full tree:

```bash
npx skills@latest add Simple-Cortex/diligent --full-depth --skill '*'
```

Inspect, update, or remove later:

```bash
npx skills@latest add Simple-Cortex/diligent --list
npx skills@latest list
npx skills@latest update
npx skills@latest remove diligent
```

### Install manually

From a checkout:

```bash
node scripts/install.mjs --dest <your-skill-directory>/diligent
```

Or copy `SKILL.md`, `references/`, `authorities/`, `rules/`, `profiles/`, `prompts/`, `schemas/`, `scripts/`, `bin/`, and `skills/` into your host's skills directory. There is nothing to build. Skill discovery paths vary by host, so use an explicit destination and confirm that your host picks it up.

## Quick start

```bash
node bin/diligent.mjs init public-site --profile us-federal-public-web --mode combined
node bin/diligent.mjs status public-site
node bin/diligent.mjs gate public-site      # exits 2: a new audit is incomplete
node bin/diligent.mjs report public-site    # needs ASSESSMENT.json, so run gate first
node bin/diligent.mjs rules validate
```

`status`, `gate`, and `report` also accept `--root <workspace>` or `--dir <audit-directory>`. `init` accepts `--force` and `--json`.

Then ask your agent to continue with `discover`, `scope`, and the later phases. The gate staying at `incomplete` until required work is done is the intended behavior.

## License

MIT. See [LICENSE](LICENSE).
