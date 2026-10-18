# tombstone

The test ingests the `24afc2-failed-example-1` capture set, then calls `mark_unavailable` for the DOM snapshot evidence. The tombstone follows the record line in `index.jsonl` and the object file is removed.

Expected: `verify` reports one tombstoned id and is clean (a tombstoned payload is not missing). `find_record` still returns the original record, and `lookup` of the DOM snapshot key still returns its key record. `plan_resume` redoes that key because its object is missing and reuses the rest.
