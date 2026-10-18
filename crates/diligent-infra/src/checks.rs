//! The twelve checks. Each is a pure function of an [`OriginObservation`].

use scraper::{Html, Selector};
use serde_json::{json, Value};

use crate::evidence::{chain_ids, dns_ids, response_id};
use crate::fetch::FetchResponse;
use crate::model::{Outcome, Status};
use crate::observe::{registrable_domain, Chain, ChainEnd, DnsObs, OriginObservation};

const ONE_YEAR: u64 = 31_536_000;

const L_VANTAGE: &str =
    "One request from one network vantage point; other regions, clients, and times can differ.";

#[allow(clippy::too_many_arguments)]
fn row(
    check_id: &'static str,
    rule: &'static str,
    channel: &'static str,
    status: Status,
    expected: &str,
    observed: Value,
    limitations: &[&str],
    evidence: Vec<String>,
) -> Outcome {
    let mut lims: Vec<String> = limitations.iter().map(|s| s.to_string()).collect();
    lims.push(L_VANTAGE.to_string());
    Outcome {
        check_id,
        rule,
        channel,
        status,
        expected: Value::String(expected.to_string()),
        observed,
        limitations: lims,
        evidence,
    }
}

fn chain_json(c: &Chain) -> Value {
    let hops: Vec<Value> = c
        .hops
        .iter()
        .map(|h| {
            let mut m = json!({ "url": h.url.as_str(), "status": h.status });
            if let (Some(loc), Some(obj)) = (h.header("location"), m.as_object_mut()) {
                obj.insert("location".to_string(), Value::String(loc));
            }
            m
        })
        .collect();
    let end = match &c.end {
        ChainEnd::Final => "final".to_string(),
        ChainEnd::TooManyRedirects => "too_many_redirects".to_string(),
        ChainEnd::BadScheme(s) => format!("non_http_scheme:{s}"),
        ChainEnd::BadLocation(s) => format!("bad_location:{s}"),
        ChainEnd::OffDomain(u) => format!("off_domain:{u}"),
        ChainEnd::Error(e) => format!("error:{e}"),
    };
    json!({ "hops": hops, "end": end })
}

/// A chain that did not finish: blocked on a request failure, otherwise needs review.
fn chain_problem(c: &Chain) -> Option<(Status, Value)> {
    match &c.end {
        ChainEnd::Final => None,
        ChainEnd::Error(e) => Some((
            Status::Blocked,
            json!({ "error": e.to_string(), "chain": chain_json(c) }),
        )),
        _ => Some((Status::NeedsReview, json!({ "chain": chain_json(c) }))),
    }
}

/// Why no https response exists for header and HTML checks.
fn no_https_response(o: &OriginObservation) -> (Status, Value) {
    match chain_problem(&o.https_home) {
        Some(p) => p,
        None => (
            Status::NeedsReview,
            json!({ "reason": "the origin did not end on an https response", "chain": chain_json(&o.https_home) }),
        ),
    }
}

fn param(value: &str, name: &str) -> Option<String> {
    value.split(';').find_map(|part| {
        let (k, v) = part.split_once('=')?;
        if k.trim().eq_ignore_ascii_case(name) {
            Some(v.trim().trim_matches('"').to_string())
        } else {
            None
        }
    })
}

