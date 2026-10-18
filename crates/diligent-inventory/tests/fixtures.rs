//! Runs every case in `fixtures/inventory/` with the fake site and a manual clock, compares the
//! output exactly with the expected files, and validates both artifacts against their schemas.

use std::path::{Path, PathBuf};

use diligent_inventory::fixture::{option_files, run_case, CaseRun};
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn validator(name: &str) -> jsonschema::Validator {
    let text = std::fs::read_to_string(repo_root().join("schemas").join(name)).unwrap();
    let schema: Value = serde_json::from_str(&text).unwrap();
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .should_validate_formats(true)
        .build(&schema)
        .unwrap()
}

fn assert_valid(v: &jsonschema::Validator, instance: &Value, what: &str) {
    let errors: Vec<String> = v
        .iter_errors(instance)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{what} is invalid: {errors:?}");
}

fn case_dir(name: &str) -> PathBuf {
    repo_root().join("fixtures/inventory").join(name)
}

fn case_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(repo_root().join("fixtures/inventory"))
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn read(path: &Path) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
    )
    .unwrap()
}

fn runs(name: &str) -> Vec<CaseRun> {
    let dir = case_dir(name);
    option_files(&dir)
        .into_iter()
        .map(|(suffix, file)| {
            run_case(&dir, &suffix, &file).unwrap_or_else(|e| panic!("{name}: {e}"))
        })
        .collect()
}

fn inventory_json(r: &CaseRun) -> Value {
    serde_json::to_value(&r.outcome.inventory).unwrap()
}

fn sample_json(r: &CaseRun) -> Value {
    serde_json::to_value(&r.outcome.sample).unwrap()
}

#[test]
fn every_case_matches_its_expected_files_and_schemas() {
    let inventory_schema = validator("inventory.schema.json");
    let sample_schema = validator("sample.schema.json");
    let names = case_names();
    assert!(
        names.len() >= 9,
        "expected the nine fixture cases, found {names:?}"
    );
    for name in names {
        let dir = case_dir(&name);
        for run in runs(&name) {
            let what = format!("{name}{}", run.suffix);
            let inv = inventory_json(&run);
            let sample = sample_json(&run);
            assert_valid(&inventory_schema, &inv, &format!("{what} INVENTORY.json"));
            assert_valid(&sample_schema, &sample, &format!("{what} SAMPLE.json"));
            let inv_path = dir.join(format!("expected-inventory{}.json", run.suffix));
            let inv_path = if inv_path.exists() {
                inv_path
            } else {
                dir.join("expected-inventory.json")
            };
            assert_eq!(
                inv,
                read(&inv_path),
                "{what}: inventory differs from {}",
                inv_path.display()
            );
            assert_eq!(
                sample,
                read(&dir.join(format!("expected-sample{}.json", run.suffix))),
                "{what}: sample differs"
            );
        }
    }
}

#[test]
fn runs_are_deterministic() {
    for name in case_names() {
        let a = runs(&name);
        let b = runs(&name);
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(
                serde_json::to_string(&x.outcome.inventory).unwrap(),
                serde_json::to_string(&y.outcome.inventory).unwrap(),
                "{name}{}",
                x.suffix
            );
            assert_eq!(
                serde_json::to_string(&x.outcome.sample).unwrap(),
                serde_json::to_string(&y.outcome.sample).unwrap(),
                "{name}{}",
                x.suffix
            );
            assert_eq!(x.requests, y.requests);
        }
    }
}

#[test]
fn sample_invariants_hold_in_every_case() {
    for name in case_names() {
        for run in runs(&name) {
            let s = &run.outcome.sample;
            let mut urls: Vec<&str> = s.structured.iter().map(|x| x.url.as_str()).collect();
            let n = urls.len();
            urls.sort_unstable();
            urls.dedup();
            assert_eq!(urls.len(), n, "{name}: structured URLs are unique");
            for r in &s.random {
                assert!(
                    !s.structured.iter().any(|x| x.url == r.url),
                    "{name}: random overlaps structured"
                );
            }
            let inv = &run.outcome.inventory;
            for item in &s.structured {
                if item.cluster_id.is_some() {
                    let route = inv
                        .routes
                        .iter()
                        .find(|r| r.url == item.url)
                        .expect("route");
                    assert!(
                        !route.excluded_by_robots && route.unreachable.is_none(),
                        "{name}: {}",
                        item.url
                    );
                }
            }
            assert_eq!(s.coverage.discovered, inv.routes.len());
            assert_eq!(s.coverage.sampled_structured, s.structured.len());
            assert_eq!(s.coverage.sampled_random, s.random.len());
            assert_eq!(s.coverage.clustered, inv.templates.len());
            assert_eq!(s.partial, !s.remaining_work.is_empty());
            assert_eq!(
                s.structured_coverage_estimate.is_none(),
                s.random.is_empty()
            );
            let templates_total: usize = inv.templates.iter().map(|t| t.size).sum();
            let clustered_routes = inv.routes.iter().filter(|r| r.cluster_id.is_some()).count();
            assert_eq!(
                templates_total, clustered_routes,
                "{name}: template sizes add up"
            );
        }
    }
}

