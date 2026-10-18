# infra fixture: vdp-page-only

The policy page exists but security.txt is 404. INFRA-08-vdp passes for INFRA-08 and FED-11, with the limitation "security.txt absent".

`observation.json` feeds `FakeFetcher` and `FakeResolver`. `expected-results.jsonl` is the exact `RESULTS.jsonl` the checks must produce.
