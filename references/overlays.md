# Overlays, research packs, and agency discovery

The baseline packs are not a substitute for jurisdiction, sector, program, or contract determinations. Each activated overlay needs its own source review, versioned controls, and an accountable reviewer before any overall coverage claim.

Diligent 0.1.0 ships only the baseline packs at the `research` lifecycle. No overlay pack ships. This file tells an agent how to recognize that an overlay is triggered, what evidence it needs, and how to keep the assessment honest until the overlay exists.

## 1. Conditional overlays

This list identifies activation points. It is not a set of researched findings of universal applicability. Extend the pack before auditing such a system.

| Trigger | Additional pack to resolve | Evidence needed |
| --- | --- | --- |
| State, local, or DC ownership | ADA Title II, local accessibility and IT standards, privacy, language, public-records, and retention laws | Entity, population, current local policies, program obligations |
| Federal financial assistance | Program-specific section 504 regulations, grant conditions, civil-rights obligations | Funding agency, award, dates, current applicable regulations |
| Health records or services | HIPAA and HITECH where entity and data relationships qualify; applicable section 504 and 1557 requirements | Covered entity or business associate analysis, data flow, current program rules |
| Education or student records | FERPA and relevant education and accessibility obligations | Institution and program status, records, consent and disclosure authority |
| Tax information | Internal Revenue Code section 6103, IRS Publication 1075 where applicable | Actual federal tax information access, agreements, safeguards |
| Criminal justice information | Applicable CJIS security policy and agreements | Agency role, actual data, system boundary, agreements |
| Controlled unclassified information, defense, or national security | CUI rules, contract clauses, applicable NIST 800-171 or DoD requirements, or the national security systems regime | Classification, contract, authoritative control baseline. Do not reuse civilian exclusions blindly |
| Payment cards or federal payments | PCI DSS by payment relationship; Treasury and agency payment rules | Merchant or provider boundary, card-data handling, payment contracts |
| Children or youth audiences | Applicable children's privacy and program rules and agency privacy guidance | Age group, collection purpose, legal authority, platform relationship. Do not assume COPPA applies identically to every federal agency |
| Public datasets or APIs | OPEN Government Data and Evidence Act, agency publication, and information-quality rules | Dataset ownership, privacy and security review, metadata, dissemination decisions |
| Comments or public forums | Records, moderation, civil-rights and constitutional, and agency participation policies | Forum purpose, approved rules, accessibility, retention, moderation evidence |
| AI in public service or audit processing | Current OMB and agency AI governance, acquisition, privacy, security, and program safeguards | Model or service approval, data handling, human oversight, affected decisions. Distinguish an internal audit assistant from a benefits decision system |
| Contract-specific acceptance criteria | Accessibility, design system, hosting, testing, and reporting clauses | Executed contract or task order and incorporated versions |

## 2. Research packs and activation gates

A pack becomes eligible for conformance use only after the research below is complete. This is controlled input work, not permission to silently skip obligations. P0 means required before making the corresponding assessment claim, not that every pack must exist before a partial audit is useful. P1 improves remediation confidence. P2 widens technical reach without changing what a pass means.

| Pack and priority | Research and evidence required | Owner role and completion condition |
| --- | --- | --- |
| Federal public web, P0 | Atomize the 21st Century IDEA Act, OMB M-23-22, and current GSA standards; distinguish agency, site, and page duties and must from should | Policy and content owner plus rule reviewer. Clause-to-rule crosswalk complete |
| Accessibility, P0 | Incorporated versions, all applicable WCAG conformance conditions, nonweb ICT, exceptions, procurement and support, agency program duties | Accessibility lead. Criteria, support, and manual-test matrix approved |
| Agency and program, P0 | Exact service law, eligibility, notices, deadlines, appeal and correction, evidence, alternate access | Program and legal owner. Approved program source pack and service brief |
| Privacy and collection, P0 if data is collected | Privacy Act applicability, section 208 and current OMB and agency rules, PIA and SORN, tracking, disclosures, field and event inventory | Privacy and PRA owners. Data map and approved collection matched to the deployment |
| Security and identity, P0 if in system scope | Categorization, FIPS requirements, selected controls and parameters, inheritance, assessment procedures, identity assurance, applicable directives | Security and identity owners. Supported assessment objectives and untested controls enumerated |
| Records and transparency, P0 where activated | Records schedules, FOIA publication and disclosure, correction and preservation procedures | Records, FOIA, and content owners. Retention and publication mappings approved |
| Contract and design system, P0 | Executed clauses, incorporated editions, acceptance terms, agency identity requirements | Contract and product owners. Terms linked to rule ids and design constraints |
| State, local, tribal, and territorial, P0 for those entities | Ownership and funding law, accessibility, local privacy, language, records, and security, program requirements | Jurisdiction specialist. Dedicated profile published before a whole-profile pass |
| Sector, P0 when activated | Health, education, tax, criminal justice, CUI, payment, children's services, or other sector facts | Relevant subject owner. Independent source and test review |
| AI processing, P0 when external models are used | Agency-approved use, model and provider and data boundary, retention, training use, procurement, AI governance | Security, privacy, and AI owners. Permitted processing manifest |
| Expanded UX, P1 | Audience evidence, top tasks, support and search data, user studies, language and assistive-technology matrix, outcome measures | Research and product lead. Hypotheses and validated barriers separated |
| Additional frameworks and formats, P2 | Parser and runtime adapters, generated content, documents, difficult widgets | Engineering and accessibility owners. Fixtures and support matrix updated |

