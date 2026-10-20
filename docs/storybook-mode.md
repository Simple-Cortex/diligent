# Storybook component mode

`diligent audit --storybook <url>` audits the stories of a built Storybook as components. Each sampled story is captured in a browser and assessed with the element-level checks. A component gate decides the exit code, and `COMPONENT-BASELINE.json` records the outcome per story.

```bash
diligent audit --storybook https://design.example.test/storybook/ components \
  --facts references/facts/us-federal-public-web.example.json --pages 20
```

## What it claims, and what it does not

A component audit is never a site claim. It says that the sampled stories, rendered as the Storybook renders them, did or did not fail the element-level checks that the profile makes mandatory and applicable. It says nothing about any page built from those components.

- Whole-page obligations are recorded as `not_tested` with the reason `component mode: whole-page obligation`. A story is not a page: it has no banner, bypass mechanism, page title criteria, footer links or analytics, so those checks would fail for the wrong reason. They are excluded from the exit code.
- The site-level `ASSESSMENT.json` is still written by the normal gate and will usually say `incomplete`. The report puts a notice above it that names the component audit, the stories sampled out of the stories found, and that it is not a site claim.
- The component gate checks one rendering of each story. It does not exercise the states a component can reach by interaction.

## Discovery

The story list comes from `<url>/index.json` (Storybook 7 and 8). Entries with `type: "story"` are audited; `docs` entries are skipped. The page for a story is `<url>/iframe.html?id=<storyId>&viewMode=story`, so each story has its own final URL and its own result and instance ids.

- Only the origin of `<url>` is contacted. A redirect off the origin is refused. A private, loopback or link-local address is refused unless `--allow-private` is given.
- If `index.json` is absent, `stories.json` is tried and read when it has the Storybook 6 shape. This is recognised from the file shape and has not been exercised against a real Storybook 6 build; treat Storybook 6 as best effort.
- A missing or non-Storybook index is an error that names the file. The audit exits 3.

Point `--storybook` at the root of a built Storybook, the folder `storybook build` writes. A running dev server works if it serves the same files.

## The cap

`--pages` caps the stories audited (default 10, at most 200), with the same seeded sampling as a site audit. Stories are grouped by `title`, which is the component. The structured sample takes the first story of each component; the random sample draws from the rest with the sampler's seeded generator (`--seed`, default the run id); the two together never exceed `--pages`. If there are more components than the cap, the components left out are listed in `SAMPLE.json` under `remaining_work` and appear in `COMPONENT-BASELINE.json` with no stories covered. The sample and the total story count are recorded in `SAMPLE.json` and `INVENTORY.json` under `extensions["diligent.storybook"]` as `total_stories` and `sampled_stories`.

## Exit codes

The component gate decides:

| Exit | Meaning |
| --- | --- |
| 1 | An element-level check of a mandatory, applicable obligation failed on at least one sampled story |
| 0 | No such failure, and every sampled story was collected and assessed |
| 2 | No such failure, but a sampled story was not collected or assessed, no story was sampled, two stories shared a final URL, or no applicable mandatory obligation was evaluated by an element-level check |
| 3 | A stage failed before the gate could run, or the binary is missing |

Which checks are element-level is read from `diligent-rs checks list --json` (`"scope": "document"` or `"element"`), not listed in Node. Applicability still comes from your facts: without facts no obligation is known to be mandatory and applicable, so nothing can fail, and the gate returns `incomplete` (exit 2) with a problem stating that no applicable mandatory obligation was evaluated by an element-level check.

The command prints one line: `Component gate: <pass|fail|incomplete> (<n> stories, <f> failed, exit <code>). Not a site claim.`

## Outputs

- `COMPONENT-BASELINE.json`: one entry per component (library `storybook`), with the sampled story ids as configurations, the story names assessed as states, and the result ids as references. `extensions["diligent.storybook"]` holds the per-story outcome (collect and assess state, failed, the failed check ids, the final URL), the gate state, and the claim text.
- `INVENTORY.json`, `SAMPLE.json`: built from the story index instead of site discovery.
- `RUN.json`: `modes` includes `component`, and `extensions.audit.mode` is `storybook`.
- `REPORT.md`: the component notice and a story, component and failed-checks table above section 1.
- The usual `RESULTS.jsonl`, `EVIDENCE.jsonl`, `SIGNALS.jsonl` and `EVENTS.jsonl`. The `infra` stage is skipped because a Storybook is not an origin to certify.

`--storybook` cannot be combined with `--url` or `--replay`.
