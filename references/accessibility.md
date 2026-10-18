# Accessibility catalog

A routed catalog of the accessibility obligations in the `a11y-*` rule packs. It lists every WCAG 2.0, 2.1, and 2.2 level A and AA success criterion, and the Section 508 content, support, and procurement rows. Read the family you need; do not load the whole catalog into a prompt.

**No row here has an executable check in Diligent 0.1.1.** Every rule is at the `research` lifecycle. A row names the proposed test and the channels that could establish it. It does not claim a check exists. See `../docs/supported-checks.md`.

## 1. How to apply the catalog

- The catalog holds 38 WCAG 2.0 criteria, 12 additions in 2.1, and 6 additions in 2.2, plus 8 Section 508 rows. WCAG 2.2 removes 4.1.1, so the cross-version union is 56. Use the version the target actually adopts. An older profile does not acquire newer criteria.
- The normative standard, its definitions, exceptions, conformance requirements, and the incorporated version govern. The linked Understanding pages are informative.
- Rule ids are `A11Y-<criterion>`, for example `A11Y-1.4.3`. Section 508 content, support, and procurement rows live in the `federal-public-web` pack as `FED-35` to `FED-42` and keep their blueprint ids (ICT-01 to ICT-08) in the source row field.
- One component passing in one state does not satisfy a criterion globally. Full pages and every step of a complete process matter. Include embedded vendors, authentication, errors, uploads, timeouts, consent dialogs, and PDFs.
- Use a mature accessibility engine for the rules it supports and preserve its version and rule tags. Do not reimplement accessible-name computation, browser layout, or every criterion with pattern matching.
- Keep the matrix an implementation target. It is not a claim of 56 automated tests.

**Authority labels.** L is a statutory or regulatory requirement within its stated scope. P is a binding policy requirement. C is conditional, triggered by facts, adoption, or contract. R is a recommended practice. O needs organizational evidence. WCAG 2.0 criteria are L through the Section 508 incorporation and also reach state and local entities through ADA Title II, which adopts WCAG 2.1 AA. WCAG 2.1 additions are C. WCAG 2.2 additions are R unless an applicable adoption makes them binding.

**Channels.** S source or configuration analysis. B browser or runtime observation. N network or deployed infrastructure. M model-assisted assessment. H human or assistive-technology validation. O documentary or operational evidence.

## 2. Criteria by family

### Alternatives (1.1.1)

**Procedure.** Inventory images, SVG, canvas, icons; compute role and name and decorative treatment; compare the alternative with task purpose.

**Pass limits.** Presence is automatable. Adequacy and complex graphics need expert or model-assisted review.

| Rule | Criterion | Level | Label | Channels | Proposed test and repair approach |
| --- | --- | --- | --- | --- | --- |
| A11Y-1.1.1 | 1.1.1 Non-text content (WCAG 2.0) | A | L | S B M H | Inventory images, SVG, canvas, icons, charts, and CAPTCHA. Check alternatives against purpose; use empty alternatives for decoration and equivalent data for meaningful graphics. |

### Media (1.2.1 to 1.2.5)

**Procedure.** Inventory media and live events; identify required alternatives; sample the actual timeline and essential visual and audio information.

**Pass limits.** Track presence does not prove accuracy, completeness, or synchronization.

| Rule | Criterion | Level | Label | Channels | Proposed test and repair approach |
| --- | --- | --- | --- | --- | --- |
| A11Y-1.2.1 | 1.2.1 Audio/video only (WCAG 2.0) | A | L | B M H | Identify prerecorded single-medium content; inspect equivalent alternatives. Link a usable transcript or other required equivalent alongside the media. |
| A11Y-1.2.2 | 1.2.2 Prerecorded captions (WCAG 2.0) | A | L | S B H | Verify actual caption coverage, timing, speaker and meaningful-sound information; a track element alone is insufficient. Correct captions against media. |
| A11Y-1.2.3 | 1.2.3 Description or media alternative (WCAG 2.0) | A | L | B H | Check essential visual information in synchronized prerecorded media; evaluate a qualifying description or full media alternative. |
| A11Y-1.2.4 | 1.2.4 Live captions (WCAG 2.0) | AA | L | B H O | Review live synchronized-media delivery and caption operations, including the actual event experience. |
| A11Y-1.2.5 | 1.2.5 Prerecorded audio description (WCAG 2.0) | AA | L | B H | Evaluate description of meaningful visuals in prerecorded synchronized video. A transcript alone does not automatically satisfy this AA requirement. |

