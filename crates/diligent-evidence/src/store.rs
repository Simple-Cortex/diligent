//! The on-disk evidence store: content-addressed objects plus append-only
//! index, cache-key and reuse logs.

use crate::error::{EvidenceError, Result};
use crate::io::{append_line, atomic_write, atomic_write_new, sha256_hex};
use crate::model::{
    is_sha256_hex, CacheKey, EvidenceRecord, IndexEntry, IndexRecord, IngestContext, IngestReport,
    KeyRecord, KeyStatus, Problem, Producer, Redaction, ResumePlan, ReuseRecord, Tombstone,
    VerifyReport, EVIDENCE_KINDS, SCHEMA_VERSION, STORE_VERSION,
};
use crate::time::now_rfc3339;
use diligent_capture::{CaptureSet, ManifestEntry, MAX_FILE_BYTES};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::path::{Component, Path, PathBuf};

const INDEX: &str = "index.jsonl";
const KEYS: &str = "keys.jsonl";
const REUSE: &str = "reuse.jsonl";
const MARKER: &str = "STORE.json";
/// Largest log file the store will read, in bytes.
const MAX_LOG_BYTES: u64 = 512 * 1024 * 1024;
/// Collector name written into records the store normalises itself.
const CAPTURE_COLLECTOR: &str = "diligent-runtime";

/// An evidence store directory.
pub struct Store {
    dir: PathBuf,
    clock: Box<dyn Fn() -> String>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

fn io_err(path: &Path, error: &std::io::Error) -> EvidenceError {
    EvidenceError::io(path, error)
}

/// Keeps only lowercase ASCII letters and digits, at most 16 of them.
fn sanitize_ext(ext: &str) -> String {
    let lower = ext.to_ascii_lowercase();
    if !lower.is_empty()
        && lower.len() <= 16
        && lower
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        lower
    } else {
        "bin".to_string()
    }
}

fn ext_of(name: &str) -> String {
    match Path::new(name).extension().and_then(|e| e.to_str()) {
        Some(ext) => sanitize_ext(ext),
        None => "bin".to_string(),
    }
}

/// Splits `<sha256>.<ext>` into its parts.
fn parse_object_name(name: &str) -> Option<(&str, &str)> {
    let (sha, ext) = name.split_once('.')?;
    let ext_ok = !ext.is_empty()
        && ext.len() <= 16
        && ext
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    (is_sha256_hex(sha) && ext_ok).then_some((sha, ext))
}

/// Splits `objects/<sha256>.<ext>` into its parts.
fn parse_stored(stored: &str) -> Option<(&str, &str)> {
    parse_object_name(stored.strip_prefix("objects/")?)
}

/// True when `path` is a regular file (not a symlink) whose bytes hash to `sha`.
fn file_hashes_to(path: &Path, sha: &str) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => fs::read(path)
            .map(|b| sha256_hex(&b) == sha)
            .unwrap_or(false),
        _ => false,
    }
}

fn read_regular_limited(path: &Path) -> std::result::Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a regular file".to_string());
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "file is {} bytes, over the {MAX_FILE_BYTES} byte limit",
            meta.len()
        ));
    }
    fs::read(path).map_err(|e| e.to_string())
}

/// What the index says about every id once all lines are applied in order.
struct IndexView {
    records: Vec<IndexRecord>,
    /// Ids whose latest line is a tombstone.
    dead: HashSet<String>,
    malformed: Vec<String>,
}

impl IndexView {
    fn live(&self) -> impl Iterator<Item = &IndexRecord> {
        self.records
            .iter()
            .filter(|r| !self.dead.contains(&r.evidence_id))
    }
}

impl Store {
    /// Opens the store at `dir`, creating the layout when it is absent.
    ///
    /// Refuses a directory whose `STORE.json` names another `store_version`.
    pub fn open(dir: impl AsRef<Path>) -> Result<Store> {
        Store::open_with_clock(dir, Box::new(now_rfc3339))
    }

    /// Like [`Store::open`] with an injected clock, used for `STORE.json` too.
    pub fn open_with_clock(dir: impl AsRef<Path>, clock: Box<dyn Fn() -> String>) -> Result<Store> {
        let dir = dir.as_ref().to_path_buf();
        let objects = dir.join("objects");
        fs::create_dir_all(&objects).map_err(|e| io_err(&objects, &e))?;
        let store = Store { dir, clock };
        let marker = store.dir.join(MARKER);
        match fs::read(&marker) {
            Ok(bytes) => check_marker(&marker, &bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let body = json!({
                    "schema_version": SCHEMA_VERSION,
                    "created_at": store.now(),
                    "store_version": STORE_VERSION,
                });
                let text = format!(
                    "{}\n",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
                if !atomic_write_new(&marker, text.as_bytes()).map_err(|e| io_err(&marker, &e))? {
                    // Another process created it first; honour what it wrote.
                    let bytes = fs::read(&marker).map_err(|e| io_err(&marker, &e))?;
                    check_marker(&marker, &bytes)?;
                }
            }
            Err(e) => return Err(io_err(&marker, &e)),
        }
        for name in [INDEX, KEYS, REUSE] {
            let path = store.dir.join(name);
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| io_err(&path, &e))?;
        }
        Ok(store)
    }

