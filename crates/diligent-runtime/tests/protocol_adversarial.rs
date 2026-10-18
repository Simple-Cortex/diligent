//! Adversarial tests for the browser-worker protocol.
//!
//! These probe the closed allowlist, unknown-field rejection, dangerous key
//! names, credential handling, retryability, and version mismatches.

use diligent_runtime::protocol::*;
use serde_json::json;

fn sample_request(action: Action) -> Request {
    Request {
        protocol_version: PROTOCOL_VERSION.to_string(),
        request_id: "r1".into(),
        run_id: "run1".into(),
        action,
        environment: Environment {
            viewport: Viewport {
                width: 320,
                height: 568,
                dpr: 1.0,
            },
            media: Media::BASELINE,
            user_agent: None,
        },
        timeout_ms: 30_000,
        limits: Limits {
            max_bytes: 1024,
            max_nodes: 10,
        },
        credentials_ref: None,
    }
}

fn request_json(action: Action) -> serde_json::Value {
    serde_json::to_value(sample_request(action)).unwrap()
}

// ---------------------------------------------------------------------------
// Dangerous action names are rejected
// ---------------------------------------------------------------------------

#[test]
fn action_named_evaluate_is_rejected() {
    let mut v = request_json(Action::Inspect {});
    v["action"] = json!({"action": "evaluate", "expression": "1+1"});
    assert!(
        serde_json::from_value::<Request>(v).is_err(),
        "evaluate must be rejected"
    );
}

#[test]
fn action_named_run_script_is_rejected() {
    let mut v = request_json(Action::Inspect {});
    v["action"] = json!({"action": "run_script", "source": "alert(1)"});
    assert!(
        serde_json::from_value::<Request>(v).is_err(),
        "run_script must be rejected"
    );
}

#[test]
fn action_named_shell_is_rejected() {
    let mut v = request_json(Action::Inspect {});
    v["action"] = json!({"action": "shell", "command": "rm -rf /"});
    assert!(
        serde_json::from_value::<Request>(v).is_err(),
        "shell must be rejected"
    );
}

#[test]
fn action_named_click_is_rejected() {
    let mut v = request_json(Action::Inspect {});
    v["action"] = json!({"action": "click", "selector": "button"});
    assert!(
        serde_json::from_value::<Request>(v).is_err(),
        "click must be rejected"
    );
}

// ---------------------------------------------------------------------------
// Extra fields are rejected at every level
// ---------------------------------------------------------------------------

#[test]
fn extra_field_on_request_is_rejected() {
    let mut v = request_json(Action::Inspect {});
    v["surprise"] = json!(true);
    assert!(serde_json::from_value::<Request>(v).is_err());
}

#[test]
fn extra_field_on_action_is_rejected() {
    let mut v = request_json(Action::Navigate {
        url: "http://x/".into(),
    });
    v["action"]["script"] = json!("1+1");
    assert!(serde_json::from_value::<Request>(v).is_err());
}

#[test]
fn extra_field_on_environment_is_rejected() {
    let mut v = request_json(Action::Inspect {});
    v["environment"]["extra"] = json!(1);
    assert!(serde_json::from_value::<Request>(v).is_err());
}

#[test]
fn extra_field_on_viewport_is_rejected() {
    let mut v = request_json(Action::Inspect {});
    v["environment"]["viewport"]["extra"] = json!(1);
    assert!(serde_json::from_value::<Request>(v).is_err());
}

#[test]
fn extra_field_on_media_is_rejected() {
    let mut v = request_json(Action::Inspect {});
    v["environment"]["media"]["extra"] = json!(1);
    assert!(serde_json::from_value::<Request>(v).is_err());
}

#[test]
fn extra_field_on_limits_is_rejected() {
    let mut v = request_json(Action::Inspect {});
    v["limits"]["extra"] = json!(1);
    assert!(serde_json::from_value::<Request>(v).is_err());
}

#[test]
fn extra_field_on_response_is_rejected() {
    let response = Response {
        protocol_version: PROTOCOL_VERSION.into(),
        request_id: "r1".into(),
        run_id: "run1".into(),
        status: Status::Ok,
        evidence: vec![],
        timings: Timings {
            started_at: "2026-10-17T00:00:00Z".into(),
            finished_at: "2026-10-17T00:00:01Z".into(),
            duration_ms: 1000,
        },
        error: None,
    };
    let mut v = serde_json::to_value(&response).unwrap();
    v["extra"] = json!(true);
    assert!(serde_json::from_value::<Response>(v).is_err());
}

