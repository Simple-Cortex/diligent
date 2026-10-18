# Federal web catalog

A routed catalog of federal identity, content, privacy, collection, records, security, identity, and hosting obligations. Rows come from the `federal-public-web`, `privacy-collection`, and `infrastructure` rule packs.

**No row here has an executable check in Diligent 0.1.1.** Every rule is at the `research` lifecycle. Registry authority ids (A01 and so on) resolve in `../authorities/registry.json`. Rows marked with a primary authority that is not source-verified say so in the pack's `source_caveat`.

Labels and channels are defined in `accessibility.md` section 1. Required placement varies between principal agency sites, secondary sites, and major entry points. Link to authoritative agency pages instead of duplicating policy content. Use the Digital.gov required-content index as an index only; the controlling law or policy decides a disputed applicability.

## 1. Official identity and required content

| Rule | Source row | Outcome | Label and source | Channels | Proposed audit procedure and repair |
| --- | --- | --- | --- | --- | --- |
| FED-01 | WEB-01 | Appropriate government domain and domain governance. | P/C, A28; A29; A28, A29 | N O | N/O: inspect canonical host, domain inventory, ownership and approved exceptions. Include redirects and vendor-hosted services. |
| FED-02 | WEB-02 | Clear official government identity. | P/R/C by provision, A05; implementation A30; A05, A30 | B H | B/H: confirm agency identity and official-site cues. A federal banner must describe the actual domain/security state and remain operable on mobile. |
| FED-03 | WEB-03 | Clear parent agency attribution. | P/R/C by provision, A05; implementation A31; A05, A31 | B | B: verify attribution and working parent link. Use a compact identifier that does not compete with the primary task. |
| FED-04 | WEB-04 | About information and applicable organizational disclosures. | P/C, A21; A21 | B O | B/O: inspect mission/ownership and appropriate agency About links. Agency-level disclosure destinations may be shared. |
| FED-05 | WEB-05 | Accessible accessibility statement and feedback path. | P, A06; A32; A06, A32 | B O | B/O: verify applied standard, known limitations, program-manager name/email, feedback, complaint instructions, reasonable accommodations information, relay guidance, relevant policies, review date. |
| FED-06 | WEB-06 | Privacy policy and collection-context notices. | P/C, A09; A09 | B N O | B/N/O: compare notices to actual data flows, collection points, analytics and vendors. Generic footer copy does not establish accurate disclosure. |
| FED-07 | WEB-07 | FOIA access and electronic reading-room duties. | L/C, A22; A22 | B O | B/O: verify request instructions and applicable proactive disclosures/indexes with the agency FOIA owner. |
| FED-08 | WEB-08 | No FEAR Act data link/content. | L/C, A21; A21 | B O | B/O: check principal agency homepage placement, authoritative destination and reporting currency. Do not require a full report on every microsite. |
| FED-09 | WEB-09 | Budget/performance information. | P/C, A21; A21 | B O | B/O: verify required plans/report destinations and appropriate principal-site access. |
| FED-10 | WEB-10 | Inspector General reporting access. | L/C, A21; A21 | B O | B/O: check applicable homepage link, audits/reports and reporting mechanism; preserve anonymity where required. |
| FED-11 | WEB-11 | Public vulnerability reporting policy. | P/C, A12; A12 | N B O | N/B/O: find VDP, reporting instructions and scope; connect to actual response ownership. `security.txt` is useful but not a universal substitute or universal mandate. |
| FED-12 | WEB-12 | Contact/help and policy access. | P/R/C by provision, A05; R layout A31; A05, A31 | B H | B/H: test contact routes and required policy access in representative layouts. Do not rely on nonfunctional mail links or inaccessible widgets. |
| FED-13 | WEB-13 | Appropriate government-wide support link. | P/C placement index, A21; A21 | B | B: verify USA.gov access at applicable principal sites, sub-agency sites and major entry points; confirm current agency implementation guidance alongside the new America.gov policy. |