    /// Opens an existing store and fails when `dir` has no `STORE.json`.
    pub fn open_existing(dir: impl AsRef<Path>) -> Result<Store> {
        let dir = dir.as_ref();
        if !dir.join(MARKER).is_file() {
            return Err(EvidenceError::NotAStore {
                path: dir.to_path_buf(),
            });
        }
        Store::open(dir)
    }

    /// Replaces the clock every timestamp comes from.
    pub fn with_clock(mut self, clock: Box<dyn Fn() -> String>) -> Store {
        self.clock = clock;
        self
    }

    /// The store directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn now(&self) -> String {
        (self.clock)()
    }

    // ---- reading the logs -------------------------------------------------

    fn read_log<T: DeserializeOwned>(&self, name: &str) -> Result<(Vec<T>, Vec<String>)> {
        let path = self.dir.join(name);
        let meta = fs::metadata(&path).map_err(|e| io_err(&path, &e))?;
        if meta.len() > MAX_LOG_BYTES {
            return Err(EvidenceError::Invalid(format!(
                "{}: log is over {MAX_LOG_BYTES} bytes",
                path.display()
            )));
        }
        let bytes = fs::read(&path).map_err(|e| io_err(&path, &e))?;
        let mut rows = Vec::new();
        let mut malformed = Vec::new();
        for (index, raw) in bytes.split(|&b| b == b'\n').enumerate() {
            let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
            if raw.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            match std::str::from_utf8(raw) {
                Ok(text) => match serde_json::from_str::<T>(text) {
                    Ok(row) => rows.push(row),
                    Err(e) => malformed.push(format!("{name}:{}: {e}", index + 1)),
                },
                Err(_) => malformed.push(format!("{name}:{}: not UTF-8", index + 1)),
            }
        }
        Ok((rows, malformed))
    }

    fn index_view(&self) -> Result<IndexView> {
        let (entries, malformed) = self.read_log::<IndexEntry>(INDEX)?;
        let mut records = Vec::new();
        let mut dead: HashSet<String> = HashSet::new();
        for entry in entries {
            match entry {
                IndexEntry::Record(record) => {
                    dead.remove(&record.evidence_id);
                    records.push(record);
                }
                IndexEntry::Tombstone(t) => {
                    dead.insert(t.evidence_id);
                }
            }
        }
        Ok(IndexView {
            records,
            dead,
            malformed,
        })
    }

    /// Every parsable line of `index.jsonl`, in order.
    pub fn index_entries(&self) -> Result<Vec<IndexEntry>> {
        Ok(self.read_log::<IndexEntry>(INDEX)?.0)
    }

    /// Latest ingested record for `evidence_id`, tombstoned or not. Provenance
    /// survives a tombstone.
    pub fn find_record(&self, evidence_id: &str) -> Result<Option<IndexRecord>> {
        Ok(self
            .index_view()?
            .records
            .into_iter()
            .rev()
            .find(|r| r.evidence_id == evidence_id))
    }

    /// Every parsable, self-consistent line of `keys.jsonl`, in order.
    pub fn key_records(&self) -> Result<Vec<KeyRecord>> {
        Ok(self.read_keys()?.0)
    }

    /// Every parsable line of `reuse.jsonl`, in order.
    pub fn reuse_records(&self) -> Result<Vec<ReuseRecord>> {
        Ok(self.read_log::<ReuseRecord>(REUSE)?.0)
    }

    fn read_keys(&self) -> Result<(Vec<KeyRecord>, Vec<String>)> {
        let (rows, mut malformed) = self.read_log::<KeyRecord>(KEYS)?;
        let mut good = Vec::with_capacity(rows.len());
        for (n, row) in rows.into_iter().enumerate() {
            if row.key_hash != row.key.hash() {
                malformed.push(format!(
                    "{KEYS}: record {} has a key_hash that does not match its key",
                    n + 1
                ));
            } else {
                good.push(row);
            }
        }
        Ok((good, malformed))
    }

    /// Latest record per key hash, ordered by where that record sits in the file.
    fn latest_keys(&self) -> Result<Vec<KeyRecord>> {
        let (all, _) = self.read_keys()?;
        let mut last: HashMap<&str, usize> = HashMap::new();
        for (i, rec) in all.iter().enumerate() {
            last.insert(rec.key_hash.as_str(), i);
        }
        let keep: HashSet<usize> = last.values().copied().collect();
        Ok(all
            .into_iter()
            .enumerate()
            .filter(|(i, _)| keep.contains(i))
            .map(|(_, r)| r)
            .collect())
    }

    // ---- objects -----------------------------------------------------------

    fn object_rel(sha: &str, ext: &str) -> String {
        format!("objects/{sha}.{ext}")
    }

    /// Writes `bytes` to `objects/<sha>.<ext>` unless an intact copy is there.
    fn store_object(
        &self,
        sha: &str,
        ext: &str,
        bytes: &[u8],
        report: &mut IngestReport,
    ) -> Result<String> {
        let rel = Store::object_rel(sha, ext);
        let path = self.dir.join(&rel);
        let mut replacing = false;
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() && file_hashes_to(&path, sha) => {
                report.objects_reused += 1;
                return Ok(rel);
            }
            Ok(_) => replacing = true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(&path, &e)),
        }
        atomic_write(&path, bytes).map_err(|e| io_err(&path, &e))?;
        if replacing {
            report.objects_replaced.push(rel.clone());
        } else {
            report.objects_written += 1;
        }
        Ok(rel)
    }

    fn append<T: serde::Serialize>(&self, file: &str, row: &T) -> Result<()> {
        let line = serde_json::to_string(row)
            .map_err(|e| EvidenceError::Invalid(format!("serialising {file} line: {e}")))?;
        let path = self.dir.join(file);
        append_line(&path, &line).map_err(|e| io_err(&path, &e))
    }

    /// Map from object hash to every file under `objects/` that carries it.
    fn object_files(&self) -> Result<HashMap<String, Vec<PathBuf>>> {
        let dir = self.dir.join("objects");
        let mut out: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for entry in fs::read_dir(&dir).map_err(|e| io_err(&dir, &e))? {
            let entry = entry.map_err(|e| io_err(&dir, &e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some((sha, _)) = parse_object_name(&name) {
                out.entry(sha.to_string()).or_default().push(entry.path());
            }
        }
        Ok(out)
    }

    // ---- ingest ------------------------------------------------------------

    /// Ingests a `diligent-rs collect` directory.
    ///
    /// The directory is loaded with [`CaptureSet::load`], so every file is
    /// hash-verified first. Each file is copied into `objects/`, each distinct
    /// evidence id gets one index line with a normalised evidence record, and
    /// each `(viewport, media, kind)` gets one complete cache-key line.
    /// Running it again on the same directory and run id adds nothing.
    pub fn ingest_captures(&self, capture_dir: &Path, ctx: &IngestContext) -> Result<IngestReport> {
        ctx.validate()?;
        let set = CaptureSet::load(capture_dir)?;
        let mut report = IngestReport::default();

        // Plan every cache key before touching the disk, so a missing reason
        // fails the whole ingest.
        struct Group {
            key: CacheKey,
            sha256s: Vec<String>,
        }
        let mut groups: Vec<Group> = Vec::new();
        let mut group_of: HashMap<String, usize> = HashMap::new();
        for entry in set.entries() {
            let sha = entry.sha256.to_ascii_lowercase();
            let key = CacheKey {
                deployment: crate::model::Deployment {
                    commit: ctx.commit.clone(),
                    deployment_id: ctx.deployment_id.clone(),
                },
                route: ctx.route.clone(),
                role: ctx.role.clone(),
                state: ctx.state.clone(),
                viewport: Some(crate::model::ViewportKey {
                    width: entry.viewport.width,
                    height: entry.viewport.height,
                    dpr: entry.viewport.dpr,
                }),
                media: Some(entry.media.clone()),
                locale: ctx.locale.clone(),
                rule_version: ctx.rule_version.clone(),
                engine_version: ctx.engine_version.clone().unwrap_or_else(|| {
                    if entry.collector_version.is_empty() {
                        "unknown".to_string()
                    } else {
                        entry.collector_version.clone()
                    }
                }),
                protocol_version: (!entry.protocol_version.is_empty())
                    .then(|| entry.protocol_version.clone()),
                collector: CAPTURE_COLLECTOR.to_string(),
                kind: entry.kind.clone(),
            };
            match group_of.get(&key.hash()).copied() {
                Some(at) => {
                    if !groups[at].sha256s.contains(&sha) {
                        groups[at].sha256s.push(sha);
                    }
                }
                None => {
                    group_of.insert(key.hash(), groups.len());
                    groups.push(Group {
                        key,
                        sha256s: vec![sha],
                    });
                }
            }
        }
        let mut reasons: Vec<BTreeMap<String, String>> = Vec::with_capacity(groups.len());
        for group in &groups {
            let mut map = BTreeMap::new();
            for dim in group.key.empty_dimensions() {
                let reason = ctx.missing_reasons.get(dim).map(|r| r.trim()).unwrap_or("");
                if reason.is_empty() {
                    return Err(EvidenceError::MissingReason(dim.to_string()));
                }
                map.insert(dim.to_string(), reason.to_string());
            }
            reasons.push(map);
        }

        // Refuse a link or directory standing in for a capture file before any
        // object is written.
        for entry in set.entries() {
            let path = set.dir().join(&entry.path);
            let regular = fs::symlink_metadata(&path)
                .map(|m| m.is_file())
                .unwrap_or(false);
            if !regular {
                return Err(EvidenceError::Invalid(format!(
                    "{}: not a regular file",
                    path.display()
                )));
            }
        }

        // Objects first: an index or key line never points at a missing file.
        let mut sizes: HashMap<String, u64> = HashMap::new();
        for entry in set.entries() {
            if sizes.contains_key(&entry.path) {
                continue;
            }
            let path = set.dir().join(&entry.path);
            let bytes = read_regular_limited(&path)
                .map_err(|why| EvidenceError::Invalid(format!("{}: {why}", path.display())))?;
            let sha = sha256_hex(&bytes);
            if sha != entry.sha256.to_ascii_lowercase() {
                return Err(EvidenceError::Invalid(format!(
                    "{}: changed while ingesting, now hashes to {sha}",
                    path.display()
                )));
            }
            self.store_object(&sha, &ext_of(&entry.path), &bytes, &mut report)?;
            sizes.insert(entry.path.clone(), bytes.len() as u64);
        }

        let existing = self.index_view()?;
        let mut seen: HashSet<(String, String, String)> = existing
            .records
            .iter()
            .map(|r| (r.evidence_id.clone(), r.run_id.clone(), r.sha256.clone()))
            .collect();
        for entry in set.entries() {
            let sha = entry.sha256.to_ascii_lowercase();
            let record = capture_record(&set, entry, &sha, ctx);
            let id = record.evidence_id.clone();
            if !seen.insert((id.clone(), ctx.run_id.clone(), sha.clone())) {
                report.records_skipped += 1;
                continue;
            }
            let stored = Store::object_rel(&sha, &ext_of(&entry.path));
            let line = IndexEntry::Record(IndexRecord {
                schema_version: SCHEMA_VERSION.to_string(),
                evidence_id: id,
                sha256: sha,
                kind: record.kind.clone(),
                producer: Producer::Collect,
                run_id: ctx.run_id.clone(),
                original_payload_ref: entry.path.clone(),
                stored: Some(stored),
                bytes: sizes.get(&entry.path).copied(),
                ingested_at: self.now(),
                record: serde_json::to_value(&record)
                    .map_err(|e| EvidenceError::Invalid(e.to_string()))?,
                extensions: None,
            });
            self.append(INDEX, &line)?;
            report.records_indexed += 1;
        }

        let latest: HashMap<String, KeyRecord> = self
            .latest_keys()?
            .into_iter()
            .map(|r| (r.key_hash.clone(), r))
            .collect();
        for (group, missing_reasons) in groups.into_iter().zip(reasons) {
            let key_hash = group.key.hash();
            if let Some(prev) = latest.get(&key_hash) {
                let mut a = prev.sha256s.clone();
                let mut b = group.sha256s.clone();
                a.sort();
                b.sort();
                if prev.status == KeyStatus::Complete && a == b {
                    report.keys_skipped += 1;
                    continue;
                }
            }
            self.append(
                KEYS,
                &KeyRecord {
                    schema_version: SCHEMA_VERSION.to_string(),
                    key_hash,
                    key: group.key,
                    missing_reasons,
                    status: KeyStatus::Complete,
                    sha256s: group.sha256s,
                    run_id: ctx.run_id.clone(),
                    created_at: self.now(),
                    extensions: None,
                },
            )?;
            report.keys_recorded += 1;
        }
        Ok(report)
    }

    /// Indexes every row of an `EVIDENCE.jsonl` written by another producer.
    ///
    /// Rows are indexed unmodified under their own ids. When a row's
    /// `payload_ref` is a relative path to a regular file under `base_dir`
    /// (no `..`, no symlink out of `base_dir`) whose bytes hash to the row's
    /// `sha256`, the file is copied into `objects/`. Anything else is indexed
    /// with `stored: null`; a ref that should have resolved and did not is
    /// listed in `payload_problems`. `dns:` refs, other URI-style refs and
    /// every `source` row are never files we hold. Cache keys need dimensions
    /// the rows do not carry, so this records none.
    pub fn ingest_evidence_jsonl(
        &self,
        path: &Path,
        base_dir: &Path,
        producer: Producer,
    ) -> Result<IngestReport> {
        let meta = fs::metadata(path).map_err(|e| io_err(path, &e))?;
        if meta.len() > MAX_LOG_BYTES {
            return Err(EvidenceError::Invalid(format!(
                "{}: file is over {MAX_LOG_BYTES} bytes",
                path.display()
            )));
        }
        let bytes = fs::read(path).map_err(|e| io_err(path, &e))?;
        let mut report = IngestReport::default();
        let existing = self.index_view()?;
        let mut seen: HashSet<(String, String, String)> = existing
            .records
            .iter()
            .map(|r| (r.evidence_id.clone(), r.run_id.clone(), r.sha256.clone()))
            .collect();
        for (index, raw) in bytes.split(|&b| b == b'\n').enumerate() {
            let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
            if raw.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let line_no = index + 1;
            let reject = |report: &mut IngestReport, why: String| {
                report.rejected.push(format!("line {line_no}: {why}"))
            };
            let Ok(text) = std::str::from_utf8(raw) else {
                reject(&mut report, "not UTF-8".to_string());
                continue;
            };
            let row: Value = match serde_json::from_str(text) {
                Ok(v) => v,
                Err(e) => {
                    reject(&mut report, format!("not JSON: {e}"));
                    continue;
                }
            };
            let field = |name: &str| {
                row.get(name)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
            };
            let (Some(id), Some(run_id), Some(kind), Some(payload_ref), Some(sha_raw)) = (
                field("evidence_id"),
                field("run_id"),
                field("kind"),
                field("payload_ref"),
                field("sha256"),
            ) else {
                reject(
                    &mut report,
                    "needs non-empty string evidence_id, run_id, kind, payload_ref and sha256"
                        .to_string(),
                );
                continue;
            };
            let sha = sha_raw.to_ascii_lowercase();
            if !is_sha256_hex(&sha) {
                reject(&mut report, format!("{id}: sha256 is not 64 hex digits"));
                continue;
            }
            if !EVIDENCE_KINDS.contains(&kind) {
                reject(&mut report, format!("{id}: unknown kind `{kind}`"));
                continue;
            }
            if !seen.insert((id.to_string(), run_id.to_string(), sha.clone())) {
                report.records_skipped += 1;
                continue;
            }

            let mut stored: Option<String> = None;
            let mut stored_bytes: Option<u64> = None;
            let problem = |report: &mut IngestReport, reason: String| {
                report.payload_problems.push(Problem {
                    evidence_id: id.to_string(),
                    payload_ref: payload_ref.to_string(),
                    reason,
                });
            };
            if producer != Producer::Source && !is_uri_like(payload_ref) {
                match resolve_payload(base_dir, payload_ref) {
                    Ok(file) => match read_regular_limited(&file) {
                        Ok(data) => {
                            let actual = sha256_hex(&data);
                            if actual == sha {
                                let rel = self.store_object(
                                    &sha,
                                    &ext_of(payload_ref),
                                    &data,
                                    &mut report,
                                )?;
                                stored = Some(rel);
                                stored_bytes = Some(data.len() as u64);
                            } else {
                                problem(
                                    &mut report,
                                    format!("payload hashes to {actual}, row says {sha}"),
                                );
                            }
                        }
                        Err(why) => problem(&mut report, why),
                    },
                    Err(why) => problem(&mut report, why),
                }
            }
            let line = IndexEntry::Record(IndexRecord {
                schema_version: SCHEMA_VERSION.to_string(),
                evidence_id: id.to_string(),
                sha256: sha,
                kind: kind.to_string(),
                producer,
                run_id: run_id.to_string(),
                original_payload_ref: payload_ref.to_string(),
                stored,
                bytes: stored_bytes,
                ingested_at: self.now(),
                record: row.clone(),
                extensions: None,
            });
            self.append(INDEX, &line)?;
            report.records_indexed += 1;
        }
        Ok(report)
    }

    // ---- keys, reuse, resume -----------------------------------------------

    /// Latest record for `key`, or `None` when the store has never seen it.
    ///
    /// The record may have status `invalidated` or `partial`; check
    /// [`KeyRecord::status`] before relying on it. Use [`Store::plan_resume`]
    /// to decide reuse.
    pub fn lookup(&self, key: &CacheKey) -> Option<KeyRecord> {
        let hash = key.hash();
        self.latest_keys()
            .ok()?
            .into_iter()
            .find(|r| r.key_hash == hash)
    }

    /// Decides, for each wanted key, whether earlier evidence can be reused.
    ///
    /// A key is reused only when its latest record is `complete`, its
    /// `engine_version`, `rule_version` and `protocol_version` equal the
    /// wanted ones, and every object it names is present and re-hashes to its
    /// name. A reuse record is appended for each reuse. Everything else goes
    /// to `redo` with a reason. A key listed twice is planned once.
    pub fn plan_resume(&self, wanted: &[CacheKey], run_id: &str) -> Result<ResumePlan> {
        let latest = self.latest_keys()?;
        let by_hash: HashMap<&str, &KeyRecord> =
            latest.iter().map(|r| (r.key_hash.as_str(), r)).collect();
        let files = self.object_files()?;
        // Records grouped by everything except the three version fields, so
        // "same key, other engine" is found without scanning every record.
        let mut by_neutral: HashMap<String, Vec<&KeyRecord>> = HashMap::new();
        for record in &latest {
            by_neutral
                .entry(neutral_hash(&record.key))
                .or_default()
                .push(record);
        }
        let mut plan = ResumePlan {
            reuse: Vec::new(),
            redo: Vec::new(),
            reasons: Vec::new(),
        };
        let mut seen: HashSet<String> = HashSet::new();
        for key in wanted {
            let hash = key.hash();
            if !seen.insert(hash.clone()) {
                continue;
            }
            let verdict = match by_hash.get(hash.as_str()) {
                None => Err(no_record_reason(key, &by_neutral)),
                Some(record) => check_reusable(record, key, &files),
            };
            match verdict {
                Ok(record) => {
                    let reuse = ReuseRecord {
                        schema_version: SCHEMA_VERSION.to_string(),
                        run_id: run_id.to_string(),
                        key_hash: hash,
                        reused_from_run_id: record.run_id.clone(),
                        sha256s: record.sha256s.clone(),
                        verified: true,
                        reason: "cache key matched a complete record; engine, rule and protocol versions are equal; every object was re-hashed".to_string(),
                        recorded_at: self.now(),
                        extensions: None,
                    };
                    self.append(REUSE, &reuse)?;
                    plan.reuse.push(reuse);
                }
                Err(reason) => {
                    plan.redo.push(key.clone());
                    plan.reasons.push((key.clone(), reason));
                }
            }
        }
        Ok(plan)
    }

    /// Appends an `invalidated` record for every key whose latest record is
    /// not already invalidated and for which `predicate` is true. Returns how
    /// many were invalidated. Use it when a lock or deployment changes.
    pub fn invalidate<F>(&self, predicate: F) -> Result<usize>
    where
        F: Fn(&KeyRecord) -> bool,
    {
        let mut count = 0;
        for record in self.latest_keys()? {
            if record.status == KeyStatus::Invalidated || !predicate(&record) {
                continue;
            }
            let mut next = record;
            next.status = KeyStatus::Invalidated;
            next.created_at = self.now();
            self.append(KEYS, &next)?;
            count += 1;
        }
        Ok(count)
    }

    // ---- verify, tombstone, derive -----------------------------------------

    /// Re-hashes every object and cross-checks the logs.
    pub fn verify(&self) -> Result<VerifyReport> {
        let mut report = VerifyReport::default();
        let dir = self.dir.join("objects");
        let mut names: Vec<String> = Vec::new();
        for entry in fs::read_dir(&dir).map_err(|e| io_err(&dir, &e))? {
            let entry = entry.map_err(|e| io_err(&dir, &e))?;
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        for name in names {
            report.objects += 1;
            let rel = format!("objects/{name}");
            let path = self.dir.join(&rel);
            match parse_object_name(&name) {
                Some((sha, _)) if file_hashes_to(&path, sha) => report.ok += 1,
                _ => report.mismatched.push(rel),
            }
        }

        let view = self.index_view()?;
        report.malformed.extend(view.malformed.iter().cloned());
        report.tombstoned = view.dead.len();
        let mut missing: Vec<String> = Vec::new();
        for record in view.live() {
            let Some(stored) = &record.stored else {
                continue;
            };
            let Some((sha, _)) = parse_stored(stored) else {
                report.malformed.push(format!(
                    "{INDEX}: {} has an invalid stored path `{stored}`",
                    record.evidence_id
                ));
                continue;
            };
            if sha != record.sha256 {
                report.malformed.push(format!(
                    "{INDEX}: {} stored path names a different hash than its sha256",
                    record.evidence_id
                ));
                continue;
            }
            let path = self.dir.join(stored);
            let present = fs::symlink_metadata(&path)
                .map(|m| m.is_file())
                .unwrap_or(false);
            if !present && !missing.contains(&record.evidence_id) {
                missing.push(record.evidence_id.clone());
            }
        }
        report.missing = missing;
        report.malformed.extend(self.read_keys()?.1);
        report
            .malformed
            .extend(self.read_log::<ReuseRecord>(REUSE)?.1);
        Ok(report)
    }

    /// Records that a payload is gone and removes its object.
    ///
    /// Appends a tombstone; the record line before it keeps provenance. The
    /// object file is deleted only when no other live record shares its hash.
    /// Marking an id that is already tombstoned does nothing.
    pub fn mark_unavailable(
        &self,
        evidence_id: &str,
        reason: &str,
        replay_limitation: &str,
    ) -> Result<()> {
        if reason.trim().is_empty() || replay_limitation.trim().is_empty() {
            return Err(EvidenceError::Invalid(
                "a tombstone needs a reason and a replay limitation".to_string(),
            ));
        }
        let view = self.index_view()?;
        let Some(record) = view
            .records
            .iter()
            .rev()
            .find(|r| r.evidence_id == evidence_id)
        else {
            return Err(EvidenceError::NotFound(format!(
                "no ingested record with evidence_id `{evidence_id}`"
            )));
        };
        if view.dead.contains(evidence_id) {
            return Ok(());
        }
        let sha = record.sha256.clone();
        let own_paths: Vec<String> = view
            .records
            .iter()
            .filter(|r| r.evidence_id == evidence_id)
            .filter_map(|r| r.stored.clone())
            .collect();
        // Live records of other ids; computed before the tombstone is appended.
        let others: Vec<(String, Option<String>)> = view
            .live()
            .filter(|r| r.evidence_id != evidence_id)
            .map(|r| (r.sha256.clone(), r.stored.clone()))
            .collect();
        self.append(
            INDEX,
            &IndexEntry::Tombstone(Tombstone {
                schema_version: SCHEMA_VERSION.to_string(),
                evidence_id: evidence_id.to_string(),
                sha256: sha,
                reason: reason.to_string(),
                marked_at: self.now(),
                replay_limitation: replay_limitation.to_string(),
                extensions: None,
            }),
        )?;
        for stored in own_paths {
            // Only `objects/<sha>.<ext>` is ever deleted, and only when no
            // other live record names that hash or that path.
            let Some((path_sha, _)) = parse_stored(&stored) else {
                continue;
            };
            let in_use = others
                .iter()
                .any(|(sha, other)| sha == path_sha || other.as_deref() == Some(stored.as_str()));
            if in_use {
                continue;
            }
            let path = self.dir.join(&stored);
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io_err(&path, &e)),
            }
        }
        Ok(())
    }

    /// Stores a redacted derivative of `parent_id` under a new id.
    ///
    /// The new record is `ev-drv-<first 12 hex of the bytes' sha256>`, has
    /// `derived_from = parent_id`, `redaction.applied = true` and the given
    /// flags, and inherits run, trust, classification and snapshot from the
    /// parent. The parent may already be tombstoned.
    pub fn derive(
        &self,
        parent_id: &str,
        bytes: &[u8],
        ext: &str,
        redaction: Redaction,
        kind: &str,
    ) -> Result<EvidenceRecord> {
        if !EVIDENCE_KINDS.contains(&kind) {
            return Err(EvidenceError::Invalid(format!(
                "unknown evidence kind `{kind}`"
            )));
        }
        let Some(parent_row) = self.find_record(parent_id)? else {
            return Err(EvidenceError::NotFound(format!(
                "no ingested record with evidence_id `{parent_id}`"
            )));
        };
        let parent: EvidenceRecord =
            serde_json::from_value(parent_row.record.clone()).map_err(|e| {
                EvidenceError::Invalid(format!(
                    "parent `{parent_id}` is not a valid evidence record: {e}"
                ))
            })?;
        let sha = sha256_hex(bytes);
        let ext = sanitize_ext(ext);
        let mut report = IngestReport::default();
        let stored = self.store_object(&sha, &ext, bytes, &mut report)?;
        let mut limitations = parent.limitations.clone();
        limitations.push(format!(
            "Redacted derivative of {parent_id}; its bytes differ from the original observation."
        ));
        let record = EvidenceRecord {
            schema_version: SCHEMA_VERSION.to_string(),
            evidence_id: format!("ev-drv-{}", &sha[..12]),
            run_id: parent.run_id.clone(),
            kind: kind.to_string(),
            collector: "diligent-evidence".to_string(),
            collector_version: env!("CARGO_PKG_VERSION").to_string(),
            captured_at: self.now(),
            snapshot_ref: parent.snapshot_ref.clone(),
            context: json!({
                "derivation": "redaction",
                "parent_evidence_id": parent_id,
                "parent_kind": parent.kind,
            }),
            payload_ref: stored.clone(),
            sha256: sha.clone(),
            data_classification: parent.data_classification.clone(),
            redaction: Redaction {
                applied: true,
                flags: redaction.flags,
            },
            trust: parent.trust.clone(),
            limitations,
            derived_from: Some(parent_id.to_string()),
            valid_until: parent.valid_until.clone(),
            extensions: None,
        };
        let view = self.index_view()?;
        let duplicate = view.records.iter().any(|r| {
            r.evidence_id == record.evidence_id && r.run_id == record.run_id && r.sha256 == sha
        });
        if !duplicate {
            self.append(
                INDEX,
                &IndexEntry::Record(IndexRecord {
                    schema_version: SCHEMA_VERSION.to_string(),
                    evidence_id: record.evidence_id.clone(),
                    sha256: sha,
                    kind: record.kind.clone(),
                    producer: Producer::Other,
                    run_id: record.run_id.clone(),
                    original_payload_ref: stored.clone(),
                    stored: Some(stored),
                    bytes: Some(bytes.len() as u64),
                    ingested_at: self.now(),
                    record: serde_json::to_value(&record)
                        .map_err(|e| EvidenceError::Invalid(e.to_string()))?,
                    extensions: None,
                }),
            )?;
        }
        Ok(record)
    }
}

