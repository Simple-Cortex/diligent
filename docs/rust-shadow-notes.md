# Rust shadow notes: `diligent-rs rules validate`

The distribution ADR (`docs/distribution-adr.md`) allows Rust to replace Node for a command only after a shadow suite proves identical output. This file records how the Rust port of `scripts/validate-rules.mjs` was built and where its output can still differ from Node's.

## How the port works

- `crates/diligent-rules/src/schema.rs` is a line-by-line port of `scripts/lib/schema.mjs`, the documented-subset validator. It produces the schema error text itself. Keyword order, JSON pointers and wording match Node, including the `anyOf` "closest" summary and the unknown-field message.
- Re-phrasing `jsonschema` crate errors into Node's wording was considered and rejected. It would couple the output to a third-party library's message structure and `anyOf` branch selection, and it would be hard to prove identical.
- The `jsonschema` crate (draft 2020-12, format assertion on) is used as an independent second opinion instead. `diligent_rules::schema_crosscheck`, or `diligent-rs rules validate --crosscheck`, reports any file where the port and the crate disagree about valid versus invalid. It never changes `errors`. On the shipped data they agree.
- `crates/diligent-rules/src/validate.rs` ports every check in `scripts/validate-rules.mjs` in the same order: registry schema, duplicate authority ids, `supersedes` / `superseded_by` / `incorporates` references, empty profile and pack directories, profile id and `selected_packs`, pack id and `profile_ids`, duplicate rule ids, the non-normative `authority_basis` rule, normative authority lookup, and the `published` lifecycle requirements.
- JavaScript value semantics that leak into Node's messages are reproduced in `jsval.rs`: truthiness, `String(value)`, `JSON.stringify`, `trim()` whitespace (which includes U+FEFF and excludes U+0085), UTF-16 `length`, `for...of` over a string, and the UTF-16 file-name sort.

## What was compared

The Rust binary and `node scripts/validate-rules.mjs --json` were run on the shipped data and on 17 mutated copies of it, and the full stdout and exit code were diffed. The mutations covered a missing registry, invalid JSON, no profiles, duplicate authorities, dangling references, schema violations of many kinds (enum, pattern, date, uri, unknown field, missing field, wrong type, `anyOf`), duplicate rule ids, wrong pack and profile ids, normative and non-normative authority rules (including whitespace-only and non-string `authority_basis`), the `published` lifecycle, missing and empty rule ids, a string where an array is expected, files that are not objects, and non-ASCII text.

Result: 17 of 18 byte-identical at the time. The one difference was the JSON parser message, which both sides now report in Diligent's own words (see below). This was a manual comparison; the permanent shadow suite now replays the mutations in CI.

## Known residual differences

1. **Other read failures.** A data file that exists but cannot be read (a directory named `x.json`, permission denied) prints the operating system's text. Node prints `EISDIR: illegal operation on a directory, read`, Rust prints `Is a directory (os error 21)`.
2. **`format: uri`.** Node uses `new URL(value)` (WHATWG). Rust has an approximation in `schema::is_uri`: a valid scheme, plus a non-empty host without forbidden characters and a numeric port for http, https, ws, wss and ftp. It does not do IDNA, IPv4 or IPv6 host validation, or percent-decoding. The shipped URLs all agree. An exotic URL could be accepted by one side and rejected by the other.
3. **Regular expressions.** Node compiles patterns as JavaScript `u` regexes. Rust compiles them with the `regex` crate in ASCII mode, so `\d` and `\w` are ASCII-only like JavaScript. The five patterns in the three schemas behave the same. A future schema pattern using `\s`, lookaround, back-references or Unicode property escapes needs review, and `check_schema` will report a pattern the `regex` crate rejects.
4. **Inputs on which Node crashes.** Node throws a `TypeError` and exits 3 with a stack trace when the registry JSON is `null` or a non-object, when an `authorities` entry is `null`, or when `selected_packs` or `profile_ids` is a truthy non-iterable such as a number or object. It also throws when a schema file is missing or uses an unsupported keyword. Rust keeps going: it reports the problem as an ordinary error line (schema problems appear as `<label>: Invalid schema <name>: ...`) or skips the non-iterable. Exit code is 3 in both cases, but stdout differs.
5. **Integer-like object keys.** JavaScript enumerates integer-like keys first in ascending order. Rust keeps insertion order. This only changes the order of "unknown field" errors for keys such as `"1"`.
6. **Exotic numbers in messages.** Numbers are printed the way JavaScript prints integers and ordinary decimals. Values at or above 1e21, below 1e-6, negative zero, and integers beyond 2^53 are not guaranteed identical.
7. **Rule-id identity.** Node's `Set` and `Map` compare strings, numbers and booleans by value and objects by reference. Rust keys strings, numbers, booleans and null by value and treats every array or object as unique. This matches Node for every shape the schema allows.
8. **Command line.** `--data-root` defaults to the current directory and `--schemas` defaults to `<data-root>/schemas`. Node always reads schemas from the package root, whatever `--data-root` says. Usage errors exit 2 with a `diligent-rs:` prefix where Node uses `diligent:`. `--help` text differs. `--crosscheck` has no Node equivalent. The human-readable `OK` and `INVALID` lines and the `--json` document are identical in format, and were identical in the runs above.

