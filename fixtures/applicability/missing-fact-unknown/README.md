`public_facing` is absent from SCOPE.json. FX-01 (`all` with an unknown) and FX-03 (`not` of unknown) are `unknown` with trace reason `missing_fact` and fact_state `absent`. FX-02 is `applicable` because `any` with a true operand is true. FX-04 is `not_applicable` because `all` with a false operand is false even though the other operand is missing; its exclusion cites only facts that exist.

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
