//! Form and flow signals: patterns that make a form easier or harder to complete.
//!
//! They run only when the page has a `form` that holds at least one visible
//! control. Each reads markup and the accessibility tree of one page state, so
//! multi-step patterns that need several pages (save and return, confirmation,
//! check your answers) are out of reach.

use std::collections::BTreeSet;

use serde_json::json;

use super::model::{Signal, SignalState};
use crate::capture_view::{normalize_space, CaptureView};
use crate::checks::fed_common::{class_contains, raw_text};
use crate::checks::Ctx;

const LIST_MAX: usize = 10;

fn input_type(v: &CaptureView<'_>, el: usize) -> String {
    v.attr(el, "type")
        .map(|t| t.trim().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Visible controls inside forms: inputs other than buttons and hidden ones, textareas and selects.
fn controls(v: &CaptureView<'_>) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    for form in v.elements().filter(|e| v.tag(*e) == "form") {
        for d in v.descendants(form) {
            if !v.is_element(d) || !seen.insert(d) {
                continue;
            }
            let ok = match v.tag(d) {
                "input" => !matches!(
                    input_type(v, d).as_str(),
                    "hidden" | "submit" | "button" | "reset" | "image"
                ),
                "textarea" | "select" => true,
                _ => false,
            };
            if ok && v.visible(d) {
                out.push(d);
            }
        }
    }
    out
}

/// Text of the labels of `el`: `label[for]`, a wrapping label, `aria-label` and `title`. Aria-hidden marks count.
fn label_text(v: &CaptureView<'_>, el: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(id) = v.attr(el, "id").filter(|i| !i.is_empty()) {
        for l in v
            .elements()
            .filter(|e| v.tag(*e) == "label" && v.attr(*e, "for") == Some(id))
        {
            parts.push(raw_text(v, l));
        }
    }
    for a in v.ancestors(el) {
        if v.is_element(a) && v.tag(a) == "label" {
            parts.push(raw_text(v, a));
        }
    }
    for attr in ["aria-label", "title"] {
        if let Some(t) = v.attr(el, attr) {
            parts.push(t.to_string());
        }
    }
    normalize_space(&parts.join(" "))
}

fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn selectors(v: &CaptureView<'_>, list: &[usize]) -> Vec<String> {
    list.iter().take(LIST_MAX).map(|e| v.selector(*e)).collect()
}

fn count_signal(
    v: &CaptureView<'_>,
    key: &'static str,
    list: &[usize],
    total: usize,
    what: &str,
    limit: &str,
) -> Signal {
    let mut s = Signal::new(
        key,
        if list.is_empty() {
            SignalState::WithinPractice
        } else {
            SignalState::Lead
        },
        json!({ "count": list.len(), "of": total, "instances": selectors(v, list) }),
        format!("{} of {total} {what}", list.len()),
    )
    .unit("inputs")
    .limit(limit)
    .on(v);
    if let Some(first) = list.first() {
        s = s.at_instance(v.selector(*first));
    }
    s
}

fn placeholder_only(v: &CaptureView<'_>, inputs: &[usize]) -> Signal {
    let bad: Vec<usize> = inputs
        .iter()
        .copied()
        .filter(|e| {
            if let Some(n) = v.ax_node(*e) {
                return n.name_source.as_deref() == Some("placeholder")
                    && n.name.as_deref().is_some_and(|s| !s.trim().is_empty());
            }
            v.attr(*e, "placeholder")
                .is_some_and(|p| !p.trim().is_empty())
                && label_text(v, *e).is_empty()
        })
        .collect();
    count_signal(
        v,
        "form.placeholder_only_labels",
        &bad,
        inputs.len(),
        "inputs are named only by their placeholder",
        "Placeholder-only names come from the accessibility tree when it was captured, else from markup.",
    )
}

fn identity_word(text: &str) -> bool {
    let t = tokens(text);
    let has = |w: &str| t.iter().any(|x| x == w);
    if [
        "email",
        "phone",
        "telephone",
        "tel",
        "address",
        "postal",
        "zip",
        "zipcode",
        "firstname",
        "lastname",
        "fullname",
        "fname",
        "lname",
    ]
    .iter()
    .any(|w| has(w))
    {
        return true;
    }
    t.windows(2).any(|w| {
        w[1] == "name"
            && matches!(
                w[0].as_str(),
                "first" | "last" | "full" | "given" | "family" | "middle" | "your"
            )
    })
}

