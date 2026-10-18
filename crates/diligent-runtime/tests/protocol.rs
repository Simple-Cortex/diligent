//! Pure protocol tests. Always run; no browser needed.

use diligent_runtime::collect::{default_viewports, CollectPlan, MediaMode};
use diligent_runtime::protocol::*;

const REQUEST_LINE: &str = r#"{"protocol_version":"1.3.0","request_id":"r1","run_id":"run1","action":{"action":"navigate","url":"http://127.0.0.1:1/"},"environment":{"viewport":{"width":320,"height":568,"dpr":1.0},"media":{"reduced_motion":false,"forced_colors":false,"color_scheme":"light"}},"timeout_ms":30000,"limits":{"max_bytes":1024,"max_nodes":100}}"#;

#[test]
fn documented_example_request_parses() {
    let request: Request = serde_json::from_str(REQUEST_LINE).expect("example request");
    assert_eq!(request.protocol_version, PROTOCOL_VERSION);
    assert!(matches!(request.action, Action::Navigate { .. }));
    assert!(request.credentials_ref.is_none());
}

#[test]
fn action_names_are_a_closed_allowlist() {
    for bad in ["evaluate", "run_script", "shell", "click", "Navigate"] {
        let line = REQUEST_LINE.replace("\"navigate\"", &format!("\"{bad}\""));
        assert!(
            serde_json::from_str::<Request>(&line).is_err(),
            "{bad} must be rejected"
        );
    }
}

#[test]
fn unknown_fields_are_rejected_everywhere() {
    let line = REQUEST_LINE.replace(
        "\"timeout_ms\":30000",
        "\"timeout_ms\":30000,\"script\":\"1\"",
    );
    assert!(serde_json::from_str::<Request>(&line).is_err());
    let line = REQUEST_LINE.replace("\"max_nodes\":100", "\"max_nodes\":100,\"x\":1");
    assert!(serde_json::from_str::<Request>(&line).is_err());
}

#[test]
fn default_plan_matches_the_work_order() {
    let widths: Vec<u32> = default_viewports().iter().map(|v| v.width).collect();
    assert_eq!(widths, [320, 768, 1024, 1280]);
    let plan = CollectPlan::new("http://x/");
    let names: Vec<&str> = plan.media_modes.iter().map(|m| m.name()).collect();
    assert_eq!(
        names,
        ["baseline", "reduced-motion", "forced-colors", "dark"]
    );
    assert_eq!(plan.limits.max_nodes, 50_000);
    assert_eq!(plan.limits.max_bytes, 52_428_800);
}

#[test]
fn media_modes_map_to_emulation() {
    assert_eq!(MediaMode::Baseline.media(), Media::BASELINE);
    assert!(MediaMode::ReducedMotion.media().reduced_motion);
    assert!(MediaMode::ForcedColors.media().forced_colors);
    assert_eq!(MediaMode::Dark.media().color_scheme, ColorScheme::Dark);
    assert_eq!(
        MediaMode::parse("forced-colors"),
        Some(MediaMode::ForcedColors)
    );
    assert_eq!(MediaMode::parse("nope"), None);
}
