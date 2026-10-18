//! Headless Chrome adapter built on `chromiumoxide`.
//!
//! Implements [`Browser`] over the Chrome DevTools Protocol. All page
//! interaction goes through typed CDP commands. The only `Runtime.evaluate`
//! calls use the constant strings in [`crate::scripts`]. Every CDP call is
//! wrapped in a timeout; a timeout is reported as [`ErrorKind::Timeout`] and
//! is never treated as success.

use crate::browser::{
    AxNode, AxTree, AxeResult, BoundingBox, Browser, DocumentMetrics, DomNode, DomSnapshot,
    FocusStop, FocusWalk, FocusWalkEnd, NavigationInfo, NetworkEntry, PerfMetrics,
    COMPUTED_STYLE_ALLOWLIST,
};
use crate::protocol::{ColorScheme, ErrorKind, Media, Viewport, WorkerError};
use crate::scripts;
use crate::util::origin_of;
use async_trait::async_trait;
use chromiumoxide::browser::{Browser as CdpBrowser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::accessibility::GetFullAxTreeParams;
use chromiumoxide::cdp::browser_protocol::dom::DescribeNodeParams;
use chromiumoxide::cdp::browser_protocol::dom_snapshot::CaptureSnapshotParams;
use chromiumoxide::cdp::browser_protocol::emulation::{
    MediaFeature, SetDeviceMetricsOverrideParams, SetFocusEmulationEnabledParams,
};
use chromiumoxide::cdp::browser_protocol::fetch::{
    ContinueRequestParams, EnableParams as FetchEnableParams, EventRequestPaused, FailRequestParams,
};
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType, InsertTextParams,
};
use chromiumoxide::cdp::browser_protocol::network::{
    EnableParams as NetworkEnableParams, EventLoadingFinished, EventRequestWillBeSent,
    EventResponseReceived,
};
use chromiumoxide::cdp::browser_protocol::network::{ErrorReason, ResourceType};
use chromiumoxide::cdp::browser_protocol::page::{
    AddScriptToEvaluateOnNewDocumentParams, RemoveScriptToEvaluateOnNewDocumentParams,
    ScriptIdentifier,
};
use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
use chromiumoxide::error::CdpError;
use chromiumoxide::page::{Page, ScreenshotParams};
use futures::StreamExt;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::{JoinHandle, JoinSet};

const REQUEST_HEADERS: [&str; 4] = ["accept", "accept-language", "user-agent", "referer"];
const RESPONSE_HEADERS: [&str; 5] = [
    "strict-transport-security",
    "content-security-policy",
    "x-frame-options",
    "referrer-policy",
    "x-content-type-options",
];

/// How long the focus walk waits after each Tab before it reads the focused
/// element, so handlers that move focus on a short timer are observed.
const FOCUS_SETTLE: Duration = Duration::from_millis(50);
/// Upper bound on one axe-core run, bundle evaluation included.
const CORROBORATOR_TIMEOUT: Duration = Duration::from_secs(60);
/// Pause between two reads of the accessibility tree while waiting for it to stop changing.
const AX_SETTLE: Duration = Duration::from_millis(100);
/// Most reads of the accessibility tree per capture.
const AX_SETTLE_READS: usize = 5;
/// Wait after load before the performance metrics are read, so late LCP and
/// layout-shift entries are in.
const PERF_SETTLE: Duration = Duration::from_millis(1_000);

/// Keys `press_key` accepts. Enter and Space are excluded on purpose: they
/// activate buttons and could submit a form.
const SAFE_KEYS: [(&str, &str, i64); 8] = [
    ("Tab", "Tab", 9),
    ("Escape", "Escape", 27),
    ("ArrowLeft", "ArrowLeft", 37),
    ("ArrowUp", "ArrowUp", 38),
    ("ArrowRight", "ArrowRight", 39),
    ("ArrowDown", "ArrowDown", 40),
    ("Home", "Home", 36),
    ("End", "End", 35),
];

/// Launch configuration for [`ChromeBrowser`].
#[derive(Debug, Clone)]
pub struct ChromeConfig {
    /// Explicit Chrome binary (`--chrome-path`). Falls back to
    /// `DILIGENT_CHROME`, then common install locations.
    pub chrome_path: Option<PathBuf>,
    /// Origins `navigate` may visit, as `scheme://host[:port]`.
    pub allowed_origins: Vec<String>,
    /// Directory holding fixture files for `fill_text` callers.
    pub fixtures_dir: Option<PathBuf>,
    /// Timeout applied to every CDP call.
    pub timeout: Duration,
    /// Pass `--no-sandbox`. Also enabled by `DILIGENT_CHROME_NO_SANDBOX=1`.
    pub no_sandbox: bool,
    /// Optional user agent override.
    pub user_agent: Option<String>,
    /// Strict mode (`--strict-origins`): block every `http`, `https` or `ws`-style
    /// request whose origin is not in `allowed_origins`, whatever its resource
    /// type, and log it with `blocked_by_policy`. Off by default, where only
    /// documents are allowlist-gated and public third-party subresources load.
    pub strict_origins: bool,
}

impl ChromeConfig {
    /// Config with a 30 second timeout and no allowed origins.
    pub fn new() -> Self {
        ChromeConfig {
            chrome_path: None,
            allowed_origins: Vec::new(),
            fixtures_dir: None,
            timeout: Duration::from_millis(30_000),
            no_sandbox: false,
            user_agent: None,
            strict_origins: false,
        }
    }
}

impl Default for ChromeConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Locate a Chrome binary: explicit path, `DILIGENT_CHROME`, then known paths.
pub fn find_chrome(explicit: Option<&Path>) -> Result<PathBuf, WorkerError> {
    if let Some(path) = explicit {
        return if path.is_file() {
            Ok(path.to_path_buf())
        } else {
            Err(WorkerError::new(
                ErrorKind::CollectorFailure,
                format!("--chrome-path {} is not a file", path.display()),
            ))
        };
    }
    if let Ok(value) = std::env::var("DILIGENT_CHROME") {
        let path = PathBuf::from(&value);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(WorkerError::new(
                ErrorKind::CollectorFailure,
                format!("DILIGENT_CHROME={value} is not a file"),
            ))
        };
    }
    const CANDIDATES: [&str; 9] = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
        "/opt/google/chrome/chrome",
    ];
    CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .ok_or_else(|| {
            WorkerError::new(
                ErrorKind::CollectorFailure,
                "no Chrome found. Install Google Chrome or Chromium, or pass --chrome-path or set DILIGENT_CHROME",
            )
        })
}