fn check_marker(path: &Path, bytes: &[u8]) -> Result<()> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|e| EvidenceError::Invalid(format!("{}: not valid JSON: {e}", path.display())))?;
    let found = value
        .get("store_version")
        .and_then(Value::as_str)
        .unwrap_or("");
    if found != STORE_VERSION {
        return Err(EvidenceError::StoreVersion {
            path: path.to_path_buf(),
            found: found.to_string(),
            expected: STORE_VERSION.to_string(),
        });
    }
    Ok(())
}

fn capture_kind(manifest_kind: &str) -> &'static str {
    match manifest_kind {
        "dom_snapshot" => "computed_style",
        "ax_tree" => "accessibility_tree",
        _ => "runtime_observation",
    }
}

fn capture_limitation(manifest_kind: &str) -> &'static str {
    match manifest_kind {
        "ax_tree" => "Accessibility tree as Chrome reported it for one page state at one viewport; no interaction.",
        "focus_order" => "Tab-only walk of one page state, limited in length; Shift+Tab, Escape and arrow keys were not tried.",
        "dom_snapshot" => "Computed styles and layout boxes of one page state at one viewport; pseudo-elements and interaction states are not captured.",
        "screenshot" => "One rendering of one page state at one viewport; fonts, timing and browser version change the pixels.",
        "perf_metrics" => "Lab measurement of one unthrottled headless page load at one viewport; not a field measurement, and timing varies between runs.",
        "axe_result" => "Third-party corroborator output for one page state at one viewport, top frame only; an inference that never replaces a deterministic result.",
        "network_log" => "Requests seen during one page load at one viewport; later requests, caching and server variation are not captured.",
        _ => "One observation of one page state at one viewport; nothing beyond the recorded bytes is known.",
    }
}

