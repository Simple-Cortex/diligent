# captures-ingest

Ingests three recorded capture sets that live under `fixtures/a11y/captures/` (referenced by path, not copied):

- `047fe0-passed-example-1`
- `24afc2-failed-example-1`
- `2ee8b8-failed-example-1`

Each set is ingested into one store with `Store::ingest_captures`, using the set's `final_url` as the route, run id `run-2026-10-18-0a1b2c`, a commit, locale and rule version, and missing reasons for `role` and `state`.

Expected, derived from the manifests at test time: one index line and one object per distinct payload, one cache key per distinct viewport, media and kind. `axe_result` and `perf_metrics` captures are `runtime_observation` records; the first has trust `inference` and a `corroborator`, the second has `context.label` `lab`. Ingesting a second time adds nothing.