#[derive(Default)]
struct NetState {
    entries: Vec<NetworkEntry>,
    by_request: HashMap<String, usize>,
    finished_bytes: HashMap<String, u64>,
    document_request: Option<String>,
    /// A Document response came from a forbidden remote address (post-connect check).
    document_violation: bool,
}

/// Whether a Document-type request to `url` may proceed. Network documents
/// must be on the allowlist; `about:`, `blob:` and `data:` carry no network
/// origin and are not fetched from anywhere.
fn document_url_allowed(allowed: &[String], url: &str) -> bool {
    match origin_of(url) {
        Some(origin) => allowed.contains(&origin),
        None => {
            let lower = url.to_ascii_lowercase();
            lower == "about:blank" || lower.starts_with("data:") || lower.starts_with("blob:")
        }
    }
}

fn lock(state: &Mutex<NetState>) -> MutexGuard<'_, NetState> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

fn pick_headers(headers: &serde_json::Value, wanted: &[&str]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(map) = headers.as_object() {
        for (name, value) in map {
            let lower = name.to_ascii_lowercase();
            if wanted.contains(&lower.as_str()) {
                let text = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                out.insert(lower, text);
            }
        }
    }
    out
}

fn apply_response(
    entry: &mut NetworkEntry,
    response: &chromiumoxide::cdp::browser_protocol::network::Response,
) {
    entry.status = u16::try_from(response.status).ok();
    entry.mime = Some(response.mime_type.clone());
    entry.from_cache = response.from_disk_cache.unwrap_or(false)
        || response.from_prefetch_cache.unwrap_or(false)
        || response.from_service_worker.unwrap_or(false);
    entry.response_headers = pick_headers(response.headers.inner(), &RESPONSE_HEADERS);
}

/// True when `response` was served from a forbidden remote address and its origin
/// is not on the operator allowlist. The connection already happened, so the
/// entry is stripped to the URL and flagged instead of recording evidence.
fn violates_policy(
    allowed: &[String],
    response: &chromiumoxide::cdp::browser_protocol::network::Response,
) -> bool {
    let forbidden = response
        .remote_ip_address
        .as_deref()
        .map(|raw| raw.trim().trim_start_matches('[').trim_end_matches(']'))
        .and_then(|raw| raw.parse::<std::net::IpAddr>().ok())
        .is_some_and(|ip| diligent_netpolicy::forbidden_reason(ip).is_some());
    forbidden && !origin_of(&response.url).is_some_and(|o| allowed.contains(&o))
}

fn strip_entry(entry: &mut NetworkEntry) {
    entry.blocked_by_policy = true;
    entry.status = None;
    entry.mime = None;
    entry.from_cache = false;
    entry.request_headers.clear();
    entry.response_headers.clear();
}

fn new_entry(url: &str, method: &str, resource_type: &str) -> NetworkEntry {
    NetworkEntry {
        url: url.to_string(),
        method: method.to_string(),
        resource_type: resource_type.to_string(),
        status: None,
        mime: None,
        from_cache: false,
        redirect_chain: Vec::new(),
        third_party: false,
        blocked_by_policy: false,
        request_headers: BTreeMap::new(),
        response_headers: BTreeMap::new(),
    }
}

fn on_request(state: &Mutex<NetState>, allowed: &[String], event: &EventRequestWillBeSent) {
    let mut st = lock(state);
    let id = event.request_id.inner().clone();
    if event.redirect_response.is_none()
        && st
            .by_request
            .get(&id)
            .is_some_and(|&i| st.entries[i].blocked_by_policy)
    {
        // The policy handler already recorded this blocked request.
        return;
    }
    let resource_type = event
        .r#type
        .as_ref()
        .map(|t| format!("{t:?}"))
        .unwrap_or_else(|| "Other".to_string());
    let mut chain = Vec::new();
    let previous_index = st.by_request.get(&id).copied();
    if let (Some(redirect), Some(prev)) = (&event.redirect_response, previous_index) {
        let mut previous = st.entries[prev].clone();
        if violates_policy(allowed, redirect) {
            strip_entry(&mut previous);
            if previous.resource_type == "Document" {
                st.document_violation = true;
            }
        } else if !previous.blocked_by_policy {
            apply_response(&mut previous, redirect);
        }
        chain = previous.redirect_chain.clone();
        chain.push(previous.url.clone());
        st.entries[prev] = previous;
    }
    let mut entry = new_entry(&event.request.url, &event.request.method, &resource_type);
    entry.redirect_chain = chain;
    entry.request_headers = pick_headers(event.request.headers.inner(), &REQUEST_HEADERS);
    st.entries.push(entry);
    let index = st.entries.len() - 1;
    st.by_request.insert(id.clone(), index);
    if resource_type == "Document" && st.document_request.is_none() {
        st.document_request = Some(id);
    }
}

const MAX_CONCURRENT_LOOKUPS: usize = 16;
const MAX_CACHED_HOSTS: usize = 1024;
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(2);

/// True when `url`'s host is a forbidden literal, or resolves (any address) to a
/// forbidden range. Fails closed: a timeout, a lookup error, a task failure or a
/// saturated lookup queue all block. Only successful lookups are cached, and the
/// cache stops growing at [`MAX_CACHED_HOSTS`] (lookups are still vetted).
async fn host_blocked(
    cache: &Mutex<HashMap<String, bool>>,
    lookups: &Semaphore,
    url: &str,
) -> bool {
    let Some(host) = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
    else {
        // Not a host-bearing URL (data:, blob:, about:): nothing to resolve.
        return false;
    };
    if diligent_netpolicy::is_forbidden_host_literal(&host) {
        return true;
    }
    if let Some(&hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&host) {
        return hit;
    }
    let Ok(Ok(_permit)) = tokio::time::timeout(LOOKUP_TIMEOUT, lookups.acquire()).await else {
        return true;
    };
    let target = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    let lookup = tokio::task::spawn_blocking(move || {
        use std::net::ToSocketAddrs;
        (target.as_str(), 0u16)
            .to_socket_addrs()
            .map(|it| it.map(|a| a.ip()).collect::<Vec<_>>())
    });
    let blocked = match tokio::time::timeout(LOOKUP_TIMEOUT, lookup).await {
        Ok(Ok(Ok(addrs))) => diligent_netpolicy::is_forbidden_addrs(&addrs).is_some(),
        // Timeout, lookup error or JoinError: block and do not cache.
        _ => return true,
    };
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if guard.len() < MAX_CACHED_HOSTS {
        guard.insert(host, blocked);
    }
    blocked
}

