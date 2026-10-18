//! Lab performance signals from a `perf_metrics` capture.
//!
//! Every value is a lab reading: one unthrottled headless load of one page.
//! None is a field measurement, and none of them is a pass or fail. LCP and
//! CLS are read against the web.dev Core Web Vitals thresholds; the rest are
//! numbers.

use diligent_capture::{PerfMetrics, Viewport};
use serde_json::{json, Value};

use super::model::{Signal, SignalState};
use super::round_to;
use crate::checks::Ctx;
use crate::model::EvidenceKey;

const LAB: &str = "Lab value from one unthrottled headless load; it is not a field measurement and will differ from what visitors see.";

fn find<'a>(ctx: &Ctx<'a>) -> Option<(Viewport, &'static str, &'a PerfMetrics)> {
    let preferred = ctx.primary.as_ref().map(|v| v.viewport);
    let pick = preferred
        .and_then(|vp| ctx.set.perf(vp).map(|p| (vp, p)))
        .or_else(|| {
            ctx.set
                .viewports()
                .into_iter()
                .find_map(|vp| ctx.set.perf(vp).map(|p| (vp, p)))
        })?;
    Some((pick.0, "perf_metrics", pick.1))
}

#[allow(clippy::too_many_arguments)]
fn reading(
    key: &'static str,
    value: Option<f64>,
    unit: &'static str,
    lead_above: Option<f64>,
    within_at_most: Option<f64>,
    vp: Viewport,
    kind: &'static str,
    p: &PerfMetrics,
) -> Signal {
    let evidence = EvidenceKey {
        kind,
        viewport: vp,
        media: None,
    };
    let mut s = match value {
        None => Signal::new(
            key,
            SignalState::NotTested,
            Value::Null,
            "the lab run did not report this value",
        ),
        Some(x) => {
            let state = match (lead_above, within_at_most) {
                (Some(l), _) if x > l => SignalState::Lead,
                (_, Some(w)) if x <= w => SignalState::WithinPractice,
                (None, None) => SignalState::Informational,
                _ => SignalState::Informational,
            };
            Signal::new(
                key,
                state,
                json!(round_to(x, 4)),
                format!("{} {unit}", round_to(x, 4)),
            )
        }
    };
    s = s.unit(unit).limit(LAB).with(evidence);
    for n in p.notes.iter().take(3) {
        s = s.limit(format!("collector note: {n}"));
    }
    s.viewport = Some(vp);
    s.media = Some("baseline".to_string());
    s
}

/// Runs the lab performance signals.
pub fn run(ctx: &Ctx<'_>) -> Vec<Signal> {
    let Some((vp, kind, p)) = find(ctx) else {
        return Vec::new();
    };
    let mut out = vec![
        reading(
            "perf.lcp_ms",
            p.lcp_ms,
            "ms",
            Some(4000.0),
            Some(2500.0),
            vp,
            kind,
            p,
        ),
        reading(
            "perf.cls",
            p.cls,
            "score",
            Some(0.25),
            Some(0.1),
            vp,
            kind,
            p,
        ),
        reading("perf.fcp_ms", p.fcp_ms, "ms", None, None, vp, kind, p),
        reading("perf.ttfb_ms", p.ttfb_ms, "ms", None, None, vp, kind, p),
        reading(
            "perf.transfer_bytes",
            p.transfer_bytes.map(|b| b as f64),
            "bytes",
            None,
            None,
            vp,
            kind,
            p,
        ),
        reading(
            "perf.resource_count",
            p.resource_count.map(f64::from),
            "resources",
            None,
            None,
            vp,
            kind,
            p,
        ),
        reading(
            "perf.long_tasks_ms",
            p.long_tasks_ms,
            "ms",
            None,
            None,
            vp,
            kind,
            p,
        ),
    ];
    if let (Some(lcp), Some(el)) = (p.lcp_ms, p.lcp_element.as_ref()) {
        let _ = lcp;
        if let Some(first) = out.first_mut() {
            first.instance = el.clone();
        }
    }
    out.push(
        Signal::new(
            "perf.inp_ms",
            SignalState::NotTested,
            Value::Null,
            "INP needs real user interaction",
        )
        .unit("ms")
        .limit("INP needs real user interaction; import from CrUX is not implemented.")
        .with(EvidenceKey {
            kind,
            viewport: vp,
            media: None,
        }),
    );
    out
}
