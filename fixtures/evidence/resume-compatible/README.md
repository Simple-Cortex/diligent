# resume-compatible

The test ingests the `24afc2-failed-example-1` capture set and plans a resume over the store's own keys.

Expected: every key is reused and none redone; one reuse record per key is appended, each `verified: true` and naming run `run-2026-10-18-0a1b2c` as the source. `Store::verify` is clean.