### Structure (1.3.1 to 1.3.5)

**Procedure.** Inspect semantics, labels, table relationships, reading sequence, orientation, and input-purpose tokens.

**Pass limits.** DOM and tree evidence plus task reading and layout review. CSS order may differ.

| Rule | Criterion | Level | Label | Channels | Proposed test and repair approach |
| --- | --- | --- | --- | --- | --- |
| A11Y-1.3.1 | 1.3.1 Relationships (WCAG 2.0) | A | L | S B H | Compare visual structure with accessibility tree: headings, lists, tables, labels, groups, and instructions. Repair semantics at the component/template level. |
| A11Y-1.3.2 | 1.3.2 Meaningful sequence (WCAG 2.0) | A | L | S B H | Compare DOM/assistive reading order with intended meaning, especially CSS reorderings and responsive layouts. |
| A11Y-1.3.3 | 1.3.3 Sensory characteristics (WCAG 2.0) | A | L | M H | Find instructions relying only on position, shape, sound, or size. Add a stable name or text reference. |
| A11Y-1.3.4 | 1.3.4 Orientation (WCAG 2.1) | AA | C | B H | Rotate common devices; identify essential-orientation exceptions. Remove unnecessary orientation locks. |
| A11Y-1.3.5 | 1.3.5 Input purpose (WCAG 2.1) | AA | C | S B | Check covered personal-data fields for programmatically identifiable purpose, commonly through valid autocomplete tokens. |

### Perception (1.4.x)

**Procedure.** Inspect color dependence, audio control, contrast, text zoom, images of text, reflow, nontext contrast, text spacing, and hover or focus content.

**Pass limits.** Separate exceptions per criterion. Complex backgrounds and essential two-dimensional layouts require review.

| Rule | Criterion | Level | Label | Channels | Proposed test and repair approach |
| --- | --- | --- | --- | --- | --- |
| A11Y-1.4.1 | 1.4.1 Color (WCAG 2.0) | A | L | B M H | Review errors, charts, links, selection, and statuses without color cues. Add text, symbols, patterns, or another suitable cue. |
| A11Y-1.4.2 | 1.4.2 Audio control (WCAG 2.0) | A | L | B | Detect audio starting automatically for more than three seconds. Provide qualifying independent control or avoid autoplay. |
| A11Y-1.4.3 | 1.4.3 Text contrast (WCAG 2.0) | AA | L | B | Measure rendered foreground/background pairs: 4.5:1 ordinary text; 3:1 large text, at least 18 pt or 14 pt bold. Honor exclusions and image/gradient uncertainty. |
| A11Y-1.4.4 | 1.4.4 Resize text (WCAG 2.0) | AA | L | B H | Resize to 200% and inspect content/function loss, with the criterion's exceptions. Replace clipping/fixed heights with layouts that accommodate text. |
| A11Y-1.4.5 | 1.4.5 Images of text (WCAG 2.0) | AA | L | S B M H | Detect rasterized text and evaluate customization/essential exceptions. Use real text where required. |
| A11Y-1.4.10 | 1.4.10 Reflow (WCAG 2.1) | AA | C | B H | Test 320 CSS px width and the equivalent 256 CSS px height for relevant writing modes. Check two-dimensional-content exceptions; avoid page-wide scrolling. |
| A11Y-1.4.11 | 1.4.11 Non-text contrast (WCAG 2.1) | AA | C | B H | Measure required control/state and meaningful graphic cues at 3:1 against adjacent colors; account for specified exclusions. |
| A11Y-1.4.12 | 1.4.12 Text spacing (WCAG 2.1) | AA | C | B | Override line height to 1.5×, paragraph spacing 2×, letter spacing .12×, word spacing .16× font size. Check loss; these are tolerances, not mandatory default styling. |
| A11Y-1.4.13 | 1.4.13 Hover/focus content (WCAG 2.1) | AA | C | B H | Test tooltips/popovers for applicable dismissible, hoverable, and persistent behavior. Include magnification and keyboard use. |

### Keyboard (2.1.x)

**Procedure.** Exercise keyboard tasks, entry and exit, custom widget interactions, and character shortcuts.

**Pass limits.** A tab-order snapshot cannot prove task operability.

