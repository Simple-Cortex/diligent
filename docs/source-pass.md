# Static HTML pass (`diligent-source`)

A browserless pass over saved HTML and over component and template files: static files, saved DOM snapshots, and JSX, TSX, Vue, Svelte, Astro, Twig and Liquid sources. It reads the markup and nothing else. Template syntax is masked, not run, so a value the source leaves to runtime is reported as `needs_review`.

## File types

| Extension | Flavour | Read as |
| --- | --- | --- |
| `.html`, `.htm` | html | Parsed as is |
| `.jsx`, `.tsx` | jsx | JSX elements and attributes; code outside JSX stays as text |
| `.vue` | vue | The top-level `<template>`; `<script>` and `<style>` are masked |
| `.svelte` | svelte | The markup; `<script>` and `<style>` are masked |
| `.astro` | astro | The markup; the `---` frontmatter is masked |
| `.twig`, `.liquid` | twig, liquid | The markup; `{# #}`, `{% %}` and comment blocks are masked |

A directory scan picks up all of these. A file passed directly is read by its extension, and as HTML when the extension is unknown.

## What it decides

Only what the markup settles by itself: attribute presence, structure, token validity, and duplicate ids. Rust makes every comparison, including the exact ones (title length, refresh delay, `maximum-scale`).

## What it never decides

- It never claims that a WCAG success criterion passes. A `pass` means one static condition held for one target.
- It does not render, compute styles, run scripts, or load linked resources. It does not judge whether text is adequate, accurate, or in context.
- It does not know what a template renders. An expression, a component, or an attribute spread is opaque (see Template flavours).
- The rule-level roll-up happens elsewhere, in `diligent-assess`.

## Positions

Evidence records carry `line`, `column` and `byte_offset` (also `line` under `extensions["diligent.source"]`) when the source text contains the element. A small lexer finds every start tag in the text, tolerating comments, `<` in text, unbalanced quotes, CDATA and raw text in `script` and `style`, and each parsed element takes the next tag of the same name in document order. Elements the parser implied (`html`, `head`, `body`, `tbody`) that are not in the text have no position, and the three fields are null. Lines end at `\n`, `\r\n` or a lone `\r`; columns count characters from 1. For template files the position is in the original text, not the masked text.

Evidence records still carry a CSS path and an outer-HTML snippet cut to 400 characters. For a template file the snippet is the masked markup, with `{expr}` for an expression.

## Output

Each finding is one evidence record and one check result, in the shapes of `schemas/evidence-record.schema.json` and `schemas/check-result.schema.json`.

- States: `pass`, `fail`, `needs_review`, `not_tested`. `not_tested` is recorded once per document per check when the preconditions were absent.
- `instance_id` is `<source path>::<css path>`. The bare CSS path is also under `extensions["diligent.source"].instance`.
- `obligation_id` is `OBL-` plus the primary pack rule. All informed rules, the ACT ids, and `scope: "static"` are in the same extension object.
- API: `scan_html`, `scan_text` (one document read as a given `Flavor`), `scan_path`, `write_jsonl`, and `cli` (`source scan <path> [--out <dir>] [--json]`, exit 0 when the scan ran, 3 on usage or I/O error).
- Selectors look like `html > body > main:nth-of-type(1) > form:nth-of-type(2) > input:nth-of-type(3)`, with `#id` used when the id is unique.

## Checks

| check_id | ACT | Informs |
| --- | --- | --- |
| `SRC-IMG-ALT-PRESENT` | 23a2a8 | A11Y-1.1.1 |
| `SRC-SVG-NAME` | 7d6734 | A11Y-1.1.1 |
| `SRC-CONTROL-NAME` | e086e5 | A11Y-4.1.2, A11Y-3.3.2 |
| `SRC-BUTTON-NAME` | 97a4e1 | A11Y-4.1.2 |
| `SRC-LINK-NAME` | c487ae | A11Y-2.4.4, A11Y-4.1.2 |
| `SRC-HTML-LANG` | b5c3f8, bf051a | A11Y-3.1.1 |
| `SRC-TITLE` | 2779a5 | A11Y-2.4.2, FED-33 |
| `SRC-META-DESCRIPTION` | none | FED-34 |
| `SRC-VIEWPORT-ZOOM` | b4f0c3 | A11Y-1.4.4 |
| `SRC-META-REFRESH` | bc659a | A11Y-2.2.1 |
| `SRC-IFRAME-NAME` | cae760 | A11Y-4.1.2 |
| `SRC-DUP-ID` | 3ea0c8 | A11Y-4.1.2 |
| `SRC-ARIA-REF` | none | A11Y-4.1.2 |
| `SRC-AUTOCOMPLETE` | 73f2c2 | A11Y-1.3.5 |
| `SRC-HEADING-ORDER` | none | A11Y-1.3.1 (best practice) |
| `SRC-TABINDEX-POSITIVE` | none | A11Y-2.4.3 |
| `SRC-TABLE-HEADERS` | none | A11Y-1.3.1 |
| `SRC-MEDIA-AUTOPLAY` | 80f0bf | A11Y-1.4.2 |
| `SRC-BANNER-PRESENT` | none | FED-32 |
| `SRC-IDENTIFIER-LINKS` | none | FED-04, 05, 06, 07, 08, 09, 10, 13 (one result per required link) |

