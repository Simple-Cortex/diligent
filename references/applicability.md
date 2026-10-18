# Applicability

Resolve what applies before running any check. Missing facts stay `unknown` with an owner and a resolution path. A model's guess is not a waiver.

## 1. Intake fact groups

Capture these in `SCOPE.json`. Each fact has `fact_id`, a typed `value`, `state` (`known`, `unknown`, `disputed`), `evidence_refs`, `provided_by`, `observed_at`, and optional `valid_until`.

| Group | Required facts | Why it matters |
| --- | --- | --- |
| Government owner | Country, jurisdiction, branch, agency, sub-agency, program, contractor relationship | Selects federal, state or local, tribal, territorial, or DC profiles |
| Audience and channel | Public, employee, contractor, or mixed; website, intranet, mobile app, PDF, API, kiosk | Public-only requirements must not be applied to internal tools |
| Lifecycle | Launch and redesign dates, legacy components, material alterations, environments | Changes applicability, transition treatment, and evidence validity |
| System | Boundary, authorizing official, FIPS 199 category, control baseline, inherited controls, authorization status | Security requirements are tailored per system |
| Data | Personal data, Privacy Act records, controlled unclassified information, health, education, tax, payment, children's data, retrieval identifiers | Activates privacy, sector, and disclosure rules |
| Service | Information only, application, benefits, payment, authentication, proofing, legal attestation | Determines identity, fraud, form, and transaction controls |
| Reach | Annual users, high-impact service designation, population or special-district status | Drives newer federal service policies and state and local dates |
| Dependencies | Hosting, CDN, analytics, fonts, embeds, identity provider, CAPTCHA, AI services, vendors | Extends review beyond the first-party repository |
| Adopted standards | WCAG version and level, design system and maturity, agency design system, supported browsers | Prevents confusing a product target with a legal minimum |
| Governance | Contracts, acquisition clauses, waivers, content owners, records schedules | Makes organizational obligations auditable |
| Permission and evidence | Allowed origins, roles, routes, test accounts, safe fixtures, prohibited actions | Enables useful testing without live submissions |

Infer low-risk inventory facts from source and runtime. Do not infer government level from a city name, program law from a page topic, a waiver from an inaccessible widget, or security authorization from a government domain. Consequential legal predicates need the evidence or reviewer their rule pack specifies; a user assertion is recorded as `owner_assertion`.

## 2. Question priority order

Ask only what materially changes the current audit, in this order:

1. Wrong jurisdiction or authority.
2. Sensitive processing boundary.
3. User task or service definition.
4. Inaccessible required journeys.
5. Agency or contract exceptions.
6. Optional design preferences.

Every unknown needs a question, an owner role, the affected obligations, and the effect on the final assessment. Inspect evidence first and offer an evidence-derived draft with explicit unknowns.

## 3. Applicability states

| State | Meaning |
| --- | --- |
| `applicable` | The predicate is true over known facts and the obligation is in effect |
| `not_applicable` | The predicate is false, with a cited exclusion and predicate facts |
| `unknown` | At least one needed fact is unknown or disputed |
| `pending_effective_date` | In scope, but the obligation is not yet in effect |

Resolve scope eligibility first, then temporal status. An obligation that becomes effective later remains pending even if the site already meets its technical target. A disputed effective date yields `unknown`, never an automatic exclusion. Non-applicability is recorded in `APPLICABILITY.json`, not in the results.

## 4. Predicate language

Applicability is a declarative expression over typed facts, never executable source supplied by a model. Rule packs serialize the expression as a string using `&&`, `||`, `!`, `==`, comparison, and `in [..]`. Allowed operations are `all`, `any`, `not`, `eq`, `in`, `gte`, `lte`, `exists`, and approved date comparisons. Reject unknown operators and invalid types.

Use three-valued logic:

