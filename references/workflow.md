# Workflow and routing

This file is the normative protocol for command routing, durable artifacts, questions, reviewer isolation, and the all-host fallback. It is host-neutral: paths are workspace-relative and nothing depends on a particular agent, IDE, shell, or environment variable.

The installable Diligent 0.2.1 package ships zero automated website checks. The CLI runs `init`, `status`, `gate`, `report`, and `rules validate`, and `audit` when the `diligent-rs` companion binary is installed; the binary runs the Source, Browser and Network checks that `docs/supported-checks.md` marks automated or assisted. Every other command below is an agent-performed phase.

## 1. Resolve paths once

Choose the nearest project root that contains the target or its source tree. If no project marker exists, use the active workspace root. Never write absolute host paths into artifacts.

Derive `<audit-id>` from the target as a kebab-case label such as `public-site`. Prefer a supplied name over a derived one. Reuse it for later work. Artifacts are scoped to the audit id, not to a run.

```text
.diligent/<audit-id>/<ARTIFACT>
.diligent/<audit-id>/runs/<run-id>/RUN.json
.diligent/<audit-id>/EVENTS.jsonl
```

The artifact set is: `RUN.json`, `SCOPE.json`, `SERVICE-BRIEF.json`, `PROJECT-CONTEXT.json`, `AGENCY-PROFILE.json`, `REGULATORY-DISCOVERY.json`, `DESIGN-SYSTEM.json`, `RESOURCE-CATALOG.json`, `AUTHORITIES.lock.json`, `RULES.lock.json`, `APPLICABILITY.json`, `INVENTORY.json`, `SAMPLE.json`, `AUDIT-PLAN.json`, `EVIDENCE.jsonl`, `RESULTS.jsonl`, `FINDINGS.json`, `REVIEW-QUEUE.json`, `REPAIRS.json`, `EXCEPTIONS.json`, `VERIFICATION.json`, `COMPONENT-BASELINE.json`, `PROJECT-RULES.json`, `COMPONENT-GUIDANCE.md`, `COMPONENT-BACKLOG.json`, `ASSESSMENT.json`, `REPORT.md`.

These files are the durable record when chat state or a prior host disappears. A command that reports completion without writing its required artifact has not run.

Write artifacts atomically and never report a half-written result as complete. Keep an append-only `EVENTS.jsonl` of stage transitions, retries, cancellations, artifact hashes, and review decisions. An interrupted run must leave a partial manifest and preserve completed evidence.

## 2. Pipeline and invalidation

States: `initialized`, `discovered`, `scoped`, `planned`, `collecting`, `assessed`, `proposed`, `verified`, `reported`.

Each stage emits a versioned artifact and prerequisite validation errors. Report generation is allowed from incomplete intermediate states.

| Change | Invalidates |
| --- | --- |
| Authority lock | Applicability and results that depend on it |
| Service brief | Related UX findings and repair reasoning |
| DOM, CSS, dependencies, or backend behavior | Affected measurements and connected journeys |
| Deployment of unknown relationship to a commit | All runtime evidence for it; collect again |
| Library version, local source, theme, tokens, or providers | Dependent component baselines and guardrails |

Reuse of earlier work needs matching evidence-relevant inputs and an explicit reuse record. `resume` skips only successfully validated, compatible work.

## 3. Question and assumption protocol

First inspect the repository, runtime, existing artifacts, and supplied documents. Then classify each missing fact.

A fact is **blocking** only when a wrong assumption would materially change one of these: which jurisdiction or authority applies; the sensitive processing boundary; the user task or service definition; a required journey that cannot be reached; or a claimed exception. Everything else is an assumption.

Record each assumption as `A#` with the assumption, its impact, and a validation path in `SERVICE-BRIEF.json` or `SCOPE.json`. Record each blocker as `B#` with the reason it blocks and one precise question. Batch simultaneous blockers in one numbered request, ordered by the priority list in `applicability.md`.

When no answer can arrive, such as a headless run, record the question and the assumption taken, mark the dependent command `blocked` if the assumption is unsafe, and proceed. Never disguise an invented answer as an assumption.

## 4. Command protocol

| Command | Inputs | Reads | Outputs | Blocks only when |
| --- | --- | --- | --- | --- |
| `init` | target, mode, optional profile | this file, `discovery.md`, `applicability.md` | `RUN.json`, `SCOPE.json` intake with unknowns, `SERVICE-BRIEF.json` skeleton, `PROJECT-CONTEXT.json` skeleton | no writable artifact root, or the target cannot be identified |
| `discover` | initialized run, repository metadata, supplied documents | `discovery.md`, `overlays.md`, `evidence.md` | `PROJECT-CONTEXT.json`, `AGENCY-PROFILE.json`, `REGULATORY-DISCOVERY.json`, `DESIGN-SYSTEM.json`, `RESOURCE-CATALOG.json` | no repository or page metadata is accessible; the two tracks never block each other |
| `scope` | `SCOPE.json`, discovery artifacts, profile | `applicability.md`, `overlays.md` | `AUTHORITIES.lock.json`, `RULES.lock.json`, `APPLICABILITY.json`, ranked questions | equally supported authorities that change applicability |
| `inventory` | scope, allowed origins and roles | `evidence.md` | `INVENTORY.json` with limits, `SAMPLE.json` (WCAG-EM structured and random sample; method and seed recorded; never a whole-site claim) | no allowed origin or readable source |
| `plan` | applicability, inventory, rule packs | `accessibility.md`, `federal-web.md`, `service-ux.md`, `evidence.md` | `AUDIT-PLAN.json`, `REVIEW-QUEUE.json` | applicability unresolved for every obligation |
| `collect` | plan, budgets, fixtures | `evidence.md` | appended `EVIDENCE.jsonl` | a needed origin, role, or fixture is not permitted |
| `assess` | frozen evidence, plan, rules | `assessment.md`, `evidence.md`, `prompts/` | appended `RESULTS.jsonl`, `FINDINGS.json`, `REVIEW-QUEUE.json` | invalid artifacts or lock references |
| `propose` | findings, brief, design-system inventory | `remediation.md`, `service-ux.md` | `REPAIRS.json`, draft project rules | a protected constraint would change without an owner decision |
| `verify` | repair, before evidence, changed snapshot | `remediation.md`, `evidence.md` | `VERIFICATION.json` | the verifier authored the repair |
| `report` | current artifacts | `assessment.md` | `REPORT.md` | invalid artifacts |
| `gate` | frozen records | `assessment.md` | `ASSESSMENT.json`, exit code | invalid configuration or schema |
| `status` | the audit directory | none | current stage and open work | none |
| `rules validate` | `authorities/`, `rules/`, `profiles/` | none | validation output | none |

