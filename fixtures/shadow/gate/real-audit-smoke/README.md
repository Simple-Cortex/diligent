# real-audit-smoke

Gate inputs of a real `diligent audit` produced by the project's own GitHub Action smoke test on a GitHub-hosted runner. Copied unchanged from the uploaded artifact: `APPLICABILITY.json`, `RESULTS.jsonl`, `REVIEW-QUEUE.json`, `RULES.lock.json` and `runs/<run-id>/RUN.json`. `EXCEPTIONS.json` and `FINDINGS.json` were absent in the audit and are absent here. Captures and evidence are not included; the gate does not read them.

In that run the home page failed to collect (cold Chrome, 30 s timeout), so the gate is `incomplete` with exit 2. That makes this a useful shadow case: unknown applicability, untested obligations, open review items and a missing page, all from real output rather than a hand-built file.