/// `INFRA-01-https-redirect`.
pub fn https_redirect(o: &OriginObservation) -> Vec<Outcome> {
    const ID: &str = "INFRA-01-https-redirect";
    let exp = "http:// redirects to https:// on the same registrable domain with permanent redirects (301 or 308), at most 5 hops.";
    let lims = ["Starts at http://host/ only; other hosts, paths, and ports are not tested."];
    let c = &o.http_redirect;
    let ev = chain_ids(c);
    let mk = |status, observed| {
        vec![row(
            ID,
            "INFRA-01",
            "automated",
            status,
            exp,
            observed,
            &lims,
            ev.clone(),
        )]
    };
    match &c.end {
        ChainEnd::Error(e) => {
            return mk(
                Status::Blocked,
                json!({ "error": e.to_string(), "chain": chain_json(c) }),
            )
        }
        ChainEnd::BadScheme(s) => {
            return mk(
                Status::Fail,
                json!({ "reason": format!("redirect to non-http scheme {s}"), "chain": chain_json(c) }),
            )
        }
        ChainEnd::BadLocation(_) | ChainEnd::OffDomain(_) | ChainEnd::TooManyRedirects => {
            return mk(Status::NeedsReview, json!({ "chain": chain_json(c) }))
        }
        ChainEnd::Final => {}
    }
    let Some(last) = c.hops.last() else {
        return mk(
            Status::Blocked,
            json!({ "error": "no response", "chain": chain_json(c) }),
        );
    };
    if last.url.scheme() != "https" {
        // Say what the http origin did. A refusal (4xx or 5xx, no redirect) is not
        // the same as an answer that simply never moves to https.
        let reason = if c.hops.len() == 1 && last.status >= 400 {
            format!(
                "the http origin refused the request (status {}) instead of redirecting to https; final URL is not https",
                last.status
            )
        } else if c.hops.len() == 1 {
            format!(
                "the http origin answered {} without redirecting to https; final URL is not https",
                last.status
            )
        } else {
            "final URL is not https".to_string()
        };
        return mk(
            Status::Fail,
            json!({ "reason": reason, "http_status": last.status, "redirected": c.hops.len() > 1, "chain": chain_json(c) }),
        );
    }
    if c.hops.iter().skip(1).any(|h| h.url.scheme() != "https") {
        return mk(
            Status::Fail,
            json!({ "reason": "a redirect hop is not https", "chain": chain_json(c) }),
        );
    }
    let final_apex = registrable_domain(last.url.host_str().unwrap_or_default());
    if final_apex != o.apex {
        return mk(
            Status::NeedsReview,
            json!({ "reason": "final host is on a different registrable domain", "chain": chain_json(c) }),
        );
    }
    let redirects = &c.hops[..c.hops.len() - 1];
    if redirects.iter().all(|h| matches!(h.status, 301 | 308)) {
        mk(Status::Pass, json!({ "chain": chain_json(c) }))
    } else {
        mk(
            Status::NeedsReview,
            json!({ "reason": "temporary redirect status (302, 303, or 307) in the chain", "chain": chain_json(c) }),
        )
    }
}

/// `INFRA-01-https-cert`.
pub fn https_cert(o: &OriginObservation) -> Vec<Outcome> {
    const ID: &str = "INFRA-01-https-cert";
    let exp = "The TLS handshake to https://host/ succeeds with a certificate chain that verifies.";
    let lims = ["Default client verification only: no revocation, expiry-horizon, key-strength, or protocol-version analysis."];
    let c = &o.https_home;
    let ev = chain_ids(c);
    let mk = |status, observed| {
        vec![row(
            ID,
            "INFRA-01",
            "automated",
            status,
            exp,
            observed,
            &lims,
            ev.clone(),
        )]
    };
    match c.error() {
        Some(crate::fetch::FetchError::Tls(m)) => {
            mk(Status::Fail, json!({ "certificate_error": m }))
        }
        Some(e) if c.hops.is_empty() => mk(Status::Blocked, json!({ "error": e.to_string() })),
        _ => {
            // If the chain did not end normally (refused, off-domain, etc.), the
            // handshake to the intended target was not completed.
            if let Some((status, observed)) = chain_problem(c) {
                mk(status, observed)
            } else {
                mk(
                    Status::Pass,
                    json!({ "handshake": "verified", "chain": chain_json(c) }),
                )
            }
        }
    }
}

