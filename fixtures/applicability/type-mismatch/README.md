A comparison across types is `unknown` with reason `type_mismatch`: string vs number (FX-01, FX-02), `gte` on a boolean (FX-03), a number used as a boolean (FX-04), an array (FX-07). `in` is true when any element matches (FX-05) but `unknown` when none match and an element has the wrong type (FX-06). FX-10 is `not_applicable`: `all` with a false operand is false even though the other operand is a type mismatch.

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
