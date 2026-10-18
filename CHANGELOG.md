# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries describe user-visible changes. See `docs/changelog-policy.md` for what
belongs here and what stays in the development log.

## [Unreleased]

## [0.2.0] - 2026-10-18

Engine preview: the first release with automated website checks. They run in a companion binary, `diligent-rs`, that you install separately. The Node package still runs no checks by itself. Every rule stays at the `research` lifecycle, so an automated result is evidence for a person to weigh, not a legal conclusion.

### Added

- `diligent audit --url <https://host/>` runs the whole chain against a live site: scope with the facts you supply, applicability, inventory, sampling, collection, assessment, network checks, gate and report. It requires the companion binary, never downloads it, limits network access to the origin you name, and exits with the gate's code.
- `diligent audit --replay <replay.json>` audits recorded captures offline with the real binary, with no Chrome and no network. The run records that it was a replay.
- The `diligent-rs` companion binary is built in CI for macOS (Apple silicon and Intel) and Linux x86-64, with a `SHA256SUMS` file. Windows is not built. Chrome or Chromium is needed for browser checks. `docs/install.md` explains downloading, verifying and placing it, and the Rust source is now in the public repository.
- `diligent-rs checks list [--json]` prints every browser check, signal, source check and network check the build knows, with the rules each informs.
- `diligent-rs assess` runs 28 deterministic checks over captured pages without a browser: 17 accessibility checks (contrast, focus, ARIA, labels in names, target size, reflow, text spacing, bypass blocks, table headers, element language) and 11 federal public web checks (official-website banner, identifier and its links, search, Digital Analytics Program script, privacy and accessibility links, title and description, contact, language access, 320 px viewport). The matching cells in `docs/supported-checks.md` move from `unsupported` to `automated` or `assisted`. Checks that inform pending standards never fail.
- `diligent-rs collect` captures pages with headless Chrome at four viewports and four media modes, including lab Core Web Vitals and computed styles. `diligent-rs infra` runs network checks on an origin (HTTPS, certificate, HSTS, SPF, DMARC, disclosure files, robots.txt, sitemap, security headers, DAP script, privacy link, `.gov` suffix), and `diligent-rs source scan` checks static HTML.
- `diligent-rs inventory` reads robots.txt and sitemaps, crawls same-origin links, clusters pages into templates, and writes `INVENTORY.json` and `SAMPLE.json`: a structured and a seeded random sample with the seed, coverage counts and remaining work. A sample never supports a whole-site conformance claim.
- A content-addressed evidence store (`diligent-rs evidence ingest | verify | resume-plan`) with an append-only index, reuse logs and tombstones. `collect` resumes interrupted runs and reuses earlier work only when its cache key and versions match and every object still re-hashes.
- Optional axe-core corroboration (`--corroborate axe`, off by default, vendored 4.14.0 under MPL-2.0). Agreement changes nothing. A disagreement moves the result to `needs_review`, and violations the checks missed become `needs_review` leads. Profiles gain an optional `corroboration.browser` list.
- Advisory signals in `SIGNALS.jsonl` (30 signals): plain readings of text, type, spacing, colour margin, motion, form patterns and lab Core Web Vitals. Signals cite a practice source and a confidence, are never normative, and never change a result or the gate. `diligent report` shows them in a new section 10, "Craft and clarity (not legal obligations)", and shows accessibility results under the three separate WCAG mappings.
- Browser worker protocol 1.3.0 (additive over 1.0), which adds the axe-core and performance captures.
- `npm run release:gate` runs every release check in order, including an end-to-end smoke against a local site and an offline replay of six recorded captures. `scripts/review-packet.mjs` writes a review packet per rule that any check informs and promotes reviewed rules to `draft`.

### Changed

- The runtime manifest reports the Rust engine as a companion (`status: "companion"`) instead of a scaffold.
- `docs/supported-checks.md` and `docs/known-limitations.md` describe the Node package and the companion binary separately, and add rows for platforms, Chrome, replay, sampling and lab performance.

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