| Rule | Criterion | Level | Label | Channels | Proposed test and repair approach |
| --- | --- | --- | --- | --- | --- |
| A11Y-2.1.1 | 2.1.1 Keyboard (WCAG 2.0) | A | L | B H | Execute every task by keyboard, including custom controls, uploads, menus, editors, and maps; account for path-dependent-input exceptions. Prefer native controls. |
| A11Y-2.1.2 | 2.1.2 No keyboard trap (WCAG 2.0) | A | L | B H | Enter and leave widgets/frames. Verify modal exit and restoration; a proper modal's temporary focus containment is not inherently a failure. |
| A11Y-2.1.4 | 2.1.4 Character shortcuts (WCAG 2.1) | A | C | S B | Check character-only shortcuts for qualifying disabling, remapping, or focus restriction. |

### Timing and motion (2.2.x, 2.3.1)

**Procedure.** Trigger time limits, extend or disable where applicable, pause content, review flashing media.

**Pass limits.** No simple screenshot flash test. Escalate unmeasured media safely.

| Rule | Criterion | Level | Label | Channels | Proposed test and repair approach |
| --- | --- | --- | --- | --- | --- |
| A11Y-2.2.1 | 2.2.1 Timing (WCAG 2.0) | A | L | S B O | Exercise session and form timers. Verify qualifying adjustment, warning/extension, or exception; coordinate with required security limits. |
| A11Y-2.2.2 | 2.2.2 Moving/updating content (WCAG 2.0) | A | L | B H | Inspect carousels, tickers, auto-updates, and animations. Provide qualifying pause/stop/hide or update-frequency control. |
| A11Y-2.3.1 | 2.3.1 Flashes (WCAG 2.0) | A | L | B H | Flag flashing media/animations; use suitable analysis for frequency and flash thresholds. Avoid unsafe flashing rather than relying on a warning. |

### Navigation and focus (2.4.x)

**Procedure.** Test bypass, titles, focus sequence, link purpose, multiple routes, headings and labels, indicators, and obscuration.

**Pass limits.** Evaluate focus after state change and under overlays. Retain context.

| Rule | Criterion | Level | Label | Channels | Proposed test and repair approach |
| --- | --- | --- | --- | --- | --- |
| A11Y-2.4.1 | 2.4.1 Bypass blocks (WCAG 2.0) | A | L | S B H | Verify a working way past repeated content. Test skip navigation target/focus behavior and relevant structural mechanisms. |
| A11Y-2.4.2 | 2.4.2 Page title (WCAG 2.0) | A | L | S B M | Compare page and SPA route titles with purpose. Generate meaningful titles for results, errors, and transaction stages. |
| A11Y-2.4.3 | 2.4.3 Focus order (WCAG 2.0) | A | L | B H | Record sequential focus through responsive layouts, inserted content, and dialogs. Repair order without arbitrary positive tabindex values. |
| A11Y-2.4.4 | 2.4.4 Link purpose (WCAG 2.0) | A | L | S B M H | Evaluate links with permitted programmatic context. Replace ambiguous link copy while preserving concise layouts. |
| A11Y-2.4.5 | 2.4.5 Multiple ways (WCAG 2.0) | AA | L | B H | Inspect ways to locate pages within a set; account for process-step exceptions. Combine navigation with search, index, or other suitable access. |
| A11Y-2.4.6 | 2.4.6 Headings/labels (WCAG 2.0) | AA | L | M H | Check whether headings and labels describe topic/purpose. A fixed heading count is not the conformance test. |
| A11Y-2.4.7 | 2.4.7 Visible focus (WCAG 2.0) | AA | L | B H | Observe every focused interactive component across states and themes. Restore a visible indicator; do not import AAA geometry into this AA criterion. |
| A11Y-2.4.11 | 2.4.11 Focus not obscured, minimum (WCAG 2.2) | AA | R | B | Ensure focused components are not entirely hidden by author-created content. Test sticky headers/footers, overlays, and scrolling. |

### Pointer and input (2.5.x)

**Procedure.** Exercise gesture alternatives, cancellation, label and name, motion alternatives, drag alternatives, and targets.

**Pass limits.** Measure hit areas and criterion-specific spacing and exceptions.