#[test]
fn private_link_target_is_never_requested_and_is_counted_refused() {
    let run = &runs("private-link-target")[0];
    assert!(
        run.requests.iter().all(|u| !u.contains("10.0.0.5")),
        "{:?}",
        run.requests
    );
    assert!(
        run.requests.iter().all(|u| !u.contains("localhost")),
        "{:?}",
        run.requests
    );
    let route = run
        .outcome
        .inventory
        .routes
        .iter()
        .find(|r| r.url == "http://10.0.0.5/admin")
        .expect("route for the private link");
    assert_eq!(route.unreachable.as_deref(), Some("refused"));
    assert!(!route.fetched);
    assert_eq!(run.outcome.sample.coverage.unreachable, 1);
    assert!(run
        .outcome
        .sample
        .structured
        .iter()
        .all(|s| !s.url.contains("10.0.0.5")));
    let hosts: Vec<&str> = run
        .outcome
        .inventory
        .vendors
        .iter()
        .map(|v| v.host.as_str())
        .collect();
    assert!(hosts.contains(&"xn--mnchen-3ya.example"), "{hosts:?}");
    assert!(
        !hosts
            .iter()
            .any(|h| h.contains("localhost") || h.contains("10.0.0.5")),
        "{hosts:?}"
    );
}

#[test]
fn off_domain_redirect_is_not_followed() {
    let run = &runs("redirect-off-domain")[0];
    assert!(
        run.requests.iter().all(|u| !u.contains("evil.example")),
        "{:?}",
        run.requests
    );
    let by = |suffix: &str| {
        run.outcome
            .inventory
            .routes
            .iter()
            .find(|r| r.url.ends_with(suffix))
            .unwrap_or_else(|| panic!("no route {suffix}"))
            .clone()
    };
    assert_eq!(by("/go").unreachable.as_deref(), Some("off_domain"));
    assert_eq!(
        by("/loop").unreachable.as_deref(),
        Some("too_many_redirects")
    );
    assert_eq!(
        by("/to-www").unreachable.as_deref(),
        Some("off_scope_redirect")
    );
    assert_eq!(by("/down").unreachable.as_deref(), Some("timeout"));
    assert_eq!(
        by("/moved").final_url.as_deref(),
        Some("https://example.gov/new-home")
    );
    assert_eq!(by("/missing").status, Some(404));
}

#[test]
fn robots_exclusions_are_not_fetched_and_the_override_is_recorded() {
    let runs = runs("robots-disallow-section");
    let (plain, ignore) = (&runs[0], &runs[1]);
    assert_eq!(plain.suffix, "");
    assert_eq!(ignore.suffix, "-ignore");
    for blocked in ["/private/records", "/data.json"] {
        assert!(
            plain.requests.iter().all(|u| !u.ends_with(blocked)),
            "{blocked} was requested"
        );
        assert!(
            ignore.requests.iter().any(|u| u.ends_with(blocked)),
            "{blocked} should be requested with the override"
        );
    }
    assert!(plain
        .requests
        .iter()
        .any(|u| u.ends_with("/private/public-faq")));
    assert_eq!(plain.outcome.sample.coverage.excluded_by_robots, 2);
    assert!(plain
        .outcome
        .inventory
        .discovery_limits
        .contains(&"robots_excluded: 2 urls".to_string()));
    assert!(ignore
        .outcome
        .inventory
        .discovery_limits
        .contains(&"robots_overridden_by_operator".to_string()));
    assert_eq!(ignore.outcome.sample.coverage.excluded_by_robots, 0);
}

#[test]
fn crawl_delay_paces_requests_on_the_manual_clock() {
    for run in runs("robots-disallow-section") {
        // robots.txt is fetched at the default pace; after it the 5 second Crawl-delay applies.
        let n = run.requests.len() as u64;
        assert!(
            run.slept_ms >= 500 + (n - 2) * 5_000,
            "{}: slept {} ms for {} requests",
            run.suffix,
            run.slept_ms,
            n
        );
        assert!(run.slept_ms <= (n - 1) * 5_000);
    }
    let small = &runs("small-site")[0];
    assert_eq!(small.slept_ms, (small.requests.len() as u64 - 1) * 500);
}

#[test]
fn sitemap_index_case_reads_gzip_and_skips_unsafe_sitemaps() {
    let run = &runs("sitemap-index-gzip")[0];
    assert!(
        run.requests
            .iter()
            .all(|u| !u.contains("10.0.0.5") && !u.contains("other.example")),
        "{:?}",
        run.requests
    );
    assert!(run
        .requests
        .iter()
        .any(|u| u.ends_with("/sitemap-a.xml.gz")));
    let from_gz = run
        .outcome
        .inventory
        .routes
        .iter()
        .filter(|r| r.url.contains("/a/p-"))
        .count();
    assert_eq!(from_gz, 4);
    assert_eq!(run.outcome.sample.coverage.off_scope, 3);
}

