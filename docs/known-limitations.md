# Known limitations

A maintained register of what Diligent does not do and what has not been tested. "Observed" means a recorded informal trace exists. It is not "supported". Where there is no evidence the entry says `unmeasured`.

Last updated: 2026-10-17, for version 0.1.0.

## Product scope

| Limitation | Impact | Status |
| --- | --- | --- |
| **Zero automated website checks.** Every rule is `unsupported`, `human`, or `documentary` | The tool cannot find or clear an accessibility or compliance issue by itself. All findings come from evidence an agent or person supplies | By design for 0.1.0. See `supported-checks.md` |
| No certification. The tool never certifies, authorizes, or approves anything | A result is never a statement of legal compliance, an authorization to operate, or an approved privacy assessment | Permanent |
| Rules are cataloged, not source-reviewed | No rule has `source_review_status` of `reviewed`. Rule text, sections, and modality words are derived from research notes and must be checked against the sources | Research lifecycle for all 159 rules |
| Clause locators are mostly unatomized | Many rules cite an authority without an exact section. A mandatory conclusion needs the exact clause | Open for source review |
| Modality is derived from a class label | The `shall`, `must`, and `should` words are mapped from L, P, C, and R labels, not copied from each source clause | Open for source review |
| Applicability predicates are not evaluated by shipped code beyond the gate's recorded states | An agent evaluates predicates by following `references/applicability.md`. Unknown stays unknown | Evaluator is future work |
| Several primary sources are not fully source-verified | Directives on vulnerability disclosure, HTTPS and email, and asset visibility, and the federal HTTPS standard, could not be fully retrieved during research. Their records are `unverified` | Verify before enabling exact timing or header checks |
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
| Node.js CLI | Unit and fixture tests for the gate, status, init, report, and rule validation | Supported. Authoritative in 0.1.0 |
| Rust workspace | Scaffold with tests of the same contracts | Scaffold only. Not used for delegation in 0.1.0 |
| Browser worker | Not implemented | Not available |
| Document and PDF analysis | Not implemented | Not available |

## Hosts

| Host | Tested | Status |
| --- | --- | --- |
| Agent Skills hosts | Skill discovery and the CLI exercised informally during development; no formal evaluation on any host | `unmeasured` as a supported host. The manual fallback in `references/workflow.md` applies |
| Generic agent with file access | No evaluation run | `unmeasured`. Manual fallback available |

## Review modes

| Mode | Status |
| --- | --- |
| Independent verification by a separate context | Required by the protocol; no automated enforcement beyond the record. The tool cannot prove two contexts are independent |
| Human review queue | Records tasks and owners. It does not notify anyone |

## How to use this register

Before claiming support for a host, model, or capability, add a row with the evidence. Do not describe a trace as support. Update the date. Keep the first line of `supported-checks.md` consistent with this file.