| Operator | Rule |
| --- | --- |
| `all` | Any false gives false. Otherwise any unknown gives unknown. Otherwise true |
| `any` | Any true gives true. Otherwise any unknown gives unknown. Otherwise false |
| `not` | Negation of unknown stays unknown |
| `exists` | Checking for missing intake data cannot manufacture substantive non-applicability |

The rule packs name the facts each predicate needs in `required_facts`. Fact ids are lower snake case, for example `section508_covered`, `ada_title2_covered`, `adopted_wcag_version`, `collects_personal_data`, `service_requires_authentication`, and `fisma_system_in_scope`.

## 4a. Three WCAG mappings

Every accessibility rule records which WCAG versions contain its criterion, in `wcag_mappings`. The three versions play different roles and are never merged.

| WCAG version | Role | Why |
| --- | --- | --- |
| 2.0 A/AA | `legal_floor` | Revised Section 508 (36 CFR Part 1194) still incorporates WCAG 2.0. No Access Board refresh docket has existed since the 2017 rule |
| 2.1 AA | `title_ii_target` | ADA Title II (28 CFR Part 35 Subpart H) and HHS Section 504 (45 CFR Part 84 Subpart I) bind WCAG 2.1 AA |
| 2.2 AA | `recommended` | OMB M-23-22 directs agencies to the most current WCAG, which is 2.2. Guidance, not a legal binding |

A criterion present in 2.0 appears in all three mappings, a 2.1-only criterion in 2.1 and 2.2, and a 2.2-only criterion in 2.2 only. A report always shows all three and never blends them into one score or one pass. A failure against the 2.2 mapping alone is a recommendation finding, not a legal failure. Criterion 4.1.1 Parsing was removed in WCAG 2.2, so its 2.2 mapping is advisory only.

## 5. Authority and conflicts

Applicable law, policy, or contract governs website obligations. Agency evidence resolves factual scope. Versioned rule records govern a specific run. A conflict between authorities is an unresolved issue for the responsible owner. Do not resolve legal hierarchy by model confidence or by choosing the strictest requirement. Retain both, name the practical conflict and the reviewer, and block only the affected conclusion.

Technical standards adopted by a contract stay tied to their adopted editions. Newer good practice is a separate, labeled target. One defect mapped to several authorities is not several independent defects.

At the start of a run load a frozen authority lock. If new policy appears, record an event, produce an impact diff, and start a new assessment revision. A hash proves content identity, not legal validity. An index page, vendor article, or search snippet cannot qualify an exact legal deadline as source-verified.

## 6. Exception records

Store exceptions in `EXCEPTIONS.json`, separate from results. Each has `exception_id`, the exact authority and clause, the affected obligation and instances, rationale, required findings, approving role or person, approval evidence, approval date, an expiration or review trigger, alternative access, and residual barriers.

Validate against the particular authority. A product risk acceptance does not establish a lawful exception. The report keeps the technical failure and records separately whether the exception changes a legal applicability determination or only a release decision. Where an exception legally removes an obligation, keep its exclusion evidence. An expired or unsupported exception cannot suppress a finding.

## 7. Time-sensitive rules

| Item | Treatment |
| --- | --- |
| ADA Title II compliance dates | 2027-04-26 for entities with a population of 50,000 or more, 2028-04-26 for smaller entities and special district governments, per the Department of Justice interim final rule 91 FR 20902 (effective 2026-04-20), which moved the dates a year. Existing ADA duties continue before those dates. HHS Section 504 (45 CFR Part 84 Subpart I) dates are 2027-05-11 for recipients with 15 or more employees and 2028-05-10 for fewer, per 91 FR 25496. Re-verify at source review |
| OMB M-26-18 | Milestones are offsets from the 2026-08-31 issuance. Dates are not computed in the registry. Read the memo for exceptions and certification alternatives |
| EO 14432 | Implementation guidance was directed within 90 days. Treat later details as unresolved until published |
| Pending GSA standards | Banner, HTML page title, and meta description are pending with no supplied timeline. Report readiness findings, never premature legal failures |
