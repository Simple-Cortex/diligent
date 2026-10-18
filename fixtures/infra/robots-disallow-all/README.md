# infra fixture: robots-disallow-all

robots.txt disallows everything for User-agent: *. FED-18-robots fails; the sitemap is still valid.

`observation.json` feeds `FakeFetcher` and `FakeResolver` (see `crates/diligent-infra/src/fake.rs`). `expected-results.jsonl` is the exact `RESULTS.jsonl` the checks must produce, with the clock and run id fixed by the observation.
