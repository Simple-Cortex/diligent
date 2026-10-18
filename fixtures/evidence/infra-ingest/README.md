# infra-ingest

`EVIDENCE.jsonl` and `evidence/` are the real output of `diligent-infra` for `fixtures/infra/federal-good/observation.json`, written by the `bless` example.

Test: `Store::ingest_evidence_jsonl(EVIDENCE.jsonl, <this dir>, Producer::Infra)`. Every row is indexed under its own `ev-http-*` or `ev-dns-*` id. HTTP rows whose body file hashes to the row are stored in `objects/`. DNS rows (`dns:TXT:<name>`) are indexed with `stored: null`. No cache keys are recorded. The tests count rows, body files and DNS rows from these files.