/// Mark (or create) the network-log entry of a request failed by the host policy.
fn record_policy_block(state: &Mutex<NetState>, event: &EventRequestPaused) {
    let mut st = lock(state);
    let index = event
        .network_id
        .as_ref()
        .and_then(|id| st.by_request.get(id.inner()).copied());
    match index {
        Some(i) => st.entries[i].blocked_by_policy = true,
        None => {
            let resource_type = format!("{:?}", event.resource_type);
            let mut entry = new_entry(&event.request.url, &event.request.method, &resource_type);
            entry.blocked_by_policy = true;
            st.entries.push(entry);
            let i = st.entries.len() - 1;
            if let Some(id) = &event.network_id {
                st.by_request.insert(id.inner().clone(), i);
            }
        }
    }
}

fn on_response(state: &Mutex<NetState>, allowed: &[String], event: &EventResponseReceived) {
    let mut st = lock(state);
    let id = event.request_id.inner().clone();
    let index = match st.by_request.get(&id) {
        Some(&i) => i,
        None => {
            let resource_type = format!("{:?}", event.r#type);
            let entry = new_entry(&event.response.url, "GET", &resource_type);
            st.entries.push(entry);
            let i = st.entries.len() - 1;
            st.by_request.insert(id, i);
            i
        }
    };
    if violates_policy(allowed, &event.response) {
        strip_entry(&mut st.entries[index]);
        if st.entries[index].resource_type == "Document" {
            st.document_violation = true;
        }
    } else if !st.entries[index].blocked_by_policy {
        apply_response(&mut st.entries[index], &event.response);
    }
}

fn on_finished(state: &Mutex<NetState>, event: &EventLoadingFinished) {
    let mut st = lock(state);
    st.finished_bytes.insert(
        event.request_id.inner().clone(),
        event.encoded_data_length.max(0.0) as u64,
    );
}

/// Per-launch Chrome profile directory, removed on drop (errors ignored).
struct ProfileDir(PathBuf);

impl ProfileDir {
    /// Fresh `diligent-runtime-<pid>-<counter>` directory under
    /// `DILIGENT_CHROME_USER_DATA_DIR` if set, else the system temp dir. Chrome
    /// creates it, so concurrent launches never share a `SingletonLock`.
    fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let parent = std::env::var_os("DILIGENT_CHROME_USER_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        ProfileDir(parent.join(format!(
            "diligent-runtime-{}-{n}-{nanos}",
            std::process::id()
        )))
    }

    fn remove(&self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Drop for ProfileDir {
    fn drop(&mut self) {
        self.remove();
    }
}

/// [`Browser`] implementation over a locally launched headless Chrome.
pub struct ChromeBrowser {
    browser: CdpBrowser,
    page: Page,
    config: ChromeConfig,
    net: Arc<Mutex<NetState>>,
    tasks: Vec<JoinHandle<()>>,
    /// Per-request policy tasks; aborted on close and drop.
    request_tasks: Arc<Mutex<JoinSet<()>>>,
    page_origin: Option<String>,
    /// Id of the installed `PERF_INSTALL` new-document script, when there is one.
    perf_script: Option<ScriptIdentifier>,
    /// Declared last so the directory is removed after the browser handle drops.
    profile: ProfileDir,
}

impl Drop for ChromeBrowser {
    fn drop(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
        self.request_tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .abort_all();
    }
}

fn cdp_error(idempotent: bool, context: &str, error: CdpError) -> WorkerError {
    let kind = match error {
        CdpError::Timeout => ErrorKind::Timeout,
        _ => ErrorKind::CollectorFailure,
    };
    let message = format!("{context}: {error}");
    if idempotent {
        WorkerError::idempotent(kind, message)
    } else {
        WorkerError::new(kind, message)
    }
}

impl ChromeBrowser {
    /// Launch headless Chrome and open a blank page.
    pub async fn launch(config: ChromeConfig) -> Result<Self, WorkerError> {
        let chrome = find_chrome(config.chrome_path.as_deref())?;
        let env_no_sandbox = std::env::var("DILIGENT_CHROME_NO_SANDBOX").is_ok_and(|v| v == "1");
        let profile = ProfileDir::new();
        let mut builder = BrowserConfig::builder()
            .chrome_executable(&chrome)
            .request_timeout(config.timeout)
            .user_data_dir(&profile.0)
            .window_size(1280, 800)
            .respect_https_errors();
        if config.no_sandbox || env_no_sandbox {
            builder = builder.no_sandbox();
        }
        let browser_config = builder.build().map_err(|e| {
            WorkerError::new(
                ErrorKind::CollectorFailure,
                format!("Chrome configuration: {e}"),
            )
        })?;
        let launch = CdpBrowser::launch(browser_config);
        let (browser, mut handler) =
            match tokio::time::timeout(config.timeout.max(Duration::from_secs(20)), launch).await {
                Err(_) => {
                    return Err(WorkerError::idempotent(
                        ErrorKind::Timeout,
                        format!("Chrome at {} did not start in time", chrome.display()),
                    ))
                }
                Ok(Err(e)) => {
                    return Err(cdp_error(
                        true,
                        &format!("launching {}", chrome.display()),
                        e,
                    ))
                }
                Ok(Ok(pair)) => pair,
            };
        let mut tasks = vec![tokio::spawn(async move {
            while let Some(result) = handler.next().await {
                if result.is_err() {
                    break;
                }
            }
        })];
        let page = tokio::time::timeout(config.timeout, browser.new_page("about:blank"))
            .await
            .map_err(|_| WorkerError::idempotent(ErrorKind::Timeout, "opening a page timed out"))?
            .map_err(|e| cdp_error(true, "opening a page", e))?;

        let net = Arc::new(Mutex::new(NetState::default()));
        let mut request_events = page
            .event_listener::<EventRequestWillBeSent>()
            .await
            .map_err(|e| cdp_error(true, "listening for requests", e))?;
        let mut response_events = page
            .event_listener::<EventResponseReceived>()
            .await
            .map_err(|e| cdp_error(true, "listening for responses", e))?;
        let mut finished_events = page
            .event_listener::<EventLoadingFinished>()
            .await
            .map_err(|e| cdp_error(true, "listening for load events", e))?;
        let state = Arc::clone(&net);
        let net_allowed = config.allowed_origins.clone();
        tasks.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(event) = request_events.next() => on_request(&state, &net_allowed, &event),
                    Some(event) = response_events.next() => on_response(&state, &net_allowed, &event),
                    Some(event) = finished_events.next() => on_finished(&state, &event),
                    else => break,
                }
            }
        }));

        // Per-request enforcement: every request is paused by `Fetch`; Document
        // requests (main frame, iframes and each redirect hop) to origins outside
        // the allowlist are failed, everything else is continued untouched.
        let mut paused_events = page
            .event_listener::<EventRequestPaused>()
            .await
            .map_err(|e| cdp_error(true, "listening for paused requests", e))?;
        let fetch_page = page.clone();
        let allowed = config.allowed_origins.clone();
        let strict = config.strict_origins;
        let fetch_state = Arc::clone(&net);
        let host_cache: Arc<Mutex<HashMap<String, bool>>> = Arc::new(Mutex::new(HashMap::new()));
        let lookups = Arc::new(Semaphore::new(MAX_CONCURRENT_LOOKUPS));
        let request_tasks: Arc<Mutex<JoinSet<()>>> = Arc::new(Mutex::new(JoinSet::new()));
        let spawn_into = Arc::clone(&request_tasks);
        tasks.push(tokio::spawn(async move {
            while let Some(event) = paused_events.next().await {
                // One task per request so a slow lookup never stalls the others.
                let page = fetch_page.clone();
                let allowed = allowed.clone();
                let state = Arc::clone(&fetch_state);
                let cache = Arc::clone(&host_cache);
                let lookups = Arc::clone(&lookups);
                let mut set = spawn_into.lock().unwrap_or_else(|e| e.into_inner());
                // Reap finished tasks so the set does not grow for the whole run.
                while set.try_join_next().is_some() {}
                set.spawn(async move {
                    let url = event.request.url.as_str();
                    // Origins the operator allowlisted are exempt from the host policy
                    // (this is how a local dev server is scanned on purpose).
                    let allowlisted = origin_of(url).is_some_and(|o| allowed.contains(&o));
                    let policy_block = !allowlisted && host_blocked(&cache, &lookups, url).await;
                    // Strict mode: any network request outside the allowlist, of any
                    // resource type. Non-network schemes (`data:`, `blob:`, `about:`)
                    // never leave the browser and are not gated.
                    let strict_block = strict
                        && !allowlisted
                        && !["data:", "blob:", "about:"]
                            .iter()
                            .any(|p| url.starts_with(p));
                    let block = policy_block
                        || strict_block
                        || (event.resource_type == ResourceType::Document
                            && !document_url_allowed(&allowed, url));
                    if policy_block || strict_block {
                        record_policy_block(&state, &event);
                    }
                    let request_id = event.request_id.clone();
                    if block {
                        let _ = page
                            .execute(FailRequestParams::new(
                                request_id,
                                ErrorReason::BlockedByClient,
                            ))
                            .await;
                    } else {
                        let _ = page.execute(ContinueRequestParams::new(request_id)).await;
                    }
                });
            }
        }));

        let this = ChromeBrowser {
            browser,
            page,
            config,
            net,
            tasks,
            request_tasks,
            page_origin: None,
            perf_script: None,
            profile,
        };
        this.call(true, "enabling network events", |p| async move {
            p.execute(NetworkEnableParams::default()).await.map(|_| ())
        })
        .await?;
        this.call(true, "enabling request interception", |p| async move {
            // No patterns: intercept every URL at the request stage.
            p.execute(FetchEnableParams::builder().build())
                .await
                .map(|_| ())
        })
        .await?;
        this.call(true, "enabling focus emulation", |p| async move {
            p.execute(SetFocusEmulationEnabledParams::new(true))
                .await
                .map(|_| ())
        })
        .await?;
        if let Some(agent) = this.config.user_agent.clone() {
            this.call(true, "setting user agent", |p| async move {
                p.set_user_agent(agent).await.map(|_| ())
            })
            .await?;
        }
        Ok(this)
    }

    /// Run one CDP operation on the page under the configured timeout.
    async fn call<T, F, Fut>(&self, idempotent: bool, context: &str, f: F) -> Result<T, WorkerError>
    where
        F: FnOnce(Page) -> Fut,
        Fut: Future<Output = Result<T, CdpError>>,
    {
        match tokio::time::timeout(self.config.timeout, f(self.page.clone())).await {
            Err(_) => {
                let message = format!(
                    "{context} timed out after {} ms",
                    self.config.timeout.as_millis()
                );
                Err(if idempotent {
                    WorkerError::idempotent(ErrorKind::Timeout, message)
                } else {
                    WorkerError::new(ErrorKind::Timeout, message)
                })
            }
            Ok(Err(e)) => Err(cdp_error(idempotent, context, e)),
            Ok(Ok(value)) => Ok(value),
        }
    }

    fn origin_allowed(&self, url: &str) -> bool {
        match origin_of(url) {
            Some(origin) => self.config.allowed_origins.contains(&origin),
            None => false,
        }
    }

    /// Forget the page origin, park the tab on `about:blank` so nothing can be
    /// captured from it, and build the `blocked_origin` error.
    async fn park_and_block(&mut self, message: String) -> WorkerError {
        self.page_origin = None;
        let _ = self
            .call(true, "leaving the blocked page", |p| async move {
                p.goto("about:blank").await.map(|_| ())
            })
            .await;
        WorkerError::new(ErrorKind::BlockedOrigin, message)
    }

    /// Guard for every non-navigate action: a navigation must have succeeded
    /// and the page's current URL must still be on the allowlist.
    async fn require_allowed_page(&mut self) -> Result<(), WorkerError> {
        if lock(&self.net).document_violation {
            return Err(self
                .park_and_block("the page was served from a non-public remote address".to_string())
                .await);
        }
        if self.page_origin.is_none() {
            return Err(WorkerError::new(
                ErrorKind::BlockedOrigin,
                "no successful navigation to an allowed origin yet",
            ));
        }
        let current = self
            .call(
                true,
                "reading the page URL",
                |p| async move { p.url().await },
            )
            .await?
            .unwrap_or_default();
        if !self.origin_allowed(&current) {
            return Err(WorkerError::new(
                ErrorKind::BlockedOrigin,
                format!("the page is at {current}, which is outside the allowed origins"),
            ));
        }
        Ok(())
    }

    async fn eval_json(
        &self,
        context: &str,
        script: &'static str,
    ) -> Result<serde_json::Value, WorkerError> {
        let result = self
            .call(true, context, |p| async move {
                let mut params = EvaluateParams::new(script);
                params.return_by_value = Some(true);
                p.execute(params).await
            })
            .await?;
        let value = result.result.result.value.ok_or_else(|| {
            WorkerError::idempotent(
                ErrorKind::MalformedResponse,
                format!("{context}: script returned no value"),
            )
        })?;
        match value {
            serde_json::Value::String(text) => serde_json::from_str(&text).map_err(|e| {
                WorkerError::idempotent(ErrorKind::MalformedResponse, format!("{context}: {e}"))
            }),
            other => Ok(other),
        }
    }

    /// Evaluates a reviewed constant script, awaiting the promise it returns,
    /// under its own `timeout`. Returns the by-value result (`None` when the
    /// script produced `undefined`). A script exception is a `collector_failure`
    /// whose message is cut to 200 characters; a deadline is a `timeout`.
    async fn eval_json_await(
        &self,
        context: &str,
        script: &'static str,
        timeout: Duration,
    ) -> Result<Option<serde_json::Value>, WorkerError> {
        let page = self.page.clone();
        let result = match tokio::time::timeout(timeout, async move {
            let mut params = EvaluateParams::new(script);
            params.await_promise = Some(true);
            params.return_by_value = Some(true);
            page.execute(params).await
        })
        .await
        {
            Err(_) => {
                return Err(WorkerError::new(
                    ErrorKind::Timeout,
                    format!("{context} timed out after {} ms", timeout.as_millis()),
                ))
            }
            Ok(Err(e)) => return Err(cdp_error(false, context, e)),
            Ok(Ok(value)) => value,
        };
        if let Some(details) = result.result.exception_details.as_ref() {
            let text = details
                .exception
                .as_ref()
                .and_then(|e| e.description.clone())
                .unwrap_or_else(|| details.text.clone());
            let message: String = format!("{context}: {text}").chars().take(200).collect();
            return Err(WorkerError::new(ErrorKind::CollectorFailure, message));
        }
        Ok(result.result.result.value)
    }

    async fn read_ax_tree(&self) -> Result<AxTree, WorkerError> {
        let response = self
            .call(true, "capturing the accessibility tree", |p| async move {
                p.execute(GetFullAxTreeParams::default()).await
            })
            .await?;
        let mut nodes = Vec::with_capacity(response.result.nodes.len());
        for node in &response.result.nodes {
            let raw = serde_json::to_value(node).map_err(|e| {
                WorkerError::idempotent(
                    ErrorKind::MalformedResponse,
                    format!("accessibility node: {e}"),
                )
            })?;
            nodes.push(convert_ax(&raw));
        }
        Ok(AxTree { nodes })
    }

    /// Injects the vendored axe-core bundle when the page has no `window.axe`,
    /// runs the reviewed wrapper and parses its JSON.
    async fn run_axe(&self) -> Result<AxeResult, WorkerError> {
        let present = self
            .eval_json_await(
                "probing for axe-core",
                scripts::AXE_PRESENT,
                CORROBORATOR_TIMEOUT,
            )
            .await?;
        if present.and_then(|v| v.as_bool()) != Some(true) {
            self.eval_json_await(
                "loading axe-core",
                scripts::AXE_BUNDLE,
                CORROBORATOR_TIMEOUT,
            )
            .await?;
        }
        let text = self
            .eval_json_await("running axe-core", scripts::AXE_RUN, CORROBORATOR_TIMEOUT)
            .await?;
        let Some(serde_json::Value::String(text)) = text else {
            return Err(WorkerError::new(
                ErrorKind::MalformedResponse,
                "running axe-core: no JSON string returned",
            ));
        };
        serde_json::from_str(&text).map_err(|e| {
            WorkerError::new(
                ErrorKind::MalformedResponse,
                format!("running axe-core: {e}")
                    .chars()
                    .take(200)
                    .collect::<String>(),
            )
        })
    }

    async fn active_backend_id(&self) -> Result<Option<i64>, WorkerError> {
        let result = self
            .call(true, "reading the active element", |p| async move {
                let mut params = EvaluateParams::new(scripts::ACTIVE_ELEMENT);
                params.return_by_value = Some(false);
                p.execute(params).await
            })
            .await?;
        let Some(object_id) = result.result.result.object_id.clone() else {
            return Ok(None);
        };
        let described = self
            .call(true, "describing the active element", |p| async move {
                let params = DescribeNodeParams {
                    object_id: Some(object_id),
                    ..Default::default()
                };
                p.execute(params).await
            })
            .await?;
        Ok(Some(*described.result.node.backend_node_id.inner()))
    }

    /// The Tab loop. Each Tab is a real key event the page can intercept. The
    /// focused element is read twice: at once, and after [`FOCUS_SETTLE`], so
    /// that a handler which moves focus a few milliseconds later (a blur
    /// handler that re-focuses, a timer) is seen instead of outrun.
    async fn walk_focus(&self, max_tabs: usize) -> Result<FocusWalk, WorkerError> {
        let mut stops: Vec<FocusStop> = Vec::new();
        let mut seen: Vec<i64> = Vec::new();
        let mut terminated_by = FocusWalkEnd::StopLimit;
        for _ in 0..max_tabs {
            self.tab().await?;
            let immediate = self.active_backend_id().await?;
            tokio::time::sleep(FOCUS_SETTLE).await;
            let read = self
                .eval_json("reading the focused element", scripts::FOCUS_READ)
                .await?;
            let settled = if read
                .get("is_body")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
            {
                None
            } else {
                self.active_backend_id().await?
            };
            let Some(backend_node_id) = settled else {
                terminated_by = if stops.is_empty() {
                    FocusWalkEnd::NoFocusable
                } else {
                    FocusWalkEnd::ReturnedToBody
                };
                break;
            };
            if let Some(position) = seen.iter().position(|id| *id == backend_node_id) {
                // A first stop reached straight from the Tab is the browser wrapping
                // around. Reached after the Tab landed elsewhere, the page pulled focus back.
                let pulled_back = immediate != Some(backend_node_id);
                terminated_by = if position == 0 && !pulled_back {
                    FocusWalkEnd::ReturnedToFirst
                } else {
                    FocusWalkEnd::RepeatOf(position)
                };
                break;
            }
            seen.push(backend_node_id);
            let bounding_box = read.get("rect").and_then(|rect| {
                let field = |key: &str| rect.get(key).and_then(serde_json::Value::as_f64);
                Some(BoundingBox {
                    x: field("x")?,
                    y: field("y")?,
                    width: field("width")?,
                    height: field("height")?,
                })
            });
            stops.push(FocusStop {
                backend_node_id,
                tag: read
                    .get("tag")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                name: read
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                had_visible_indicator: read.get("indicator").and_then(|v| v.as_bool()),
                bounding_box,
            });
        }
        Ok(FocusWalk {
            stops,
            terminated_by,
        })
    }

    async fn tab(&self) -> Result<(), WorkerError> {
        self.dispatch_key("Tab").await
    }

    async fn dispatch_key(&self, key: &str) -> Result<(), WorkerError> {
        let Some(&(name, code, vk)) = SAFE_KEYS.iter().find(|(k, _, _)| *k == key) else {
            return Err(WorkerError::new(
                ErrorKind::UnsupportedFeature,
                format!("key `{key}` is not in the safe key allowlist"),
            ));
        };
        for kind in [
            DispatchKeyEventType::RawKeyDown,
            DispatchKeyEventType::KeyUp,
        ] {
            let mut params = DispatchKeyEventParams::new(kind);
            params.key = Some(name.to_string());
            params.code = Some(code.to_string());
            params.windows_virtual_key_code = Some(vk);
            params.native_virtual_key_code = Some(vk);
            self.call(true, "dispatching a key event", |p| async move {
                p.execute(params).await.map(|_| ())
            })
            .await?;
        }
        Ok(())
    }
}

