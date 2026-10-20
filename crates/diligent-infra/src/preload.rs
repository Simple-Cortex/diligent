//! HSTS preload list lookup via hstspreload.org API.
//!
//! Queries the Chromium HSTS preload list to determine if a domain is preloaded.
//! This is an inference from a third-party list snapshot, not a direct observation.
//! Off by default; never runs under strict origins.

use serde::Deserialize;
use url::Url;

use crate::fetch::{FetchError, Fetcher};

/// Response from hstspreload.org API.
#[derive(Debug, Deserialize)]
struct PreloadResponse {
    status: String,
}

/// Result of a preload lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreloadStatus {
    /// Domain is preloaded.
    Preloaded,
    /// Domain is not preloaded or status is unknown/pending.
    NotPreloaded(String),
    /// Lookup failed (network error, non-200, unparseable).
    Failed(String),
}

/// Result of a preload lookup.
#[derive(Debug, Clone)]
pub struct PreloadResult {
    /// The preload status.
    pub status: PreloadStatus,
    /// The response, if one was received (even on non-200 or parse error).
    pub response: Option<crate::fetch::FetchResponse>,
}

/// Query hstspreload.org for the preload status of `domain`.
///
/// Returns a `PreloadResult` with the status and the response (if any).
/// The lookup uses the provided fetcher with the configured timeout and one attempt.
/// The response body is clipped at 4 KB on a character boundary.
pub fn lookup_preload(fetcher: &dyn Fetcher, domain: &str) -> PreloadResult {
    let url = match Url::parse(&format!(
        "https://hstspreload.org/api/v2/status?domain={}",
        domain
    )) {
        Ok(u) => u,
        Err(e) => {
            return PreloadResult {
                status: PreloadStatus::Failed(format!("invalid URL: {e}")),
                response: None,
            }
        }
    };

    let response = match fetcher.get(&url) {
        Ok(r) => r,
        Err(FetchError::Timeout) => {
            return PreloadResult {
                status: PreloadStatus::Failed("timeout".to_string()),
                response: None,
            }
        }
        Err(e) => {
            return PreloadResult {
                status: PreloadStatus::Failed(format!("request failed: {e}")),
                response: None,
            }
        }
    };

    if response.status != 200 {
        let status = PreloadStatus::Failed(format!("HTTP {}", response.status));
        return PreloadResult {
            status,
            response: Some(response),
        };
    }

    let body = response.text();
    let clipped = if body.len() > 4096 {
        // Truncate on char boundary to avoid panicking on multibyte characters
        let mut end = 4096;
        while end > 0 && !body.is_char_boundary(end) {
            end -= 1;
        }
        body[..end].to_string()
    } else {
        body
    };

    match serde_json::from_str::<PreloadResponse>(&clipped) {
        Ok(resp) => {
            let status = if resp.status == "preloaded" {
                PreloadStatus::Preloaded
            } else {
                PreloadStatus::NotPreloaded(resp.status)
            };
            PreloadResult {
                status,
                response: Some(response),
            }
        }
        Err(e) => PreloadResult {
            status: PreloadStatus::Failed(format!("unparseable response: {e}")),
            response: Some(response),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeFetcher;
    use serde_json::json;

    fn make_fetcher(status: u16, body: &str) -> FakeFetcher {
        let obs = json!({
            "responses": {
                "https://hstspreload.org/api/v2/status?domain=example.gov": {
                    "status": status,
                    "body": body
                }
            }
        });
        FakeFetcher::from_observation(&obs)
    }

    #[test]
    fn preloaded_domain_returns_preloaded() {
        let fetcher = make_fetcher(200, r#"{"status":"preloaded"}"#);
        let result = lookup_preload(&fetcher, "example.gov");
        assert_eq!(result.status, PreloadStatus::Preloaded);
        assert!(result.response.is_some());
    }

    #[test]
    fn unknown_status_returns_not_preloaded() {
        let fetcher = make_fetcher(200, r#"{"status":"unknown"}"#);
        let result = lookup_preload(&fetcher, "example.gov");
        assert_eq!(
            result.status,
            PreloadStatus::NotPreloaded("unknown".to_string())
        );
        assert!(result.response.is_some());
    }

    #[test]
    fn non_200_returns_failed() {
        let fetcher = make_fetcher(500, "error");
        let result = lookup_preload(&fetcher, "example.gov");
        assert!(matches!(result.status, PreloadStatus::Failed(_)));
        assert!(result.response.is_some());
    }

    #[test]
    fn multibyte_body_longer_than_clip_does_not_panic() {
        let body = "é".repeat(5000);
        let fetcher = make_fetcher(200, &body);
        let result = lookup_preload(&fetcher, "example.gov");
        assert!(matches!(result.status, PreloadStatus::Failed(_)));
        assert!(result.response.is_some());
    }
}
