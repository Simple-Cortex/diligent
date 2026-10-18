# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries describe user-visible changes. See `docs/changelog-policy.md` for what
belongs here and what stays in the development log.

## [Unreleased]

### Added

These ship as 0.2 with the separately installed native engine and are not in the 0.1.1 package.

- `diligent-rs applicability` evaluates every selected rule's predicate against SCOPE.json facts with three-valued logic and writes a schema-valid APPLICABILITY.json with a per-rule trace. Predicates are parsed from the shipped string grammar or a structured object form; unknown operators are rejected.
- `diligent-rs collect` drives headless Chrome over the DevTools Protocol (new `diligent-runtime` browser worker with a versioned JSON Lines protocol) and writes content-addressed DOM, accessibility, focus-order, screenshot and network captures with a `manifest.jsonl` at four viewports and four media modes.
- `diligent-rs infra --url <https://host/> --out <dir>` runs deterministic network checks on an origin (HTTPS redirect and certificate, HSTS, SPF, DMARC, vulnerability-disclosure files, robots.txt, sitemap, security headers, DAP script, privacy link, `.gov` suffix) and writes `RESULTS.jsonl`, `EVIDENCE.jsonl`, and content-addressed response bodies.

## [0.1.1] - 2026-10-18

A data-only release: corrected and extended authority records, rule mappings, and documents. The installable package still ships no automated website checks.

### Added

- Authority records for the HHS Section 504 web and mobile accessibility rule, the DOJ interim final rule that extended the ADA Title II dates, OMB memoranda M-25-21 and M-25-22 on AI, CISA BOD 26-04, ACT Rules Format 1.1, VPAT 2.5Rev, the OpenACR schema, and the ICT Testing Baseline for Electronic Documents.
- Accessibility rules now carry `wcag_mappings`, which keep WCAG 2.0 (Section 508 floor), 2.1 (ADA Title II and HHS Section 504 target), and 2.2 (recommended) separate.
- WCAG 2.0 rules carry `baseline_test_id`, the ICT Testing Baseline for Web test that covers them. The supported-checks matrix lists the rules with no baseline test.
- The three pending federal website standards (banner, page title, meta description) carry `standards_status` and their acceptance criteria.
- Applicability guide section on the three WCAG mappings; federal web guide table of federal website standards status.

### Changed

- ADA Title II record cites the interim final rule for its 2027-04-26 and 2028-04-26 dates.
- BOD 20-01 and BOD 18-01 records are now `active`; BOD 22-01 is recorded as revoked and the known-exploited-vulnerabilities rule carries a caveat.
- ICT Testing Baseline record states version 3.1 (2024-04-01) and 24 web tests.
- Known-limitations register notes that state-law WCAG versions are deliberately not encoded.
- INFRA-07 now cites CISA BOD 26-04 (A64), successor to the revoked BOD 22-01 known-exploited-vulnerabilities requirement, instead of A14; the clause locator is still pending source review.

## [0.1.0] - 2026-10-17

First release: the router, the artifact chain and schemas, the authority
registry, seven research rule packs, and a deterministic gate. It ships no
automated website checks yet, because every rule is still at the research
lifecycle. See `docs/supported-checks.md` for exactly what is and is not
covered.

### Added

- Router skill and ten phase skills: `init`, `discover`, `scope`, `inventory`,
  `plan`, `collect`, `assess`, `propose`, `verify`, and `report`.
- Artifact chain under `.diligent/<audit-id>/`, with a JSON Schema for each
  artifact.
- Authority registry of laws, policies, standards, and guidance. Each record
  states its status and review state; sources that could not be fully
  retrieved are marked unverified.
- Seven rule packs, all at the research lifecycle: WCAG 2.0, 2.1, and 2.2
  accessibility, federal public web, privacy and collection, infrastructure,
  and service UX. Two profiles: a draft federal profile and a research profile
  for state and local entities under ADA Title II.
- Four bounded, review-only model task prompts.
- Deterministic gate. `diligent gate` computes `fail`, `incomplete`,
  `pass_for_declared_scope`, or `not_applicable` from recorded results, and
  exits 0, 1, 2, or 3. A skipped test is never a pass.
- `diligent init`, `status`, `report`, and `rules validate` commands, and a
  report renderer.
- Install script for copying the skill into a host's skills directory.
- Reference guides for workflow, applicability, accessibility, federal web
  requirements, overlays, service UX, evidence handling, remediation,
  assessment, and discovery.
- Supported-checks matrix and known-limitations register.