## 2. Content, discoverability, and design

These rows define audit objectives. Their tests go beyond detecting a class name or script name.

| Rule | Source row | Outcome | Label and source | Channels | Proposed audit procedure and repair |
| --- | --- | --- | --- | --- | --- |
| FED-14 | EXP-01 | Consistent appearance across a service. | L qualified, A04 §3; A04 | B M H | B/M/H: compare navigation, hierarchy, terminology, and control behavior across templates. Fix shared tokens/components while retaining necessary task differences. |
| FED-15 | EXP-02 | Appropriate federal design-system/website-standard adoption. | L/P/C, A04 §3(e); A05; A33; A04, A05, A33 | S O | S/O: record applicable GSA/agency standards and approved approach. Absence of the USWDS npm package alone is not proof of noncompliance. |
| FED-16 | EXP-03 | User-centered design informed by evidence. | L qualified, A04; A04 | O H | O/H: require research and task evidence; inspect whether priority journeys work for actual audiences. A model persona is not participant research. |
| FED-17 | EXP-04 | Public content can be searched. | L qualified, A04; A34; A04, A34 | B | B: run known-item and top-task queries; verify scope and result destinations. Assess narrow-site exceptions/alternative implementations using controlling guidance. |
| FED-18 | EXP-05 | Public information is discoverable externally. | P/R by provision, A05; A35; A05, A35 | N S | N/S: inspect robots rules, response codes, canonical URLs, sitemap and indexability. Distinguish intentional restricted data from accidental public-content blocking. |
| FED-19 | EXP-06 | Search results are usable. | R, A34/A35; A34, A35 | B H | B/H: check empty/error states, spelling variants, meaningful snippets, accessible controls and result freshness; never index private records. |
| FED-20 | EXP-07 | Authoritative, current, nonduplicative content. | P/R/C by provision, A05; L qualified consolidation A04; A05, A04 | O M | O/M: compare approved source facts, owners and review history; flag contradictions and obsolete copies. Do not label every undated page legally stale. |
| FED-21 | EXP-08 | Plain writing in covered public material. | L/C, A07; A07 | M H | M/H: assess audience, jargon, instructions, headings, and task comprehension. Preserve legally necessary text with clear explanations; no universal grade-level mandate. |
| FED-22 | EXP-09 | Accurate policies, claims, eligibility and deadlines. | C program authority; R evidence discipline | M O | M/O: compare against supplied authoritative program rules. Never invent fees, appeal rights, approval times, or legal effects while rewriting. |
| FED-23 | EXP-10 | Functional and usable on common mobile devices. | L qualified, A04; A04 | B H | B/H: complete tasks on relevant widths, touch devices, zoom and virtual-keyboard states; inspect fixed elements and file uploads. |
| FED-24 | EXP-11 | Digital forms/services and signature alternatives. | L/P qualified, A04 §§4-5; A05; A04, A05 | B O | B/O: identify print/mail/wet-signature handoffs and documented constraints; propose equivalent digital paths where required. |
| FED-25 | EXP-12 | Access for people unable to use digital services. | L, A04 §4(e); A04 | B O | B/O: inspect alternative completion information and actual operations. Digitization must not silently remove required access. |
| FED-26 | EXP-13 | Shared services and appropriate customization. | L/P qualified, A04/A05; A04, A05 | S O H | S/O/H: review unnecessary duplicate capabilities and preference/data reuse. Do not require accounts or tracking merely to personalize a public page. |
| FED-27 | EXP-14 | Digital Analytics Program use where applicable. | P/C, A36; A36 | B N O | B/N/O: inspect production configuration and actual allowed requests; script presence alone is not proof. Record exclusions and protected routes. |
| FED-28 | EXP-15 | Experience measurement and service improvement. | P/C/O, A37; A37 | O | O: inspect applicable CX measurement, feedback and service plans. Test no personal data leaks through event names, URLs or survey payloads. |
| FED-29 | EXP-16 | Language access under current applicable authority. | C, A19; DC example A38; A19, A38 | O B H | O/B/H: resolve program/jurisdiction obligations; evaluate vital content, human review, language declaration and completion paths. Machine translation alone is not proof of fulfillment. |
| FED-30 | EXP-17 | Complete task states and recovery. | R; specific accessibility duties remain L/P | B H | B/H: test empty/loading/error/partial/save/resume/offline/interrupted/expired states. Preserve entered information when safe and explain recovery. |
| FED-31 | EXP-18 | Honest visual priority and task flow. | R | M H | M/H: evaluate user decision, ranked questions, visual hierarchy, action scope and density. Repair compliance issues without burying the main task. |

