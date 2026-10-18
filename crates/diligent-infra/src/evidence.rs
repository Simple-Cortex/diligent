//! Evidence records and stored bodies built from an observation.

use std::collections::HashSet;

use diligent_source::sha256::sha256_hex;
use serde_json::{json, Map, Value};

use crate::fetch::FetchResponse;
use crate::model::{EvidenceRecord, Redaction, StoredBody, COLLECTOR, ENGINE_VERSION};
use crate::observe::{Chain, DnsObs, OriginObservation};

/// Headers never written to evidence.
const STRIPPED_HEADERS: [&str; 2] = ["set-cookie", "authorization"];

/// Evidence id for an HTTP response.
pub fn response_id(r: &FetchResponse) -> String {
    let body = sha256_hex(&r.body);
    let id = sha256_hex(format!("{}\0{}\0{}", r.url, r.status, body).as_bytes());
    format!("ev-http-{}", &id[..12])
}

/// Evidence id for a DNS TXT answer.
pub fn dns_id(name: &str, txt: &[String]) -> String {
    let id = sha256_hex(format!("{name}\0{}", txt.join("\n")).as_bytes());
    format!("ev-dns-{}", &id[..12])
}

/// Evidence ids for every response in a chain.
pub fn chain_ids(c: &Chain) -> Vec<String> {
    c.hops.iter().map(response_id).collect()
}

fn extension_for(r: &FetchResponse) -> &'static str {
    let ct = r
        .header("content-type")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if r.body.is_empty() {
        "bin"
    } else if ct.contains("html") {
        "html"
    } else if ct.contains("xml") {
        "xml"
    } else if ct.contains("json") {
        "json"
    } else if ct.starts_with("text/") {
        "txt"
    } else {
        "bin"
    }
}

fn all_chains(o: &OriginObservation) -> Vec<&Chain> {
    let mut v = vec![
        &o.http_redirect,
        &o.https_home,
        &o.vdp_page,
        &o.security_txt,
        &o.robots,
        &o.sitemap,
    ];
    v.extend(o.declared_sitemaps.iter());
    v
}

/// Builds evidence records and body files for every response and DNS answer in `o`.
pub fn build(
    o: &OriginObservation,
    max_body_bytes: usize,
) -> (Vec<EvidenceRecord>, Vec<StoredBody>) {
    let mut records = Vec::new();
    let mut bodies: Vec<StoredBody> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for chain in all_chains(o) {
        for r in &chain.hops {
            let id = response_id(r);
            if !seen.insert(id.clone()) {
                continue;
            }
            let sha = sha256_hex(&r.body);
            let path = format!("evidence/{sha}.{}", extension_for(r));
            if !bodies.iter().any(|b| b.path == path) {
                bodies.push(StoredBody {
                    path: path.clone(),
                    bytes: r.body.clone(),
                });
            }
            let mut stripped = false;
            let mut headers: Vec<(String, String)> = Vec::new();
            for (k, v) in &r.headers {
                if STRIPPED_HEADERS.contains(&k.as_str()) {
                    stripped = true;
                    continue;
                }
                match headers.iter_mut().find(|(n, _)| n == k) {
                    Some((_, existing)) => {
                        existing.push_str(", ");
                        existing.push_str(v);
                    }
                    None => headers.push((k.clone(), v.clone())),
                }
            }
            headers.sort();
            let mut header_map = Map::new();
            for (k, v) in headers {
                header_map.insert(k, Value::String(v));
            }
            let mut limitations = vec![
                "Single GET from one network vantage point at one time; headers and body can differ by client, region, and time.".to_string(),
            ];
            if r.truncated {
                limitations.push(format!(
                    "Body truncated at {max_body_bytes} bytes; the hash covers the stored bytes only."
                ));
            }
            records.push(EvidenceRecord {
                schema_version: "1.0.0".to_string(),
                evidence_id: id,
                run_id: o.run_id.clone(),
                kind: "http_response".to_string(),
                collector: COLLECTOR.to_string(),
                collector_version: ENGINE_VERSION.to_string(),
                captured_at: o.captured_at.clone(),
                snapshot_ref: None,
                context: json!({
                    "scope": "network",
                    "protocol": "http",
                    "method": "GET",
                    "url": r.url.as_str(),
                    "status": r.status,
                    "headers": Value::Object(header_map),
                    "body_bytes": r.body.len(),
                    "body_truncated": r.truncated,
                }),
                payload_ref: path,
                sha256: sha,
                data_classification: "public_web_response".to_string(),
                redaction: Redaction {
                    applied: stripped,
                    flags: if stripped {
                        vec!["headers_stripped:set-cookie,authorization".to_string()]
                    } else {
                        Vec::new()
                    },
                },
                trust: "direct_observation".to_string(),
                limitations,
                derived_from: None,
            });
        }
    }

    for obs in [&o.spf, &o.dmarc] {
        if let DnsObs::Answer { name, txt } = obs {
            let id = dns_id(name, txt);
            if !seen.insert(id.clone()) {
                continue;
            }
            records.push(EvidenceRecord {
                schema_version: "1.0.0".to_string(),
                evidence_id: id,
                run_id: o.run_id.clone(),
                kind: "runtime_observation".to_string(),
                collector: COLLECTOR.to_string(),
                collector_version: ENGINE_VERSION.to_string(),
                captured_at: o.captured_at.clone(),
                snapshot_ref: None,
                context: json!({
                    "scope": "network",
                    "protocol": "dns",
                    "record_type": "TXT",
                    "name": name,
                    "answers": txt,
                }),
                payload_ref: format!("dns:TXT:{name}"),
                sha256: sha256_hex(txt.join("\n").as_bytes()),
                data_classification: "public_dns_record".to_string(),
                redaction: Redaction {
                    applied: false,
                    flags: Vec::new(),
                },
                trust: "direct_observation".to_string(),
                limitations: vec![
                    "One recursive-resolver answer at one time; caches and split-horizon DNS can differ from authoritative data.".to_string(),
                ],
                derived_from: None,
            });
        }
    }
    (records, bodies)
}

/// Evidence ids for a DNS observation, empty when it did not answer.
pub fn dns_ids(d: &DnsObs) -> Vec<String> {
    match d {
        DnsObs::Answer { name, txt } => vec![dns_id(name, txt)],
        _ => Vec::new(),
    }
}