/// The normalised evidence record for one manifest entry.
fn capture_record(
    set: &CaptureSet,
    entry: &ManifestEntry,
    sha: &str,
    ctx: &IngestContext,
) -> EvidenceRecord {
    let is_axe = entry.kind == "axe_result";
    let mut context = json!({
        "viewport": { "width": entry.viewport.width, "height": entry.viewport.height, "dpr": entry.viewport.dpr },
        "media": entry.media,
        "final_url": entry.final_url,
        "capture_kind": entry.kind,
        "protocol_version": if entry.protocol_version.is_empty() { Value::Null } else { json!(entry.protocol_version) },
        "route": ctx.route,
        "commit": ctx.commit,
        "deployment_id": ctx.deployment_id,
        "role": ctx.role,
        "state": ctx.state,
        "locale": ctx.locale,
    });
    if entry.kind == "perf_metrics" {
        context["label"] = json!("lab");
    }
    if is_axe {
        // Same shape `assess` writes: the engine name and the version axe reported.
        let axe = set.axe(entry.viewport, &entry.media);
        context["corroborator"] = json!({
            "engine": axe.map_or("axe-core", |a| a.engine.as_str()),
            "version": axe.and_then(|a| a.version.clone()),
        });
    }
    EvidenceRecord {
        schema_version: SCHEMA_VERSION.to_string(),
        evidence_id: format!("ev-cap-{}", &sha[..12]),
        run_id: ctx.run_id.clone(),
        kind: capture_kind(&entry.kind).to_string(),
        collector: CAPTURE_COLLECTOR.to_string(),
        collector_version: if entry.collector_version.is_empty() {
            "unknown".to_string()
        } else {
            entry.collector_version.clone()
        },
        captured_at: entry.captured_at.clone(),
        snapshot_ref: None,
        context,
        payload_ref: Store::object_rel(sha, &ext_of(&entry.path)),
        sha256: sha.to_string(),
        data_classification: ctx
            .data_classification
            .clone()
            .unwrap_or_else(|| "public".to_string()),
        redaction: Redaction {
            applied: false,
            flags: Vec::new(),
        },
        trust: if is_axe {
            "inference"
        } else {
            "direct_observation"
        }
        .to_string(),
        limitations: vec![capture_limitation(&entry.kind).to_string()],
        derived_from: None,
        valid_until: None,
        extensions: None,
    }
}

