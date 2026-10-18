# infra fixture: no-dmarc

No TXT record at _dmarc.example.gov. INFRA-03-dmarc fails.

`observation.json` feeds `FakeFetcher` and `FakeResolver` (see `crates/diligent-infra/src/fake.rs`). `expected-results.jsonl` is the exact `RESULTS.jsonl` the checks must produce, with the clock and run id fixed by the observation.
