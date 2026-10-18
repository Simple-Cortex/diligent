//! Output and option types. `Inventory` serializes to `INVENTORY.json`
//! (`schemas/inventory.schema.json`) and `Sample` to `SAMPLE.json`
//! (`schemas/sample.schema.json`).

use diligent_infra::Options;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version of the `diligent-sampler` selection algorithm recorded in `SAMPLE.json`.
pub const ALGORITHM_ID: &str = "diligent-sampler/1";
/// Sampling method recorded in `SAMPLE.json`.
pub const METHOD: &str = "wcag-em-2.0";
/// The fixed statement every `SAMPLE.json` carries.
pub const STATEMENT: &str =
    "A sample never supports a whole-site conformance claim. Results apply to the sampled pages and processes.";
/// The sentence that qualifies `structured_coverage_estimate`.
pub const ESTIMATE_NOTE: &str = "This estimates how well the structured sample represents the discovered pages; it is not a conformance claim.";

/// Default time budget of a run, in milliseconds (five minutes).
pub const DEFAULT_MAX_TIME_MS: u64 = 300_000;

/// One caller-supplied multi-step task. Processes are never inferred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessSpec {
    /// Short name, used in sample reasons as `process:<name>`.
    pub name: String,
    /// `high` or `normal`.
    pub consequence: String,
    /// Step URLs in order.
    pub steps: Vec<String>,
}

/// Parses a `--processes` document: `[{name, consequence, steps: [url...]}]`.
pub fn parse_processes(value: &Value) -> Result<Vec<ProcessSpec>, String> {
    let list: Vec<ProcessSpec> = serde_json::from_value(value.clone())
        .map_err(|e| format!("processes must be [{{name, consequence, steps}}]: {e}"))?;
    for p in &list {
        if p.name.trim().is_empty() {
            return Err("a process has an empty name".to_string());
        }
        if !matches!(p.consequence.as_str(), "high" | "normal") {
            return Err(format!(
                "process `{}`: consequence must be `high` or `normal`",
                p.name
            ));
        }
        if p.steps.is_empty() {
            return Err(format!("process `{}` has no steps", p.name));
        }
    }
    Ok(list)
}

/// Run-time settings for [`crate::run`].
#[derive(Debug, Clone)]
pub struct InventoryOptions {
    /// Network settings shared with `diligent-infra`: timeout, body cap, private-origin exemption.
    pub net: Options,
    /// `run-YYYY-MM-DD-xxxxxx`.
    pub run_id: String,
    /// Kebab-case audit id.
    pub audit_id: String,
    /// Seed string for the random sample; `None` uses `run_id`.
    pub seed: Option<String>,
    /// Origins kept in addition to the start origin (`https://host`).
    pub allowed_origins: Vec<String>,
    /// Most pages fetched. Default 200.
    pub max_fetch: usize,
    /// Most in-scope URLs recorded. Default 5,000.
    pub max_discovered: usize,
    /// Most requests per second. Default 2.
    pub requests_per_second: f64,
    /// Query keys that identify a page. Every other key is dropped.
    pub query_keys: Vec<String>,
    /// Fetch pages that robots.txt disallows (operator override).
    pub ignore_robots: bool,
    /// Caller-supplied processes.
    pub processes: Vec<ProcessSpec>,
    /// Most documents in the structured sample. Default 10.
    pub max_documents: usize,
    /// Time budget for the whole run in milliseconds, checked before every request. Default 300,000.
    pub max_time_ms: u64,
}

impl InventoryOptions {
    /// Defaults with the given ids.
    pub fn new(run_id: impl Into<String>, audit_id: impl Into<String>) -> Self {
        InventoryOptions {
            net: Options::default(),
            run_id: run_id.into(),
            audit_id: audit_id.into(),
            seed: None,
            allowed_origins: Vec::new(),
            max_fetch: 200,
            max_discovered: 5_000,
            requests_per_second: 2.0,
            query_keys: Vec::new(),
            ignore_robots: false,
            processes: Vec::new(),
            max_documents: 10,
            max_time_ms: DEFAULT_MAX_TIME_MS,
        }
    }

    /// The seed string actually used.
    pub fn seed_string(&self) -> &str {
        self.seed.as_deref().unwrap_or(&self.run_id)
    }
}

/// One discovered page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Route {
    /// Normalized URL.
    pub url: String,
    /// Template cluster; `null` for pages with no usable response, and for pages robots.txt excluded
    /// unless robots.txt disallowed the start URL, when excluded pages are clustered by path shape only.
    pub cluster_id: Option<String>,
    /// How it was found: `sitemap`, `robots_sitemap`, `link`, `process`, `start`.
    pub sources: Vec<String>,
    /// A response was received.
    pub fetched: bool,
    /// HTTP status of the final response.
    pub status: Option<u16>,
    /// URL of the final response.
    pub final_url: Option<String>,
    /// Page title (untrusted text, cut to 200 characters).
    pub title: Option<String>,
    /// The page contains a `form`.
    pub has_form: bool,
    /// `lastmod` from a sitemap.
    pub lastmod: Option<String>,
    /// robots.txt disallowed it, so it was not fetched.
    pub excluded_by_robots: bool,
    /// Why no response was usable: `refused`, `off_domain`, `timeout`, `connect`, `tls`,
    /// `error`, `too_many_redirects`, `bad_scheme`, `bad_location`, `off_scope_redirect`.
    pub unreachable: Option<String>,
}