/// `dns:TXT:name`, `https://...` and similar: not a file path.
fn is_uri_like(payload_ref: &str) -> bool {
    match payload_ref.split_once(':') {
        Some((scheme, _)) => {
            scheme.len() > 1
                && scheme
                    .bytes()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
                && scheme
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
        }
        None => false,
    }
}

/// Resolves a producer's relative `payload_ref` to a regular file under `base`.
fn resolve_payload(base: &Path, payload_ref: &str) -> std::result::Result<PathBuf, String> {
    if payload_ref.contains('\0') || payload_ref.contains('\\') {
        return Err("payload_ref has a NUL or backslash".to_string());
    }
    let relative = Path::new(payload_ref);
    if relative.is_absolute() {
        return Err("payload_ref is an absolute path".to_string());
    }
    for component in relative.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err("payload_ref leaves the base directory".to_string()),
        }
    }
    let base_real = fs::canonicalize(base).map_err(|e| format!("base directory: {e}"))?;
    let real = fs::canonicalize(base.join(relative)).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "payload file not found under the base directory".to_string()
        } else {
            e.to_string()
        }
    })?;
    if !real.starts_with(&base_real) {
        return Err("payload_ref resolves outside the base directory".to_string());
    }
    Ok(real)
}

/// Hash of a key with its engine, rule and protocol versions blanked.
fn neutral_hash(key: &CacheKey) -> String {
    let mut neutral = key.clone();
    neutral.engine_version = String::new();
    neutral.rule_version = None;
    neutral.protocol_version = None;
    neutral.hash()
}

