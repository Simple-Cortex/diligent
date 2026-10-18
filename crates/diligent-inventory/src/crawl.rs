//! Discovery: robots.txt, sitemaps, then a shape-first breadth-first crawl of same-origin links.
//!
//! Every request goes through `diligent_infra::observe::follow`, which vets each hop against the
//! address policy, pins the connection to the vetted addresses, revalidates redirects and refuses
//! to leave the registrable domain. Requests are one at a time and rate limited with a monotonic
//! clock. Page content is data: it never changes scope, rate or permissions.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use diligent_infra::fetch::{FetchError, Fetcher, Resolver, MAX_REDIRECTS};
use diligent_infra::observe::{declared_sitemap_urls, follow, registrable_domain, Chain, ChainEnd};
use url::Url;

use crate::clock::{Clock, SystemClock};
use crate::links::{analyze, resolve_links};
use crate::model::{
    Document, Inventory, InventoryOptions, Outcome, ProcessResult, Route, Template, Vendor,
};
use crate::robots::{Robots, AGENT};
use crate::sample::{build, SampleInput};
use crate::signature::{path_shape, signature, DomFacts};
use crate::sitemap::{self, Limits, SitemapFetch};
use crate::urls::{
    doc_kind_from_content_type, doc_kind_from_path, host_string, normalize, origin_key,
    parse_origin,
};

/// Largest `Crawl-delay` honored, in seconds.
const MAX_CRAWL_DELAY: f64 = 30.0;
/// Most `Sitemap:` lines read from robots.txt.
const MAX_DECLARED_SITEMAPS: usize = 10;
/// Most distinct off-origin URLs remembered for the `off_scope` count.
const MAX_OFF_SCOPE: usize = 100_000;
/// Source labels in output order.
const SOURCE_ORDER: [&str; 5] = ["sitemap", "robots_sitemap", "link", "process", "start"];

struct Page {
    url: Url,
    shape: String,
    sources: Vec<&'static str>,
    lastmod: Option<String>,
    excluded: bool,
    fetched: bool,
    status: Option<u16>,
    final_url: Option<String>,
    title: Option<String>,
    dom: Option<DomFacts>,
    unreachable: Option<String>,
    doc_kind: Option<&'static str>,
}

struct Crawler<'a> {
    opts: &'a InventoryOptions,
    fetcher: &'a dyn Fetcher,
    resolver: &'a dyn Resolver,
    clock: &'a dyn Clock,
    apex: String,
    start_bare: String,
    start_scheme: String,
    exempt: Option<String>,
    allowed: BTreeSet<String>,
    robots: Robots,
    interval_ms: u64,
    last_request_ms: Option<u64>,
    requests: usize,
    attempts: usize,
    pages: Vec<Page>,
    index: HashMap<String, usize>,
    pending: Vec<usize>,
    fetched_shapes: HashSet<String>,
    off_scope: HashSet<String>,
    off_scope_overflow: bool,
    vendors: BTreeMap<String, String>,
    docs: BTreeMap<String, &'static str>,
    notes: Vec<String>,
    remaining: Vec<String>,
    discovery_capped: bool,
    hrefs_capped_pages: usize,
    /// Clock reading when the run began, and the reading at which the time budget ends.
    started_ms: u64,
    deadline_ms: u64,
    time_budget_hit: bool,
    /// Why the request interval is what it is, for `discovery_limits`.
    interval_source: String,
    /// robots.txt disallowed the start URL and no page was fetched.
    robots_excluded_all: bool,
}

fn unreachable_kind(end: &ChainEnd) -> Option<&'static str> {
    match end {
        ChainEnd::Final => None,
        ChainEnd::TooManyRedirects => Some("too_many_redirects"),
        ChainEnd::BadScheme(_) => Some("bad_scheme"),
        ChainEnd::BadLocation(s) if s.starts_with("refused ") => Some("refused"),
        ChainEnd::BadLocation(_) => Some("bad_location"),
        ChainEnd::OffDomain(_) => Some("off_domain"),
        ChainEnd::Error(FetchError::Timeout) => Some("timeout"),
        ChainEnd::Error(FetchError::Tls(_)) => Some("tls"),
        ChainEnd::Error(FetchError::Connect(_)) => Some("connect"),
        ChainEnd::Error(FetchError::Refused(_)) => Some("refused"),
        ChainEnd::Error(FetchError::Other(_)) => Some("error"),
    }
}