fn autocomplete_missing(v: &CaptureView<'_>, inputs: &[usize]) -> Option<Signal> {
    let candidates: Vec<usize> = inputs
        .iter()
        .copied()
        .filter(|e| {
            v.tag(*e) == "input"
                && !matches!(
                    input_type(v, *e).as_str(),
                    "password" | "file" | "checkbox" | "radio" | "range" | "color"
                )
        })
        .filter(|e| {
            let attrs = format!(
                "{} {}",
                v.attr(*e, "name").unwrap_or(""),
                v.attr(*e, "id").unwrap_or("")
            );
            let exact_name = v
                .attr(*e, "name")
                .is_some_and(|n| n.trim().eq_ignore_ascii_case("name"));
            exact_name
                || identity_word(&attrs)
                || identity_word(&label_text(v, *e))
                || matches!(input_type(v, *e).as_str(), "email" | "tel")
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let bad: Vec<usize> = candidates
        .iter()
        .copied()
        .filter(|e| !v.has_attr(*e, "autocomplete"))
        .collect();
    Some(count_signal(
        v,
        "form.autocomplete_missing_identity",
        &bad,
        candidates.len(),
        "identity inputs have no autocomplete attribute",
        "Identity inputs are found by type, name, id and label words; a field named for something else may be missed or caught wrongly.",
    ))
}

fn required_marking(v: &CaptureView<'_>, inputs: &[usize]) -> Option<Signal> {
    let required: Vec<usize> = inputs
        .iter()
        .copied()
        .filter(|e| {
            v.has_attr(*e, "required")
                || v.attr(*e, "aria-required")
                    .is_some_and(|a| a.trim().eq_ignore_ascii_case("true"))
        })
        .collect();
    if required.is_empty() {
        return None;
    }
    let page = v
        .elements()
        .find(|e| v.tag(*e) == "body")
        .map(|b| raw_text(v, b).to_lowercase())
        .unwrap_or_default();
    let all_required = [
        "all fields are required",
        "all fields required",
        "all fields must be",
    ]
    .iter()
    .any(|p| page.contains(p));
    let unmarked: Vec<usize> = if all_required {
        Vec::new()
    } else {
        required
            .iter()
            .copied()
            .filter(|e| {
                let label = label_text(v, *e).to_lowercase();
                !(label.contains('*') || label.contains("required"))
            })
            .collect()
    };
    Some(count_signal(
        v,
        "form.required_marking",
        &unmarked,
        required.len(),
        "required inputs have no visible required marking",
        "A marking is an asterisk or the word required in the label, or page text saying all fields are required. Marks drawn by CSS are not seen.",
    ))
}

fn error_summary(v: &CaptureView<'_>, inputs: &[usize]) -> Option<Signal> {
    let invalid: Vec<usize> = v
        .elements()
        .filter(|e| {
            v.attr(*e, "aria-invalid")
                .is_some_and(|a| a.trim().eq_ignore_ascii_case("true"))
        })
        .collect();
    let _ = inputs;
    if invalid.is_empty() {
        return None;
    }
    let containers: Vec<usize> = v
        .elements()
        .filter(|e| {
            v.visible(*e)
                && (v
                    .attr(*e, "role")
                    .is_some_and(|r| r.split_ascii_whitespace().next() == Some("alert"))
                    || class_contains(v, *e, "usa-alert--error")
                    || class_contains(v, *e, "error-summary"))
        })
        .collect();
    let linked = containers.iter().any(|c| {
        v.descendants(*c).into_iter().any(|a| {
            v.is_element(a)
                && v.tag(a) == "a"
                && v.attr(a, "href")
                    .and_then(|h| h.trim().strip_prefix('#'))
                    .is_some_and(|id| {
                        v.by_id(id).iter().any(|t| {
                            invalid
                                .iter()
                                .any(|i| t == i || v.is_ancestor(*t, *i) || v.is_ancestor(*i, *t))
                        })
                    })
        })
    });
    Some(
        Signal::new(
            "form.error_summary_pattern",
            if linked { SignalState::WithinPractice } else { SignalState::Lead },
            json!({ "invalid_fields": invalid.len(), "summary_containers": containers.len(), "links_to_invalid_fields": linked }),
            if linked {
                format!("{} invalid fields and an error summary that links to them", invalid.len())
            } else {
                format!("{} invalid fields and no error summary that links to them", invalid.len())
            },
        )
        .unit("fields")
        .at_instance(v.selector(invalid[0]))
        .limit("Only a page state that already shows errors can be read; the error state itself is not provoked.")
        .on(v),
    )
}

/// True when `text` holds "step N of M".
fn has_step_of(text: &str) -> bool {
    let lower = text.to_lowercase();
    let mut rest = lower.as_str();
    while let Some(i) = rest.find("step ") {
        let after = &rest[i + 5..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() && after[digits.len()..].starts_with(" of ") {
            let tail = &after[digits.len() + 4..];
            if tail.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return true;
            }
        }
        rest = after;
    }
    false
}

fn step_indicator(v: &CaptureView<'_>, inputs: &[usize]) -> Signal {
    let by_class = v
        .elements()
        .any(|e| class_contains(v, e, "usa-step-indicator"));
    let by_aria = v.elements().any(|e| {
        v.attr(e, "aria-current")
            .is_some_and(|c| c.trim().eq_ignore_ascii_case("step"))
    });
    let by_text = v
        .elements()
        .find(|e| v.tag(*e) == "body")
        .is_some_and(|b| has_step_of(&v.visible_text(b)));
    let present = by_class || by_aria || by_text;
    let state = if present {
        SignalState::WithinPractice
    } else if inputs.len() >= 8 {
        SignalState::Lead
    } else {
        SignalState::Informational
    };
    Signal::new(
        "form.step_indicator",
        state,
        json!({ "present": present, "inputs": inputs.len() }),
        if present { "the page shows a step indicator".to_string() } else { format!("no step indicator on a form of {} inputs", inputs.len()) },
    )
    .unit("inputs")
    .limit("One page state: a long form split across pages shows its indicator on each page, and a single page cannot show the split.")
    .on(v)
}

fn memorable_dates(v: &CaptureView<'_>, inputs: &[usize]) -> Option<Signal> {
    let single: Vec<usize> = inputs
        .iter()
        .copied()
        .filter(|e| input_type(v, *e) == "date")
        .collect();
    let mut day = false;
    let mut month = false;
    let mut year = false;
    for e in inputs.iter().copied().filter(|e| {
        v.tag(*e) == "input" && !matches!(input_type(v, *e).as_str(), "date" | "checkbox" | "radio")
    }) {
        let words = tokens(&format!(
            "{} {} {}",
            label_text(v, e),
            v.attr(e, "name").unwrap_or(""),
            v.attr(e, "id").unwrap_or("")
        ));
        day |= words.iter().any(|w| w == "day");
        month |= words.iter().any(|w| w == "month");
        year |= words.iter().any(|w| w == "year");
    }
    let three = day && month && year;
    if single.is_empty() && !three {
        return None;
    }
    let lead = !single.is_empty();
    let mut s = Signal::new(
        "form.memorable_date_inputs",
        if lead { SignalState::Lead } else { SignalState::WithinPractice },
        json!({ "date_inputs": single.len(), "day_month_year_fields": three }),
        if lead { format!("{} single date input fields", single.len()) } else { "the date is asked as day, month and year fields".to_string() },
    )
    .unit("inputs")
    .limit("A single date input suits a date picked from a calendar; the pattern applies to dates a person knows by heart, which this cannot tell.")
    .on(v);
    if let Some(first) = single.first() {
        s = s.at_instance(v.selector(*first));
    }
    Some(s)
}

fn guidance_word(text: &str) -> bool {
    tokens(text).iter().any(|t| {
        matches!(
            t.as_str(),
            "mb" | "kb"
                | "gb"
                | "size"
                | "format"
                | "formats"
                | "pdf"
                | "jpg"
                | "jpeg"
                | "png"
                | "gif"
                | "doc"
                | "docx"
                | "accepted"
                | "maximum"
                | "limit"
                | "megabytes"
        ) || (t.len() > 2
            && t.starts_with(|c: char| c.is_ascii_digit())
            && (t.ends_with("mb") || t.ends_with("kb") || t.ends_with("gb")))
    })
}

fn upload_guidance(v: &CaptureView<'_>, inputs: &[usize]) -> Option<Signal> {
    let files: Vec<usize> = inputs
        .iter()
        .copied()
        .filter(|e| v.tag(*e) == "input" && input_type(v, *e) == "file")
        .collect();
    if files.is_empty() {
        return None;
    }
    let bad: Vec<usize> = files
        .iter()
        .copied()
        .filter(|e| {
            let mut text = vec![label_text(v, *e)];
            if let Some(ids) = v.attr(*e, "aria-describedby") {
                for id in ids.split_ascii_whitespace() {
                    text.extend(v.by_id(id).iter().map(|t| raw_text(v, *t)));
                }
            }
            for anchor in [Some(*e), v.parent_element(*e)].into_iter().flatten() {
                if let Some(p) = v.parent(anchor) {
                    let sibs = v.children(p);
                    if let Some(pos) = sibs.iter().position(|s| *s == anchor) {
                        text.extend(
                            sibs[pos + 1..]
                                .iter()
                                .filter(|s| v.is_element(**s))
                                .take(2)
                                .map(|s| raw_text(v, *s)),
                        );
                    }
                }
            }
            !guidance_word(&text.join(" "))
        })
        .collect();
    Some(count_signal(
        v,
        "form.upload_guidance",
        &bad,
        files.len(),
        "file inputs have no nearby size or format guidance",
        "Guidance is text in the label, its description or the next two siblings that names a size or a file format; the accept attribute alone does not count.",
    ))
}

/// Runs the form signals.
pub fn run(ctx: &Ctx<'_>) -> Vec<Signal> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let inputs = controls(v);
    if inputs.is_empty() {
        return Vec::new();
    }
    let mut out = vec![placeholder_only(v, &inputs)];
    out.extend(autocomplete_missing(v, &inputs));
    out.extend(required_marking(v, &inputs));
    out.extend(error_summary(v, &inputs));
    out.push(step_indicator(v, &inputs));
    out.extend(memorable_dates(v, &inputs));
    out.extend(upload_guidance(v, &inputs));
    out
}