/// `INFRA-02-hsts` and `INFRA-02-hsts-preload`.
pub fn hsts(o: &OriginObservation) -> Vec<Outcome> {
    let exp1 = "strict-transport-security with max-age of at least 31536000 seconds.";
    let exp2 = "strict-transport-security includes both includeSubDomains and preload.";
    let l1 = ["Reads the header on one https response; does not check subdomains."];
    let l2 = ["Does not consult the Chromium HSTS preload list, so it cannot say the domain is preloaded. The list holds individual .gov domains (thousands, not every one) and has no entry for the .gov or .mil top-level domains, so a missing preload token here does not show the domain is unprotected; check hstspreload.org."];
    let Some(r) = o.https_response() else {
        let (st, obs) = no_https_response(o);
        let ev = chain_ids(&o.https_home);
        return vec![
            row(
                "INFRA-02-hsts",
                "INFRA-02",
                "automated",
                st,
                exp1,
                obs.clone(),
                &l1,
                ev.clone(),
            ),
            row(
                "INFRA-02-hsts-preload",
                "INFRA-02",
                "assisted",
                st,
                exp2,
                obs,
                &l2,
                ev,
            ),
        ];
    };
    let ev = vec![response_id(r)];
    let header = r.header("strict-transport-security");
    let (st1, obs1) = match &header {
        None => (Status::Fail, json!({ "header": Value::Null })),
        Some(h) => match param(h, "max-age").and_then(|v| v.parse::<u64>().ok()) {
            Some(n) if n >= ONE_YEAR => (Status::Pass, json!({ "header": h, "max_age": n })),
            Some(n) => (
                Status::NeedsReview,
                json!({ "header": h, "max_age": n, "reason": "max-age below one year" }),
            ),
            None => (
                Status::NeedsReview,
                json!({ "header": h, "reason": "max-age missing or not a number" }),
            ),
        },
    };
    let mut missing: Vec<&str> = Vec::new();
    let lower = header.clone().unwrap_or_default().to_ascii_lowercase();
    let has = |d: &str| lower.split(';').any(|p| p.trim() == d);
    if !has("includesubdomains") {
        missing.push("includeSubDomains");
    }
    if !has("preload") {
        missing.push("preload");
    }
    let (st2, obs2) = if header.is_none() {
        (
            Status::NeedsReview,
            json!({ "header": Value::Null, "missing": missing }),
        )
    } else if missing.is_empty() {
        (Status::Pass, json!({ "header": header }))
    } else {
        (
            Status::NeedsReview,
            json!({ "header": header, "missing": missing }),
        )
    };
    vec![
        row(
            "INFRA-02-hsts",
            "INFRA-02",
            "automated",
            st1,
            exp1,
            obs1,
            &l1,
            ev.clone(),
        ),
        row(
            "INFRA-02-hsts-preload",
            "INFRA-02",
            "assisted",
            st2,
            exp2,
            obs2,
            &l2,
            ev,
        ),
    ]
}

fn dns_gate(
    d: &DnsObs,
    id: &'static str,
    exp: &str,
    lims: &[&str],
) -> Result<(String, Vec<String>), Box<Outcome>> {
    match d {
        DnsObs::Skipped => Err(Box::new(row(
            id,
            "INFRA-03",
            "automated",
            Status::NotTested,
            exp,
            json!({ "reason": "DNS checks skipped by configuration" }),
            lims,
            Vec::new(),
        ))),
        DnsObs::Failed { name, error } => Err(Box::new(row(
            id,
            "INFRA-03",
            "automated",
            Status::Blocked,
            exp,
            json!({ "name": name, "error": error.to_string() }),
            lims,
            Vec::new(),
        ))),
        DnsObs::Answer { name, .. } => Ok((name.clone(), dns_ids(d))),
    }
}

/// `INFRA-03-spf`.
pub fn spf(o: &OriginObservation) -> Vec<Outcome> {
    const ID: &str = "INFRA-03-spf";
    let exp = "Exactly one v=spf1 record at the apex that ends in -all or ~all.";
    let lims = ["Does not evaluate include chains or lookup limits, and cannot tell whether the domain sends mail."];
    let (name, ev) = match dns_gate(&o.spf, ID, exp, &lims) {
        Ok(x) => x,
        Err(r) => return vec![*r],
    };
    let DnsObs::Answer { txt, .. } = &o.spf else {
        return Vec::new();
    };
    let recs: Vec<&String> = txt
        .iter()
        .filter(|t| {
            let l = t.trim().to_ascii_lowercase();
            l == "v=spf1" || l.starts_with("v=spf1 ")
        })
        .collect();
    let (st, obs) = match recs.len() {
        0 => (Status::Fail, json!({ "name": name, "records": [] })),
        1 => {
            let rec = recs[0].trim();
            let last = rec
                .split_whitespace()
                .last()
                .unwrap_or("")
                .to_ascii_lowercase();
            let st = match last.as_str() {
                "-all" | "~all" => Status::Pass,
                _ => Status::NeedsReview,
            };
            (
                st,
                json!({ "name": name, "record": rec, "final_term": last }),
            )
        }
        _ => (
            Status::Error,
            json!({ "name": name, "records": recs, "reason": "more than one v=spf1 record" }),
        ),
    };
    vec![row(ID, "INFRA-03", "automated", st, exp, obs, &lims, ev)]
}

