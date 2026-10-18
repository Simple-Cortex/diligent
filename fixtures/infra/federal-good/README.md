# infra fixture: federal-good

Every check passes: permanent redirect to https, HSTS with preload, SPF -all, DMARC reject, VDP page and security.txt, permissive robots, valid sitemap, all security headers, DAP script, privacy link, .gov host. Expect 14 rows, all pass.

`observation.json` feeds `FakeFetcher` and `FakeResolver` (see `crates/diligent-infra/src/fake.rs`). `expected-results.jsonl` is the exact `RESULTS.jsonl` the checks must produce, with the clock and run id fixed by the observation.
