# infra fixture: timeout-everywhere

Every request and DNS lookup times out. Every row is blocked except FED-01-gov-domain, which is a predicate on the host name.

`observation.json` feeds `FakeFetcher` and `FakeResolver` (see `crates/diligent-infra/src/fake.rs`). `expected-results.jsonl` is the exact `RESULTS.jsonl` the checks must produce, with the clock and run id fixed by the observation.