#[test]
fn process_incomplete_lists_the_missing_step_and_marks_the_run_partial() {
    let s = &runs("process-incomplete")[0].outcome.sample;
    assert!(s.partial);
    let apply = s
        .processes
        .iter()
        .find(|p| p.name == "apply-permit")
        .unwrap();
    assert!(!apply.complete);
    assert_eq!(
        apply.missing,
        vec!["https://example.gov/apply/confirm".to_string()]
    );
    let contact = s.processes.iter().find(|p| p.name == "contact-us").unwrap();
    assert!(contact.complete && contact.missing.is_empty());
}

#[test]
fn budget_exhaustion_is_partial_with_remaining_work() {
    let run = &runs("budget-exhausted")[0];
    let s = &run.outcome.sample;
    assert!(s.partial);
    assert_eq!(s.coverage.fetched, 3);
    assert!(s.remaining_work.iter().any(|r| r.contains("not fetched")));
    assert!(run
        .outcome
        .inventory
        .routes
        .iter()
        .any(|r| !r.fetched && r.cluster_id.is_some()));
    assert!(run
        .outcome
        .inventory
        .templates
        .iter()
        .any(|t| t.dom_shape.is_none() && t.fetched_members == 0));
}

#[test]
fn seed_changes_the_random_sample_but_not_the_inventory() {
    let runs = runs("seed-stability");
    assert_eq!(runs.len(), 2);
    let (a, b) = (&runs[0], &runs[1]);
    assert_eq!(inventory_json(a), inventory_json(b));
    assert_eq!(a.outcome.sample.seed, "alpha");
    assert_eq!(b.outcome.sample.seed, "beta");
    assert_eq!(a.outcome.sample.structured, b.outcome.sample.structured);
    assert_ne!(a.outcome.sample.random, b.outcome.sample.random);
    assert_eq!(a.outcome.sample.random.len(), 7);
}

#[test]
fn seed_defaults_to_the_run_id_and_is_recorded() {
    let small = &runs("small-site")[0].outcome.sample;
    assert_eq!(small.seed, small.run_id);
}

#[test]
fn audit_plan_sampling_carries_method_seed_and_rationale() {
    let s = &runs("small-site")[0].outcome.sample;
    let v = diligent_inventory::audit_plan_sampling(s);
    assert_eq!(v["method"], "wcag-em-2.0");
    assert_eq!(v["seed"], s.seed.as_str());
    let rationale = v["rationale"].as_str().unwrap();
    assert!(rationale.contains("never supports a whole-site conformance claim"));
    assert!(rationale.contains(&s.algorithm));
    let plan_schema: Value = read(&repo_root().join("schemas/audit-plan.schema.json"));
    let sampling_schema = plan_schema["properties"]["sampling"].clone();
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&sampling_schema)
        .unwrap();
    assert_valid(&validator, &v, "audit plan sampling");
}

#[test]
fn https_start_redirecting_to_plain_http_www_is_not_trusted() {
    let run = &runs("scheme-downgrade")[0];
    assert!(run.outcome.start_failure.is_some());
    let route = &run.outcome.inventory.routes[0];
    assert_eq!(route.unreachable.as_deref(), Some("off_scope_redirect"));
    assert!(run
        .outcome
        .inventory
        .discovery_limits
        .contains(&"redirect_scheme_downgrade_not_followed: http://www.example.gov/".to_string()));
    assert!(run.outcome.sample.structured.is_empty());
}

#[test]
fn time_budget_stops_the_run_and_names_the_crawl_delay() {
    let run = &runs("time-budget")[0];
    let s = &run.outcome.sample;
    assert!(s.partial);
    assert_eq!(
        s.remaining_work,
        ["time budget of 60 s reached after 6 fetches"]
    );
    assert_eq!(
        run.requests.len(),
        6,
        "the seventh request would start at 60 s"
    );
    let limits = &run.outcome.inventory.discovery_limits;
    assert!(limits.iter().any(|l| l.starts_with("time_budget_reached:")));
    assert!(limits
        .iter()
        .any(|l| l.starts_with("request_interval:") && l.contains("Crawl-delay 10 s")));
    assert!(run.outcome.start_failure.is_none());
}

#[test]
fn robots_disallow_all_still_writes_routes_and_an_empty_sample() {
    let run = &runs("robots-disallow-all")[0];
    assert!(run.outcome.start_failure.is_none());
    let inv = &run.outcome.inventory;
    assert!(!inv.routes.is_empty());
    assert!(inv
        .routes
        .iter()
        .all(|r| r.excluded_by_robots && !r.fetched && r.cluster_id.is_some()));
    assert!(inv
        .discovery_limits
        .iter()
        .any(|l| l.starts_with("robots_excluded_all:")));
    let s = &run.outcome.sample;
    assert!(s.partial && s.structured.is_empty() && s.random.is_empty());
    assert_eq!(s.coverage.excluded_by_robots, inv.routes.len());
    assert_eq!(
        run.requests,
        [
            "https://example.gov/robots.txt",
            "https://example.gov/sitemap.xml"
        ]
    );
}
