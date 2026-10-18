//! Browser-worker protocol, version 1.3.0.
//!
//! Versioned JSON Lines: one [`Request`] or [`Response`] object per line.
//! Every struct rejects unknown fields and every enum is closed, so a model or
//! a stale client cannot smuggle a new capability through the wire format.
//! The [`Action`] allowlist carries no script, no shell and no raw selector
//! expression beyond a CSS selector string that the adapter resolves through
//! the DOM domain, never through `Runtime.evaluate`.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Wire protocol version spoken by this crate.
pub const PROTOCOL_VERSION: &str = "1.3.0";

pub use diligent_capture::{ColorScheme, Media, Viewport};

/// Browser environment requested for an action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    /// Viewport to apply.
    pub viewport: Viewport,
    /// Media features to apply.
    pub media: Media,
    /// Optional user agent override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}

/// Hard limits for a single request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Maximum total evidence bytes.
    pub max_bytes: u64,
    /// Maximum DOM nodes in one snapshot.
    pub max_nodes: u32,
}

/// The closed allowlist of worker actions.
///
/// No variant carries a script or a shell command. Unknown action names fail
/// deserialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Navigate to a URL inside the run's allowed origins.
    Navigate {
        /// Absolute http(s) URL.
        url: String,
    },
    /// Capture DOM snapshot and accessibility tree of the current page.
    Inspect {},
    /// Focus the first element matching a CSS selector.
    Focus {
        /// CSS selector.
        selector: String,
    },
    /// Press one key from the safe key allowlist (never Enter or Space).
    Keypress {
        /// Key name, for example `Tab`.
        key: String,
    },
    /// Type fixture text into the element matching a selector.
    FillFixture {
        /// CSS selector.
        selector: String,
        /// File name of a fixture under the fixtures directory.
        fixture_ref: String,
    },
    /// Capture a PNG screenshot.
    Screenshot {},
    /// Reserved. Always answered with `unsupported_feature` in this slice.
    SubmitToTestEndpoint {
        /// CSS selector of the form.
        selector: String,
        /// Test endpoint identifier.
        endpoint: String,
    },
}

impl Action {
    /// Whether repeating the action cannot change page or server state.
    pub fn is_idempotent(&self) -> bool {
        matches!(
            self,
            Action::Navigate { .. } | Action::Inspect {} | Action::Screenshot {}
        )
    }
}

/// One worker request line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// Must equal [`PROTOCOL_VERSION`].
    pub protocol_version: String,
    /// Caller-chosen request id, echoed in the response.
    pub request_id: String,
    /// Run id, echoed in the response.
    pub run_id: String,
    /// The allowlisted action.
    pub action: Action,
    /// Requested browser environment.
    pub environment: Environment,
    /// Per-request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Hard limits.
    pub limits: Limits,
    /// Credentials by reference only; never inline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials_ref: Option<String>,
}

/// Outcome of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The action completed.
    Ok,
    /// The action failed; see `error`.
    Error,
}

/// Reference to one stored evidence file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    /// Content hash; also the identifier.
    pub evidence_id: String,
    /// Capture kind, for example `dom_snapshot`.
    pub kind: String,
    /// Path relative to the evidence directory.
    pub path: String,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    /// File size in bytes.
    pub bytes: u64,
}

/// Wall-clock timings of a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timings {
    /// RFC 3339 UTC start.
    pub started_at: String,
    /// RFC 3339 UTC finish.
    pub finished_at: String,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// Error taxonomy from the audit specification section 20.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// Credentials were missing or rejected.
    AuthFailure,
    /// A deadline passed. Never evidence of conformance.
    Timeout,
    /// The action or feature is not available in this build.
    UnsupportedFeature,
    /// The target origin is outside the run's allowed origins.
    BlockedOrigin,
    /// The browser returned data the worker could not interpret.
    MalformedResponse,
    /// A referenced fixture does not exist.
    MissingFixture,
    /// The browser, a limit or the local environment failed.
    CollectorFailure,
}

impl ErrorKind {
    /// Wire name of the kind.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::AuthFailure => "auth_failure",
            ErrorKind::Timeout => "timeout",
            ErrorKind::UnsupportedFeature => "unsupported_feature",
            ErrorKind::BlockedOrigin => "blocked_origin",
            ErrorKind::MalformedResponse => "malformed_response",
            ErrorKind::MissingFixture => "missing_fixture",
            ErrorKind::CollectorFailure => "collector_failure",
        }
    }
}

/// Retry rule: only `timeout` and `collector_failure` on an idempotent action.
pub fn is_retryable(kind: ErrorKind, idempotent: bool) -> bool {
    idempotent && matches!(kind, ErrorKind::Timeout | ErrorKind::CollectorFailure)
}

/// Structured worker error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerError {
    /// Taxonomy entry.
    pub kind: ErrorKind,
    /// Human-readable message.
    pub message: String,
    /// Whether the caller may retry the same request.
    pub retryable: bool,
}

