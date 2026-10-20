# Known limitations

A maintained register of what Diligent does not do and what has not been tested. "Observed" means a recorded informal trace exists. It is not "supported". Where there is no evidence the entry says `unmeasured`.

Last updated: 2026-10-20, for version 0.2.1.

**The installable Node package runs zero automated website checks by itself.** Automated and assisted Source, Browser and Network checks come from the `diligent-rs` companion binary, which is installed separately, runs on macOS and Linux only, and needs Chrome or Chromium for browser checks. Every rule stays at the `research` lifecycle and craft signals are advisory.

## Product scope

| Limitation | Impact | Status |
| --- | --- | --- |
| **No automated checks without the companion binary.** Every rule is `unsupported`, `human`, or `documentary` in the Node package alone | Without `diligent-rs` the tool cannot find or clear an accessibility or compliance issue by itself. Findings come from evidence an agent or person supplies | By design. See `supported-checks.md` for the 48 rules with an automated or assisted cell |
| Automated results are leads, not determinations | 45 engine-informed rules were reviewed against primary sources on 2026-10-20 and are at `draft`; all other rules are at `research`. Either way an automated pass or failure is evidence for a person to weigh, not a legal conclusion. Axe-core corroboration can only move a result to `needs_review` | By design for 0.2.1 |
| A sampled audit does not cover the whole site | The structured and random samples cover the pages they name. A pass or a failure applies to the declared scope only, and the omitted pages are recorded in `SAMPLE.json` | Permanent |
| No certification. The tool never certifies, authorizes, or approves anything | A result is never a statement of legal compliance, an authorization to operate, or an approved privacy assessment | Permanent |
| Most rules are cataloged, not source-reviewed | 45 engine-informed rules have `source_review_status` of `reviewed` (2026-10-20). For the other 114, rule text, sections, and modality words are derived from research notes and must be checked against the sources | Research lifecycle for 114 of 159 rules |
| Clause locators are mostly unatomized | Many rules cite an authority without an exact section. A mandatory conclusion needs the exact clause | Open for source review |
| Modality is derived from a class label | The `shall`, `must`, and `should` words are mapped from L, P, C, and R labels, not copied from each source clause | Open for source review |
| Applicability predicates are evaluated only by the companion binary | `diligent audit` evaluates them with three-valued logic over the facts you supply. Without the binary an agent evaluates predicates by following `references/applicability.md`. Unknown stays unknown | Node-only evaluator is not planned |
| Several primary sources are not fully source-verified | The vulnerability disclosure and HTTPS and email directives are still cited by CISA in 2024 to 2026 documents and no rescission was found, so their records are `active`, but their directive text was not re-fetched. The asset visibility directive and the federal HTTPS standard remain `unverified` | Verify before enabling exact timing or header checks |
| State-law WCAG versions are not encoded | Secondary sources conflict on whether Colorado, California, New York, Texas, Minnesota, and Illinois require WCAG 2.0, 2.1, or 2.2. No rule hard-codes a state version | Ask the entity, cite its own statute, and record the adopted version as `adopted_wcag_version` |
| Dates are only those a source states | Offsets such as the 60-day and 240-day milestones are not turned into calendar dates | By design |
| Pending standards produce readiness findings only | The banner, page title, and meta description standards are pending with no supplied timeline | Track the standards catalog |

## Coverage boundaries

| Boundary | Impact |
| --- | --- |
| Federal executive branch public websites first | State, local, tribal, territorial, and DC overlays are not researched. The state and local profile is a research profile |
| No sector overlays | Health, education, tax, criminal justice information, controlled unclassified information, payment, children's, and AI-in-service overlays are activation points only |
| No agency or program rule packs | Agency and program obligations arrive through regulatory discovery and need owner review. Missing public results create evidence requests, not findings |
| Organizational evidence is out of reach | Authorization records, privacy assessments, system of records notices, information collection clearances, records schedules, and contracts must be supplied. A repository cannot prove their absence |
| Sampling is declared, not exhaustive | A pass applies to the declared scope only. A sample never proves a whole product conformant |
| Assistive-technology behavior needs people | Screen-reader and speech-input experience cannot be established from the accessibility tree |