fn string_at(strings: &[String], index: i64) -> Option<&str> {
    if index < 0 {
        None
    } else {
        strings.get(index as usize).map(String::as_str)
    }
}

fn ax_text(value: &serde_json::Value) -> Option<String> {
    let inner = value.get("value")?;
    match inner {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn convert_ax(raw: &serde_json::Value) -> AxNode {
    let name_source = raw
        .get("name")
        .and_then(|n| n.get("sources"))
        .and_then(|s| s.as_array())
        .and_then(|sources| {
            sources
                .iter()
                .find(|s| {
                    s.get("superseded").and_then(|v| v.as_bool()) != Some(true)
                        && s.get("value").is_some()
                })
                .or_else(|| sources.first())
        })
        .and_then(|s| s.get("type"))
        .and_then(|t| t.as_str())
        .map(str::to_string);
    let properties = raw
        .get("properties")
        .and_then(|p| p.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|p| {
                    let name = p.get("name")?.as_str()?.to_string();
                    let value = p
                        .get("value")?
                        .get("value")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    Some((name, value))
                })
                .collect()
        })
        .unwrap_or_default();
    AxNode {
        node_id: raw
            .get("nodeId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        role: raw.get("role").and_then(ax_text),
        name: raw.get("name").and_then(ax_text),
        name_source,
        description: raw.get("description").and_then(ax_text),
        value: raw.get("value").and_then(ax_text),
        properties,
        ignored: raw
            .get("ignored")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        backend_dom_node_id: raw.get("backendDOMNodeId").and_then(|v| v.as_i64()),
        children: raw
            .get("childIds")
            .and_then(|c| c.as_array())
            .map(|ids| {
                ids.iter()
                    .filter_map(|i| i.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

#[async_trait]
impl Browser for ChromeBrowser {
    async fn navigate(
        &mut self,
        url: &str,
        timeout: Duration,
    ) -> Result<NavigationInfo, WorkerError> {
        if !self.origin_allowed(url) {
            return Err(WorkerError::new(
                ErrorKind::BlockedOrigin,
                format!("{url} is outside the allowed origins"),
            )
            .for_action(&crate::protocol::Action::Navigate {
                url: url.to_string(),
            }));
        }
        {
            let mut st = lock(&self.net);
            *st = NetState::default();
        }
        let target = url.to_string();
        let navigation = async {
            self.page.goto(target.as_str()).await?;
            self.page.wait_for_navigation().await?;
            Ok::<(), CdpError>(())
        };
        match tokio::time::timeout(timeout, navigation).await {
            Err(_) => {
                return Err(WorkerError::idempotent(
                    ErrorKind::Timeout,
                    format!(
                        "navigation to {url} timed out after {} ms",
                        timeout.as_millis()
                    ),
                ))
            }
            Ok(Err(e)) => return Err(cdp_error(true, &format!("navigating to {url}"), e)),
            Ok(Ok(())) => {}
        }
        let final_url = self
            .call(
                true,
                "reading the page URL",
                |p| async move { p.url().await },
            )
            .await?
            .unwrap_or_else(|| url.to_string());
        if !self.origin_allowed(&final_url) {
            self.page_origin = None;
            // Park the tab so no later capture can read the off-origin page.
            let _ = self
                .call(true, "leaving the blocked page", |p| async move {
                    p.goto("about:blank").await.map(|_| ())
                })
                .await;
            return Err(WorkerError::new(
                ErrorKind::BlockedOrigin,
                format!("navigation ended at {final_url}, which is outside the allowed origins"),
            )
            .for_action(&crate::protocol::Action::Navigate {
                url: url.to_string(),
            }));
        }
        if lock(&self.net).document_violation {
            return Err(self
                .park_and_block(format!(
                    "{final_url} was served from a non-public remote address"
                ))
                .await
                .for_action(&crate::protocol::Action::Navigate {
                    url: url.to_string(),
                }));
        }
        self.page_origin = origin_of(&final_url);
        let meta = self
            .eval_json("reading document metadata", scripts::DOC_META)
            .await?;
        let title = meta
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let lang = meta
            .get("lang")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let st = lock(&self.net);
        // Redirects reuse the request id, so this is the final hop of the
        // first document request.
        let document = st
            .document_request
            .as_ref()
            .and_then(|id| st.by_request.get(id))
            .map(|&i| &st.entries[i]);
        let (status, redirect_chain) = match document {
            Some(d) => (d.status, d.redirect_chain.clone()),
            None => (None, Vec::new()),
        };
        let document_bytes = st
            .document_request
            .as_ref()
            .and_then(|id| st.finished_bytes.get(id).copied())
            .unwrap_or(0);
        Ok(NavigationInfo {
            final_url,
            status,
            redirect_chain,
            title,
            lang,
            document_bytes,
        })
    }

    async fn set_viewport(&mut self, viewport: Viewport) -> Result<(), WorkerError> {
        let params = SetDeviceMetricsOverrideParams::new(
            i64::from(viewport.width),
            i64::from(viewport.height),
            viewport.dpr,
            false,
        );
        self.call(true, "setting the viewport", |p| async move {
            p.execute(params).await.map(|_| ())
        })
        .await
    }

    async fn emulate_media(&mut self, media: Media) -> Result<(), WorkerError> {
        let features = vec![
            MediaFeature::new(
                "prefers-reduced-motion",
                if media.reduced_motion {
                    "reduce"
                } else {
                    "no-preference"
                },
            ),
            MediaFeature::new(
                "forced-colors",
                if media.forced_colors {
                    "active"
                } else {
                    "none"
                },
            ),
            MediaFeature::new(
                "prefers-color-scheme",
                match media.color_scheme {
                    ColorScheme::Light => "light",
                    ColorScheme::Dark => "dark",
                },
            ),
        ];
        self.call(true, "emulating media features", |p| async move {
            p.emulate_media_features(features).await.map(|_| ())
        })
        .await
    }

    async fn dom_snapshot(&mut self) -> Result<DomSnapshot, WorkerError> {
        self.require_allowed_page().await?;
        let styles: Vec<String> = COMPUTED_STYLE_ALLOWLIST
            .iter()
            .map(|s| s.to_string())
            .collect();
        let params = CaptureSnapshotParams::new(styles);
        let response = self
            .call(true, "capturing the DOM snapshot", |p| async move {
                p.execute(params).await
            })
            .await?;
        let result = response.result;
        let strings = &result.strings;
        let document = result.documents.first().ok_or_else(|| {
            WorkerError::idempotent(
                ErrorKind::MalformedResponse,
                "DOM snapshot contained no documents",
            )
        })?;
        let nodes = &document.nodes;
        let count = nodes.node_type.as_ref().map_or(0, Vec::len);

        let mut layout_of: HashMap<usize, usize> = HashMap::new();
        for (k, node_index) in document.layout.node_index.iter().enumerate() {
            if *node_index >= 0 {
                layout_of.entry(*node_index as usize).or_insert(k);
            }
        }

        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let node_type = nodes
                .node_type
                .as_ref()
                .and_then(|v| v.get(i))
                .copied()
                .unwrap_or(0);
            let raw_name = nodes
                .node_name
                .as_ref()
                .and_then(|v| v.get(i))
                .and_then(|s| string_at(strings, *s.inner()))
                .unwrap_or_default();
            let tag = if node_type == 1 {
                raw_name.to_ascii_lowercase()
            } else {
                raw_name.to_string()
            };
            let attributes = nodes
                .attributes
                .as_ref()
                .and_then(|v| v.get(i))
                .map(|a| {
                    a.inner()
                        .chunks(2)
                        .filter_map(|pair| {
                            let name = string_at(strings, *pair.first()?.inner())?;
                            let value = pair
                                .get(1)
                                .and_then(|v| string_at(strings, *v.inner()))
                                .unwrap_or_default();
                            Some((name.to_string(), value.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let text = if node_type == 3 {
                nodes
                    .node_value
                    .as_ref()
                    .and_then(|v| v.get(i))
                    .and_then(|s| string_at(strings, *s.inner()))
                    .map(str::to_string)
            } else {
                None
            };
            let parent = nodes
                .parent_index
                .as_ref()
                .and_then(|v| v.get(i))
                .copied()
                .filter(|p| *p >= 0)
                .map(|p| p as usize);
            let backend_node_id = nodes
                .backend_node_id
                .as_ref()
                .and_then(|v| v.get(i))
                .map(|b| *b.inner());
            let (bounding_box, style_map) = match layout_of.get(&i) {
                Some(&k) => {
                    let bounding_box = document.layout.bounds.get(k).and_then(|r| {
                        let v = r.inner();
                        (v.len() == 4).then(|| BoundingBox {
                            x: v[0],
                            y: v[1],
                            width: v[2],
                            height: v[3],
                        })
                    });
                    let mut map = BTreeMap::new();
                    if let Some(values) = document.layout.styles.get(k) {
                        for (name, index) in
                            COMPUTED_STYLE_ALLOWLIST.iter().zip(values.inner().iter())
                        {
                            if let Some(value) = string_at(strings, *index.inner()) {
                                map.insert((*name).to_string(), value.to_string());
                            }
                        }
                    }
                    (bounding_box, map)
                }
                None => (None, BTreeMap::new()),
            };
            out.push(DomNode {
                index: i,
                node_type,
                tag,
                attributes,
                text,
                parent,
                backend_node_id,
                bounding_box,
                styles: style_map,
            });
        }
        let metrics = self
            .eval_json("reading document metrics", scripts::DOCUMENT_METRICS)
            .await?;
        let dimension = |key: &str| -> u32 {
            metrics
                .get(key)
                .and_then(serde_json::Value::as_f64)
                .map_or(0, |v| v.clamp(0.0, f64::from(u32::MAX)) as u32)
        };
        Ok(DomSnapshot {
            document_url: string_at(strings, *document.document_url.inner())
                .unwrap_or_default()
                .to_string(),
            nodes: out,
            document_metrics: Some(DocumentMetrics {
                scroll_width: dimension("scroll_width"),
                scroll_height: dimension("scroll_height"),
                client_width: dimension("client_width"),
                client_height: dimension("client_height"),
            }),
        })
    }

    async fn ax_tree(&mut self) -> Result<AxTree, WorkerError> {
        self.require_allowed_page().await?;
        // Chrome builds the accessibility tree lazily, and a late image error or
        // layout can still change it right after load, mostly on a busy machine.
        // Capture until two reads in a row agree, up to AX_SETTLE_READS reads.
        let mut previous = self.read_ax_tree().await?;
        for _ in 1..AX_SETTLE_READS {
            tokio::time::sleep(AX_SETTLE).await;
            let next = self.read_ax_tree().await?;
            if next == previous {
                return Ok(next);
            }
            previous = next;
        }
        Ok(previous)
    }

    async fn focus_order(&mut self, max_tabs: usize) -> Result<FocusWalk, WorkerError> {
        self.require_allowed_page().await?;
        self.eval_json("resetting focus", scripts::FOCUS_RESET)
            .await?;
        self.eval_json("recording unfocused styles", scripts::FOCUS_BASELINE)
            .await?;
        let walk = self.walk_focus(max_tabs).await;
        // Always clean up the page, even when the walk failed.
        let finished = self
            .eval_json("finishing the focus walk", scripts::FOCUS_FINISH)
            .await;
        let reset = self
            .eval_json("resetting focus", scripts::FOCUS_RESET)
            .await;
        let walk = walk?;
        finished?;
        reset?;
        Ok(walk)
    }

    async fn run_corroborator(&mut self) -> Result<AxeResult, WorkerError> {
        self.require_allowed_page().await?;
        let ran = self.run_axe().await;
        // Always drop axe from the page, even when the run failed.
        let cleaned = self
            .eval_json_await(
                "removing axe-core",
                scripts::AXE_CLEANUP,
                CORROBORATOR_TIMEOUT,
            )
            .await;
        let result = ran?;
        cleaned?;
        Ok(result)
    }

    async fn install_perf(&mut self) -> Result<(), WorkerError> {
        if self.perf_script.is_some() {
            return Ok(());
        }
        let added = self
            .call(
                true,
                "installing the performance observers",
                |p| async move {
                    p.execute(AddScriptToEvaluateOnNewDocumentParams::new(
                        scripts::PERF_INSTALL,
                    ))
                    .await
                },
            )
            .await?;
        self.perf_script = Some(added.result.identifier);
        Ok(())
    }

    async fn remove_perf(&mut self) -> Result<(), WorkerError> {
        if let Some(id) = self.perf_script.take() {
            self.call(true, "removing the performance observers", |p| async move {
                p.execute(RemoveScriptToEvaluateOnNewDocumentParams::new(id))
                    .await
                    .map(|_| ())
            })
            .await?;
        }
        Ok(())
    }

    async fn read_perf(&mut self) -> Result<PerfMetrics, WorkerError> {
        self.require_allowed_page().await?;
        tokio::time::sleep(PERF_SETTLE).await;
        let read = self
            .eval_json("reading performance metrics", scripts::PERF_READ)
            .await;
        // Always drop the page-side state, even when the read failed.
        let cleaned = self
            .eval_json("removing the performance observers", scripts::PERF_CLEANUP)
            .await;
        let metrics = serde_json::from_value(read?).map_err(|e| {
            WorkerError::idempotent(
                ErrorKind::MalformedResponse,
                format!("reading performance metrics: {e}"),
            )
        })?;
        cleaned?;
        Ok(metrics)
    }

    async fn apply_text_spacing(&mut self) -> Result<(), WorkerError> {
        self.require_allowed_page().await?;
        let applied = self
            .eval_json(
                "applying the text-spacing stylesheet",
                scripts::TEXT_SPACING_APPLY,
            )
            .await?;
        if applied.as_bool() == Some(true) {
            Ok(())
        } else {
            Err(WorkerError::new(
                ErrorKind::CollectorFailure,
                "the page refused the text-spacing stylesheet (a Content-Security-Policy may forbid inline styles)",
            ))
        }
    }

    async fn remove_text_spacing(&mut self) -> Result<(), WorkerError> {
        self.require_allowed_page().await?;
        self.eval_json(
            "removing the text-spacing stylesheet",
            scripts::TEXT_SPACING_REMOVE,
        )
        .await?;
        Ok(())
    }

    async fn screenshot(&mut self) -> Result<Vec<u8>, WorkerError> {
        self.require_allowed_page().await?;
        self.call(true, "capturing a screenshot", |p| async move {
            p.screenshot(ScreenshotParams::builder().build()).await
        })
        .await
    }

    async fn network_log(&mut self) -> Result<Vec<NetworkEntry>, WorkerError> {
        self.require_allowed_page().await?;
        let origin = self.page_origin.clone();
        let st = lock(&self.net);
        Ok(st
            .entries
            .iter()
            .map(|entry| {
                let mut entry = entry.clone();
                entry.third_party = match (&origin, origin_of(&entry.url)) {
                    (Some(page), Some(other)) => *page != other,
                    // Non-http(s) schemes such as data: and blob: are not third parties.
                    _ => false,
                };
                entry
            })
            .collect())
    }

    async fn focus(&mut self, selector: &str) -> Result<(), WorkerError> {
        self.require_allowed_page().await?;
        let selector = selector.to_string();
        self.call(false, "focusing an element", |p| async move {
            let element = p.find_element(selector).await?;
            element.focus().await.map(|_| ())
        })
        .await
    }

    async fn press_key(&mut self, key: &str) -> Result<(), WorkerError> {
        self.require_allowed_page().await?;
        self.dispatch_key(key).await.map_err(|mut e| {
            e.retryable = false;
            e
        })
    }

    async fn fill_text(&mut self, selector: &str, text: &str) -> Result<(), WorkerError> {
        self.require_allowed_page().await?;
        self.focus(selector).await?;
        let params = InsertTextParams::new(text);
        self.call(false, "inserting text", |p| async move {
            p.execute(params).await.map(|_| ())
        })
        .await
    }

    async fn close(&mut self) -> Result<(), WorkerError> {
        let closed = tokio::time::timeout(self.config.timeout, self.browser.close()).await;
        if let Ok(Ok(_)) = closed {
            let _ = tokio::time::timeout(Duration::from_secs(5), self.browser.wait()).await;
        }
        for task in self.tasks.drain(..) {
            task.abort();
        }
        self.request_tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .abort_all();
        self.profile.remove();
        match closed {
            Err(_) => Err(WorkerError::new(
                ErrorKind::Timeout,
                "closing Chrome timed out",
            )),
            Ok(Err(e)) => Err(cdp_error(false, "closing Chrome", e)),
            Ok(Ok(_)) => Ok(()),
        }
    }
}