## Not covered

Both the `--json` document and the human-readable form were compared for stdout, stderr and exit code on the clean data and on five of the mutated copies. The `jsonschema` crate was only used for the valid/invalid cross-check, never for message text.

## Permanent suite

`scripts/shadow-rules.mjs` runs `node scripts/validate-rules.mjs --json` and `diligent-rs rules validate --json` on every case in `fixtures/shadow/rules/<case>/`, diffs stdout, stderr, and exit code, and exits 1 on any difference. `tests/shadow-rules.test.mjs` asserts exit 0. CI runs `npm run shadow:rules` after building the binary.

### Adding a case

Each case is a directory under `fixtures/shadow/rules/<case>/` containing overlay files that replace or add to the shipped data. The script copies the shipped `authorities/`, `rules/`, `profiles/`, and `schemas/` to a temp directory, then copies the overlay files on top. To add a case:

1. Create `fixtures/shadow/rules/<case-name>/`.
2. Add the mutated files (e.g., `authorities/registry.json` with a duplicate authority id).
3. Run `npm run shadow:rules` locally with `DILIGENT_RS_BIN` set. Both sides should produce identical output.
4. Commit the overlay directory.

Git does not record empty directories, so a case that needs a missing or empty directory declares it in `case.json` instead of an overlay: `{ "remove_dirs": ["profiles"] }` deletes the shipped directory from the temp copy, `{ "empty_dirs": ["profiles"] }` replaces it with an empty one. `case.json` itself is never copied. `no-profiles` and `empty-profile-dir` use this.

### No normalizations

Parse errors are reported in Diligent's own words on both sides (`<file>: not valid JSON`), so they are covered by the `invalid-json` case. Stdout, stderr, and exit code are compared byte for byte with no normalization.

## Gate

`scripts/shadow-gate.mjs` runs `node scripts/gate.mjs` and `diligent-rs gate` on every case directory under `fixtures/gate/` and `fixtures/shadow/gate/`, with and without `--json`, and compares exit code, stdout, stderr, `ASSESSMENT.json`, `EVENTS.jsonl`, and `runs/*/RUN.json` byte for byte. One normalization: the `"ts":"…"` value in `EVENTS.jsonl` becomes `<ts>` on both sides. The per-run temporary audit directory path, which appears in the `Wrote` line and in the missing-directory sentence, becomes `<audit>` in stdout and stderr. `tests/shadow-gate.test.mjs` asserts exit 0. CI runs `npm run shadow:gate` after building the binary.

The `fixtures/shadow/gate/real-audit-smoke/` directory is the gate input of a real audit from a GitHub runner. Byte identity on that directory is the bar; hand-built cases alone are not.

### Adding a case

Each case is a directory under `fixtures/shadow/gate/<case>/` containing a complete audit directory structure. The script copies the case to two temp directories and runs both versions. To add a case:

1. Create `fixtures/shadow/gate/<case-name>/` with the audit files (APPLICABILITY.json, RESULTS.jsonl, etc.).
2. Optionally add `case.json` with `{ "args": [...] }` to override the default arguments.
3. Run `npm run shadow:gate` locally with `DILIGENT_RS_BIN` set. Both sides should produce identical output (after normalization).
4. Commit the case directory.

### The normalizations

The `ts` field in `EVENTS.jsonl` is wall clock on both sides. The script normalizes `"ts":"…"` to `"ts":"<ts>"` on both sides before diffing. The temporary directory of each side is replaced by `<audit>` in stdout and stderr, because the two sides necessarily run in different directories. Everything else must be byte-identical.

### Engine selection

`bin/diligent.mjs` runs `gate` and `rules validate` through the `diligent-rs` binary when `DILIGENT_RS_BIN` (or `diligent-rs` on `PATH`) reports protocol 1, and through the Node scripts otherwise. Set `DILIGENT_ENGINE=node` to force the Node path for every command. The shadow scripts call the Node scripts directly, so they always compare Node against the binary; the test helpers point `DILIGENT_RS_BIN` at a nonexistent file so the rest of the suite stays on Node. `diligent --version --json` reports `executed_by: "native-rust"` and `delegated: ["gate", "rules validate"]` when the binary will run them.

### Parse errors

When an artifact is not valid JSON, both sides record `<file>: not valid JSON` (or `<file> line N: not valid JSON` for a bad line in `RESULTS.jsonl`) without the parser's own text. These strings are covered by the `malformed-json` and `malformed-jsonl-line` cases. Schema violations, which the gate reports in its own words, are covered too.