### Research ticket template

Each unresolved item has an id, a question, the affected profile, rule, or task, the consequence if unresolved, a primary-source search path, the required agency evidence, an owner role, a status, a decision, citations, a review date, and any rule or fixture changes. Status values: `open`, `researching`, `awaiting_owner`, `resolved`, `out_of_scope_with_evidence`.

A ticket is complete when the question is answered with an applicable primary source or authorized agency evidence, the decision is reviewed, and affected rules and tests are updated. A search result or a plausible model explanation is not completion.

### Carried-forward work

- Verify the CISA and HTTPS source text that could not be retrieved before enabling exact timing or header checks.
- Recheck dates and current status of every time-sensitive authority at implementation.
- Obtain real agency documents for authorization and controls, PIA and SORN, PRA clearances, records schedules, contracts, and exceptions. None has been supplied.
- Do not assume the federal profile applies to a site merely because its owner works in Washington, DC. The U.S. federal scope is an explicit starting assumption that a separate jurisdiction pack can change.

## 3. Agency and program discovery sequence

Once candidate identity is grounded, search official sources for the applicable layers, in this order:

1. The government-wide baseline for the correct jurisdiction and branch.
2. Department-wide directives and manuals: accessibility, security, privacy, records, public communications, digital service.
3. Agency and bureau supplements, technical standards, design-system requirements, implementation guidance.
4. Program-specific statutes, regulations, and published guidance for the exact service: notices, eligibility, forms, submission rights, deadlines, appeals, language, alternative access.
5. Executed contracts, grants, interagency agreements, authorization and control records, and local approved policies supplied by the project.

Search by confirmed official entity and program names, policy identifiers, and service subject, using official policy indexes, legal publications, and agency domains. Keep a search log with the query, official domains and indexes checked, results, dates, and unresolved coverage. Archived or superseded documents are leads until currency is established.

Public search cannot reveal every internal directive or contract. Missing public results create an evidence request, not a finding that no further requirements exist. Use connected approved tools for internal evidence when available. Otherwise name the exact artifact and the owner role needed. Do not probe restricted systems or request broad access for convenience.

Relevant review roles include the agency's Section 508 manager, privacy officer, security owner, records officer, contracting officer, and program owner. A policy template published for agencies is a starting point, not evidence that a particular agency adopted its text.

### Candidate to active obligation

Store every candidate in `REGULATORY-DISCOVERY.json` with source identity, exact section, issuer, date, version and status, authority type, affected project facts, modality, scope, timing, exceptions, reviewer, and disposition. Candidate statuses: `candidate`, `source_verified`, `applicability_unresolved`, `applicable`, `excluded_with_evidence`, `superseded`, `conflict_needs_review`.

- Convert a source-verified applicable obligation into a reviewed agency or program rule, or reuse an exact existing match. Link the chain project fact, source clause, predicate, obligation, checks, findings.
- A general agency mention is not enough. Apply the clause's actual scope and dates. Separate design-system mandates from optional recommendations and from technical conformance obligations.
- A mandatory rule activates only with verified source status, supported scope facts, and completed source review. If an owner decision is open, the status is unknown and the assessment stays incomplete.
- A stricter proposed target is a labeled product recommendation, never disguised as binding law.
- The profile composes government-wide, department, agency, program, contract, and system packs. Deduplicate identical outcomes while preserving every authority reference. Do not assume a lower-level policy supersedes its parent or that a contract can waive a statute. For conflicts, retain both, name the responsible reviewer, and block only the affected conclusion.

### Translate requirements into UX constraints

For each applicable program or agency obligation, record its consequence for task flow and component usage. Examples to research, not universal requirements: fixed notice text before collecting a field; required review and correction before submission; deadlines and time-zone disclosure; receipt and retention behavior; a mandated form version; representative access; appeals and nondigital access routes. Link them to the service brief, component and pattern rules, and repair acceptance tests.

Ask focused questions only after available evidence has been inspected. The minimum unresolved questions usually concern the legal owner and program, the primary user and outcome, the data collected, public versus internal scope, and where agency and contract policies live. Refresh agency discovery when ownership, funding, contract, service purpose, data practices, system boundary, or governing policy changes, and keep prior profiles for historical replay.