| Rule | Criterion | Level | Label | Channels | Proposed test and repair approach |
| --- | --- | --- | --- | --- | --- |
| A11Y-2.5.1 | 2.5.1 Pointer gestures (WCAG 2.1) | A | C | B H | Test multipoint/path gestures with a single-pointer alternative unless essential. Add controls for pinch, swipe, or path functions as needed. |
| A11Y-2.5.2 | 2.5.2 Pointer cancellation (WCAG 2.1) | A | C | S B | Inspect down/up event behavior and abort/undo alternatives, with specified exceptions. Avoid irreversible activation on pointer-down. |
| A11Y-2.5.3 | 2.5.3 Label in name (WCAG 2.1) | A | C | B | Compare visible text with accessible names, especially speech-input targets. Preserve visible wording within the name. |
| A11Y-2.5.4 | 2.5.4 Motion actuation (WCAG 2.1) | A | C | S B H | Inspect device/user motion actions; provide qualifying UI alternatives and disabling controls except where exempt. |
| A11Y-2.5.7 | 2.5.7 Dragging (WCAG 2.2) | AA | R | B H | Supply a single-pointer method without dragging unless a specified exception applies; keyboard support alone is insufficient for this criterion. |
| A11Y-2.5.8 | 2.5.8 Target size, minimum (WCAG 2.2) | AA | R | B | Evaluate 24×24 CSS px targets or qualifying spacing/exceptions. Use the actual geometry rules; 44×44 is not the universal AA minimum. |

### Understandability (3.1.x, 3.2.x)

**Procedure.** Inspect page and part language, on-focus and on-input changes, consistency, and help location.

**Pass limits.** Automated language guessing cannot prove correctness.

| Rule | Criterion | Level | Label | Channels | Proposed test and repair approach |
| --- | --- | --- | --- | --- | --- |
| A11Y-3.1.1 | 3.1.1 Page language (WCAG 2.0) | A | L | S B M | Compare declared document language with actual content. Fix templates and localized route output. |
| A11Y-3.1.2 | 3.1.2 Language changes (WCAG 2.0) | AA | L | S B M H | Check meaningful changes in human language, with specified exceptions. Mark appropriate passages and UI strings. |
| A11Y-3.2.1 | 3.2.1 Focus behavior (WCAG 2.0) | A | L | B | Focus controls without activation; detect unexpected context changes. Move navigation/submission to intentional actions. |
| A11Y-3.2.2 | 3.2.2 Input behavior (WCAG 2.0) | A | L | B H | Change selects/toggles/inputs; check context changes and advance notice. Use explicit submission when appropriate. |
| A11Y-3.2.3 | 3.2.3 Consistent navigation (WCAG 2.0) | AA | L | S B | Compare repeated navigation order across page sets, with permitted user changes. Fix shared templates. |
| A11Y-3.2.4 | 3.2.4 Consistent identification (WCAG 2.0) | AA | L | S B M | Compare labels/icons for the same function across the site. Align names through design-system components. |
| A11Y-3.2.6 | 3.2.6 Consistent help (WCAG 2.2) | A | R | B | Compare repeated help mechanisms' relative order across pages. Do not claim this criterion requires a chatbot. |

### Assistance (3.3.x)

**Procedure.** Trigger errors, instructions, suggestions, consequential review and correction, repeated entry, and authentication tasks.

**Pass limits.** Test real interaction and business behavior with safe fixtures.

| Rule | Criterion | Level | Label | Channels | Proposed test and repair approach |
| --- | --- | --- | --- | --- | --- |
| A11Y-3.3.1 | 3.3.1 Error identification (WCAG 2.0) | A | L | B H | Submit invalid fixtures; check textual identification and description of detected input errors. Map backend errors to meaningful fields. |
| A11Y-3.3.2 | 3.3.2 Labels/instructions (WCAG 2.0) | A | L | S B M H | Inspect required input instructions, labels, formats, and expectations. Placeholder text alone is often inadequate. |
| A11Y-3.3.3 | 3.3.3 Error suggestions (WCAG 2.0) | AA | L | B M H | When correction is known, inspect actionable suggestions, with security/purpose exceptions. Avoid vague “invalid input” loops. |
| A11Y-3.3.4 | 3.3.4 Error prevention (WCAG 2.0) | AA | L | B H O | Test covered legal/financial/data operations for qualifying reversibility, checking, or confirmation. Add review/edit without inventing cancellation rights. |
| A11Y-3.3.7 | 3.3.7 Redundant entry (WCAG 2.2) | A | R | B H | Identify repeated information within the same process. Offer qualifying reuse/selection, subject to security, validity, and essential exceptions. |
| A11Y-3.3.8 | 3.3.8 Accessible authentication (WCAG 2.2) | AA | R | B H | Assess cognitive-function tests and permitted alternatives/mechanisms/exceptions. Test password-manager support, paste, and actual recovery flows. |

### Robustness (4.1.x)