/// One template cluster.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Template {
    /// First 12 hex characters of the signature hash.
    pub cluster_id: String,
    /// Path shape.
    pub path_shape: String,
    /// DOM shape; `null` for clusters of pages that were not fetched.
    pub dom_shape: Option<String>,
    /// Pages in the cluster.
    pub size: usize,
    /// Shortest-path member, then smallest URL.
    pub representative: String,
    /// Members that were fetched.
    pub fetched_members: usize,
}

/// A document that was inventoried but not fetched.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Document {
    /// URL.
    pub url: String,
    /// `pdf`, `docx`, `xlsx` or `pptx`.
    pub kind: String,
}

/// An off-origin host that a fetched page links to.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Vendor {
    /// Host name.
    pub host: String,
    /// First fetched page (in crawl order) that links to it.
    pub first_seen_on: String,
}

/// `INVENTORY.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Inventory {
    /// `1.0.0`.
    pub schema_version: String,
    /// Run id.
    pub run_id: String,
    /// Audit id.
    pub audit_id: String,
    /// `diligent-inventory/<version>`.
    pub generated_by: String,
    /// Pages, sorted by URL.
    pub routes: Vec<Route>,
    /// Clusters, largest first.
    pub templates: Vec<Template>,
    /// Always empty: a fetch-only crawler does not see components.
    pub components: Vec<Value>,
    /// Documents, sorted by URL.
    pub documents: Vec<Document>,
    /// Vendors, sorted by host.
    pub vendors: Vec<Vendor>,
    /// Always empty; see `discovery_limits`.
    pub roles: Vec<Value>,
    /// Always empty; see `discovery_limits`.
    pub states: Vec<Value>,
    /// What the inventory does not cover.
    pub discovery_limits: Vec<String>,
}

/// A page in the structured sample.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StructuredItem {
    /// URL.
    pub url: String,
    /// Cluster, `null` for documents.
    pub cluster_id: Option<String>,
    /// `home`, `template:<id>`, `unique`, `form`, `process:<name>`, `document`, `vendor:<host>`.
    pub reason: String,
}

/// A page in the random sample.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RandomItem {
    /// URL.
    pub url: String,
    /// Cluster.
    pub cluster_id: Option<String>,
    /// Its template is represented in the structured sample.
    pub in_structured_cluster: bool,
}

/// How a caller-supplied process fared.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProcessResult {
    /// Name.
    pub name: String,
    /// `high` or `normal`.
    pub consequence: String,
    /// Steps as supplied.
    pub steps: Vec<String>,
    /// Every step was fetched with status below 400.
    pub complete: bool,
    /// Steps that were not.
    pub missing: Vec<String>,
}

/// Counts with their denominators.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Coverage {
    /// In-scope pages found (the denominator for everything below).
    pub discovered: usize,
    /// Pages with a response.
    pub fetched: usize,
    /// Template clusters.
    pub clustered: usize,
    /// Pages in the structured sample.
    pub sampled_structured: usize,
    /// Pages in the random sample.
    pub sampled_random: usize,
    /// Pages robots.txt excluded.
    pub excluded_by_robots: usize,
    /// Pages with no usable response.
    pub unreachable: usize,
    /// Distinct URLs on other origins that were not inventoried.
    pub off_scope: usize,
    /// Documents inventoried and not fetched.
    pub documents: usize,
}

/// `SAMPLE.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sample {
    /// `1.0.0`.
    pub schema_version: String,
    /// Run id.
    pub run_id: String,
    /// Audit id.
    pub audit_id: String,
    /// `wcag-em-2.0`.
    pub method: String,
    /// `diligent-sampler/1`.
    pub algorithm: String,
    /// Seed string.
    pub seed: String,
    /// PRNG id.
    pub prng: String,
    /// Structured sample.
    pub structured: Vec<StructuredItem>,
    /// Random sample.
    pub random: Vec<RandomItem>,
    /// Process outcomes.
    pub processes: Vec<ProcessResult>,
    /// Counts.
    pub coverage: Coverage,
    /// Share of random pages whose template is in the structured sample; `null` when none were drawn.
    pub structured_coverage_estimate: Option<f64>,
    /// The fixed statement.
    pub statement: String,
    /// Some work was left undone.
    pub partial: bool,
    /// What was left.
    pub remaining_work: Vec<String>,
}

/// The result of one inventory run.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// `INVENTORY.json`.
    pub inventory: Inventory,
    /// `SAMPLE.json`.
    pub sample: Sample,
    /// Set when the start URL was refused, unreachable or answered with an error status.
    pub start_failure: Option<String>,
    /// Requests sent (robots, sitemaps and pages).
    pub requests: usize,
}