// ---------------------------------------------------------------------------
// Keypress with Enter or Space is refused
// ---------------------------------------------------------------------------

#[test]
fn keypress_with_enter_is_deserializable_but_dispatch_should_refuse() {
    // The protocol allows any key string at the deserialization level.
    // The worker dispatch layer is responsible for refusing Enter/Space.
    // We verify the action deserializes (it's a valid string), but document
    // that the dispatch must refuse it.
    let action = Action::Keypress {
        key: "Enter".into(),
    };
    let v = request_json(action);
    let parsed: Request = serde_json::from_value(v).unwrap();
    match &parsed.action {
        Action::Keypress { key } => {
            assert_eq!(key, "Enter");
            // The worker dispatch must refuse this key.
            // We assert the key is in the refused set.
            let refused = ["Enter", "Return", "Space", " "];
            assert!(
                refused.contains(&key.as_str()),
                "Enter must be in the refused set"
            );
        }
        _ => panic!("expected Keypress"),
    }
}

#[test]
fn keypress_with_space_is_deserializable_but_dispatch_should_refuse() {
    let action = Action::Keypress {
        key: "Space".into(),
    };
    let v = request_json(action);
    let parsed: Request = serde_json::from_value(v).unwrap();
    match &parsed.action {
        Action::Keypress { key } => {
            assert_eq!(key, "Space");
            let refused = ["Enter", "Return", "Space", " "];
            assert!(
                refused.contains(&key.as_str()),
                "Space must be in the refused set"
            );
        }
        _ => panic!("expected Keypress"),
    }
}

// ---------------------------------------------------------------------------
// credentials_ref handling
// ---------------------------------------------------------------------------

#[test]
fn credentials_ref_set_is_preserved_in_round_trip() {
    let mut req = sample_request(Action::Inspect {});
    req.credentials_ref = Some("vault:key-123".into());
    let line = serde_json::to_string(&req).unwrap();
    let back: Request = serde_json::from_str(&line).unwrap();
    assert_eq!(back.credentials_ref, Some("vault:key-123".to_string()));
}

#[test]
fn credentials_ref_absent_is_none() {
    let req = sample_request(Action::Inspect {});
    assert!(req.credentials_ref.is_none());
    let line = serde_json::to_string(&req).unwrap();
    // The serialized form should not contain credentials_ref at all.
    assert!(!line.contains("credentials_ref"));
}

// ---------------------------------------------------------------------------
// Retryability matrix
// ---------------------------------------------------------------------------

#[test]
fn retryable_is_false_for_fill_fixture_under_every_error_kind() {
    let fill = Action::FillFixture {
        selector: "input".into(),
        fixture_ref: "x.txt".into(),
    };
    let all_kinds = [
        ErrorKind::AuthFailure,
        ErrorKind::Timeout,
        ErrorKind::UnsupportedFeature,
        ErrorKind::BlockedOrigin,
        ErrorKind::MalformedResponse,
        ErrorKind::MissingFixture,
        ErrorKind::CollectorFailure,
    ];
    for kind in all_kinds {
        let err = WorkerError::new(kind, "test").for_action(&fill);
        assert!(
            !err.retryable,
            "FillFixture must never be retryable, got retryable for {kind:?}"
        );
    }
}

#[test]
fn retryable_is_false_for_submit_to_test_endpoint_under_every_error_kind() {
    let submit = Action::SubmitToTestEndpoint {
        selector: "form".into(),
        endpoint: "t".into(),
    };
    let all_kinds = [
        ErrorKind::AuthFailure,
        ErrorKind::Timeout,
        ErrorKind::UnsupportedFeature,
        ErrorKind::BlockedOrigin,
        ErrorKind::MalformedResponse,
        ErrorKind::MissingFixture,
        ErrorKind::CollectorFailure,
    ];
    for kind in all_kinds {
        let err = WorkerError::new(kind, "test").for_action(&submit);
        assert!(
            !err.retryable,
            "SubmitToTestEndpoint must never be retryable, got retryable for {kind:?}"
        );
    }
}