## 3. Published GSA standards: preserve their actual status

The catalog retrieved on the research date says there are no standards in its "required" group. The three below are pending; their pages say a compliance timeline will be provided. Contact page, content timeliness and site search are draft; external links, language selector and page-level feedback are research. This status does not cancel obligations already imposed by IDEA, Section 508 or another source.

The research date is 2026-10-15 and the hub was re-checked on 2026-10-17. Do not invent a deadline for any pending standard.

### Federal website standards status

**None of these standards is required yet.** As of 2026-10-17 the hub at standards.digital.gov lists no required standard. Pending standards were published 2024-09-26 and give no compliance dates. Rules carry `standards_status`; flip it to `required` and add a date only when the hub does.

| Standard | Hub status | Rule | Acceptance criteria |
| --- | --- | --- | --- |
| Government banner | pending | FED-32 | On every page, on its own line, with the flag, official-site text, and an expandable explanation of .gov and HTTPS |
| HTML page title | pending | FED-33 | Unique on every page, at least 20 characters |
| Meta page description | pending | FED-34 | Unique on every page, at least 50 characters |
| Contact page | draft | none | Not encoded |
| Content timeliness indicator | draft | none | Not encoded |
| Site search | draft | none | Not encoded; site search on public sites is separately a must item in OMB M-23-22 |
| External link | research | none | Not encoded |
| Language selector | research | none | Not encoded |
| Page-level feedback | research | none | Not encoded |

The pending standards apply to executive-branch public sites on .gov or .mil.

| Rule | Source row | Pending standard | Deterministic acceptance checks to prepare | Gate treatment |
| --- | --- | --- | --- | --- |
| FED-32 | GSA-01 | Federal government banner | Top of every page, on a line by itself, with flag, official-site text and dropdown information, for .gov and .mil on HTTPS. Scope is public executive-branch .gov and .mil services. | Readiness finding unless independently required by adopted agency or contract authority. `pending_effective_date`. |
| FED-33 | GSA-02 | HTML page title | Every page has a unique title of at least 20 characters under this pending standard. Descriptiveness needs semantic review. | Readiness finding unless independently required by adopted agency or contract authority. `pending_effective_date`. |
| FED-34 | GSA-03 | Meta page description | Every page has a unique description of at least 50 characters under this pending standard. Accuracy is assessed separately. | Readiness finding unless independently required by adopted agency or contract authority. `pending_effective_date`. |

Keep the pending-specific length and uniqueness checks separate from the already-binding WCAG 2.4.2.

## 4. Privacy, information collection, records, and open information