impl<'a> Crawler<'a> {
    fn throttle(&mut self) {
        if let Some(last) = self.last_request_ms {
            let due = last.saturating_add(self.interval_ms);
            let now = self.clock.now_ms();
            if now < due {
                self.clock.sleep_ms(due - now);
            }
        }
        self.last_request_ms = Some(self.clock.now_ms());
    }

    /// True when the next request could not start before the time budget ends. Sticky: once
    /// true it stays true, so every later fetch is skipped.
    fn out_of_time(&mut self) -> bool {
        if self.time_budget_hit {
            return true;
        }
        let now = self.clock.now_ms();
        let next_start = match self.last_request_ms {
            Some(last) => now.max(last.saturating_add(self.interval_ms)),
            None => now,
        };
        if next_start >= self.deadline_ms {
            self.time_budget_hit = true;
        }
        self.time_budget_hit
    }

    fn fetch_chain(&mut self, url: &Url) -> Chain {
        self.throttle();
        self.requests += 1;
        follow(
            self.fetcher,
            self.resolver,
            url,
            MAX_REDIRECTS,
            &self.apex,
            self.exempt.as_deref(),
        )
    }

    /// True when `url` is on the start host, with or without a leading `www.`.
    fn is_start_variant(&self, url: &Url) -> bool {
        host_string(url).is_some_and(|h| bare_host(&h) == self.start_bare)
    }

    /// Whether the origin of `url` may be added after a redirect: the host is the start host
    /// (or its `www.` twin) and the scheme is https, or the start scheme under `--allow-private`.
    /// A downgrade is recorded in the limits and refused.
    fn may_extend_origin(&mut self, url: &Url) -> bool {
        if !self.is_start_variant(url) {
            return false;
        }
        let ok = url.scheme() == "https"
            || (self.opts.net.allow_private_origin && url.scheme() == self.start_scheme);
        if !ok {
            self.notes
                .push(format!("redirect_scheme_downgrade_not_followed: {url}"));
        }
        ok
    }

    fn in_scope(&self, url: &Url) -> bool {
        matches!(url.scheme(), "http" | "https") && self.allowed.contains(&origin_key(url))
    }

    fn note_off_scope(&mut self, url: &Url) {
        if self.off_scope.len() >= MAX_OFF_SCOPE {
            self.off_scope_overflow = true;
        } else {
            self.off_scope.insert(url.as_str().to_string());
        }
    }

    /// Records an in-scope URL. Returns its page index; documents and out-of-scope URLs return `None`.
    fn add_page(
        &mut self,
        url: Url,
        source: &'static str,
        lastmod: Option<String>,
    ) -> Option<usize> {
        if !self.in_scope(&url) {
            self.note_off_scope(&url);
            return None;
        }
        if let Some(kind) = doc_kind_from_path(&url) {
            if self.docs.len() < self.opts.max_discovered {
                self.docs.entry(url.as_str().to_string()).or_insert(kind);
            } else {
                self.discovery_capped = true;
            }
            return None;
        }
        let key = url.as_str().to_string();
        if let Some(&i) = self.index.get(&key) {
            let p = &mut self.pages[i];
            if !p.sources.contains(&source) {
                p.sources.push(source);
            }
            if p.lastmod.is_none() {
                p.lastmod = lastmod;
            }
            return Some(i);
        }
        if self.pages.len() >= self.opts.max_discovered {
            self.discovery_capped = true;
            return None;
        }
        let excluded = !self.opts.ignore_robots && !self.robots.allowed(&url);
        let i = self.pages.len();
        self.pages.push(Page {
            shape: path_shape(&url, &self.opts.query_keys),
            url,
            sources: vec![source],
            lastmod,
            excluded,
            fetched: false,
            status: None,
            final_url: None,
            title: None,
            dom: None,
            unreachable: None,
            doc_kind: None,
        });
        self.index.insert(key, i);
        if !excluded {
            self.pending.push(i);
        }
        Some(i)
    }

    fn take_pending(&mut self, idx: usize) -> bool {
        if let Some(pos) = self.pending.iter().position(|&p| p == idx) {
            self.pending.remove(pos);
            true
        } else {
            false
        }
    }