Every rule id above exists in `rules/packs/*.json`. No check lacks a mapping. Two mappings are a judgement call, not a lookup: the pending standards for the page title and meta description map to FED-33 and FED-34, and each identifier link maps to the FED rule for that subject (About FED-04, Accessibility FED-05, Privacy FED-06, FOIA FED-07, No FEAR FED-08, Performance FED-09, Inspector General FED-10, USA.gov FED-13).

Where a check follows ACT, its module doc comment states what is and is not decided. Deliberate departures from ACT:

- `SRC-CONTROL-NAME` fails a control whose only hint is a placeholder (`placeholder_only: true`). ACT e086e5 accepts it.
- `SRC-IMG-ALT-PRESENT` returns `needs_review` for any non-empty text alternative, because adequacy is a human call. ACT passes it.
- `SRC-TITLE` returns `needs_review` when there is more than one title or the title is outside `head`. ACT passes if the first title is non-empty.
- `SRC-HTML-LANG` returns `needs_review` for three-letter primary subtags, which need the IANA registry. ACT rejects `eng`.
- `SRC-BUTTON-NAME` does not count `value` as a name for a `button` element, and counts the browser default label of a submit or reset input.
- `SRC-META-REFRESH` follows ACT: a delay of 0, or over 72000 seconds, passes.

## Template flavours

The pre-pass (`template.rs`) rewrites a file into plain HTML before parsing. It has no dependencies and never runs or compiles anything. Each output byte maps back to a source byte.

| Source | Becomes |
| --- | --- |
| Expression in an attribute value (`alt={x}`, `:alt="x"`, `alt="{{ x }}"`, Svelte shorthand `{alt}`) | An attribute whose value is a marker. A plain string literal in braces (`alt={""}`) is kept as that literal |
| Expression in text (`{x}`, `{{ x }}`) | A text marker |
| `{...props}`, `v-bind="obj"`, a bare `{{ attrs }}` in a tag | A spread marker on the element |
| `className`, `htmlFor`, `httpEquiv` | `class`, `for`, `http-equiv` |
| `<Foo>`, `<Foo.Bar>`, `<svelte:head>` (not Twig or Liquid) | An opaque component element. Self-closing tags get an explicit end tag |
| `<>` | A fragment element |
| Vue `<template>` | An ordinary element, so its content is scanned |
| `v-if`, `v-else`, `v-for`, `{#if}`, `{#each}`, `{% if %}`, `{% for %}`, and JSX or Astro expressions that hold tags (`&&`, ternary, `map`) | Nothing, but the file is marked as holding conditionals |
| `<script>` and `<style>` in Vue and Svelte, Astro frontmatter, Twig and Liquid comments, template statements | Spaces, with line breaks kept |

What changes in the checks:

