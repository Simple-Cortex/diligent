//! `BRW-ARIA-ATTR-VALID` (WCAG 4.1.2; ACT 5c01ea, 6a7281, 5f99a7, 4e8ab6, 674b10).
//!
//! Decides, for each element that has a `role` or any `aria-*` attribute
//! (hidden or not: invalid markup is invalid either way; an element whose only
//! ARIA is a `role` equal to its implicit role is skipped), against the embedded WAI-ARIA 1.2 tables in [`crate::aria12`]:
//!
//! * `fail` when a `role` has no valid token, or names an abstract role;
//! * `fail` for an `aria-*` attribute that does not exist;
//! * `fail` for an attribute the computed role does not support;
//! * `fail` for a required state the role needs and the element lacks;
//! * `fail` for a value of the wrong type or an id reference to nothing;
//! * `pass` otherwise.
//!
//! The computed role is the first valid token of `role`, else the implicit
//! role of the element. Where the implicit role depends on context the module
//! does not model, the support and required-state tests are skipped for that
//! element. Does not decide: required owned elements or parent context, or
//! prohibited naming on some roles.

use serde_json::json;

use super::{finding, Ctx};
use crate::aria12::{
    attr_def, first_valid_role, implicit_role, is_abstract_role, role_def, role_supports,
    validate_value, RoleDef,
};
use crate::capture_view::CaptureView;
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-ARIA-ATTR-VALID",
    act: &["5c01ea", "6a7281", "5f99a7", "4e8ab6", "674b10"],
    informs: &["A11Y-4.1.2"],
    summary:
        "ARIA roles, states and properties exist, are allowed on the element, and have valid values",
    channel: "browser",
    never_fails: false,
    absent_reason: "No element has a role or an aria-* attribute.",
};

const LIMIT: &str = "Checked against an embedded WAI-ARIA 1.2 table. Required owned elements, required context roles and prohibited naming are not checked. Where the implicit role depends on context, attribute support is not checked.";

/// True when the host language already supplies the state or property.
fn natively_provided(v: &CaptureView<'_>, el: usize, prop: &str) -> bool {
    let tag = v.tag(el);
    let kind = v.attr(el, "type").unwrap_or("").to_ascii_lowercase();
    match prop {
        "aria-checked" => tag == "input" && matches!(kind.as_str(), "checkbox" | "radio"),
        "aria-valuenow" => (tag == "input" && kind == "range") || tag == "meter",
        "aria-selected" => tag == "option",
        "aria-expanded" => matches!(tag, "select" | "input" | "textarea"),
        _ => false,
    }
}

/// The computed role of an element: the first valid `role` token, else the implicit role.
pub fn computed_role(v: &CaptureView<'_>, el: usize) -> Option<&'static RoleDef> {
    if let Some(def) = v.attr(el, "role").and_then(first_valid_role) {
        return Some(def);
    }
    let lookup = |name: &str| v.attr(el, name).map(str::to_string);
    implicit_role(v.tag(el), &lookup).and_then(role_def)
}

fn evaluate(v: &CaptureView<'_>, el: usize) -> Option<Finding> {
    let role_attr = v.attr(el, "role").map(str::trim).filter(|r| !r.is_empty());
    let aria: Vec<(String, String)> = v
        .node(el)
        .attributes
        .iter()
        .filter(|(k, _)| k.len() > 5 && k.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("aria-")))
        .map(|(k, val)| (k.to_ascii_lowercase(), val.clone()))
        .collect();
    if role_attr.is_none() && aria.is_empty() {
        return None;
    }
    if aria.is_empty() {
        // A role that repeats the element's own implicit role says nothing to validate.
        let lookup = |name: &str| v.attr(el, name).map(str::to_string);
        let implicit = implicit_role(v.tag(el), &lookup);
        let first = role_attr.and_then(|r| r.split_ascii_whitespace().next());
        if implicit.is_some() && first.map(str::to_ascii_lowercase).as_deref() == implicit {
            return None;
        }
    }
    let mut problems: Vec<String> = Vec::new();

    let explicit = role_attr.and_then(first_valid_role);
    if let Some(role) = role_attr {
        if explicit.is_none() {
            if role.split_ascii_whitespace().any(is_abstract_role) {
                problems.push(format!("role `{role}` is an abstract role"));
            } else {
                problems.push(format!("role `{role}` is not a WAI-ARIA 1.2 role"));
            }
        }
    }
    let computed = computed_role(v, el);

    let id_exists = |id: &str| !v.by_id(id).is_empty();
    for (name, value) in &aria {
        let Some(def) = attr_def(name) else {
            problems.push(format!("`{name}` is not a WAI-ARIA 1.2 attribute"));
            continue;
        };
        if let Some(role) = computed {
            if !role_supports(role, name) {
                problems.push(format!("`{name}` is not supported on role `{}`", role.name));
            }
        }
        // A collapsed widget may point at content that is not in the page yet.
        let skip_refs = match name.as_str() {
            "aria-controls" => v
                .attr(el, "aria-expanded")
                .is_some_and(|x| x.trim().eq_ignore_ascii_case("false")),
            "aria-errormessage" => !v.attr(el, "aria-invalid").is_some_and(|x| {
                matches!(
                    x.trim().to_ascii_lowercase().as_str(),
                    "true" | "grammar" | "spelling"
                )
            }),
            _ => false,
        };
        let always = |_: &str| true;
        let result = if skip_refs {
            validate_value(value, &def.kind, &always)
        } else {
            validate_value(value, &def.kind, &id_exists)
        };
        if let Err(why) = result {
            problems.push(format!("`{name}`: {why}"));
        }
    }
    if let Some(role) = explicit {
        for required in role.required {
            let present = aria.iter().any(|(k, _)| k == required);
            if !present && !natively_provided(v, el, required) {
                problems.push(format!("role `{}` requires `{required}`", role.name));
            }
        }
    }

    // An expanded custom combobox must say which popup it controls.
    if explicit.is_some_and(|r| r.name == "combobox")
        && !matches!(v.tag(el), "input" | "select" | "textarea")
        && v.attr(el, "aria-expanded")
            .is_some_and(|x| x.trim().eq_ignore_ascii_case("true"))
        && !aria
            .iter()
            .any(|(k, _)| k == "aria-controls" || k == "aria-owns")
    {
        problems
            .push("role `combobox` with aria-expanded=true requires `aria-controls`".to_string());
    }

    let state = if problems.is_empty() {
        State::Pass
    } else {
        State::Fail
    };
    Some(
        finding(
            &INFO,
            v,
            v.selector(el),
            state,
            "valid, allowed ARIA with all required states",
            json!({
                "tag": v.tag(el),
                "role_attribute": role_attr,
                "computed_role": computed.map(|r| r.name),
                "aria_attributes": aria.iter().map(|(k, val)| format!("{k}={val}")).collect::<Vec<_>>(),
                "problems": problems,
            }),
        )
        .limit(LIMIT),
    )
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    v.elements().filter_map(|el| evaluate(v, el)).collect()
}
