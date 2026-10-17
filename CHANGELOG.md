# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries describe user-visible changes. See `docs/changelog-policy.md` for what
belongs here and what stays in the development log.

## [Unreleased]

## [0.1.0] - 2026-10-17

First release. A contracts-and-gate release: it ships no automated website
checks. See `docs/supported-checks.md` for exactly what is and is not covered.

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
