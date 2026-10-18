# infra fixture: non-gov-host

Host is under .com. FED-01-gov-domain is needs_review; all other checks pass.

`observation.json` feeds `FakeFetcher` and `FakeResolver` (see `crates/diligent-infra/src/fake.rs`). `expected-results.jsonl` is the exact `RESULTS.jsonl` the checks must produce, with the clock and run id fixed by the observation.