/// `INFRA-03-dmarc`.
pub fn dmarc(o: &OriginObservation) -> Vec<Outcome> {
    const ID: &str = "INFRA-03-dmarc";
    let exp = "A v=DMARC1 record at _dmarc.<apex> with p=reject.";
    let lims = ["Does not check rua/ruf reporting, subdomain policy, alignment, or whether mail actually authenticates."];
    let (name, ev) = match dns_gate(&o.dmarc, ID, exp, &lims) {
        Ok(x) => x,
        Err(r) => return vec![*r],
    };
    let DnsObs::Answer { txt, .. } = &o.dmarc else {
        return Vec::new();
    };
    let recs: Vec<&String> = txt
        .iter()
        .filter(|t| {
            t.split(';')
                .next()
                .map(|f| {
                    let f: String = f.split_whitespace().collect::<Vec<_>>().join("");
                    f.eq_ignore_ascii_case("v=DMARC1")
                })
                .unwrap_or(false)
        })
        .collect();
    let (st, obs) = match recs.len() {
        0 => (Status::Fail, json!({ "name": name, "records": [] })),
        1 => {
            let rec = recs[0].trim();
            let p = param(rec, "p").map(|v| v.to_ascii_lowercase());
            let st = match p.as_deref() {
                Some("reject") => Status::Pass,
                Some("quarantine") => Status::NeedsReview,
                Some("none") => Status::Fail,
                _ => Status::NeedsReview,
            };
            (st, json!({ "name": name, "record": rec, "policy": p }))
        }
        _ => (
            Status::Error,
            json!({ "name": name, "records": recs, "reason": "more than one v=DMARC1 record" }),
        ),
    };
    vec![row(ID, "INFRA-03", "automated", st, exp, obs, &lims, ev)]
}

enum Page {
    Present(String),
    Empty(u16),
    Missing,
    Other(u16),
    Unfinished(Status),
}

fn page(c: &Chain) -> Page {
    match c.final_response() {
        None => Page::Unfinished(chain_problem(c).map(|p| p.0).unwrap_or(Status::NeedsReview)),
        Some(r) if r.status == 200 => {
            let t = r.text();
            if t.trim().is_empty() {
                Page::Empty(r.status)
            } else {
                Page::Present(t)
            }
        }
        Some(r) if r.status == 404 => Page::Missing,
        Some(r) => Page::Other(r.status),
    }
}

fn page_json(p: &Page) -> Value {
    match p {
        Page::Present(_) => json!("present"),
        Page::Empty(s) => json!({ "state": "empty", "status": s }),
        Page::Missing => json!("not_found"),
        Page::Other(s) => json!({ "state": "other", "status": s }),
        Page::Unfinished(Status::Blocked) => json!("request_failed"),
        Page::Unfinished(_) => json!("redirect_problem"),
    }
}