## Model-assisted review

| Limitation | Status |
| --- | --- |
| No model task is calibrated or benchmarked | All four prompts are review-only |
| No expert-labeled benchmark exists | Not started |
| Provider approval, data boundary, and terms are not evaluated by the tool | The user must check approval against the data classification before dispatch |

## Engine

| Engine | Tested | Status |
| --- | --- | --- |
| Node.js CLI | Unit and fixture tests for the gate, status, init, report, rule validation, and the `audit` orchestration | Supported. The reference for the gate and the authority for the report in 0.2.1 |
| Rust workspace | Unit, fixture and end-to-end tests, including an offline replay of recorded captures | Ships as the `diligent-rs` companion binary, built in CI for macOS (Apple silicon and Intel) and Linux x86-64. Windows is not built. `diligent audit` requires it and never downloads it |
| Browser worker | Part of `diligent-rs`; exercised against recorded captures and a local test site | Requires Chrome or Chromium on macOS or Linux. Not available on Windows |
| Offline replay (`diligent audit --replay`) | Runs the real binary over recorded captures with no Chrome and no network | Audits only what was recorded. Discovery, collection and network checks are skipped, the sample is marked partial, and the result says it was a replay. It is not a live audit of the site |
| Storybook component mode (`diligent audit --storybook`) | Runs the real binary over the sampled stories of a built Storybook; exercised against a small fixture Storybook | Docs entries are skipped, `play` functions are not run, and each story is captured once at the standard viewport set, so states reached by interaction are untested. Whole-page obligations are not tested and the result is not a site claim. Storybook 6 `stories.json` is recognised by shape only and has not been tried against a real build |
| Storybook discovery checks that the host resolves to a public address before each request but does not pin the connection to that address | A DNS answer that changes between the check and the connection is not caught; the story pages themselves are fetched by the companion binary under its own network policy | Not available (no dependency-free way to pin the connection) |
| Document and PDF analysis | Not implemented | Not available |

## Hosts

| Host | Tested | Status |
| --- | --- | --- |
| Agent Skills hosts | Skill discovery and the CLI exercised informally during development; no formal evaluation on any host | `unmeasured` as a supported host. The manual fallback in `references/workflow.md` applies |
| Generic agent with file access | No evaluation run | `unmeasured`. Manual fallback available |

## Lab performance

| Limitation | Impact | Status |
| --- | --- | --- |
| Core Web Vitals in `SIGNALS.jsonl` are lab values: one unthrottled headless load on the machine that ran `collect`. LCP, CLS, FCP, TTFB, transfer size and long tasks are not field measurements | They can differ a lot from what visitors see on slow networks and devices, and they vary between runs. The signals are labelled `lab` and are advisory | Observed as a design limit |
| Infra checks skip http origins | `diligent audit` runs the network checks only on an https origin. On an http origin, such as a local smoke site, the infra stage is skipped with an event and a run warning, so the HTTPS redirect, certificate, HSTS and security header obligations are untested there | Not available |
| INP is not measured | INP needs real user interaction. Importing field data from CrUX is not implemented, so `perf.inp_ms` is always `not_tested` | Not available |
| Template scanning is textual | `source scan` reads JSX, Vue, Svelte, Astro, Twig and Liquid by masking template syntax, not by compiling it. Expressions, components and attribute spreads are opaque, so a result that rests on one is `needs_review`. Script blocks outside JSX are not parsed. Rendered output can differ from what the source shows. See `docs/source-pass.md` | By design |

## Review modes

| Mode | Status |
| --- | --- |
| Independent verification by a separate context | Required by the protocol; no automated enforcement beyond the record. The tool cannot prove two contexts are independent |
| Human review queue | Records tasks and owners. It does not notify anyone |

## How to use this register

Before claiming support for a host, model, or capability, add a row with the evidence. Do not describe a trace as support. Update the date. Keep the first line of `supported-checks.md` consistent with this file.
