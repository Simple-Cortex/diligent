Scope is true for FX-01 to FX-06; temporal status decides the state. FX-01 has a future effective date: `pending_effective_date`, with its transition recorded as a trace entry (op `transition`) that does not change the state. FX-02 (effective on the as-of date) and FX-03 (past) are `applicable`. FX-04 (`pending_effective_date: true`, no date) and FX-05 (unparseable date) are `unknown`. FX-06 has no date and no pending flag: `applicable`. FX-07 is false in scope, so it is `not_applicable` regardless of its future date (its temporal_state is still reported as `pending`).

## Parameters

- profile: `fixture-profile` (data root: `data/`)
- as-of: `2026-10-17`
- evaluated-at: `2026-10-17T12:00:00Z`
- run id: `run-2026-10-17-ffffff` (set with `--run-id`; SCOPE.json carries a different run id)

`CASE.json` carries the same parameters for the test harness. Run by hand:

```
diligent-rs applicability --scope SCOPE.json --profile fixture-profile --data-root data \
  --as-of 2026-10-17 --evaluated-at 2026-10-17T12:00:00Z --run-id run-2026-10-17-ffffff
```
