# infra fixture: vdp-security-txt-only

security.txt with Contact exists but the policy page is 404. INFRA-08-vdp is needs_review for both INFRA-08 and FED-11.

`observation.json` feeds `FakeFetcher` and `FakeResolver` (see `crates/diligent-infra/src/fake.rs`). `expected-results.jsonl` is the exact `RESULTS.jsonl` the checks must produce, with the clock and run id fixed by the observation.
