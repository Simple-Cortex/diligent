# resume-engine-changed

The test ingests the `24afc2-failed-example-1` capture set, then asks for the same keys with `engine_version` changed to `0.2.0`.

Expected: nothing is reused and every key is redone. Each reason names `engine_version` and both versions. No reuse record is appended.
