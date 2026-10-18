//! WCAG-EM 2.0 sample selection: a structured set, a seeded random set, and a comparison.
//!
//! Structured set, first reason wins per URL: the start URL (`home`); each template
//! representative (`template:<id>`, a fetched member when one exists, and never for an unfetched
//! cluster whose path shape a fetched cluster already covers); each singleton template (`unique`); every fetched page
//! with a form (`form`); every process step (`process:<name>`); up to `max_documents`
//! documents (`document`); the first page linking to each vendor host (`vendor:<host>`).
//!
//! Pages that robots.txt excluded or that had no usable response are never sampled; they are
//! counted in the coverage. Random set: `k = clamp(ceil(0.05 * discovered), 3, 25)` pages drawn without replacement
//! from the other discovered pages, which are sorted by URL before the draw so the same
//! inventory and seed always give the same sample.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{json, Value};

use crate::model::{
    Coverage, Document, InventoryOptions, ProcessResult, RandomItem, Route, Sample, StructuredItem,
    Template, Vendor, ALGORITHM_ID, METHOD, STATEMENT,
};
use crate::prng::{sample_without_replacement, XorShift64Star, PRNG_ID};

/// Everything the selection reads.
pub(crate) struct SampleInput<'a> {
    pub opts: &'a InventoryOptions,
    pub start_url: &'a str,
    pub routes: &'a [Route],
    pub templates: &'a [Template],
    pub documents: &'a [Document],
    pub vendors: &'a [Vendor],
    pub processes: &'a [ProcessResult],
    /// In-scope normalized step URLs per process, parallel to `processes`.
    pub process_steps: &'a [Vec<String>],
    pub off_scope: usize,
    pub partial: bool,
    pub remaining_work: &'a [String],
    /// robots.txt disallowed the start URL and no page was fetched: nothing is sampled.
    pub robots_excluded_all: bool,
}

/// Draw size for the random set.
pub fn random_sample_size(discovered: usize) -> usize {
    ((discovered * 5).div_ceil(100)).clamp(3, 25)
}

/// A route the sample may name: robots.txt did not exclude it and a response (or a pending fetch) exists.
fn sampleable(r: &Route) -> bool {
    !r.excluded_by_robots && r.unreachable.is_none()
}