| Rule | Source row | Outcome | Label and source | Channels | Proposed audit procedure and repair |
| --- | --- | --- | --- | --- | --- |
| PRIV-01 | PRIV-01 | Identify lawful collection and data flows. | L/C, A08; P/C A09; A08, A09 | S N O | S/N/O: inventory fields, uploads, URL parameters, cookies, logs, analytics, embeds and processors. Map purpose and destination; remove unnecessary collection. |
| PRIV-02 | PRIV-02 | Privacy Act statements when applicable. | L/C, A08; A09; A08, A09 | B O | B/O: confirm authority, purpose, routine uses, mandatory/voluntary nature and consequences with privacy counsel. Display at the required collection context. |
| PRIV-03 | PRIV-03 | System of Records Notice where triggered. | L/C/O, A08; A08 | O | O: determine actual retrieval by name/identifier and applicable SORN. A database with PII is not automatically the complete legal test. |
| PRIV-04 | PRIV-04 | Privacy impact assessment where triggered. | L/C/O, A09; A09 | O | O: inspect applicable PIA, publication/redaction treatment, updates and actual system boundary. Repository search cannot prove absence. |
| PRIV-05 | PRIV-05 | Tracking classification, notice and controls. | P/C, A40; A40 | N B O | N/B/O: classify session/persistent, PII and customization use against policy tiers, approvals and user choices. Do not impose a universal EU-style cookie banner on every federal site. |
| PRIV-06 | PRIV-06 | Third-party practices match disclosed/approved handling. | C, A09/A40; adopted controls A11; A09, A40, A11 | N O | N/O: inspect external scripts, fonts, maps, video, session replay, CAPTCHA and AI endpoints. Consider click-to-load or first-party assets with equivalent UX. |
| PRIV-07 | PRIV-07 | Prevent unnecessary disclosure of sensitive data. | C controls/program law; R engineering; A11 | S N B | S/N/B: inspect referrers, logs, telemetry, caches, browser storage, public bundles and error bodies. Redact without removing needed support correlation. |
| PRIV-08 | PRIV-08 | Rights, sharing and retention match authority. | L/C/O, A08; A23; A08, A23 | O | O: verify allowed disclosure, access/amendment processes, retention schedules and legal holds. Do not promise deletion when records law requires retention. |
| PRIV-09 | FORM-01 | PRA applicability determination. | L/C/O, A10; A10 | O | O: assess standardized information collection from ten or more persons over the relevant period, with exemptions and special cases. A field count is not the test. |
| PRIV-10 | FORM-02 | Valid clearance and required display. | L/C/O, A10; A10 | B O | B/O: check actual collection against active approval, OMB control number, expiration/display rules, burden and required notices. Confirm any exception to display. |
| PRIV-11 | FORM-03 | Collection changes reviewed before release. | L/C/O, A10; A10 | S O | S/O: diff fields, instructions, purpose and burden against approved collection. Route material changes to the PRA owner. |
| PRIV-12 | FORM-04 | E-signature/attestation treatment matches program authority. | L/C/O, A04; program rules; A04 | B O | B/O: distinguish typed-name signature, identity proofing, consent and legal attestation. Preserve required exact wording and signature records. |
| PRIV-13 | REC-01 | Identify and schedule web records. | L/C/O, A23; A23 | O | O: include content, approvals, transactional records and relevant administration/context. Git alone is not a complete retention system. |
| PRIV-14 | REC-02 | Preserve before removal or redesign. | L/C/O, A23; A23 | O | O: verify capture/versioning, disposition authority, schedules and holds. Public unpublishing is distinct from authorized records destruction. |
| PRIV-15 | REC-03 | FOIA proactive electronic disclosure. | L/C/O, A22; A22 | O B | O/B: review applicable decisions, policies, manuals and frequently requested records with FOIA staff; apply lawful redactions. |
| PRIV-16 | DATA-01 | Open-data/data-catalog obligations when applicable. | L/P/C/O, A41; A41 | O N | O/N: resolve dataset publication authority and exemptions, metadata, formats and catalog requirements. Do not publish protected data to satisfy a generic openness check. |
| PRIV-17 | DATA-02 | Information-quality correction process. | C/O, A21; A21 | B O | B/O: identify applicable agency guidelines and correction mechanism. Link to the controlling agency process rather than inventing one. |

## 5. Security, identity, hosting, and operations

Security results must distinguish a demonstrated application flaw, a missing selected control, a recommended hardening measure, and absent organizational evidence. A web security verification catalog is a useful technical reference, not itself a universal federal statute.