fn no_record_reason(wanted: &CacheKey, by_neutral: &HashMap<String, Vec<&KeyRecord>>) -> String {
    let nearest = by_neutral.get(&neutral_hash(wanted)).and_then(|records| {
        records
            .iter()
            .max_by(|a, b| a.created_at.cmp(&b.created_at))
    });
    match nearest {
        None => "no cache record for this key".to_string(),
        Some(record) => format!(
            "no cache record for this key; the nearest record differs in {}",
            version_diff(&record.key, wanted).join(", ")
        ),
    }
}

fn version_diff(cached: &CacheKey, wanted: &CacheKey) -> Vec<String> {
    let mut out = Vec::new();
    if cached.engine_version != wanted.engine_version {
        out.push(format!(
            "engine_version (cached {}, wanted {})",
            cached.engine_version, wanted.engine_version
        ));
    }
    if cached.rule_version != wanted.rule_version {
        out.push(format!(
            "rule_version (cached {}, wanted {})",
            cached.rule_version.as_deref().unwrap_or("none"),
            wanted.rule_version.as_deref().unwrap_or("none")
        ));
    }
    if cached.protocol_version != wanted.protocol_version {
        out.push(format!(
            "protocol_version (cached {}, wanted {})",
            cached.protocol_version.as_deref().unwrap_or("none"),
            wanted.protocol_version.as_deref().unwrap_or("none")
        ));
    }
    out
}

/// Ok with the record when it may be reused for `wanted`, else the reason not.
fn check_reusable<'a>(
    record: &'a KeyRecord,
    wanted: &CacheKey,
    files: &HashMap<String, Vec<PathBuf>>,
) -> std::result::Result<&'a KeyRecord, String> {
    if record.status != KeyStatus::Complete {
        return Err(format!(
            "latest record for this key is {}",
            record.status.as_str()
        ));
    }
    let diff = version_diff(&record.key, wanted);
    if !diff.is_empty() {
        return Err(format!("versions differ: {}", diff.join(", ")));
    }
    if record.sha256s.is_empty() {
        return Err("the record selects no objects".to_string());
    }
    for sha in &record.sha256s {
        if !is_sha256_hex(sha) {
            return Err(format!(
                "the record names `{sha}`, which is not a lowercase sha256"
            ));
        }
        let Some(candidates) = files.get(sha) else {
            return Err(format!("object {sha} is missing"));
        };
        if !candidates.iter().any(|p| file_hashes_to(p, sha)) {
            return Err(format!("object {sha} does not re-hash to its name"));
        }
    }
    Ok(record)
}