**Procedure.** Apply parsing only to versions that retain it; inspect names, roles, and values and dynamic status announcements.

**Pass limits.** Accessibility-tree correctness does not prove screen-reader experience.

| Rule | Criterion | Level | Label | Channels | Proposed test and repair approach |
| --- | --- | --- | --- | --- | --- |
| A11Y-4.1.1 | 4.1.1 Parsing (WCAG 2.0) | A in 2.0 and 2.1; removed in 2.2 | L | S B | Retain for standards that still incorporate it. Assess the applicable version/errata; distinguish parsing defects from actual name/role or relationship failures. |
| A11Y-4.1.2 | 4.1.2 Name/role/value (WCAG 2.0) | A | L | S B H | Inspect computed accessibility properties and changes. Test custom controls, expanded state, labels, and values against actual behavior. |
| A11Y-4.1.3 | 4.1.3 Status messages (WCAG 2.1) | AA | C | B H | Observe asynchronous status/error/progress updates without focus moves. Use suitable programmatic status exposure and avoid excessive announcements. |

### Section 508 content, support, and procurement

These rows are in the `federal-public-web` pack. Rule ids are `FED-35` to `FED-42`.

| Rule | Outcome | Label and source | Channels | Proposed evidence and repair |
| --- | --- | --- | --- | --- |
| FED-35 | Correctly scope public and covered non-public electronic content. | L/C, A01 E205 | H O | Inventory official communications and documents. Record exclusions per item, not an “internal = exempt” shortcut. |
| FED-36 | Evaluate applicable functional performance criteria and software requirements. | L/C, A01 E204/E207; chapters 3/5 | H O | Scope platform services, interoperability, applications, and authoring tools; use ART or a qualified evaluator. Avoid applying irrelevant desktop provisions to all web apps. |
| FED-37 | Accessible documentation/support. | L/C, A01 E208; chapter 6 | H O | Inspect help, support channels, accessibility information, and alternative-format delivery. |
| FED-38 | Exceptions require authorized basis and alternative access where required. | L/C/O, A01 E202 | H O | Preserve written burden/fundamental-alteration or best-meets determinations and affected provisions. Age alone does not prove legacy safe-harbor eligibility. |
| FED-39 | Prioritize HTML; use PDFs when necessary and accessible. | P, A06; A26 | H O | Review PDF purpose and available HTML. Test structure, reading order, headings, language, links, tables, form names, OCR/text, and alternatives. A tag tree alone is insufficient. |
| FED-40 | Acquisition requirements must address applicable accessibility standards. | L/P/O, A06; A27 | H O | Check solicitation/contract requirements and acceptance evidence. Validate the exact product/version/configuration, including embedded vendors. |
| FED-41 | Maintain accessibility program, testing/remediation evidence, and required reporting. | P/O, A06 | H O | Ask for named owner, release checks, issue tracking, and current reporting evidence; no page scan can prove program operation. |
| FED-42 | ACR/VPAT is evidence, not certification of the deployed service. | R/O, A27 | H O | Compare vendor statements to independent tests and limitations. Retest customized components and complete workflows. |

## 3. Recipe summaries

These twelve recipes are proposed engineering procedures. Keep the normative criterion, exceptions, and source version attached to each evaluator.

**TC-01 Text contrast.**
1. Select visible text instances in required states and collect resolved foreground and background, font size and weight, opacity, layering, and clipping.
2. Decide normal or large text from the criterion's definition and the typography evidence. Evaluate exemptions explicitly, including inactive controls and logotypes.
3. For opaque sRGB pairs, linearize each channel: c/12.92 when c is at most 0.04045, otherwise ((c + 0.055)/1.055)^2.4. Compute L as 0.2126 R + 0.7152 G + 0.0722 B, then (lighter L + 0.05)/(darker L + 0.05).
4. Compare the unrounded ratio with 4.5:1 for normal text or 3:1 for qualifying large text. Display rounding must not change the verdict. Test boundary fixtures just below, at, and above the limit.
5. If gradients, imagery, blend modes, or uncertain layering prevent a valid pair, use a validated rendering-aware method or return `needs_review`. Never sample antialiased edge pixels as the text color.
6. Repair approved semantic color tokens and retest their uses, themes, and states. This covers measured text only.

**TC-02 Labels and accessible names.** Discover interactive targets in the rendered accessibility tree. Obtain the computed name, role, state, and visible label. Check programmatic naming and label-in-name separately, using a conforming name algorithm. Document any string normalization. Fixtures include icon buttons, hidden labels, label overrides, duplicate ids, dynamic names, and controls inside frames.