/// `INFRA-08-vdp`, emitted once for INFRA-08 and once for FED-11.
pub fn vdp(o: &OriginObservation) -> Vec<Outcome> {
    const ID: &str = "INFRA-08-vdp";
    let exp = "/vulnerability-disclosure-policy returns 200 with text (the BOD 20-01 obligation); /.well-known/security.txt is recommended.";
    let lims = [
        "Checks that a policy page and a security.txt exist, not that the policy is adequate, current, or answered.",
        "security.txt signature, Expires field, and Contact reachability are not verified.",
    ];
    let p = page(&o.vdp_page);
    let s = page(&o.security_txt);
    let contact = match &s {
        Page::Present(t) => Some(
            t.lines()
                .any(|l| l.trim_start().to_ascii_lowercase().starts_with("contact:")),
        ),
        _ => None,
    };
    let sec_present = matches!(s, Page::Present(_));
    let status = match (&p, &s) {
        (Page::Present(_), _) => Status::Pass,
        (Page::Unfinished(Status::Blocked), Page::Unfinished(Status::Blocked)) => Status::Blocked,
        (Page::Missing, Page::Missing) => Status::Fail,
        _ => Status::NeedsReview,
    };
    let mut extra: Vec<String> = Vec::new();
    if matches!(p, Page::Present(_)) {
        if !sec_present {
            extra.push("security.txt absent".to_string());
        } else if contact != Some(true) {
            extra.push("security.txt lacks Contact:".to_string());
        }
    }
    let observed = json!({
        "policy_page": page_json(&p),
        "security_txt": page_json(&s),
        "security_txt_has_contact": contact,
    });
    let mut ev = chain_ids(&o.vdp_page);
    ev.extend(chain_ids(&o.security_txt));
    ["INFRA-08", "FED-11"]
        .into_iter()
        .map(|rule| {
            let mut o = row(
                ID,
                rule,
                "assisted",
                status,
                exp,
                observed.clone(),
                &lims,
                ev.clone(),
            );
            o.limitations.extend(extra.iter().cloned());
            o
        })
        .collect()
}

/// `FED-01-gov-domain`.
pub fn gov_domain(o: &OriginObservation) -> Vec<Outcome> {
    let host = o.final_host();
    let ok = host == "gov" || host == "mil" || host.ends_with(".gov") || host.ends_with(".mil");
    let ev = o
        .https_response()
        .map(|r| vec![response_id(r)])
        .unwrap_or_default();
    vec![row(
        "FED-01-gov-domain",
        "FED-01",
        "automated",
        if ok { Status::Pass } else { Status::NeedsReview },
        "The final https host is under .gov or .mil.",
        json!({ "host": host, "from_final_response": o.https_response().is_some() }),
        &["A non-.gov host can be legitimate for state, local, and some federal contexts; domain governance is not verified."],
        ev,
    )]
}

/// Parses robots.txt and reports whether the `*` group disallows everything.
pub fn robots_disallows_all(text: &str) -> bool {
    let mut in_star = false;
    let mut after_rule = false;
    let mut blocked = false;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((field, value)) = line.split_once(':') else {
            continue;
        };
        let field = field.trim().to_ascii_lowercase();
        let value = value.trim();
        match field.as_str() {
            "user-agent" => {
                if after_rule {
                    in_star = false;
                    after_rule = false;
                }
                if value == "*" {
                    in_star = true;
                }
            }
            "disallow" | "allow" => {
                after_rule = true;
                if in_star && field == "disallow" && (value == "/" || value == "/*") {
                    blocked = true;
                }
            }
            _ => {}
        }
    }
    blocked
}

/// `FED-18-robots`.
pub fn robots(o: &OriginObservation) -> Vec<Outcome> {
    const ID: &str = "FED-18-robots";
    let exp = "robots.txt is absent or does not disallow all crawling for User-agent: *.";
    let lims = ["Only the User-agent: * group is read; Allow overrides, wildcards in other paths, and agent-specific groups are not evaluated."];
    let c = &o.robots;
    let ev = chain_ids(c);
    let mk = |st, obs| {
        vec![row(
            ID,
            "FED-18",
            "assisted",
            st,
            exp,
            obs,
            &lims,
            ev.clone(),
        )]
    };
    let Some(r) = c.final_response() else {
        let (st, obs) = chain_problem(c).unwrap_or((Status::NeedsReview, json!({})));
        return mk(st, obs);
    };
    match r.status {
        404 | 410 => mk(
            Status::Pass,
            json!({ "status": r.status, "robots": "absent" }),
        ),
        200 => {
            let ct = r
                .header("content-type")
                .unwrap_or_default()
                .to_ascii_lowercase();
            let text = r.text();
            let htmlish = ct.contains("html") || text.trim_start().starts_with('<');
            if htmlish || std::str::from_utf8(&r.body).is_err() {
                mk(
                    Status::NeedsReview,
                    json!({ "status": 200, "reason": "body is HTML or not valid UTF-8 text", "content_type": ct }),
                )
            } else if robots_disallows_all(&text) {
                mk(
                    Status::Fail,
                    json!({ "status": 200, "disallow_all_for_star": true }),
                )
            } else {
                mk(
                    Status::Pass,
                    json!({ "status": 200, "disallow_all_for_star": false }),
                )
            }
        }
        s => mk(
            Status::NeedsReview,
            json!({ "status": s, "reason": "unexpected status for robots.txt" }),
        ),
    }
}