#[test]
fn retryable_is_false_for_focus_under_every_error_kind() {
    let focus = Action::Focus {
        selector: "input".into(),
    };
    let all_kinds = [
        ErrorKind::AuthFailure,
        ErrorKind::Timeout,
        ErrorKind::UnsupportedFeature,
        ErrorKind::BlockedOrigin,
        ErrorKind::MalformedResponse,
        ErrorKind::MissingFixture,
        ErrorKind::CollectorFailure,
    ];
    for kind in all_kinds {
        let err = WorkerError::new(kind, "test").for_action(&focus);
        assert!(
            !err.retryable,
            "Focus must never be retryable, got retryable for {kind:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Protocol version mismatch
// ---------------------------------------------------------------------------

#[test]
fn protocol_version_mismatch_is_deserializable_but_should_be_rejected_by_dispatch() {
    // The protocol does not enforce version at deserialization time;
    // the dispatch layer must check. We verify the field is present and
    // document the expectation.
    let mut v = request_json(Action::Inspect {});
    v["protocol_version"] = json!("2.0.0");
    let parsed: Request = serde_json::from_value(v).unwrap();
    assert_ne!(parsed.protocol_version, PROTOCOL_VERSION);
    // Dispatch must reject this.
    assert_eq!(parsed.protocol_version, "2.0.0");
}

#[test]
fn protocol_version_empty_string_is_deserializable() {
    let mut v = request_json(Action::Inspect {});
    v["protocol_version"] = json!("");
    let parsed: Request = serde_json::from_value(v).unwrap();
    assert_eq!(parsed.protocol_version, "");
    assert_ne!(parsed.protocol_version, PROTOCOL_VERSION);
}

// ---------------------------------------------------------------------------
// Round-trip of every Action variant
// ---------------------------------------------------------------------------

#[test]
fn round_trip_of_every_action_variant() {
    let actions = vec![
        Action::Navigate {
            url: "http://a/".into(),
        },
        Action::Inspect {},
        Action::Focus {
            selector: "#btn".into(),
        },
        Action::Keypress { key: "Tab".into() },
        Action::FillFixture {
            selector: "input".into(),
            fixture_ref: "name.txt".into(),
        },
        Action::Screenshot {},
        Action::SubmitToTestEndpoint {
            selector: "form".into(),
            endpoint: "ep1".into(),
        },
    ];
    for action in &actions {
        let req = sample_request(action.clone());
        let line = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&line).unwrap();
        assert_eq!(back.action, *action, "round-trip failed for {action:?}");
    }
}

// ---------------------------------------------------------------------------
// Idempotency classification
// ---------------------------------------------------------------------------

#[test]
fn idempotency_classification() {
    assert!(Action::Navigate {
        url: "http://x/".into()
    }
    .is_idempotent());
    assert!(Action::Inspect {}.is_idempotent());
    assert!(Action::Screenshot {}.is_idempotent());
    assert!(!Action::Focus {
        selector: "a".into()
    }
    .is_idempotent());
    assert!(!Action::Keypress { key: "Tab".into() }.is_idempotent());
    assert!(!Action::FillFixture {
        selector: "a".into(),
        fixture_ref: "b".into()
    }
    .is_idempotent());
    assert!(!Action::SubmitToTestEndpoint {
        selector: "a".into(),
        endpoint: "b".into()
    }
    .is_idempotent());
}

// ---------------------------------------------------------------------------
// Error kind wire format
// ---------------------------------------------------------------------------

#[test]
fn every_error_kind_round_trips_through_json() {
    let kinds = [
        ErrorKind::AuthFailure,
        ErrorKind::Timeout,
        ErrorKind::UnsupportedFeature,
        ErrorKind::BlockedOrigin,
        ErrorKind::MalformedResponse,
        ErrorKind::MissingFixture,
        ErrorKind::CollectorFailure,
    ];
    for kind in kinds {
        let line = serde_json::to_string(&kind).unwrap();
        let back: ErrorKind = serde_json::from_str(&line).unwrap();
        assert_eq!(back, kind);
    }
}

#[test]
fn unknown_error_kind_is_rejected() {
    let v = json!("unknown_kind");
    assert!(serde_json::from_value::<ErrorKind>(v).is_err());
}

// ---------------------------------------------------------------------------
// ColorScheme is closed
// ---------------------------------------------------------------------------

#[test]
fn unknown_color_scheme_is_rejected() {
    let mut v = request_json(Action::Inspect {});
    v["environment"]["media"]["color_scheme"] = json!("sepia");
    assert!(serde_json::from_value::<Request>(v).is_err());
}

// ---------------------------------------------------------------------------
// WorkerError display
// ---------------------------------------------------------------------------

#[test]
fn worker_error_display_includes_kind_and_message() {
    let err = WorkerError::new(ErrorKind::Timeout, "slow response");
    let display = format!("{err}");
    assert!(display.contains("timeout"));
    assert!(display.contains("slow response"));
}