| Rule | Source row | Outcome | Label and source | Channels | Proposed audit procedure and repair |
| --- | --- | --- | --- | --- | --- |
| INFRA-01 | SEC-01 | HTTPS across public websites/services. | P, A43; A13; A43, A13 | N B | N/B: test HTTP redirect, valid certificate/hostname/chain, TLS configuration, mixed content and embedded assets. Capture actual deployment evidence. |
| INFRA-02 | SEC-02 | HSTS in applicable scope. | P/C, A13; A13 | N O | N/O: inspect headers and subordinate-domain readiness; validate directive specifics. Do not add includeSubDomains/preload without checking affected hosts. |
| INFRA-03 | SEC-03 | Protect mail-enabled domains. | P/C, A13; A13 | N O | N/O: assess relevant STARTTLS, SPF, DKIM, DMARC and reporting requirements against the domain's mail role. Do not demand mail records for unrelated hostnames. |
| INFRA-04 | SEC-04 | System categorization, selected controls, authorization. | L/P/C/O, A11; A11 | O | O: inspect boundary, FIPS category, RMF artifacts, ATO/conditions, inherited controls and assessment currency. Cloud status alone does not authorize the application. |
| INFRA-05 | SEC-05 | Appropriate cloud-service assurance. | L/P/C/O, A15; A15 | O | O: evaluate actual use case, offering, service boundary and required FedRAMP status; identify agency responsibilities and applicable exclusions. |
| INFRA-06 | SEC-06 | Asset/software/hardware inventory. | P/C/O, A14; A18; A14, A18 | S O | S/O: compare domains, hosts, deployed versions, packages and vendors with inventory. Build-time dependencies alone do not describe production. |
| INFRA-07 | SEC-07 | Known exploited vulnerabilities and vulnerability cadence. | P/C/O, A14; A14 | S N O | S/N/O: join exact products/versions to current KEV entries and required due dates; confirm exposure, reachability and mitigation. CVSS alone is not exploitability. |
| INFRA-08 | SEC-08 | Vulnerability disclosure intake and response. | P/C/O, A12; A12 | B O | B/O: verify reporting route and tracked response/remediation procedures. Never submit fake vulnerability reports merely to test contact forms. |
| INFRA-09 | SEC-09 | Server-side authorization. | C selected controls A11; R test detail A42; A11, A42 | S B | S/B: use authorized fixtures for object, role and tenant boundaries. Hiding a button is insufficient; verify denial at the API. |
| INFRA-10 | SEC-10 | Session and credential protection. | C, A44; A11; A44, A11 | S B | S/B: test cookie scope/flags, session renewal/invalidation, CSRF controls, logout and recovery. Preserve accessible timeout warnings within required limits. |
| INFRA-11 | SEC-11 | Injection, unsafe rendering and request handling. | C selected controls; R test detail A42; A11, A42 | S B | S/B: inspect SQL/command/template/HTML sinks, SSRF, deserialization and URL handling. Prefer typed validation, parameterization and contextual encoding. |
| INFRA-12 | SEC-12 | Secure configuration and browser protections. | C selected controls; R detail A42; A11, A42 | N S | N/S: evaluate CSP, frame protections, content-type handling, CORS, referrer behavior, debug modes and error output. Missing CSP is not universally a statutory failure. |
| INFRA-13 | SEC-13 | Safe upload processing. | C selected controls; R detail A42; A11, A42 | S B | S/B: verify size/type checks, signature handling, malware isolation, naming/storage, private access and processing limits. Test safe files only. |
| INFRA-14 | SEC-14 | Secret and key management. | C selected controls A11; A11 | S O | S/O: inspect secrets handling, bundle exposure, least-privilege credentials and rotation process; do not place actual secret values in reports or model prompts. |
| INFRA-15 | SEC-15 | Cryptography meets applicable standards. | C, A11/A44; A11, A44 | O S | O/S: verify approved algorithms/modules/configuration for the selected system and use. HTTPS or a library's marketing does not establish FIPS module validation. |
| INFRA-16 | SEC-16 | Supply-chain assurance based on current policy. | P/C/O, A18; R SSDF A45; A18, A45 | S O | S/O: review pinned dependencies, provenance, build/release controls and vendor evidence against agency risk policy. Require SBOM/attestation only when authority/contract calls for it. |
| INFRA-17 | SEC-17 | Monitoring, incident response and audit logs. | C/O selected controls A11; A11 | O | O: verify ownership, alerts, retention and response procedures. Balance investigation needs with privacy and records requirements. |
| INFRA-18 | SEC-18 | Backups, recovery and continuity. | C/O selected controls A11; A11 | O | O: inspect restoration evidence and agreed recovery targets. Do not invent a universal federal availability percentage or recovery time. |
| INFRA-19 | SEC-19 | Workforce zero-trust/enterprise identity controls. | C/O agency policy; A16 references; A16 | O S | O/S: assess applicable enterprise requirements separately from public-user identity. Do not require PIV/CAC for every citizen task. |
| INFRA-20 | ID-01 | Identity proofing/authentication levels fit service risk. | P/C/O, A16/A44; A16, A44 | O H | O/H: document IAL/AAL/FAL and risk assessment; separate authentication from identity proofing. Evaluate user burden and recovery. |
| INFRA-21 | ID-02 | Login.gov for in-scope services, with dates/exceptions. | P/C, A16; A16 | S B O | S/B/O: inspect sign-on options, applicable cohort/deadline and certified exceptions. Default presentation and alternative providers follow the memo's scope. |
| INFRA-22 | ID-03 | Password behavior meets adopted NIST profile. | C, A44; A44 | B S | B/S: for the final Rev.4 profile, minimum 15 characters for single-factor passwords; an MFA password may use minimum eight. The profile recommends permitting at least 64 characters; verify paste/manager support, blocklists, and the prohibitions on composition and routine rotation rules. |
| INFRA-23 | ID-04 | MFA/phishing resistance and recovery meet assurance requirements. | C, A44; A44 | B O | B/O: inspect required factors, phishing-resistant options and account recovery; do not certify assurance from a screenshot of an MFA prompt. |
| INFRA-24 | ID-05 | Federation protects assertions and redirects. | C, A44 family; R detail A42; A44, A42 | S B | S/B: inspect issuer/audience validation, redirect allowlists, state/nonce, replay and session handling using approved test identity accounts. |
| INFRA-25 | ID-06 | Identity UX is monitored for exclusion and friction. | P/C/O, A16; A16 | O H | O/H: examine completion, abandonment and time alongside fraud/privacy outcomes; review failure paths and support access. |
| INFRA-26 | GOV-01 | America.gov covered-service planning and execution. | P/C/O, A17; A17 | O | O: record annual-user predicate, exclusions and implementation guidance. Expose only authorized data/functions; track upcoming requirements separately from overdue ones. |

## 6. Time-sensitive security and identity items

- The CISA directives and the federal HTTPS standard were not fully retrievable during research. Their registry records have status `unverified`. Verify directive text and status before enabling exact timing or header checks.
- For OMB M-26-18, track the appendix milestones from issuance: 60-day inventory, 240-day digital identity risk assessment, one-year existing-site implementation practices and high-impact service deployment, and two-year deployment for remaining existing in-scope websites. The scope excludes services accessed solely by organizations or representatives. Mixed services still need the individual-user assessment.
- EO 14432 covers services with more than 100,000 users in a 12-month period and online access or application, with stated exclusions. Treat future implementation details as unresolved until published. Do not require every website to expose citizen records or transactional APIs.
- For the final NIST digital identity Rev. 4 profile, a single-factor password has a minimum of 15 characters and a password used with multi-factor authentication may have a minimum of eight. The profile recommends permitting at least 64 characters and prohibits composition rules and routine rotation. Verify against the source before enabling the check.
