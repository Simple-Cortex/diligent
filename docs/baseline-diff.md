# Baseline and diff mode

A team with 300 findings on day one wants to accept the backlog and block only new problems. Baseline and diff mode lets you record the fingerprints of findings from a run, then compare subsequent runs against that baseline.

## Baseline

Write a baseline from a run directory:

```bash
diligent baseline write --run <run-id> --out baseline.json [--dir <audit-dir>]
diligent baseline write --dir <run-dir> --out baseline.json
```

The baseline file records:

```json
{
  "schema_version": "1.0",
  "created_from_run": "run-2026-10-18-abcdef",
  "engine_version": "0.2.1",
  "fingerprints": [
    {
      "source": "result",
      "fingerprint": "3f6c0e...",
      "rule_id": "INFRA-02",
      "check_id": "INFRA-02-hsts",
      "instance_id": "https://agency.example.gov/",
      "first_seen_run": "run-2026-10-18-abcdef"
    },
    {
      "source": "finding",
      "fingerprint": "fp-aaa-111",
      "rule_id": "INFRA-02",
      "finding_id": "find-001",
      "first_seen_run": "run-2026-10-18-abcdef"
    }
  ]
}
```

Entries come from two sources: `result` (a check result with status `fail` or `needs_review`, fingerprinted from `check_id`, `instance_id`, the element instance and the rule id, the same value as `diligent/result/v1` in the SARIF output; see `docs/sarif.md`) and `finding` (a finding in `FINDINGS.json`, which is optional). Fingerprints are sorted alphabetically. The baseline is a snapshot of what was accepted at a point in time.

## Diff

Compare a run against a baseline:

```bash
diligent diff --run <run-id> --baseline baseline.json [--json] [--fail-on new|any|none]
diligent diff --dir <run-dir> --baseline baseline.json [--json] [--fail-on new|any|none]
```

`--run` reads `RESULTS.jsonl` and `FINDINGS.json` from the audit directory and `RUN.json` from `runs/<run-id>`; `--dir <audit-dir>/runs/<run-id>` does the same, and a directory that holds all of them also works.

The diff partitions the run's entries, from both sources, into four categories:

- **new**: in the current run, not in the baseline
- **known**: in the current run and in the baseline (by fingerprint)
- **resolved**: in the baseline, not in the current run, and the rule was executed in this run
- **unverified**: in the baseline, not in the current run, and the rule was NOT executed in this run

Human output lists counts and new entries, each as its source and rule id, then the finding id or `check_id @ instance_id`. `--json` emits the full partitioned objects.

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success (or `--fail-on` not matched) |
| 1 | `--fail-on` matched |
| 2 | Usage error |
| 3 | Missing or invalid run directory or baseline |

### `--fail-on` modes

- `new`: exit 1 when there are any new findings
- `any`: exit 1 when there are new or resolved findings
- `none`: never exit 1 (default)

Use `--fail-on new` in CI to block on new findings only. Use `--fail-on any` to also block when known findings are resolved (so you can update the baseline).

## SARIF with baseline

`diligent sarif --baseline <file>` marks results with `baselineState`:

- `baselineState: "unchanged"` for known results
- `baselineState: "new"` for new results
- Resolved findings are not emitted

This lets GitHub code scanning show which findings are new vs. already accepted.

## Fingerprint collisions

If two findings in one run share a fingerprint, both are kept and the collision is noted in `--json` output under `warnings[]`. The same applies to baseline fingerprint collisions.

## Schema

The baseline schema is `schemas/baseline.schema.json`. It validates the baseline file structure.
