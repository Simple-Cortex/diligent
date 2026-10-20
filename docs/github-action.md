# Use Diligent in GitHub Actions

The Action audits one URL, writes a SARIF file for code scanning, and fails the job only on the condition you choose. It does not certify anything. A pass applies to the declared scope and the sampled pages only.

```yaml
uses: Simple-Cortex/diligent@v0.2.1
```

## Requirements

- A Linux x86-64 runner such as `ubuntu-latest`. The release builds no Windows binary and no Linux arm64 binary, so the Action refuses to run elsewhere.
- Chrome on the runner as `google-chrome`, which the GitHub-hosted Ubuntu image has. Set `DILIGENT_CHROME` in the job environment to use another Chrome.
- The Action downloads the `diligent-rs` binary from the release and verifies it against `SHA256SUMS` before running it. A checksum mismatch stops the job. The Node skill itself never downloads anything, so the download happens here, in the open, and nowhere else.
- No npm install. The Action runs the Node code in its own checkout, and the package has no dependencies.

## Inputs

| Input | Default | Meaning |
| --- | --- | --- |
| `url` | required | Site to audit. Its origin is the only origin tested. |
| `audit-id` | `ci` | Audit id under the workspace root. |
| `profile` | `us-federal-public-web` | Profile id. |
| `pages` | `10` | Most pages to collect. |
| `facts` | empty | JSON object of `fact_id: value`, written to a temporary file and passed with `--facts`. |
| `facts-file` | empty | Path to a facts file. `facts` wins when both are set. |
| `baseline` | empty | Path to a baseline file. Enables the diff and marks SARIF results as new or unchanged. |
| `fail-on` | `gate` | When the step fails. See below. |
| `upload-sarif` | `true` | Upload the SARIF file to code scanning. |
| `upload-artifacts` | `true` | Upload the audit directory as the artifact `diligent-<audit-id>`. |
| `sarif-path` | `diligent.sarif` | Where to write the SARIF file. A relative path is resolved against the workspace. |
| `diligent-rs-version` | empty | Release version to download. Empty means the version in the Action's own `package.json`. |
| `diligent-rs-bin` | empty | Path to an existing binary. Skips the download. For CI of this repository and source builds. |
| `allow-private` | `false` | Pass `--allow-private`, so the start host may be a private address. |
| `baseline-out` | empty | Write a baseline of this run to this path and upload it with the audit artifacts. |
| `chrome-no-sandbox` | `false` | Set `DILIGENT_CHROME_NO_SANDBOX=1` for the audit step only. Needed in root containers, not on GitHub-hosted runners. |
| `timeout-ms` | `90000` | Per-operation timeout for the audit. The CLI default is 30000; the first page on a cold GitHub-hosted runner took longer than that in testing, which left that page uncollected. |
| `extra-args` | empty | Appended to `diligent audit`, split on whitespace. |

Facts are supplied by you, never guessed. Without facts, applicability stays unknown and the gate is incomplete. `references/facts/us-federal-public-web.example.json` shows the shape.

The audited site's content is untrusted, so keep the Chrome sandbox on, give the job minimal permissions and no secrets it does not need.

## Outputs

| Output | Meaning |
| --- | --- |
| `gate-exit` | Exit code of the audit: the gate's code (0 pass for declared scope, 1 mandatory failure, 2 incomplete) or 3 when a stage failed. |
| `overall` | Overall result from `ASSESSMENT.json`. |
| `run-id` | Run id. |
| `audit-dir` | Audit directory. |
| `sarif-path` | Absolute path of the SARIF file. |
| `baseline-path` | Absolute path of the baseline written by `baseline-out`. Empty otherwise. |
| `summary-path` | Path of the audit's `--json` summary: every stage with its state, exit code and, for a failed stage, the engine's error detail. Uploaded with the audit artifacts. Stages that did not finish cleanly are also printed as warnings in the log. |
| `diff-exit` | Exit code of `diligent diff`. Empty without a baseline. |
| `new-count` | New entries against the baseline. Empty without a baseline. |
| `resolved-count` | Baseline entries this run resolved. Empty without a baseline. |

## fail-on

| Value | The step fails when |
| --- | --- |
| `gate` | The gate exits 1 (mandatory failure). Exit 2 (incomplete) only prints a warning. |
| `new` | `diligent diff --fail-on new` exits 1: the run has entries that are not in the baseline. Needs `baseline`. |
| `any` | `diligent diff --fail-on any` exits 1: the run has new entries or resolved ones. Needs `baseline`. |
| `none` | Never because of findings. |

With `new`, `any` and `none` the gate exit is reported in the output and the log line but does not fail the step. In every mode the step also fails when a stage failed before the gate could run (gate exit 3), because nothing was audited, and with `new` or `any` when the diff itself errors, because the condition cannot be evaluated.

The last log line of the decision step reads `Diligent: overall <overall>, gate exit <n>`, with `, new <k>, resolved <m>` added when a baseline is set.

## Permissions

Uploading SARIF needs `security-events: write`. Reading the repository needs `contents: read`.

```yaml
permissions:
  contents: read
  security-events: write
```

Set `upload-sarif: 'false'` to drop the `security-events` permission.

## Minimal workflow

```yaml
name: Diligent
on:
  schedule:
    - cron: '0 6 * * 1'
  workflow_dispatch:

permissions:
  contents: read
  security-events: write

jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Simple-Cortex/diligent@v0.2.1
        with:
          url: https://www.example.gov/
          facts-file: .diligent-facts.json
```

This fails the job when the gate finds a mandatory failure, uploads the SARIF file to code scanning, and uploads the audit directory as an artifact.

## Baseline workflow

A team with a backlog of known findings wants to block only new ones. Write a baseline on the default branch and compare against it on pull requests.

Two workflows share one baseline. On the default branch, the Action writes the baseline with `baseline-out`, and the workflow commits it. On pull requests, the Action compares against the committed file with `baseline` and fails on new entries. Refresh the baseline on purpose when you accept new findings.

On the default branch:

```yaml
name: Diligent baseline
on:
  workflow_dispatch:
  push:
    branches: [main]

permissions:
  contents: write
  security-events: write

jobs:
  baseline:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Simple-Cortex/diligent@v0.2.1
        with:
          url: https://staging.example.gov/
          facts-file: .diligent-facts.json
          fail-on: none
          baseline-out: .diligent-baseline.json
      - name: Commit the baseline
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
          git add .diligent-baseline.json
          git diff --cached --quiet || git commit -m "Update Diligent baseline"
          git push
```

`contents: write` is needed only for the commit step. Do not give pull request workflows that permission. The baseline is also uploaded with the audit artifacts. You can write one by hand with `diligent baseline write --run <run-id> --dir <audit-dir> --out <file>` (see `docs/baseline-diff.md`).

On pull requests:

```yaml
name: Diligent on pull requests
on:
  pull_request:

permissions:
  contents: read
  security-events: write

jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Simple-Cortex/diligent@v0.2.1
        with:
          url: https://staging.example.gov/
          facts-file: .diligent-facts.json
          baseline: .diligent-baseline.json
          fail-on: new
```

With a baseline, SARIF results carry `baselineState` of `new` or `unchanged`, and code scanning shows new results as new alerts.

`diligent diff` compares the run's check results and findings against the baseline by fingerprint. A run without `FINDINGS.json`, which is what an audit writes, is compared on its results.

## Linux only

The Action checks `RUNNER_OS` and `RUNNER_ARCH` first and stops with one sentence on anything but Linux x86-64. On macOS or Windows, install the binary yourself as `docs/install.md` describes and run `diligent audit` directly.
