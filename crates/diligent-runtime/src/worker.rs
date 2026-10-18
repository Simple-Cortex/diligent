//! Executes one protocol [`Request`] against a [`Browser`].
//!
//! This is the allowlist in code: the `match` on [`Action`] is exhaustive and
//! contains no path that evaluates caller-supplied script.

use crate::browser::Browser;
use crate::protocol::{
    Action, ErrorKind, EvidenceRef, Request, Response, Status, Timings, WorkerError,
    PROTOCOL_VERSION,
};
use crate::util::{now_millis, rfc3339_from_millis, sha256_hex};
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::time::Duration;

fn store(
    out: &Path,
    kind: &str,
    extension: &str,
    bytes: &[u8],
) -> Result<EvidenceRef, WorkerError> {
    fs::create_dir_all(out).map_err(|e| {
        WorkerError::new(
            ErrorKind::CollectorFailure,
            format!("creating {}: {e}", out.display()),
        )
    })?;
    let sha = sha256_hex(bytes);
    let name = format!("{sha}.{extension}");
    let path = out.join(&name);
    if !path.exists() {
        fs::write(&path, bytes).map_err(|e| {
            WorkerError::new(
                ErrorKind::CollectorFailure,
                format!("writing {}: {e}", path.display()),
            )
        })?;
    }
    Ok(EvidenceRef {
        evidence_id: sha.clone(),
        kind: kind.to_string(),
        path: name,
        sha256: sha,
        bytes: bytes.len() as u64,
    })
}

fn store_json<T: Serialize>(out: &Path, kind: &str, value: &T) -> Result<EvidenceRef, WorkerError> {
    let bytes = serde_json::to_vec(value).map_err(|e| {
        WorkerError::new(
            ErrorKind::MalformedResponse,
            format!("serializing {kind}: {e}"),
        )
    })?;
    store(out, kind, "json", &bytes)
}

/// Read a fixture by file name from `fixtures_dir`. Path separators and `..`
/// are rejected so a fixture reference cannot escape the directory.
fn read_fixture(fixtures_dir: Option<&Path>, fixture_ref: &str) -> Result<String, WorkerError> {
    let Some(dir) = fixtures_dir else {
        return Err(WorkerError::new(
            ErrorKind::MissingFixture,
            "no --fixtures-dir was configured",
        ));
    };
    if fixture_ref.is_empty() || fixture_ref.contains(['/', '\\']) || fixture_ref.contains("..") {
        return Err(WorkerError::new(
            ErrorKind::MissingFixture,
            format!("fixture reference `{fixture_ref}` must be a plain file name"),
        ));
    }
    fs::read_to_string(dir.join(fixture_ref)).map_err(|e| {
        WorkerError::new(
            ErrorKind::MissingFixture,
            format!("fixture `{fixture_ref}`: {e}"),
        )
    })
}

async fn run_action(
    browser: &mut dyn Browser,
    request: &Request,
    out: &Path,
    fixtures_dir: Option<&Path>,
) -> Result<Vec<EvidenceRef>, WorkerError> {
    if request.protocol_version != PROTOCOL_VERSION {
        return Err(WorkerError::new(
            ErrorKind::MalformedResponse,
            format!(
                "protocol_version {} is not {PROTOCOL_VERSION}",
                request.protocol_version
            ),
        ));
    }
    if request.credentials_ref.is_some() {
        return Err(WorkerError::new(
            ErrorKind::AuthFailure,
            "credentials_ref is not resolved in this slice",
        ));
    }
    let timeout = Duration::from_millis(request.timeout_ms);
    browser.set_viewport(request.environment.viewport).await?;
    browser.emulate_media(request.environment.media).await?;
    match &request.action {
        Action::Navigate { url } => {
            let info = browser.navigate(url, timeout).await?;
            Ok(vec![store_json(out, "navigation", &info)?])
        }
        Action::Inspect {} => {
            let dom = browser.dom_snapshot().await?;
            if dom.nodes.len() as u64 > u64::from(request.limits.max_nodes) {
                return Err(WorkerError::idempotent(
                    ErrorKind::CollectorFailure,
                    format!("limit max_nodes ({}) exceeded", request.limits.max_nodes),
                ));
            }
            let ax = browser.ax_tree().await?;
            Ok(vec![store_json(out, "dom_snapshot", &dom)?, store_json(out, "ax_tree", &ax)?])
        }
        Action::Focus { selector } => {
            browser.focus(selector).await?;
            Ok(vec![])
        }
        Action::Keypress { key } => {
            browser.press_key(key).await?;
            Ok(vec![])
        }
        Action::FillFixture { selector, fixture_ref } => {
            let text = read_fixture(fixtures_dir, fixture_ref)?;
            browser.fill_text(selector, &text).await?;
            Ok(vec![])
        }
        Action::Screenshot {} => {
            let png = browser.screenshot().await?;
            if png.len() as u64 > request.limits.max_bytes {
                return Err(WorkerError::idempotent(
                    ErrorKind::CollectorFailure,
                    format!("limit max_bytes ({}) exceeded", request.limits.max_bytes),
                ));
            }
            Ok(vec![store(out, "screenshot", "png", &png)?])
        }
        Action::SubmitToTestEndpoint { .. } => Err(WorkerError::new(
            ErrorKind::UnsupportedFeature,
            "submit_to_test_endpoint is not supported in this slice; the worker never submits forms",
        )),
    }
}

/// Execute `request` and build the response line. Never panics on browser errors.
pub async fn execute_request(
    browser: &mut dyn Browser,
    request: &Request,
    out: &Path,
    fixtures_dir: Option<&Path>,
) -> Response {
    let start = now_millis();
    let result = run_action(browser, request, out, fixtures_dir).await;
    let end = now_millis();
    let (status, evidence, error) = match result {
        Ok(evidence) => (Status::Ok, evidence, None),
        Err(error) => (
            Status::Error,
            Vec::new(),
            Some(error.for_action(&request.action)),
        ),
    };
    Response {
        protocol_version: PROTOCOL_VERSION.to_string(),
        request_id: request.request_id.clone(),
        run_id: request.run_id.clone(),
        status,
        evidence,
        timings: Timings {
            started_at: rfc3339_from_millis(start),
            finished_at: rfc3339_from_millis(end),
            duration_ms: end.saturating_sub(start),
        },
        error,
    }
}