- A `pass` or `fail` that rests on a marker (an attribute, or a name computed from text or from a component's content) is reported as `needs_review`.
- A `fail` on an element that has a spread, or an expression in an attribute that could supply what is missing, is `needs_review`. A control whose label has an expression for `for` is `needs_review` too.
- Components never produce a finding. Their children are scanned, and a component inside a link or button counts as unknown content for its name.
- An expression `id` is not compared with other ids, and an ARIA reference to one is `needs_review`.
- When the file holds a conditional or loop, a duplicate id (`SRC-DUP-ID`) is `needs_review`, because the two elements may never render together. The heading-order check already reports `needs_review`.
- JavaScript outside JSX in `.jsx` and `.tsx` files is not masked. It becomes text, which is noise but never a finding. A `<` that follows an identifier or a closing bracket there is read as an operator or a type argument, not as an element, so generic calls such as `useState<string>(...)` do not create elements.
- Evidence records for template files add a limitation saying the snippet is the masked markup.

### Document-level checks and fragments

A template with no `<html` and no `<body` after the pre-pass is a fragment: a component or partial, not a page. The checks that read the page as a whole record one `not_tested` for it, with a reason that names the fragment. Those checks are `SRC-HTML-LANG`, `SRC-TITLE`, `SRC-META-DESCRIPTION`, `SRC-VIEWPORT-ZOOM`, `SRC-META-REFRESH`, `SRC-BANNER-PRESENT` and `SRC-IDENTIFIER-LINKS` (the `DOCUMENT_LEVEL` list in `checks/mod.rs`). A base template with its own `<html>` runs them. Plain `.html` files are never treated as fragments, so existing results do not change.

## Build plugin

`plugins/vite-plugin-diligent-src/` is a Vite plugin that stamps native elements with their source location so a later browser run can point back at the file. It is private to this repository: it is not published and not in the npm package.

```js
import { diligentSrc } from './plugins/vite-plugin-diligent-src/index.mjs';

export default { plugins: [diligentSrc()] };
```

- Options: `enabled` (default: true only when Vite's mode is `development`) and `root` (default: the Vite root; paths are written relative to it).
- It runs before other plugins and rewrites `.jsx`, `.tsx`, `.vue`, `.svelte` and `.astro` files. Every start tag with a lowercase name gets `data-diligent-src="<path>:<line>"`. Components, fragments, tags that already carry the attribute, and tags in a Vue or Svelte script or style block are left alone.
- It uses the same tolerant tag lexing as the Rust pre-pass, written in JavaScript with no dependencies. Source maps are not produced: the attribute is inserted inside a tag, which shifts columns on that line only.
- `diligent-rs assess` reads the attribute from the captured DOM. A result about a stamped element carries it as `extensions["diligent.assess"].source_ref`, and the snapshot evidence record lists `context.source_refs`, a map from each such result's selector to its reference (an evidence record covers a whole snapshot, so one `source_ref` per record would not be correct). Results about unstamped elements are unchanged, and a value is cut to 300 characters.

## Fixture provenance

- `fixtures/act/` holds 189 W3C ACT test cases for the 14 ACT rules above, retrieved on 2026-10-17 from `https://act-rules.github.io/testcases.json`, unmodified, under the W3C Software and Document Notice and License (`fixtures/act/LICENSE.md`). They are untrusted data and are only ever parsed as text.
- `fixtures/act/manifest.json` records, for every case, the ACT outcome, the Diligent state expected, a note where the two differ, and a `skipped_reason` for each case excluded from the tests. 172 cases are kept and 17 are excluded.
- `fixtures/source/` holds Diligent-authored fixtures for the checks with no ACT rule, plus `manifest.json`. Each such check has at least one passing or clean case and one failing or flagged case. `fixtures/source/templates/<flavour>/` holds two or more files for each template flavour, one clean and one with expressions, components or conditionals.
- Tests live in `crates/diligent-source/tests/`. `fixtures.rs` runs both manifests through `scan_html`. `output.rs` checks the schema shape, rule ids against the packs, JSONL writing, and `scan_path`. `positions.rs` asserts exact lines, and `templates.rs` asserts source lines through the pre-pass, opacity, fragments and downgrades.

## Known limitations

- Visibility is limited to `aria-hidden`, the `hidden` attribute, and inline `display:none` or `visibility:hidden` on the element or an ancestor. Style sheets are ignored.
- A presentational role is honoured unless the element is focusable or carries a global ARIA attribute.
- Content in `iframe srcdoc`, `template`, shadow trees, and anything a script builds is not scanned.
- Template scanning is textual. It cannot see props, slots, includes, `extends` parents, or what a component renders, so those are opaque. Elements produced by `map`, `v-for` or `{% for %}` appear once.
- A line number can be off by one element when the parser drops or duplicates an element (a stray `<td>`, reconstructed formatting elements). Alignment is by tag name in document order.
- Custom widgets (`div role=textbox`, `role=combobox`) are outside `SRC-CONTROL-NAME`. Image map `area` elements are outside `SRC-LINK-NAME`.
- `SRC-VIEWPORT-ZOOM` does not parse spaces around `=` in the viewport content string.
- Non-UTF-8 files are decoded lossily. Files over 8 MiB are skipped and counted.