    /// Next page to fetch: the earliest pending page whose path shape has not been fetched yet,
    /// else the earliest pending page.
    fn next_pending(&mut self) -> Option<usize> {
        if self.pending.is_empty() {
            return None;
        }
        let pos = self
            .pending
            .iter()
            .position(|&i| !self.fetched_shapes.contains(&self.pages[i].shape))
            .unwrap_or(0);
        Some(self.pending.remove(pos))
    }

    fn fetch_sitemap_file(&mut self, url: &Url) -> SitemapFetch {
        if !self.in_scope(url) {
            self.note_off_scope(url);
            return SitemapFetch::Skip("off scope".to_string());
        }
        if self.out_of_time() {
            return SitemapFetch::Skip("time budget".to_string());
        }
        let chain = self.fetch_chain(url);
        if let Some(kind) = unreachable_kind(&chain.end) {
            return SitemapFetch::Skip(kind.to_string());
        }
        match chain.final_response() {
            Some(r) if r.status == 200 => SitemapFetch::Body {
                body: r.body.clone(),
                truncated: r.truncated,
            },
            Some(r) => SitemapFetch::Skip(format!("status {}", r.status)),
            None => SitemapFetch::Skip("no response".to_string()),
        }
    }

    fn fetch_page(&mut self, idx: usize, extend_origin: bool) {
        self.attempts += 1;
        let url = self.pages[idx].url.clone();
        let chain = self.fetch_chain(&url);
        self.fetched_shapes.insert(self.pages[idx].shape.clone());
        if let Some(kind) = unreachable_kind(&chain.end) {
            self.pages[idx].unreachable = Some(kind.to_string());
            return;
        }
        let Some(resp) = chain.final_response() else {
            self.pages[idx].unreachable = Some("error".to_string());
            return;
        };
        let final_key = origin_key(&resp.url);
        if !self.allowed.contains(&final_key) {
            if extend_origin && self.may_extend_origin(&resp.url) {
                self.notes
                    .push(format!("origin_added_after_redirect: {final_key}"));
                self.allowed.insert(final_key);
            } else {
                self.pages[idx].unreachable = Some("off_scope_redirect".to_string());
                return;
            }
        }
        let final_url = resp.url.clone();
        let status = resp.status;
        {
            let p = &mut self.pages[idx];
            p.fetched = true;
            p.status = Some(status);
            p.final_url = Some(final_url.as_str().to_string());
        }
        let content_type = resp.header("content-type").unwrap_or_default();
        if let Some(kind) = doc_kind_from_content_type(&content_type) {
            self.pages[idx].doc_kind = Some(kind);
            self.docs.entry(url.as_str().to_string()).or_insert(kind);
            return;
        }
        let lower = content_type.to_ascii_lowercase();
        let parseable = lower.is_empty() || lower.contains("html") || lower.contains("xml");
        if !parseable {
            return;
        }
        let facts = analyze(&resp.text());
        {
            let p = &mut self.pages[idx];
            p.title = facts.title.clone();
            p.dom = Some(facts.dom.clone());
        }
        if status >= 400 {
            return;
        }
        if facts.hrefs_capped {
            self.hrefs_capped_pages += 1;
        }
        let page_url = url.as_str().to_string();
        for link in resolve_links(&facts, &final_url, &self.opts.query_keys) {
            if self.in_scope(&link) {
                self.add_page(link, "link", None);
            } else {
                let forbidden = link
                    .host_str()
                    .is_none_or(diligent_netpolicy::is_forbidden_host_literal);
                if !forbidden {
                    if let Some(host) = host_string(&link) {
                        self.vendors.entry(host).or_insert_with(|| page_url.clone());
                    }
                }
                self.note_off_scope(&link);
            }
        }
    }
}

fn bare_host(host: &str) -> String {
    host.strip_prefix("www.").unwrap_or(host).to_string()
}

fn describe_robots_error(kind: &str) -> String {
    format!("robots_unavailable: robots.txt could not be read ({kind}); treated as disallow all (RFC 9309)")
}

/// Runs the inventory with the real clock.
pub fn inventory(
    start: &Url,
    opts: &InventoryOptions,
    fetcher: &dyn Fetcher,
    resolver: &dyn Resolver,
) -> Inventory {
    run(start, opts, fetcher, resolver, &SystemClock::new()).inventory
}

