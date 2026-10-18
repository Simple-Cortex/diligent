# infra fixture: http-only

http://host/ answers 200 over plain http with no redirect. INFRA-01-https-redirect fails; everything else is unchanged.

`observation.json` feeds `FakeFetcher` and `FakeResolver` (see `crates/diligent-infra/src/fake.rs`). `expected-results.jsonl` is the exact `RESULTS.jsonl` the checks must produce, with the clock and run id fixed by the observation.
