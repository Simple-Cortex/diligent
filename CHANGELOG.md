# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries describe user-visible changes. See `docs/changelog-policy.md` for what
belongs here and what stays in the development log.

## [Unreleased]

## [0.2.1] - 2026-10-20

Developer integration lands: SARIF output for code scanning, a reusable GitHub Action, baseline and diff for backlog management, and Storybook component mode. The native gate and rules validator run through the companion binary when it is present, source-pass template support covers JSX, Vue, Svelte, Astro, Twig and Liquid, and 45 engine-informed rules were reviewed against primary sources and promoted to `draft`.

### Added

- `diligent audit --storybook <url>` audits the stories of a built Storybook (7 or 8) as components: each sampled story is captured and assessed with the element-level checks, whole-page obligations are recorded as not tested, a component gate sets the exit code, and `COMPONENT-BASELINE.json` records each story. A component audit is not a site claim. `diligent-rs assess --component` and `checks list --json` (`scope`) support it. See `docs/storybook-mode.md`.
- `diligent sarif --run <id> | --dir <run-dir> [--out <file>] [--baseline <file>]` emits SARIF 2.1.0 from a run so results can be uploaded to GitHub code scanning. Every `fail` and `needs_review` check result becomes a SARIF result with a stable fingerprint, the rule's obligation text as its description, and the lifecycle note carried into the result. Hand-authored `FINDINGS.json` entries are emitted alongside when present. See `docs/sarif.md`.
- `diligent baseline write --run <id> --out <file>` records the fingerprints of a run, and `diligent diff --run <id> --baseline <file> [--fail-on new|any|none]` partitions a later run into new, known, resolved and unverified. A team can accept a backlog and block only new problems. With `--baseline`, `diligent sarif` marks each result `new` or `unchanged`. See `docs/baseline-diff.md`.
- `diligent-rs infra --preload-lookup` asks hstspreload.org whether the registrable domain is on the Chromium HSTS preload list and records the answer as inference evidence on the `INFRA-02-hsts-preload` row. Off by default; the Node orchestrator does not pass it in this release.
- A reusable GitHub Action (`uses: Simple-Cortex/diligent@<tag>`, `action.yml` at the repository root) audits one URL on a Linux x86-64 runner, downloads the companion binary from the release and verifies it against `SHA256SUMS`, uploads the SARIF file to code scanning and the audit directory as an artifact, and fails the job only on the condition you choose (`fail-on: gate | new | any | none`). `baseline-out` writes a baseline for a write-on-main, diff-on-pull-request pair. See `docs/github-action.md`.
- `diligent-rs source scan` reads `.jsx`, `.tsx`, `.vue`, `.svelte`, `.astro`, `.twig` and `.liquid` files as well as HTML, and every finding now carries a source line, a column and a byte offset when the file contains the element. Expressions, components and attribute spreads are opaque and turn a result into `needs_review`; a template with no `html` or `body` tag is treated as a fragment and skips document-level checks. SARIF results for these findings point at the file and line. See `docs/source-pass.md`.
- `plugins/vite-plugin-diligent-src` is a Vite plugin that adds `data-diligent-src="<file>:<line>"` to native elements in JSX, Vue, Svelte and Astro files during development, and `diligent-rs assess` records the attribute as a `source_ref` on the finding it belongs to. The plugin lives in the repository and is not part of the npm package.
- `npm run shadow:rules` compares `diligent rules validate` in Node against the companion binary over 25 mutation cases and fails on any difference. CI runs it on every push.
- `diligent-rs gate` ports the regulatory gate to the companion binary. `npm run shadow:gate` runs the Node gate and the binary over every gate fixture, including a real audit directory from a GitHub runner, with and without `--json`, and fails on any difference in exit code, output, `ASSESSMENT.json`, `EVENTS.jsonl` or `RUN.json`. The Node gate stays authoritative. See `docs/rust-shadow-notes.md`.

### Changed

- 45 engine-informed rules were reviewed against their primary sources (clause locator, modality word, baseline id) and promoted from `research` to `draft`; FED-12 and INFRA-12 stay at `research`. Four modality words changed (FED-05 `shall`, FED-08 `must`, FED-18 `must`, FED-29 `should`). `review-packet.mjs` gained `apply`, and `promote` now refuses a rule whose clause locator is still `unatomized`. See `docs/review/2026-10-20-source-review-packet.md`.
- Authority A43 (OMB M-15-13) now points at the archived memo; `https.cio.gov` no longer resolves.
- The gate's limitation sentence and the claim boundary no longer say Diligent runs no automated website checks; they say automated checks run only through the separately installed `diligent-rs` binary, in both the Node and native gates.
- `diligent gate` and `diligent rules validate` run the `diligent-rs` binary when one reporting protocol 1 is present and fall back to the Node scripts otherwise; `DILIGENT_ENGINE=node` forces Node, and `diligent --version --json` lists the delegated commands. Argument errors from `diligent-rs gate` now carry the `diligent:` prefix to match Node.
- Artifact and rules-data JSON parse errors no longer quote the JavaScript engine's message; they read `<file>: not valid JSON` so the Node and native outputs match.

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

### Fixed

- `diligent sarif`, `diligent baseline write` and `diligent diff` read `RESULTS.jsonl`, `REVIEW-QUEUE.json` and `FINDINGS.json` from the audit directory when given a run under `runs/<run-id>`, which is where an audit writes them. Before this fix they emitted nothing for a real audit.
- The npm package now ships the SARIF, baseline and diff scripts and their docs; a test keeps every script the `diligent` command can dispatch inside the package.

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