fn sitemap_entries(text: &str) -> Option<usize> {
    let doc = roxmltree::Document::parse(text.trim_start_matches('\u{feff}')).ok()?;
    let root = doc.root_element().tag_name().name().to_string();
    if root != "urlset" && root != "sitemapindex" {
        return None;
    }
    Some(
        doc.descendants()
            .filter(|n| n.is_element() && matches!(n.tag_name().name(), "loc" | "sitemap"))
            .count(),
    )
}

/// `FED-18-sitemap`.
pub fn sitemap(o: &OriginObservation) -> Vec<Outcome> {
    const ID: &str = "FED-18-sitemap";
    let exp = "/sitemap.xml, or a sitemap declared in robots.txt, returns 200 XML with at least one <loc> or <sitemap>.";
    let lims = [
        "Does not fetch the URLs a sitemap lists, follow sitemap indexes, or read gzip sitemaps.",
    ];
    let mut candidates: Vec<&Chain> = vec![&o.sitemap];
    candidates.extend(o.declared_sitemaps.iter());
    let mut ev = Vec::new();
    let mut seen = Vec::new();
    let mut found: Option<Value> = None;
    for c in &candidates {
        ev.extend(chain_ids(c));
        let item = match c.final_response() {
            None => json!({ "chain": chain_json(c) }),
            Some(r) => {
                let entries = if r.status == 200 {
                    sitemap_entries(&r.text())
                } else {
                    None
                };
                if found.is_none() && entries.map(|n| n > 0).unwrap_or(false) {
                    found = Some(json!({ "url": r.url.as_str(), "entries": entries }));
                }
                json!({ "url": r.url.as_str(), "status": r.status, "xml_entries": entries })
            }
        };
        seen.push(item);
    }
    let all_blocked = candidates
        .iter()
        .all(|c| matches!(c.end, ChainEnd::Error(_)));
    let (st, obs) = if let Some(f) = found {
        (Status::Pass, json!({ "valid_sitemap": f, "checked": seen }))
    } else if all_blocked {
        (Status::Blocked, json!({ "checked": seen }))
    } else {
        (Status::NeedsReview, json!({ "checked": seen }))
    };
    vec![row(ID, "FED-18", "assisted", st, exp, obs, &lims, ev)]
}