impl WorkerError {
    /// Error for a non-idempotent context: never retryable.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        WorkerError {
            kind,
            message: message.into(),
            retryable: false,
        }
    }

    /// Error raised by an idempotent action (navigate, inspect, screenshot).
    pub fn idempotent(kind: ErrorKind, message: impl Into<String>) -> Self {
        WorkerError {
            kind,
            message: message.into(),
            retryable: is_retryable(kind, true),
        }
    }

    /// Recompute `retryable` for the action that produced the error.
    pub fn for_action(mut self, action: &Action) -> Self {
        self.retryable = is_retryable(self.kind, action.is_idempotent());
        self
    }
}

impl fmt::Display for WorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.as_str(), self.message)
    }
}

impl std::error::Error for WorkerError {}

/// One worker response line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    /// Always [`PROTOCOL_VERSION`].
    pub protocol_version: String,
    /// Echo of the request id.
    pub request_id: String,
    /// Echo of the run id.
    pub run_id: String,
    /// Outcome.
    pub status: Status,
    /// Evidence produced by the action.
    pub evidence: Vec<EvidenceRef>,
    /// Timings.
    pub timings: Timings,
    /// Present exactly when `status` is `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<WorkerError>,
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn request_round_trips_for_every_action() {
        let actions = vec![
            Action::Navigate {
                url: "http://a/".into(),
            },
            Action::Inspect {},
            Action::Focus {
                selector: "a".into(),
            },
            Action::Keypress { key: "Tab".into() },
            Action::FillFixture {
                selector: "input".into(),
                fixture_ref: "x.txt".into(),
            },
            Action::Screenshot {},
            Action::SubmitToTestEndpoint {
                selector: "form".into(),
                endpoint: "t".into(),
            },
        ];
        for action in actions {
            let request = sample_request(action);
            let line = serde_json::to_string(&request).unwrap();
            assert!(!line.contains('\n'));
            let back: Request = serde_json::from_str(&line).unwrap();
            assert_eq!(back, request);
        }
    }

    #[test]
    fn response_round_trips() {
        let response = Response {
            protocol_version: PROTOCOL_VERSION.into(),
            request_id: "r1".into(),
            run_id: "run1".into(),
            status: Status::Error,
            evidence: vec![],
            timings: Timings {
                started_at: "2026-10-17T00:00:00Z".into(),
                finished_at: "2026-10-17T00:00:01Z".into(),
                duration_ms: 1000,
            },
            error: Some(WorkerError::idempotent(ErrorKind::Timeout, "slow")),
        };
        let line = serde_json::to_string(&response).unwrap();
        assert!(line.contains("\"timeout\""));
        let back: Response = serde_json::from_str(&line).unwrap();
        assert_eq!(back, response);
    }

    #[test]
    fn unknown_action_is_rejected() {
        let mut value = serde_json::to_value(sample_request(Action::Inspect {})).unwrap();
        value["action"] = serde_json::json!({"action": "run_script", "source": "alert(1)"});
        assert!(serde_json::from_value::<Request>(value).is_err());
    }

    #[test]
    fn unknown_field_is_rejected() {
        let mut value = serde_json::to_value(sample_request(Action::Inspect {})).unwrap();
        value["surprise"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Request>(value).is_err());

        let mut value =
            serde_json::to_value(sample_request(Action::Navigate { url: "u".into() })).unwrap();
        value["action"]["script"] = serde_json::json!("1+1");
        assert!(serde_json::from_value::<Request>(value).is_err());
    }

    #[test]
    fn retryable_matrix() {
        use ErrorKind::*;
        let all = [
            AuthFailure,
            Timeout,
            UnsupportedFeature,
            BlockedOrigin,
            MalformedResponse,
            MissingFixture,
            CollectorFailure,
        ];
        for kind in all {
            let expected = matches!(kind, Timeout | CollectorFailure);
            assert_eq!(is_retryable(kind, true), expected, "{kind:?} idempotent");
            assert!(!is_retryable(kind, false), "{kind:?} non-idempotent");
        }
        let fill = Action::FillFixture {
            selector: "a".into(),
            fixture_ref: "b".into(),
        };
        let submit = Action::SubmitToTestEndpoint {
            selector: "a".into(),
            endpoint: "b".into(),
        };
        assert!(!WorkerError::new(Timeout, "t").for_action(&fill).retryable);
        assert!(
            !WorkerError::new(CollectorFailure, "t")
                .for_action(&submit)
                .retryable
        );
        assert!(
            WorkerError::new(Timeout, "t")
                .for_action(&Action::Navigate { url: "u".into() })
                .retryable
        );
        assert!(
            WorkerError::new(CollectorFailure, "t")
                .for_action(&Action::Screenshot {})
                .retryable
        );
    }

    #[test]
    fn error_kind_wire_names() {
        assert_eq!(
            serde_json::to_string(&ErrorKind::BlockedOrigin).unwrap(),
            "\"blocked_origin\""
        );
        assert_eq!(ErrorKind::MissingFixture.as_str(), "missing_fixture");
    }
}
