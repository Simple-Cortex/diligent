# `diligent audit`

One command that runs the whole chain against a live site: scope, applicability, inventory, sampling, collection, assessment, the network checks, the gate and the report. The Node skill orchestrates. The `diligent-rs` companion binary executes the automated stages. Outside `diligent audit`, the binary runs `diligent gate` and `diligent rules validate` when present, with Node as the fallback and the reference the shadow suites compare against. Inside `diligent audit` the gate and the report stages run as Node child processes.

```bash
diligent audit --url https://example.gov/ my-audit --facts references/facts/us-federal-public-web.example.json
```

The binary is required and is never downloaded; see `docs/install.md`. Without it the command exits 3 and says so. `init`, `status`, `gate` and `report` work without it.

## Storybook component mode

`diligent audit --storybook <url>` audits the stories of a built Storybook (7 or 8) as components instead of the pages of a site. It reads `<url>/index.json`, samples stories (`--pages` caps them, grouped by component, seeded like a site sample), captures each story's `iframe.html` page, assesses it with `diligent-rs assess --component` (whole-page checks are `not_tested` with the reason `component mode: whole-page obligation`), skips `infra`, and exits with the component gate: 1 when an element-level mandatory check fails on a sampled story, else 0 when every sampled story was collected and assessed, else 2. It writes `COMPONENT-BASELINE.json` and prints `Component gate: ... Not a site claim.` The option excludes `--url` and `--replay`. See `docs/storybook-mode.md`.

## Offline replay

`diligent audit --replay <replay.json>` audits recorded captures with the real binary and needs no Chrome and no network. The file is `{ "origin": "https://example.gov/", "pages": [{ "url": "https://example.gov/", "captures": "fixtures/a11y/captures/<case_id>" }] }`. Every url must be on the origin and appear once. A capture directory may appear once, and the url must have the same path and query as the page recorded in that capture's `manifest.jsonl` (its first line's `final_url`); scheme and host may differ, so a capture recorded on a local server can be replayed under the real origin. Result and instance ids come from the recorded `final_url`, so a mismatch is refused with exit 2 before anything is written, naming both urls. A relative `captures` path is tried against the replay file's directory, then the package root, then the current directory.

- **Inventory** is not run. `INVENTORY.json` and `SAMPLE.json` are written from the list: every page is structured with reason `replay`, the random sample is empty, the sample is marked partial, and the discovery limit reads "replay: no discovery performed".
- **Collect** is skipped. Each capture directory is used as it is, and `diligent-rs assess` loads it (a directory without a `manifest.jsonl` fails the collect stage by name, exit 3). Payload files are copied into `captures/` by the merger exactly as in a live audit.
- **Infra** is skipped with the warning "replay: network stages skipped".
- **Applicability, plan, assess, merge, gate and report** run unchanged. Facts come from `--facts` as usual.
- **No network is granted.** `network_allowed` stays false and the scope event is `scope_replay_recorded`.
- **Recorded:** `RUN.json` carries `extensions.audit.mode` (`replay` or `live`) and, in replay, `replay_sha256`, the hash of the replay file.

`--replay` and `--url` cannot be combined, and `--pages`, `--seed` and `--processes` do not apply. A bad replay file is a usage error (exit 2) before anything is written.

`node scripts/e2e-replay.mjs` replays `fixtures/e2e/replay.json` (six recorded captures: federal-good banner, text contrast, bypass blocks, reflow, dense regulatory text, a perf page) with the example facts and checks every artifact, the gate exit, minimum counts and the exact failing check ids in `fixtures/e2e/replay-expected.json`. It needs the binary, not Chrome.

## Facts are supplied, never guessed

`--facts <file.json>` takes an object of `fact_id: value`. `--fact id=value` adds one fact and may repeat; a value is JSON when it parses, otherwise text. Each fact is written to `SCOPE.json` as `known`, `provided_by: "operator"`, with a time. A fact id that no selected rule reads is rejected with exit 2 before anything is created. Without facts, applicability is unknown and the gate is `incomplete`, and the scope stage says so.

`references/facts/us-federal-public-web.example.json` is an example for the default profile. It is an example, not a default: copy it and answer for your own service.

Running the command also records, as an event named `scope_permissions_recorded`, that you granted network access to the origin. It sets `permissions.allowed_origins` to that origin and `processing_boundary.network_allowed` to true. Model dispatch stays off.

## Stages

Each stage appends a start and finish event to `EVENTS.jsonl`, writes its artifacts atomically and records its state, exit code, duration and warning. `--json` prints them. A stage that had failing engine calls also carries a `detail` of at most 4 KB: for each failing call, the exit code, the error kind and message from the engine's JSON, and the first 20 lines of its stderr. That is where to look first when a page was not collected.