/// `INFRA-12-security-headers`.
pub fn security_headers(o: &OriginObservation) -> Vec<Outcome> {
    const ID: &str = "INFRA-12-security-headers";
    let exp = "The https home page sends content-security-policy, x-content-type-options: nosniff, referrer-policy, and x-frame-options or CSP frame-ancestors.";
    let lims = [
        "Checks presence only; policy strength is not judged. These headers are not a cited must, so this check never fails.",
        "Only the home page is read.",
    ];
    let Some(r) = o.https_response() else {
        let (st, obs) = no_https_response(o);
        return vec![row(
            ID,
            "INFRA-12",
            "assisted",
            st,
            exp,
            obs,
            &lims,
            chain_ids(&o.https_home),
        )];
    };
    let csp = r.header("content-security-policy");
    let mut missing: Vec<&str> = Vec::new();
    if csp.as_deref().map(str::trim).unwrap_or("").is_empty() {
        missing.push("content-security-policy");
    }
    if !r
        .header("x-content-type-options")
        .map(|v| v.to_ascii_lowercase().contains("nosniff"))
        .unwrap_or(false)
    {
        missing.push("x-content-type-options: nosniff");
    }
    if r.header("referrer-policy").is_none() {
        missing.push("referrer-policy");
    }
    let framing = r.header("x-frame-options").is_some()
        || csp
            .as_deref()
            .map(|c| c.to_ascii_lowercase().contains("frame-ancestors"))
            .unwrap_or(false);
    if !framing {
        missing.push("x-frame-options or frame-ancestors");
    }
    let st = if missing.is_empty() {
        Status::Pass
    } else {
        Status::NeedsReview
    };
    vec![row(
        ID,
        "INFRA-12",
        "assisted",
        st,
        exp,
        json!({ "missing": missing }),
        &lims,
        vec![response_id(r)],
    )]
}

fn home_html(
    o: &OriginObservation,
) -> Result<(&FetchResponse, Html), (Status, Value, Vec<String>)> {
    match o.https_response() {
        None => {
            let (st, obs) = no_https_response(o);
            Err((st, obs, chain_ids(&o.https_home)))
        }
        Some(r) if !(200..300).contains(&r.status) => Err((
            Status::NeedsReview,
            json!({ "reason": "home page status is not 2xx", "status": r.status }),
            vec![response_id(r)],
        )),
        Some(r) => Ok((r, Html::parse_document(&r.text()))),
    }
}

/// `FED-27-dap`.
pub fn dap(o: &OriginObservation) -> Vec<Outcome> {
    const ID: &str = "FED-27-dap";
    let exp = "The home page loads dap.digitalgov.gov/Universal-Federated-Analytics-Min.js.";
    let lims = [
        "Reads server-rendered HTML only: a script injected by JavaScript or a tag manager is not seen.",
        "Whether DAP applies to this site is decided by applicability facts, not by this check.",
    ];
    let (r, html) = match home_html(o) {
        Ok(x) => x,
        Err((st, obs, ev)) => return vec![row(ID, "FED-27", "assisted", st, exp, obs, &lims, ev)],
    };
    let needle = "dap.digitalgov.gov/universal-federated-analytics-min.js";
    let found = Selector::parse("script[src]").ok().and_then(|sel| {
        html.select(&sel)
            .filter_map(|e| e.value().attr("src"))
            .find(|s| s.to_ascii_lowercase().contains(needle))
            .map(str::to_string)
    });
    let (st, obs) = match found {
        Some(src) => (Status::Pass, json!({ "script_src": src })),
        None => (Status::NeedsReview, json!({ "script_src": Value::Null })),
    };
    vec![row(
        ID,
        "FED-27",
        "assisted",
        st,
        exp,
        obs,
        &lims,
        vec![response_id(r)],
    )]
}

/// `FED-06-privacy-link`.
pub fn privacy_link(o: &OriginObservation) -> Vec<Outcome> {
    const ID: &str = "FED-06-privacy-link";
    let exp = "The home page has a link whose text or href contains \"privacy\".";
    let lims = [
        "Finds a link, not a privacy policy: its content, accuracy, and collection-context notices are not read.",
        "Reads server-rendered HTML only; links added by JavaScript are not seen.",
    ];
    let (r, html) = match home_html(o) {
        Ok(x) => x,
        Err((st, obs, ev)) => return vec![row(ID, "FED-06", "assisted", st, exp, obs, &lims, ev)],
    };
    let hit = Selector::parse("a").ok().and_then(|sel| {
        html.select(&sel).find_map(|a| {
            let text: String = a.text().collect::<Vec<_>>().join(" ");
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let href = a.value().attr("href").unwrap_or("").to_string();
            let label = a.value().attr("aria-label").unwrap_or("");
            let title = a.value().attr("title").unwrap_or("");
            let hay = format!("{text} {label} {title} {href}").to_ascii_lowercase();
            if hay.contains("privacy") {
                Some(json!({ "text": text, "href": href }))
            } else {
                None
            }
        })
    });
    let (st, obs) = match hit {
        Some(h) => (Status::Pass, json!({ "link": h })),
        None => (Status::NeedsReview, json!({ "link": Value::Null })),
    };
    vec![row(
        ID,
        "FED-06",
        "assisted",
        st,
        exp,
        obs,
        &lims,
        vec![response_id(r)],
    )]
}