pub(crate) fn build(input: &SampleInput<'_>) -> Sample {
    let routes: BTreeMap<&str, &Route> = input.routes.iter().map(|r| (r.url.as_str(), r)).collect();
    let cluster_of = |url: &str| routes.get(url).and_then(|r| r.cluster_id.clone());
    let mut structured: Vec<StructuredItem> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push = |url: &str, cluster: Option<String>, reason: String| {
        if seen.insert(url.to_string()) {
            structured.push(StructuredItem {
                url: url.to_string(),
                cluster_id: cluster,
                reason,
            });
        }
    };

    if input.robots_excluded_all {
        // Nothing may be sampled: every page is disallowed and none was fetched.
    } else if routes.get(input.start_url).is_some_and(|r| sampleable(r)) {
        push(
            input.start_url,
            cluster_of(input.start_url),
            "home".to_string(),
        );
    }
    // An unfetched path-only cluster is covered by a fetched cluster of the same path shape.
    let fetched_shapes: HashSet<&str> = input
        .templates
        .iter()
        .filter(|t| t.fetched_members > 0)
        .map(|t| t.path_shape.as_str())
        .collect();
    let covered =
        |t: &Template| t.fetched_members == 0 && fetched_shapes.contains(t.path_shape.as_str());
    let usable = |t: &Template| {
        !input.robots_excluded_all
            && routes
                .get(t.representative.as_str())
                .is_some_and(|r| sampleable(r))
    };
    for t in input
        .templates
        .iter()
        .filter(|t| t.size > 1 && !covered(t) && usable(t))
    {
        push(
            &t.representative,
            Some(t.cluster_id.clone()),
            format!("template:{}", t.cluster_id),
        );
    }
    for t in input
        .templates
        .iter()
        .filter(|t| t.size == 1 && !covered(t) && usable(t))
    {
        push(
            &t.representative,
            Some(t.cluster_id.clone()),
            "unique".to_string(),
        );
    }
    for r in input
        .routes
        .iter()
        .filter(|r| r.fetched && r.has_form && sampleable(r))
    {
        push(&r.url, r.cluster_id.clone(), "form".to_string());
    }
    for (p, steps) in input.processes.iter().zip(input.process_steps) {
        for url in steps {
            if routes.get(url.as_str()).is_some_and(|r| sampleable(r)) {
                push(url, cluster_of(url), format!("process:{}", p.name));
            }
        }
    }
    for d in input
        .documents
        .iter()
        .take(input.opts.max_documents)
        .filter(|_| !input.robots_excluded_all)
    {
        push(&d.url, None, "document".to_string());
    }
    for v in input.vendors.iter().filter(|_| !input.robots_excluded_all) {
        if routes.contains_key(v.first_seen_on.as_str()) {
            push(
                &v.first_seen_on,
                cluster_of(&v.first_seen_on),
                format!("vendor:{}", v.host),
            );
        }
    }

    // Random set.
    let discovered = input.routes.len();
    let candidates: Vec<&Route> = input
        .routes
        .iter()
        .filter(|r| sampleable(r) && !seen.contains(&r.url))
        .collect();
    let k = random_sample_size(discovered).min(candidates.len());
    let mut rng = XorShift64Star::from_seed(input.opts.seed_string());
    let drawn = sample_without_replacement(&candidates, k, &mut rng);

    let shape_of: HashMap<&str, &str> = input
        .templates
        .iter()
        .map(|t| (t.cluster_id.as_str(), t.path_shape.as_str()))
        .collect();
    let structured_ids: HashSet<&str> = structured
        .iter()
        .filter_map(|s| s.cluster_id.as_deref())
        .collect();
    let structured_shapes: HashSet<&str> = structured_ids
        .iter()
        .filter_map(|id| shape_of.get(id).copied())
        .collect();
    let random: Vec<RandomItem> = drawn
        .iter()
        .map(|r| {
            let in_cluster = match r.cluster_id.as_deref() {
                None => false,
                Some(id) if r.fetched => structured_ids.contains(id),
                Some(id) => {
                    structured_ids.contains(id)
                        || shape_of
                            .get(id)
                            .is_some_and(|s| structured_shapes.contains(s))
                }
            };
            RandomItem {
                url: r.url.clone(),
                cluster_id: r.cluster_id.clone(),
                in_structured_cluster: in_cluster,
            }
        })
        .collect();
    let estimate = if random.is_empty() {
        None
    } else {
        Some(random.iter().filter(|r| r.in_structured_cluster).count() as f64 / random.len() as f64)
    };

    let coverage = Coverage {
        discovered,
        fetched: input.routes.iter().filter(|r| r.fetched).count(),
        clustered: input.templates.len(),
        sampled_structured: structured.len(),
        sampled_random: random.len(),
        excluded_by_robots: input.routes.iter().filter(|r| r.excluded_by_robots).count(),
        unreachable: input
            .routes
            .iter()
            .filter(|r| r.unreachable.is_some())
            .count(),
        off_scope: input.off_scope,
        documents: input.documents.len(),
    };
    Sample {
        schema_version: "1.0.0".to_string(),
        run_id: input.opts.run_id.clone(),
        audit_id: input.opts.audit_id.clone(),
        method: METHOD.to_string(),
        algorithm: ALGORITHM_ID.to_string(),
        seed: input.opts.seed_string().to_string(),
        prng: PRNG_ID.to_string(),
        structured,
        random,
        processes: input.processes.to_vec(),
        coverage,
        structured_coverage_estimate: estimate,
        statement: STATEMENT.to_string(),
        partial: input.partial,
        remaining_work: input.remaining_work.to_vec(),
    }
}

/// The `sampling` object for `AUDIT-PLAN.json`: `{method, seed, rationale}`.
pub fn audit_plan_sampling(sample: &Sample) -> Value {
    let c = &sample.coverage;
    let estimate = match sample.structured_coverage_estimate {
        Some(e) => format!(
            "{} of {} random pages were in a template the structured sample covers",
            (e * sample.random.len() as f64).round() as usize,
            sample.random.len()
        ),
        None => "no random pages were drawn".to_string(),
    };
    let rationale = format!(
        "Structured sample of {} pages (home, template representatives, unique pages, forms, process steps, documents, vendor links) \
         plus a seeded random sample of {} pages drawn without replacement from {} discovered pages ({} fetched, {} templates); \
         {}. Algorithm {} with {}. {}",
        c.sampled_structured,
        c.sampled_random,
        c.discovered,
        c.fetched,
        c.clustered,
        estimate,
        sample.algorithm,
        sample.prng,
        sample.statement
    );
    json!({
        "method": sample.method,
        "seed": sample.seed,
        "rationale": rationale,
    })
}