**TC-03 Reflow and spacing.** Capture the task at the applicable reflow dimensions, including 320 CSS pixels wide. Exercise zoom in the specified browser. Apply the criterion's text-spacing overrides: line height 1.5, paragraph spacing 2, letter spacing 0.12, word spacing 0.16, all times font size. Inspect clipping, overlap, lost controls, and unexpected two-axis scrolling. Treat essential two-dimensional content separately. Retest after font loading and with long translations.

**TC-04 Focus and dialogs.** Start from a known focused element, open the widget by keyboard, record the focus transition, operate and close it, and verify a sensible return location. Test Tab, Shift+Tab, Escape where the component supports it, scroll, validation, and rerender. A modal's intentional contained focus must still permit exit. Inspect focus visibility and obscuration at every step.

**TC-05 Forms and consequential actions.** Use synthetic invalid, missing, and valid values. Check labels, instructions, required-state communication, error identification, associations, and correction guidance. Verify preservation of safe input, review and correction for consequential actions, outcomes, and duplicate handling. A visible error does not prove an announcement. Never submit real applications or payments.

**TC-06 Institutional links and notices.** Resolve the rule's agency, site, and page scope before looking for content. Check availability, discoverability, and destination identity; follow only allowed redirects. HTTP 200 or a matching phrase does not prove the required content. Distinguish network failure from a genuinely missing destination.

**TC-07 Collection and telemetry.** Build a field and event inventory from source and runtime, including hidden fields, URL parameters, storage, third-party requests, and error logging. Link each to purpose, authority, required or optional status, recipient, retention, and notice. A missing data-flow record is an evidence gap. Never export real payloads to models.

**TC-08 HTTPS and deployment.** Check authorized production origins, redirect chains, certificates, and response headers across representative responses. Report the observation time and environment. Source configuration alone cannot establish production behavior. A missing recommended header is not automatically a legal violation.

**TC-09 Documents and media handoffs.** Inventory file types, versions, page counts, language, forms, signatures, and task purpose. For PDFs inspect tagging, reading order, headings, table relationships, field labels, alternatives, and keyboard operation. OCR text alone does not establish accessibility. Never remove the authoritative record without owner review.

**TC-10 Content accuracy and task focus.** Compare claims, eligibility language, deadlines, fees, and required steps with approved program sources. Separate factual discrepancy from readability. Require cited excerpts and abstention where source facts are missing. A model cannot invent eligibility advice.

**TC-11 Session and recovery.** With approved accounts test invalid login, password-manager and paste behavior, factor choice, timeout, expiration, back-navigation, recovery, and logout. Confirm recovery does not bypass security and an interrupted task does not silently lose data. Record tradeoffs for the security and product owner.

**TC-12 Search and navigation.** Build a versioned query set from confirmed top tasks, including synonyms, misspellings, ambiguous queries, and no-result cases. Assess whether current content appears with descriptive context and usable filters. Test keyboard, mobile, and back-navigation. Measure against labeled expected results.

### Obsolescence and baseline mapping fields

WCAG 2.2 removed 4.1.1 Parsing as obsolete. The rule keeps its 2.0 and 2.1 mappings, and its 2.2 mapping is advisory: a 2.2-only target must not fail it.

Each WCAG 2.0 rule carries `baseline_test_id`, the number (01 to 24) of the primary ICT Testing Baseline for Web v3.1 test that covers it. Tests that also touch the criterion are listed in `extensions.baseline_related_test_ids`. Baseline test 03 Non-Interference and test 20 Conforming Alternate Version are aggregate tests and are never a primary mapping. The Baseline covers WCAG 2.0 only, so WCAG 2.1 and 2.2 rules have no `baseline_test_id`. `wcag_mappings` records which versions contain each criterion; see `applicability.md` for the three-mapping policy.

## 4. Coverage and sampling

Report separate counts for selected obligations, applicable obligations, unresolved applicability, pending obligations, required check instances, executed instances, reviewed instances, and unsupported obligations. Report counts with denominators, never a single blended score.

A check instance is one rule, one target, one context. Reuse across instances needs an equivalence argument covering markup, computed style, state behavior, content dependencies, and deployment identity. Sampling includes common templates, unique features, high-consequence tasks, vendors, documents, and complete selected processes, plus a random sample with recorded method and seed. Never claim a sampled whole product is fully conformant from the sample. WCAG-EM is an informative method, not a source of requirements.
