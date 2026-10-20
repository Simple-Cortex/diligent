//! Infrastructure and federal-web origin checks.
//!
//! Deterministic, network-level checks on one https origin: redirect and certificate,
//! HSTS, SPF and DMARC, vulnerability-disclosure files, robots and sitemap, security
//! headers, the DAP script, a privacy link, and the host suffix. No browser, no layout,
//! no model.
//!
//! The flow is `capture` (all network I/O, through the [`Fetcher`] and [`Resolver`]
//! traits) then `evaluate` (pure functions of an [`OriginObservation`]) then row and
//! evidence construction. [`run`] does all three. Rows and evidence follow
//! `schemas/check-result.schema.json` and `schemas/evidence-record.schema.json`.
//!
//! Entry points: [`run`] and [`cli::cli`] (`infra --url <https://host/> --out <dir>`).

pub mod checks;
pub mod cli;
pub mod evidence;
pub mod fake;
pub mod fetch;
pub mod model;
pub mod observe;
pub mod preload;

use diligent_source::sha256::sha256_hex;
use serde_json::json;
use url::Url;

pub use fetch::{
    vet_target, DnsResolver, FetchError, FetchResponse, Fetcher, Options, ReqwestFetcher,
    ResolveError, Resolver,
};
pub use model::{CheckResult, EvidenceRecord, InfraReport, Outcome, Status, StoredBody};
pub use observe::OriginObservation;

/// The responsibility this crate owns in the audit specification's module boundaries.
pub const CRATE_ROLE: &str =
    "Authorized HTTP, TLS, DNS and header checks and deployment evidence adapters.";

/// Captures `origin`, evaluates every check, and builds rows and evidence.
///
/// An origin without a host yields an empty report.
pub fn run(
    origin: &Url,
    fetcher: &dyn Fetcher,
    resolver: &dyn Resolver,
    opts: &Options,
) -> InfraReport {
    let Some(obs) = observe::capture(origin, fetcher, resolver, opts) else {
        return InfraReport {
            rows: Vec::new(),
            evidence: Vec::new(),
            bodies: Vec::new(),
        };
    };
    let outcomes = checks::evaluate(&obs);
    let (evidence, bodies) = evidence::build(&obs, opts.max_body_bytes);
    let prefix = sha256_hex(format!("{}\0{}", obs.origin, obs.run_id).as_bytes())[..10].to_string();
    let rows = outcomes
        .into_iter()
        .enumerate()
        .map(|(i, o)| {
            let obligation = format!("OBL-{}", o.rule);
            CheckResult {
                schema_version: "1.0.0".to_string(),
                result_id: format!("res-{prefix}-{:04}", i + 1),
                check_id: o.check_id.to_string(),
                check_version: model::ENGINE_VERSION.to_string(),
                obligation_id: obligation.clone(),
                instance_id: obs.origin.to_string(),
                applicability_ref: obligation,
                status: o.status,
                evidence_refs: o.evidence,
                expected: o.expected,
                observed: o.observed,
                limitations: {
                    let mut l = o.limitations;
                    if opts.allow_private_origin {
                        l.push(
                            "allow_private_origin=true: private origin allowed by operator"
                                .to_string(),
                        );
                    }
                    l
                },
                executed_at: obs.captured_at.clone(),
                engine_version: model::ENGINE_VERSION.to_string(),
                supersedes_result_id: None,
                extensions: json!({
                    (model::EXTENSION_KEY): {
                        "scope": "network",
                        "channel": o.channel,
                        "rule_ids": [o.rule],
                    }
                }),
            }
        })
        .collect();
    InfraReport {
        rows,
        evidence,
        bodies,
    }
}

#[cfg(test)]
mod tests {
    use super::CRATE_ROLE;

    #[test]
    fn crate_role_is_not_empty() {
        assert!(!CRATE_ROLE.is_empty());
    }
}
