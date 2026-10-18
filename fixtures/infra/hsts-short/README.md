# infra fixture: hsts-short

HSTS max-age is one day. INFRA-02-hsts is needs_review; the preload row is needs_review naming includeSubDomains and preload.

`observation.json` feeds `FakeFetcher` and `FakeResolver` (see `crates/diligent-infra/src/fake.rs`). `expected-results.jsonl` is the exact `RESULTS.jsonl` the checks must produce, with the clock and run id fixed by the observation.
