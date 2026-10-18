# truncated-object

The test ingests the `24afc2-failed-example-1` capture set, then cuts the DOM snapshot object to half its length so it no longer hashes to its name.

Expected: `Store::verify` lists that object in `mismatched`, with nothing missing. `plan_resume` over the store's own keys reuses all but the DOM snapshot key and redoes that one, with a reason that mentions the failed re-hash. Re-ingesting the capture repairs the object.
