# infra fixture: spf-multiple-records

Two v=spf1 records at the apex, which is invalid. INFRA-03-spf is error.

`observation.json` feeds `FakeFetcher` and `FakeResolver` (see `crates/diligent-infra/src/fake.rs`). `expected-results.jsonl` is the exact `RESULTS.jsonl` the checks must produce, with the clock and run id fixed by the observation.
