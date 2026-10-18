A disputed fact keeps its value in SCOPE.json but evaluates as unknown. A `known` fact with a null value is unknown too (reported as fact_state `unknown`), and so is a fact whose state is `unknown`. All four rules are `unknown`; FX-01 and FX-02 are mandatory, so they count toward `mandatory_unknown`.

## Parameters

- profile: `fixture-profile` (data root: `data/`)
- as-of: `2026-10-17`
- evaluated-at: `2026-10-17T12:00:00Z`
- run id: `run-2026-10-17-a1b2c3` (from SCOPE.json)

`CASE.json` carries the same parameters for the test harness. Run by hand:

```
diligent-rs applicability --scope SCOPE.json --profile fixture-profile --data-root data \
  --as-of 2026-10-17 --evaluated-at 2026-10-17T12:00:00Z
```