| # | Stage | What it does | Artifact |
| --- | --- | --- | --- |
| 1 | `init` | creates the audit with frozen locks | `SCOPE.json` and the rest of the starter set |
| 2 | `scope` | applies facts and the origin grant | `SCOPE.json` |
| 3 | `applicability` | `diligent-rs applicability` | `APPLICABILITY.json` |
| 4 | `inventory` | `diligent-rs inventory`; exit 2 is a partial inventory and continues with a warning | `INVENTORY.json`, `SAMPLE.json` |
| 5 | `plan` | applicable obligations, sampled pages as instances, manual and documentary work | `AUDIT-PLAN.json` |
| 6 | `collect` | `diligent-rs collect` per sampled page, structured pages first, up to `--pages` | `work/captures/<slug>/` |
| 7 | `assess` | `diligent-rs assess` per collected page | `work/assess/<slug>/` |
| 8 | `infra` | `diligent-rs infra` on the origin, https only | `work/infra/` |
| 9 | `merge` | one `RESULTS.jsonl`, `EVIDENCE.jsonl`, `SIGNALS.jsonl`, plus the review queue | those four files and `captures/` |
| 10 | `gate` | `scripts/gate.mjs` | `ASSESSMENT.json` |
| 11 | `report` | `scripts/report.mjs` | `REPORT.md` |

`<slug>` is the first 12 hex characters of the SHA-256 of the page URL. The `work/` directory holds per-page outputs and can be deleted after a run.

## Merging

Every payload lives under `captures/` with its content-addressed name. Infra payload refs of the form `evidence/<name>` are rewritten to `captures/<name>`; refs such as `dns:TXT:host` are not files and are kept. The merger fails the stage, and writes nothing, when:

- an evidence row carries a different `run_id` than the audit;
- a `payload_ref` is not exactly `captures/<name>` or, for infra, `evidence/<name>`, or the name is not a plain file name;
- a payload is missing or its bytes do not match the recorded `sha256`;
- the same `evidence_id` appears with different content (identical repeats are deduplicated, first wins);
- two sources produce the same `result_id` or `signal_id`;
- a result or signal cites an evidence id that no source recorded.

Each merged file is read back through its schema before the stage passes. A failed merge means the gate does not run and the exit code is 3.

## Review queue

Every `needs_review` result becomes one blocking, open item in `REVIEW-QUEUE.json`: kind `human`, task `result_review: <check> on <instance>`, owner role taken from the rule. `EXCEPTIONS.json` is never written by the orchestrator; exceptions stay a human act.

## Run record

`RUN.json` records `execution_status` (`running`, then `completed` or `failed`), the engine as `native-rust` executed by `mixed` with `binary_version`, `tool_versions["diligent-rs"]`, the budgets, the collected URLs as snapshot refs, and per-page and per-stage outcomes with warnings under `extensions.audit`.

## Exit codes

The gate's exit code when the gate ran: 0 pass for declared scope, 1 mandatory failure, 2 incomplete. 3 when a stage failed before the gate could run, the gate itself failed on invalid input, or the binary is missing. 2 also means a usage error before anything started, such as an unknown fact id, `--pages 0`, or an audit id that already exists.

## What is skipped, and why

- **Infra on http origins.** The network checks apply to https. On an http origin, such as a local smoke site, the stage is skipped with an event and a run warning, and those obligations stay untested.
- **Pages outside the origin.** A sampled URL on another origin is not collected and is named in the plan warning.
- **Blocked or failed pages.** A page the collector could not load is recorded as `blocked` or `failed` with the error kind, and its obligations stay untested.
- **Signals.** `--no-signals` skips the advisory craft and content signals and `SIGNALS.jsonl`.
- **Corroboration.** Off unless asked; when it resolves to off, every `assess` call gets `--no-corroborate` (which skips the corroboration step entirely and reports `corroboration_disabled: true` in the assess summary), so a capture that carries an `axe_result` is not corroborated. In replay mode this keeps a recorded page's results the same whether or not it was recorded with axe. The selected profile's `corroboration.browser` list enables axe-core when it contains `axe-core`; `--corroborate axe` forces it on and `--no-corroborate` forces it off (the two flags together are a usage error). The applied setting and why are recorded in `RUN.json` under `extensions.audit.corroboration` (`applied`, `source` as `profile`, `flag`, `flag_off` or `none`, and `profile_requested`) and in the `--json` output as `corroboration`. A corroborator can only agree or send a result to review.
- **Not covered at all.** Discovery, the service brief, proposals, verification, exceptions, parallel collection, resume across runs, and Windows binaries.

## Release gate and smoke

`npm run release:gate` runs validation, the Node and packed-artifact tests, skill validation, `cargo fmt`, `cargo clippy`, `cargo test`, a debug build, the offline replay and the live smoke, and stops at the first failure. `--skip-cargo-test` leaves out `cargo test`; `--skip-rust` leaves out every cargo step, the replay and the smoke. The replay always runs when the binary is built. The live smoke runs only when Chrome is found and is printed as SKIPPED otherwise; `--require-chrome` turns a missing Chrome into a failure, which CI should use.

`node scripts/e2e-smoke.mjs` serves `fixtures/inventory/small-site/site.json` plus a failing-contrast page on 127.0.0.1, runs `diligent audit` on it with the real binary and Chrome, and checks every artifact, the expected gate exit in `fixtures/e2e/expected.json` and the minimum counts. It fails loudly when the binary or Chrome is missing. `--allow-missing-chrome` turns that into SKIPPED and exists for unit-test jobs only.
