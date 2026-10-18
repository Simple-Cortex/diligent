//! `SRC-VIEWPORT-ZOOM` (ACT b4f0c3, informs A11Y-1.4.4).
//!
//! Decides: a `meta[name=viewport]` that sets `user-scalable` or
//! `maximum-scale` does not block zoom. `user-scalable=no`, or a numeric value
//! under one, fails. A `maximum-scale` under two fails, and the keyword `yes`
//! counts as one. A negative `maximum-scale` is ignored, as browsers do.
//!
//! A viewport meta with neither key is `not_tested`, as ACT treats it as
//! inapplicable.
//!
//! Does not decide: whether text actually resizes to 200 percent, or zoom
//! limits imposed by CSS or script. Spaces around `=` in the content string
//! are not supported.

use scraper::ElementRef;
use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-VIEWPORT-ZOOM",
    act: &["b4f0c3"],
    informs: &["A11Y-1.4.4"],
    summary: "Viewport meta does not disable zoom or cap scale below 2",
    absent_reason: "No meta viewport sets user-scalable or maximum-scale.",
};

const EXPECTED: &str = "viewport does not set user-scalable=no or maximum-scale below 2";

/// Parsed zoom-relevant viewport settings.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Zoom {
    pub user_scalable: Option<String>,
    pub maximum_scale: Option<String>,
}

pub(crate) fn parse_viewport(content: &str, acc: &mut Zoom) {
    for token in content.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
        let Some((k, v)) = token.split_once('=') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim().to_ascii_lowercase();
        match k.as_str() {
            "user-scalable" => acc.user_scalable = Some(v),
            "maximum-scale" => acc.maximum_scale = Some(v),
            _ => {}
        }
    }
}

/// Reasons zoom is restricted; empty when it is not.
pub(crate) fn restrictions(z: &Zoom) -> Vec<&'static str> {
    let mut out = Vec::new();
    if let Some(v) = &z.user_scalable {
        let disabled = v == "no" || v.parse::<f64>().is_ok_and(|n| n.abs() < 1.0);
        if disabled {
            out.push("user-scalable disables zoom");
        }
    }
    if let Some(v) = &z.maximum_scale {
        let scale = match v.as_str() {
            "yes" => Some(1.0),
            other => other.parse::<f64>().ok(),
        };
        if let Some(n) = scale {
            if (0.0..2.0).contains(&n) {
                out.push("maximum-scale is below 2");
            }
        }
    }
    out
}

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    let metas: Vec<ElementRef<'a>> = ctx
        .elements
        .iter()
        .copied()
        .filter(|e| {
            dom::is_html(*e)
                && dom::tag(*e) == "meta"
                && dom::attr(*e, "name").is_some_and(|n| n.trim().eq_ignore_ascii_case("viewport"))
        })
        .collect();
    let mut zoom = Zoom::default();
    let mut anchor: Option<ElementRef<'a>> = None;
    for meta in &metas {
        let before = (zoom.user_scalable.clone(), zoom.maximum_scale.clone());
        if let Some(content) = dom::attr(*meta, "content") {
            parse_viewport(content, &mut zoom);
        }
        if (zoom.user_scalable.clone(), zoom.maximum_scale.clone()) != before {
            anchor = Some(*meta);
        }
    }
    let Some(anchor) = anchor else {
        return;
    };
    let reasons = restrictions(&zoom);
    let state = if reasons.is_empty() {
        State::Pass
    } else {
        State::Fail
    };
    let observed = json!({
        "user_scalable": zoom.user_scalable,
        "maximum_scale": zoom.maximum_scale,
        "restrictions": reasons,
        "content": dom::attr(anchor, "content").map(|c| dom::truncate_chars(c, 200)),
    });
    em.emit(&INFO, anchor, Outcome::new(state, EXPECTED, observed));
}

#[cfg(test)]
mod tests {
    use super::{parse_viewport, restrictions, Zoom};

    fn judge(content: &str) -> usize {
        let mut z = Zoom::default();
        parse_viewport(content, &mut z);
        restrictions(&z).len()
    }

    #[test]
    fn matches_act_examples() {
        assert_eq!(judge("user-scalable=yes"), 0);
        assert_eq!(judge("maximum-scale=2.0"), 0);
        assert_eq!(judge("maximum-scale=-1"), 0);
        assert_eq!(judge("user-scalable=no"), 1);
        assert_eq!(
            judge("user-scalable=yes, initial-scale=0.8, maximum-scale=1.5"),
            1
        );
        assert_eq!(judge("maximum-scale=1.0"), 1);
        assert_eq!(judge("maximum-scale=yes"), 1);
    }
}