/// Runs discovery and sampling with an explicit clock.
pub fn run(
    start: &Url,
    opts: &InventoryOptions,
    fetcher: &dyn Fetcher,
    resolver: &dyn Resolver,
    clock: &dyn Clock,
) -> Outcome {
    let start_n = normalize(start, &opts.query_keys, true);
    let host = host_string(&start_n).unwrap_or_default();
    let apex = registrable_domain(&host);
    let exempt = opts.net.allow_private_origin.then(|| host.clone());
    let mut allowed: BTreeSet<String> = BTreeSet::new();
    allowed.insert(origin_key(&start_n));
    let mut bad_origins = Vec::new();
    for o in &opts.allowed_origins {
        match parse_origin(o) {
            Some(k) => {
                allowed.insert(k);
            }
            None => bad_origins.push(o.clone()),
        }
    }
    let rps = if opts.requests_per_second.is_finite() && opts.requests_per_second > 0.0 {
        opts.requests_per_second
    } else {
        2.0
    };
    let mut c = Crawler {
        opts,
        fetcher,
        resolver,
        clock,
        apex,
        start_bare: bare_host(&host),
        start_scheme: start_n.scheme().to_string(),
        exempt,
        allowed,
        robots: Robots::allow_all(),
        interval_ms: ((1000.0 / rps).ceil() as u64).max(1),
        last_request_ms: None,
        requests: 0,
        attempts: 0,
        pages: Vec::new(),
        index: HashMap::new(),
        pending: Vec::new(),
        fetched_shapes: HashSet::new(),
        off_scope: HashSet::new(),
        off_scope_overflow: false,
        vendors: BTreeMap::new(),
        docs: BTreeMap::new(),
        notes: Vec::new(),
        remaining: Vec::new(),
        discovery_capped: false,
        hrefs_capped_pages: 0,
        started_ms: clock.now_ms(),
        deadline_ms: clock.now_ms().saturating_add(opts.max_time_ms),
        time_budget_hit: false,
        interval_source: String::new(),
        robots_excluded_all: false,
    };
    c.interval_source = format!("--rps {}", trim_float(rps));
    for o in bad_origins {
        c.notes.push(format!(
            "allowed_origin_ignored: `{o}` is not an http(s) origin"
        ));
    }

    // 1. robots.txt
    let mut robots_url = start_n.clone();
    robots_url.set_path("/robots.txt");
    robots_url.set_query(None);
    let mut robots_text: Option<String> = None;
    {
        let chain = c.fetch_chain(&robots_url);
        let failure: Option<String> = match (unreachable_kind(&chain.end), chain.final_response()) {
            (Some(kind), _) => Some(kind.to_string()),
            (None, Some(r)) if (500..600).contains(&r.status) => {
                Some(format!("status {}", r.status))
            }
            _ => None,
        };
        if let Some(kind) = failure {
            if !opts.ignore_robots {
                c.notes.push(describe_robots_error(&kind));
                c.robots = Robots::parse("User-agent: *\nDisallow: /\n", AGENT);
            }
        } else if let Some(r) = chain.final_response().filter(|r| r.status == 200) {
            let key = origin_key(&r.url);
            if c.may_extend_origin(&r.url) && c.allowed.insert(key.clone()) {
                c.notes.push(format!("origin_added_after_redirect: {key}"));
            }
            let text = r.text();
            c.robots = Robots::parse(&text, AGENT);
            robots_text = Some(text);
        }
    }
    if opts.ignore_robots {
        let delay = c.robots.crawl_delay;
        c.robots = Robots::allow_all();
        c.robots.crawl_delay = delay;
    }
    if let Some(delay) = c.robots.crawl_delay {
        let capped = delay.min(MAX_CRAWL_DELAY);
        let wanted = (capped * 1000.0).ceil() as u64;
        if wanted > c.interval_ms {
            c.interval_source = format!(
                "robots.txt Crawl-delay {} s (longer than the {} ms from --rps {})",
                trim_float(capped),
                c.interval_ms,
                trim_float(rps)
            );
        }
        c.interval_ms = c.interval_ms.max(wanted);
        c.notes.push(format!(
            "crawl_delay: robots.txt asks for {delay} seconds between requests"
        ));
        if delay > MAX_CRAWL_DELAY {
            c.notes.push(format!(
                "crawl_delay_capped: honored {MAX_CRAWL_DELAY} seconds"
            ));
        }
    }

    // The start page is registered first so it is route zero.
    let start_idx = c.add_page(start_n.clone(), "start", None);

    // 2. sitemaps
    let mut roots: Vec<(Url, &'static str)> = Vec::new();
    let mut default_sitemap = start_n.clone();
    default_sitemap.set_path("/sitemap.xml");
    default_sitemap.set_query(None);
    roots.push((default_sitemap, "sitemap"));
    if let Some(text) = &robots_text {
        let declared = declared_sitemap_urls(text);
        if declared.len() > MAX_DECLARED_SITEMAPS {
            c.notes.push(format!(
                "robots_sitemap_limit: read the first {MAX_DECLARED_SITEMAPS} of {} Sitemap lines",
                declared.len()
            ));
        }
        for u in declared.into_iter().take(MAX_DECLARED_SITEMAPS) {
            roots.push((u, "robots_sitemap"));
        }
    }
    let limits = Limits {
        max_depth: sitemap::MAX_DEPTH,
        max_files: sitemap::MAX_FILES,
        max_body_bytes: opts.net.max_body_bytes,
        max_entries: opts.max_discovered,
    };
    let collected = sitemap::collect(&roots, &limits, &mut |u| c.fetch_sitemap_file(u));
    c.notes.extend(collected.notes);
    c.remaining.extend(collected.remaining);
    for (entry, label) in collected.entries {
        let url = normalize(&entry.loc, &opts.query_keys, false);
        c.add_page(url, label, entry.lastmod);
    }

    // 3. start page
    if let Some(i) = start_idx {
        if c.take_pending(i) && !c.out_of_time() {
            c.fetch_page(i, true);
        }
    }

    // 4. processes: fetched before the breadth-first crawl, inside the same budget.
    let mut step_pages: Vec<Vec<Result<usize, String>>> = Vec::new();
    for p in &opts.processes {
        let mut steps = Vec::new();
        for raw in &p.steps {
            let parsed = Url::parse(raw.trim())
                .ok()
                .filter(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some());
            let Some(u) = parsed else {
                steps.push(Err("not an http(s) URL".to_string()));
                continue;
            };
            let u = normalize(&u, &opts.query_keys, true);
            if !c.in_scope(&u) {
                c.note_off_scope(&u);
                steps.push(Err("off scope".to_string()));
                continue;
            }
            if doc_kind_from_path(&u).is_some() {
                c.add_page(u, "process", None);
                steps.push(Err("document, not fetched".to_string()));
                continue;
            }
            match c.add_page(u, "process", None) {
                None => steps.push(Err("not recorded (discovery limit)".to_string())),
                Some(i) => {
                    if c.pages[i].excluded {
                        steps.push(Err("excluded by robots.txt".to_string()));
                        continue;
                    }
                    if c.pending.contains(&i) && c.attempts < opts.max_fetch && !c.out_of_time() {
                        c.take_pending(i);
                        c.fetch_page(i, false);
                    }
                    steps.push(Ok(i));
                }
            }
        }
        step_pages.push(steps);
    }

    // 5. breadth-first crawl
    while c.attempts < opts.max_fetch {
        if c.out_of_time() {
            break;
        }
        let Some(i) = c.next_pending() else { break };
        c.fetch_page(i, false);
    }
    if c.time_budget_hit {
        let seconds = trim_float(opts.max_time_ms as f64 / 1000.0);
        c.remaining.push(format!(
            "time budget of {seconds} s reached after {} fetches",
            c.requests
        ));
        c.notes.push(format!(
            "time_budget_reached: stopped after {} requests ({} ms of {} ms) with {} discovered pages not fetched",
            c.requests,
            c.clock.now_ms().saturating_sub(c.started_ms),
            opts.max_time_ms,
            c.pending.len()
        ));
    } else if !c.pending.is_empty() {
        c.remaining.push(format!(
            "{} discovered pages were not fetched (max_fetch {})",
            c.pending.len(),
            opts.max_fetch
        ));
        c.notes.push(format!(
            "max_fetch_reached: stopped after {} pages with {} discovered pages not fetched",
            c.attempts,
            c.pending.len()
        ));
    }
    if c.discovery_capped {
        c.remaining.push(format!(
            "more than {} URLs were discovered; the rest were not recorded",
            opts.max_discovered
        ));
        c.notes.push(format!(
            "max_discovered_reached: recorded the first {} URLs",
            opts.max_discovered
        ));
    }
    if c.hrefs_capped_pages > 0 {
        c.notes.push(format!(
            "links_per_page_capped: {} pages had more than {} links; the rest were ignored",
            c.hrefs_capped_pages,
            crate::links::MAX_HREFS_PER_PAGE
        ));
    }

    // Start failure.
    let start_excluded = start_idx.map(|i| &c.pages[i]).is_some_and(|p| p.excluded);
    // robots.txt was read and disallows the start URL, and nothing could be fetched: still
    // write the inventory (routes from the sitemaps) and an empty sample, partial.
    c.robots_excluded_all = start_excluded && robots_text.is_some() && c.attempts == 0;
    if c.robots_excluded_all {
        c.notes.push(
            "robots_excluded_all: start URL disallowed; no page fetched; the FED-18 robots check carries the finding"
                .to_string(),
        );
        c.remaining
            .push("start URL disallowed by robots.txt: no page was fetched".to_string());
    }
    let start_failure = match start_idx.map(|i| &c.pages[i]) {
        None => Some("start URL is not an inventoriable page".to_string()),
        Some(_) if c.robots_excluded_all => None,
        Some(p) if p.excluded => Some("start URL is disallowed by robots.txt".to_string()),
        Some(p) => match (&p.unreachable, p.status) {
            (Some(k), _) => Some(format!("start URL could not be fetched ({k})")),
            (None, Some(s)) if s >= 400 => Some(format!("start URL answered with status {s}")),
            (None, None) if c.time_budget_hit => None,
            (None, None) => Some("start URL was not fetched".to_string()),
            _ => None,
        },
    };

    finish(c, start_n, step_pages, start_failure)
}

fn finish(
    mut c: Crawler<'_>,
    start_n: Url,
    step_pages: Vec<Vec<Result<usize, String>>>,
    start_failure: Option<String>,
) -> Outcome {
    let opts = c.opts;
    // Process results.
    let mut processes: Vec<ProcessResult> = Vec::new();
    let mut process_steps: Vec<Vec<String>> = Vec::new();
    for (p, steps) in opts.processes.iter().zip(&step_pages) {
        let mut missing = Vec::new();
        let mut urls = Vec::new();
        for (raw, step) in p.steps.iter().zip(steps) {
            let why = match step {
                Err(why) => Some(why.clone()),
                Ok(i) => {
                    let page = &c.pages[*i];
                    urls.push(page.url.as_str().to_string());
                    match (&page.unreachable, page.status, page.fetched) {
                        (Some(k), _, _) => Some(format!("unreachable ({k})")),
                        (None, Some(s), true) if s >= 400 => Some(format!("status {s}")),
                        (None, _, true) => None,
                        _ => Some("not fetched (max_fetch)".to_string()),
                    }
                }
            };
            if let Some(why) = why {
                missing.push(raw.clone());
                c.remaining.push(format!(
                    "process {}: step {} not completed ({})",
                    p.name, raw, why
                ));
            }
        }
        processes.push(ProcessResult {
            name: p.name.clone(),
            consequence: p.consequence.clone(),
            steps: p.steps.clone(),
            complete: missing.is_empty(),
            missing,
        });
        process_steps.push(urls);
    }

    // Routes and clusters.
    let mut by_cluster: BTreeMap<String, (String, Option<String>, Vec<usize>)> = BTreeMap::new();
    let mut cluster_ids: Vec<Option<String>> = vec![None; c.pages.len()];
    for (i, p) in c.pages.iter().enumerate() {
        if (p.excluded && !c.robots_excluded_all) || p.doc_kind.is_some() || p.unreachable.is_some()
        {
            continue;
        }
        let sig = signature(&p.url, p.dom.as_ref(), &opts.query_keys);
        cluster_ids[i] = Some(sig.cluster_id.clone());
        by_cluster
            .entry(sig.cluster_id)
            .or_insert_with(|| (sig.path_shape, sig.dom_shape, Vec::new()))
            .2
            .push(i);
    }
    let mut templates: Vec<Template> = by_cluster
        .into_iter()
        .map(|(id, (shape, dom, members))| {
            let any_fetched = members.iter().any(|&i| c.pages[i].fetched);
            let rep = members
                .iter()
                .filter(|&&i| !any_fetched || c.pages[i].fetched)
                .map(|&i| &c.pages[i].url)
                .min_by(|a, b| {
                    a.path()
                        .len()
                        .cmp(&b.path().len())
                        .then_with(|| a.as_str().cmp(b.as_str()))
                })
                .map(|u| u.as_str().to_string())
                .unwrap_or_default();
            Template {
                cluster_id: id,
                path_shape: shape,
                dom_shape: dom,
                size: members.len(),
                representative: rep,
                fetched_members: members.iter().filter(|&&i| c.pages[i].fetched).count(),
            }
        })
        .collect();
    templates.sort_by(|a, b| {
        b.size
            .cmp(&a.size)
            .then_with(|| a.representative.cmp(&b.representative))
    });

    let mut routes: Vec<Route> = c
        .pages
        .iter()
        .enumerate()
        .filter(|(_, p)| p.doc_kind.is_none())
        .map(|(i, p)| {
            let sources: Vec<String> = SOURCE_ORDER
                .iter()
                .filter(|s| p.sources.contains(s))
                .map(|s| (*s).to_string())
                .collect();
            Route {
                url: p.url.as_str().to_string(),
                cluster_id: cluster_ids[i].clone(),
                sources,
                fetched: p.fetched,
                status: p.status,
                final_url: p.final_url.clone(),
                title: p.title.clone(),
                has_form: p.dom.as_ref().is_some_and(|d| d.any_form),
                lastmod: p.lastmod.clone(),
                excluded_by_robots: p.excluded,
                unreachable: p.unreachable.clone(),
            }
        })
        .collect();
    routes.sort_by(|a, b| a.url.cmp(&b.url));

    let documents: Vec<Document> = c
        .docs
        .iter()
        .map(|(u, k)| Document {
            url: u.clone(),
            kind: (*k).to_string(),
        })
        .collect();
    let vendors: Vec<Vendor> = c
        .vendors
        .iter()
        .map(|(h, p)| Vendor {
            host: h.clone(),
            first_seen_on: p.clone(),
        })
        .collect();

    // Limits.
    let excluded = routes.iter().filter(|r| r.excluded_by_robots).count();
    let mut limits: Vec<String> = vec![
        "roles_and_states: a fetch-only crawler sees one anonymous role and one default state; authenticated roles and interactive states are not inventoried".to_string(),
        "client_rendered_navigation_not_followed: links created by JavaScript are not discovered".to_string(),
    ];
    if opts.ignore_robots {
        limits.push("robots_overridden_by_operator".to_string());
    } else {
        limits.push(format!("robots_excluded: {excluded} urls"));
    }
    limits.push(format!(
        "rate_limit: at most {} requests per second ({} ms between requests), one request at a time",
        trim_float(1000.0 / c.interval_ms as f64),
        c.interval_ms
    ));
    if c.interval_source.starts_with("robots.txt") {
        limits.push(format!(
            "request_interval: {} ms between requests, set by {}",
            c.interval_ms, c.interval_source
        ));
    }
    if !documents.is_empty() {
        limits.push(format!(
            "documents_not_fetched: {} documents",
            documents.len()
        ));
    }
    let off = c.off_scope.len();
    if off > 0 || c.off_scope_overflow {
        limits.push(format!(
            "off_scope: {}{} urls on other origins were not inventoried",
            off,
            if c.off_scope_overflow { "+" } else { "" }
        ));
    }
    limits.append(&mut c.notes);
    limits.push(format!("partial: {}", !c.remaining.is_empty()));
    for r in &c.remaining {
        limits.push(format!("remaining_work: {r}"));
    }

    let inventory = Inventory {
        schema_version: "1.0.0".to_string(),
        run_id: opts.run_id.clone(),
        audit_id: opts.audit_id.clone(),
        generated_by: format!("diligent-inventory/{}", env!("CARGO_PKG_VERSION")),
        routes,
        templates,
        components: Vec::new(),
        documents,
        vendors,
        roles: Vec::new(),
        states: Vec::new(),
        discovery_limits: limits,
    };
    let partial = !c.remaining.is_empty();
    let sample = build(&SampleInput {
        opts,
        start_url: start_n.as_str(),
        routes: &inventory.routes,
        templates: &inventory.templates,
        documents: &inventory.documents,
        vendors: &inventory.vendors,
        processes: &processes,
        process_steps: &process_steps,
        off_scope: off,
        partial,
        remaining_work: &c.remaining,
        robots_excluded_all: c.robots_excluded_all,
    });
    Outcome {
        inventory,
        sample,
        start_failure,
        requests: c.requests,
    }
}

fn trim_float(x: f64) -> String {
    let s = format!("{x:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
