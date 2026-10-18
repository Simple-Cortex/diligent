`launch_date` is `2026-02-30`, which is not a calendar date: FX-01 is `unknown` with reason `invalid_date`. FX-02 is `applicable` and FX-03 is `not_applicable` (valid date, comparison false). FX-04 compares a numeric fact as a date: `unknown` with reason `type_mismatch`. FX-01 and FX-03 use the structured form; FX-02 and FX-04 use the string call form.

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