/// Runs every check in report order.
pub fn evaluate(o: &OriginObservation) -> Vec<Outcome> {
    let mut v = Vec::new();
    v.extend(https_redirect(o));
    v.extend(https_cert(o));
    v.extend(hsts(o));
    v.extend(spf(o));
    v.extend(dmarc(o));
    v.extend(vdp(o));
    v.extend(gov_domain(o));
    v.extend(robots(o));
    v.extend(sitemap(o));
    v.extend(security_headers(o));
    v.extend(dap(o));
    v.extend(privacy_link(o));
    v
}

/// Static description of one infra check, for `diligent-rs checks list`.
#[derive(Debug, Clone, Copy)]
pub struct CatalogEntry {
    /// Check id as written to `RESULTS.jsonl`.
    pub id: &'static str,
    /// Obligation rule id the check informs (`OBL-<rule>`).
    pub rule: &'static str,
    /// `automated` or `assisted`.
    pub channel: &'static str,
    /// What the check expects.
    pub summary: &'static str,
}

/// Every infra check id [`evaluate`] can emit, sorted by id.
pub const CATALOG: [CatalogEntry; 13] = [
    CatalogEntry { id: "FED-01-gov-domain", rule: "FED-01", channel: "automated", summary: "The final https host is under .gov or .mil." },
    CatalogEntry { id: "FED-06-privacy-link", rule: "FED-06", channel: "assisted", summary: "The home page has a link whose text or href contains \"privacy\"." },
    CatalogEntry { id: "FED-18-robots", rule: "FED-18", channel: "assisted", summary: "robots.txt is absent or does not disallow all crawling for User-agent: *." },
    CatalogEntry { id: "FED-18-sitemap", rule: "FED-18", channel: "assisted", summary: "/sitemap.xml, or a sitemap declared in robots.txt, returns 200 XML with at least one <loc> or <sitemap>." },
    CatalogEntry { id: "FED-27-dap", rule: "FED-27", channel: "assisted", summary: "The home page loads dap.digitalgov.gov/Universal-Federated-Analytics-Min.js." },
    CatalogEntry { id: "INFRA-01-https-cert", rule: "INFRA-01", channel: "automated", summary: "The TLS handshake to https://host/ succeeds with a certificate chain that verifies." },
    CatalogEntry { id: "INFRA-01-https-redirect", rule: "INFRA-01", channel: "automated", summary: "http:// redirects to https:// on the same registrable domain with permanent redirects (301 or 308), at most 5 hops." },
    CatalogEntry { id: "INFRA-02-hsts", rule: "INFRA-02", channel: "automated", summary: "strict-transport-security with max-age of at least 31536000 seconds." },
    CatalogEntry { id: "INFRA-02-hsts-preload", rule: "INFRA-02", channel: "assisted", summary: "strict-transport-security includes both includeSubDomains and preload." },
    CatalogEntry { id: "INFRA-03-dmarc", rule: "INFRA-03", channel: "automated", summary: "A v=DMARC1 record at _dmarc.<apex> with p=reject." },
    CatalogEntry { id: "INFRA-03-spf", rule: "INFRA-03", channel: "automated", summary: "Exactly one v=spf1 record at the apex that ends in -all or ~all." },
    CatalogEntry { id: "INFRA-08-vdp", rule: "INFRA-08", channel: "assisted", summary: "/vulnerability-disclosure-policy returns 200 with text (the BOD 20-01 obligation); /.well-known/security.txt is recommended." },
    CatalogEntry { id: "INFRA-12-security-headers", rule: "INFRA-12", channel: "assisted", summary: "The https home page sends content-security-policy, x-content-type-options: nosniff, referrer-policy, and x-frame-options or CSP frame-ancestors." },
];