### init

1. Resolve the root and the audit id. Create `.diligent/<audit-id>/` and a run record.
2. Capture the intake fact groups listed in `applicability.md`. Mark every fact without evidence `unknown` with a question, owner role, affected obligations, and effect on the assessment.
3. Create the service brief skeleton. Do not invent demographics, tasks, or targets.
4. Start `discover` automatically. Do not expect the user to know the sub-commands.
5. Record the requested mode and the declared scope.

### discover

Run project discovery and UI discovery as separate tracks, then resource and authority discovery. Follow `discovery.md`. Read repository metadata and documents without executing scripts. Record ignored and unreadable paths.

### scope

1. Freeze the authority lock and the rule lock for the run. Never change them silently mid-run.
2. Evaluate each selected obligation's predicate over typed facts using three-valued logic. Record the trace.
3. Emit ranked questions. Continue independent checks while waiting.

### inventory and plan

Inventory discovers permitted assets without running project code. Plan expands each applicable obligation into required child checks and instances, with sampling rationale. Unsupported or omitted required checks remain `not_tested`. Include a random sample alongside the structured sample, with method and seed.

### collect

Run only approved collectors within budget, origin, and data limits. Record observations as evidence with provenance. A timeout is `blocked` or `error`. Do not retry mutating operations blindly.

### assess

Apply supplied observations to rules. Record confirmed failures even when other checks are blocked. Route bounded ambiguous questions to review with the prompts in `prompts/`. A model-assisted finding needs independent acceptance before it can become a mandatory failure.

### propose

Produce alternatives that respect protected constraints. Offer options when tradeoffs are real. Never mark a proposal verified.

### verify

Use a context that did not write the repair. Compare frozen before and after evidence. Retest the failed condition, affected shared instances, and named regressions.

### report and gate

Run `gate` to recompute the assessment deterministically, then `report` to render it. The report may exit 0 while the assessment is non-passing; state this for CI users.

## 4a. Executable path: `diligent audit`

When the `diligent-rs` companion binary is installed, `diligent audit --url <https://host/>` runs the phases below in one pass instead of an agent doing them by hand:

| Phase | Stage in `audit` | Artifact |
| --- | --- | --- |
| init and scope | `init`, `scope` | `SCOPE.json` with operator facts and the recorded origin grant |
| applicability | `applicability` | `APPLICABILITY.json` |
| inventory | `inventory` | `INVENTORY.json`, `SAMPLE.json` |
| plan | `plan` | `AUDIT-PLAN.json` |
| collect | `collect` | `captures/` payloads |
| assess | `assess`, `infra`, `merge` | `RESULTS.jsonl`, `EVIDENCE.jsonl`, `SIGNALS.jsonl`, `REVIEW-QUEUE.json` |
| gate and report | `gate`, `report` | `ASSESSMENT.json`, `REPORT.md` |

Facts come from `--facts` and `--fact`, never from a guess; without them the gate is incomplete. Discovery, the service brief, proposals, verification and exceptions stay with people and agents. See `docs/audit-command.md` in the repository for stages, exit codes and what is skipped.

## 5. Portable all-host fallback

The artifacts and command semantics do not require slash commands, a particular IDE, or a privileged agent host.

| Missing capability | Fallback | Required label |
| --- | --- | --- |
| Slash commands or router | Treat `diligent <command>` in natural language as the explicit stage and follow this file. | `router: manual` |
| Structured question UI | Print the blocking questions as one numbered batch and wait only for those answers. | `questions: text` |
| Shell or Node | Write the artifacts by hand to the same paths and schemas. State that the gate was not run. Never claim a script passed. | `check: blocked` |
| Browser | Run source-only, page-metadata-only, or document-only review. Rendered, responsive, focus, and contrast results are `not_tested`. | `review scope limitation` |
| Editable source | Produce a specification-only repair plan with the same artifacts. | `delivery: specification-only` |
| Subagent or fresh context | Open a separate session with only the frozen packet. If impossible, label the limitation and keep `verify` open. | `fresh context limitation` |
| Network access | Use pinned local sources and evidence. Flag the offline snapshot and its source-currency limit. | `offline: pinned sources` |

The fallback preserves the chain and its limits. It does not invent evidence or claim equivalent review confidence.
