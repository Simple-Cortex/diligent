`federal_executive_agency` is false. FX-01 is `not_applicable` with an exclusion that cites `AX1 1.1`, lists both predicate facts, and unions their evidence refs (sorted, de-duplicated). FX-02 and FX-03 are `applicable`. FX-04 has the constant predicate `false` and no facts: it must not become `not_applicable`, so it is `unknown` with trace reason `no_facts`.

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
