# Evidence store fixtures

Cases for `crates/diligent-evidence/tests/store.rs`. Each directory has a `README.md`; most also have a small hand-written `expected.json` with the words a test checks for.

No case ships a pre-built store. The tests build each store at test time: they ingest one recorded capture set from `fixtures/a11y/captures` into a temporary directory with a fixed clock and fixed context, then apply the case's mutation. Counts come from the capture manifests, so re-recording the captures cannot make a case stale. The only generated files are in `infra-ingest`; regenerate them with `cargo run -p diligent-evidence --example bless_evidence`.

| Case | What it pins |
| --- | --- |
| `captures-ingest` | Three recorded capture sets ingest to one index line per distinct payload, one object each and one cache key per viewport, media and kind. |
| `infra-ingest` | An `EVIDENCE.jsonl` and `evidence/` from `diligent-infra` index in full; DNS rows are not stored. |
| `truncated-object` | One object is cut in half: `verify` reports it and `plan_resume` redoes its key. |
| `resume-compatible` | Same key and versions: everything is reused. |
| `resume-engine-changed` | The same keys at another `engine_version`: nothing is reused. |
| `tombstone` | The DOM snapshot is marked unavailable: `verify` counts it, provenance survives, the key is redone. |
