`exists` is true only for a known, non-null fact. For an unknown, disputed, null or absent fact it is `unknown`, never `false`, so missing intake data cannot make a rule `not_applicable`. FX-05 (`not` of that) stays unknown; FX-06 is true because one `any` operand is true.

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
