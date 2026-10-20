# SARIF 2.1.0 output

`diligent sarif` converts a run directory into a [SARIF 2.1.0](https://sarifweb.azurewebsites.net/) file that GitHub code scanning and other SARIF-aware tools accept. Every result traces back to a check result or finding and its evidence.

## Usage

```bash
# From an audit directory and run id
diligent sarif --run <run-id> [--dir <audit-dir>] [--out report.sarif]

# Directly from a run directory path
diligent sarif --dir <path-to-run-dir> [--out report.sarif]
```

Without `--out`, the SARIF document is written to stdout.

## Level mapping

| Finding lifecycle | Finding severity | SARIF level |
| --- | --- | --- |
| research | any | note |
| draft or published | critical | error |
| draft or published | high | error |
| draft or published | medium | warning |
| draft or published | low | note |

Research-lifecycle results always carry a `diligent_note` property:

> This rule is at the research lifecycle: the finding is a lead for human review, not a compliance determination.

## Tool descriptor

- `tool.driver.name`: "Diligent"
- `tool.driver.semanticVersion`: from `package.json`
- `tool.driver.informationUri`: `https://diligent.style`
- `tool.driver.rules[]`: see Rules below

## Rules

One entry per distinct rule id among the emitted results and findings, sorted by id. Each rule carries:

- `id`
- `shortDescription.text` and `fullDescription.text`: the `obligation` text of the pack rule with that id, or the id itself when no pack rule matches
- `properties.lifecycle`: the pack rule's lifecycle, `research` when unknown
- `properties.authority_refs`: sorted union of `authority_refs` over the findings for that rule

The rule id of a result-derived entry is, in order:

1. `rule_ids[0]` from any engine extension on the result row (`diligent.infra`, `diligent.source`, `diligent.assess`, and so on)
2. the row's `obligation_id` minus the `OBL-` prefix, except `OBL-UNMAPPED-*`
3. the row's `check_id`

## Results

Results come from two sources and are merged into one list.

**Result-derived entries** come from `RESULTS.jsonl`. Only rows with status `fail` or `needs_review` are emitted; `pass`, `not_applicable` and the other statuses are counted but not emitted. Each entry carries:

- `ruleId` and `ruleIndex`
- `message.text`: `observed.reason`, else `observed.header`, else the `check_id` (clipped at 1000 characters)
- `level`: `note` for `needs_review`; otherwise the level table above, using the pack rule's lifecycle and default severity
- `partialFingerprints["diligent/result/v1"]`: the SHA-256 hex digest of `check_id`, a NUL byte, `instance_id`, a NUL byte, the element instance, a NUL byte, and the rule id (chosen as described under Rules). The element instance is the first string-valued `instance` under any engine extension of the row (browser and source rows carry the element selector there), or the empty string, as for infra rows
- `locations[0].physicalLocation.artifactLocation.uri`: the row's `instance_id`
- `properties`: `check_id`, `result_id`, `instance_id`, `obligation_id`, `status`, `evidence_refs`, `lifecycle`
- `properties.needs_review: true` when the status is `needs_review`

**Finding-derived entries** come from `FINDINGS.json` when it exists, one per finding. Each carries:

- `ruleId` and `ruleIndex`
- `message.text`: the finding's `observation` (plain text, clipped at 1000 characters)
- `level`: per the table above
- `partialFingerprints["diligent/fingerprint/v1"]`: the finding's `fingerprint`
- `locations[]`: URL locations become `physicalLocation.artifactLocation.uri`; selectors become `logicalLocations[0].fullyQualifiedName`
- `properties`: `finding_id`, `obligation_id`, `review_state`, `certainty_basis`, `evidence_refs`, `lifecycle`

Entries of both kinds that sit at the research lifecycle carry `properties.diligent_note`.

Results are sorted by `(ruleId, fingerprint, finding_id or result_id)`, where the fingerprint is whichever of the two fingerprint properties the entry has.

## Baseline

`--baseline <file>` takes a baseline written by `diligent baseline` (see `docs/baseline-diff.md`). Each emitted result is compared to the baseline by its fingerprint and gets a `baselineState`:

- `unchanged`: the fingerprint is in the baseline
- `new`: the fingerprint is not in the baseline

Entries that were in the baseline and are gone from the run are resolved. They are not emitted. `diligent diff` reports them.

## Invocations

`invocations[0]` records:

- `executionSuccessful`: from `RUN.json` `assessment_complete`
- `properties.run_id`, `properties.execution_status`
- `properties.review_queue_count`: number of items in `REVIEW-QUEUE.json`
- `properties.results_total`: number of rows in `RESULTS.jsonl`
- `properties.results_emitted`: number of entries in `results` after merging both sources
- `properties.results_by_status`: count of `RESULTS.jsonl` rows per status

## Determinism

Two runs over the same directory produce byte-identical output. Collections are sorted, keys are in stable alphabetical order, and no wall-clock time is embedded except where the source artifact already carries a timestamp.

## Uploading to GitHub

```bash
gh api repos/{owner}/{repo}/code-scanning/sarifs \
  --method PUT \
  -f sarif=@report.sarif \
  -f ref=refs/heads/main \
  -f commit_sha=$(git rev-parse HEAD)
```

Or use the `github/codeql-action/upload-sarif` action in a workflow:

```yaml
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: report.sarif
```

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 2 | Usage error (missing arguments) |
| 3 | Run directory missing or artifact failed schema validation |
