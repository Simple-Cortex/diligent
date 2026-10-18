//! Sitemap parsing and traversal: `urlset` and `sitemapindex`, `loc` and `lastmod`,
//! gzip detected by magic bytes, bounded depth, file count, body size and entries.

use std::collections::{HashSet, VecDeque};
use std::io::Read;

use flate2::read::GzDecoder;
use url::Url;

/// Largest nesting of sitemap indexes followed (the root file is depth 0).
pub const MAX_DEPTH: usize = 3;
/// Deepest element nesting accepted in one sitemap file; the XML parser recurses on nesting.
pub const MAX_XML_DEPTH: usize = 64;
/// Largest number of sitemap files fetched in one inventory.
pub const MAX_FILES: usize = 50;

/// What a sitemap document is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SitemapKind {
    /// A `urlset` of page URLs.
    UrlSet,
    /// A `sitemapindex` of child sitemaps.
    Index,
}

/// One `url` or `sitemap` element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitemapEntry {
    /// The `loc` URL (http or https only).
    pub loc: Url,
    /// The `lastmod` text, when present and short.
    pub lastmod: Option<String>,
}

/// A parsed sitemap document.
#[derive(Debug, Clone)]
pub struct ParsedSitemap {
    /// Urlset or index.
    pub kind: SitemapKind,
    /// Entries in document order.
    pub entries: Vec<SitemapEntry>,
    /// True when the body was cut (size cap, decompression cap or truncated download).
    pub truncated: bool,
    /// Entries skipped because `loc` was missing, not a URL, or not http(s).
    pub skipped_invalid: usize,
}

/// Why a body is not a usable sitemap.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SitemapError {
    /// The XML could not be parsed.
    #[error("malformed XML: {0}")]
    Malformed(String),
    /// Well-formed XML whose root is neither `urlset` nor `sitemapindex`.
    #[error("not a sitemap (root element `{0}`)")]
    NotSitemap(String),
}

/// Gunzips `body` when it starts with the gzip magic bytes, reading at most `max_bytes`
/// of output. Returns the bytes and whether they were cut or the stream was damaged.
/// A plain body is returned as is (cut to `max_bytes`).
pub fn decode(body: &[u8], max_bytes: usize) -> (Vec<u8>, bool) {
    if body.len() >= 2 && body[0] == 0x1f && body[1] == 0x8b {
        let mut out = Vec::new();
        let limit = (max_bytes as u64).saturating_add(1);
        let mut reader = GzDecoder::new(body).take(limit);
        let damaged = reader.read_to_end(&mut out).is_err();
        let over = out.len() > max_bytes;
        out.truncate(max_bytes);
        (out, over || damaged)
    } else if body.len() > max_bytes {
        (body[..max_bytes].to_vec(), true)
    } else {
        (body.to_vec(), false)
    }
}

/// Parses a sitemap body (plain or gzip). `truncated_download` says the fetcher already cut
/// the body at its cap. A cut document is repaired by closing it after its last complete entry.
pub fn parse(
    body: &[u8],
    max_bytes: usize,
    truncated_download: bool,
) -> Result<ParsedSitemap, SitemapError> {
    let (bytes, cut) = decode(body, max_bytes);
    let truncated = cut || truncated_download;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        text = close_cut_document(&text);
    }
    if exceeds_depth(&text, MAX_XML_DEPTH) {
        return Err(SitemapError::Malformed(format!(
            "elements nested deeper than {MAX_XML_DEPTH} levels"
        )));
    }
    let doc = match roxmltree::Document::parse(&text) {
        Ok(d) => d,
        Err(e) => return Err(SitemapError::Malformed(e.to_string())),
    };
    let root = doc.root_element();
    let (kind, item) = match root.tag_name().name() {
        "urlset" => (SitemapKind::UrlSet, "url"),
        "sitemapindex" => (SitemapKind::Index, "sitemap"),
        other => return Err(SitemapError::NotSitemap(other.to_string())),
    };
    let mut entries = Vec::new();
    let mut skipped_invalid = 0usize;
    for node in root
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == item)
    {
        let mut loc: Option<String> = None;
        let mut lastmod: Option<String> = None;
        for child in node.children().filter(|c| c.is_element()) {
            let value = child
                .text()
                .map(|t| t.trim().to_string())
                .unwrap_or_default();
            match child.tag_name().name() {
                "loc" if loc.is_none() => loc = Some(value),
                "lastmod" if lastmod.is_none() => lastmod = Some(value),
                _ => {}
            }
        }
        let parsed = loc
            .as_deref()
            .and_then(|l| Url::parse(l).ok())
            .filter(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some());
        match parsed {
            Some(loc) => entries.push(SitemapEntry {
                loc,
                lastmod: lastmod.filter(|l| !l.is_empty() && l.len() <= 40),
            }),
            None => skipped_invalid += 1,
        }
    }
    Ok(ParsedSitemap {
        kind,
        entries,
        truncated,
        skipped_invalid,
    })
}

/// True when element nesting in `text` goes deeper than `limit`. A linear, quote-aware scan that
/// skips comments, CDATA, processing instructions and declarations.
fn exceeds_depth(text: &str, limit: usize) -> bool {
    let b = text.as_bytes();
    let mut i = 0usize;
    let mut depth = 0usize;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &text[i..];
        let skip_to =
            |end: &str, from: usize| rest[from..].find(end).map(|j| i + from + j + end.len());
        let next = if rest.starts_with("<!--") {
            skip_to("-->", 4)
        } else if rest.starts_with("<![CDATA[") {
            skip_to("]]>", 9)
        } else if rest.starts_with("<?") {
            skip_to("?>", 2)
        } else if rest.starts_with("<!") {
            skip_to(">", 2)
        } else if rest.starts_with("</") {
            depth = depth.saturating_sub(1);
            skip_to(">", 2)
        } else {
            // Start tag: find its closing `>` outside quotes.
            let mut quote: Option<u8> = None;
            let mut j = 1usize;
            let rb = rest.as_bytes();
            let mut end = None;
            while j < rb.len() {
                match (quote, rb[j]) {
                    (None, b'"') | (None, b'\'') => quote = Some(rb[j]),
                    (Some(q), c) if c == q => quote = None,
                    (None, b'>') => {
                        end = Some(j);
                        break;
                    }
                    _ => {}
                }
                j += 1;
            }
            end.map(|j| {
                if rb[j - 1] != b'/' {
                    depth += 1;
                }
                i + j + 1
            })
        };
        if depth > limit {
            return true;
        }
        match next {
            Some(n) => i = n,
            None => break,
        }
    }
    false
}

/// Cuts `text` after its last complete `</url>` or `</sitemap>` and appends the closing root tag.
fn close_cut_document(text: &str) -> String {
    let url_end = text
        .rfind("</url>")
        .map(|i| (i + "</url>".len(), "</urlset>"));
    let sm_end = text
        .rfind("</sitemap>")
        .map(|i| (i + "</sitemap>".len(), "</sitemapindex>"));
    let best = match (url_end, sm_end) {
        (Some(a), Some(b)) => Some(if a.0 >= b.0 { a } else { b }),
        (a, b) => a.or(b),
    };
    match best {
        Some((end, closer)) => format!("{}{}", &text[..end], closer),
        None => text.to_string(),
    }
}

/// What the traversal asks its caller to do for one sitemap file.
pub enum SitemapFetch {
    /// A response body (status 200 only; other statuses are `Skip`).
    Body {
        /// Raw bytes as fetched (possibly gzip).
        body: Vec<u8>,
        /// The fetcher cut the body at its cap.
        truncated: bool,
    },
    /// The file was not fetched or not usable; the text says why.
    Skip(String),
}

/// Bounds for [`collect`].
#[derive(Debug, Clone)]
pub struct Limits {
    /// Largest index nesting.
    pub max_depth: usize,
    /// Largest number of files.
    pub max_files: usize,
    /// Largest decoded size of one file, in bytes.
    pub max_body_bytes: usize,
    /// Largest number of page entries kept in total.
    pub max_entries: usize,
}

/// Everything gathered from a set of sitemap roots.
#[derive(Debug, Clone, Default)]
pub struct Collected {
    /// Page entries with the source label of the root they came from, in discovery order.
    pub entries: Vec<(SitemapEntry, &'static str)>,
    /// Files fetched.
    pub files_fetched: usize,
    /// Deterministic one-line notes (skips, truncation, caps).
    pub notes: Vec<String>,
    /// Remaining work the caps left behind.
    pub remaining: Vec<String>,
}

/// Walks sitemap `roots` breadth first. `fetch` performs (and vets, rate limits and scopes)
/// each request.
pub fn collect(
    roots: &[(Url, &'static str)],
    limits: &Limits,
    fetch: &mut dyn FnMut(&Url) -> SitemapFetch,
) -> Collected {
    let mut out = Collected::default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(Url, usize, &'static str)> = VecDeque::new();
    for (u, label) in roots {
        if seen.insert(u.as_str().to_string()) {
            queue.push_back((u.clone(), 0, label));
        }
    }
    let mut skipped_files = 0usize;
    let mut too_deep = 0usize;
    let mut entries_capped = false;
    while let Some((url, depth, label)) = queue.pop_front() {
        if out.files_fetched >= limits.max_files {
            skipped_files += 1 + queue.len();
            break;
        }
        out.files_fetched += 1;
        let (body, truncated_download) = match fetch(&url) {
            SitemapFetch::Body { body, truncated } => (body, truncated),
            SitemapFetch::Skip(why) => {
                // A missing default /sitemap.xml is normal and not worth a limit line.
                if !(depth == 0 && label == "sitemap" && why == "status 404") {
                    out.notes.push(format!("sitemap_skipped: {url} ({why})"));
                }
                continue;
            }
        };
        let parsed = match parse(&body, limits.max_body_bytes, truncated_download) {
            Ok(p) => p,
            Err(e) => {
                out.notes.push(format!("sitemap_unparsable: {url} ({e})"));
                continue;
            }
        };
        if parsed.truncated {
            out.notes.push(format!("sitemap_truncated: {url}"));
            out.remaining.push(format!(
                "sitemap {url} was cut at the size limit; later entries were not read"
            ));
        }
        if parsed.skipped_invalid > 0 {
            out.notes.push(format!(
                "sitemap_invalid_loc: {url} ({} entries without a usable http(s) loc)",
                parsed.skipped_invalid
            ));
        }
        match parsed.kind {
            SitemapKind::Index => {
                for entry in parsed.entries {
                    if depth + 1 > limits.max_depth {
                        too_deep += 1;
                        continue;
                    }
                    if seen.insert(entry.loc.as_str().to_string()) {
                        queue.push_back((entry.loc, depth + 1, label));
                    }
                }
            }
            SitemapKind::UrlSet => {
                for entry in parsed.entries {
                    if out.entries.len() >= limits.max_entries {
                        entries_capped = true;
                        break;
                    }
                    out.entries.push((entry, label));
                }
            }
        }
    }
    if skipped_files > 0 {
        out.notes.push(format!(
            "sitemap_file_limit: {} files fetched",
            limits.max_files
        ));
        out.remaining.push(format!(
            "{skipped_files} sitemap files were not fetched (limit {} files)",
            limits.max_files
        ));
    }
    if too_deep > 0 {
        out.notes.push(format!(
            "sitemap_depth_limit: indexes nested deeper than {} were not followed",
            limits.max_depth
        ));
        out.remaining.push(format!(
            "{too_deep} child sitemaps were not followed (index nesting limit {})",
            limits.max_depth
        ));
    }
    if entries_capped {
        out.notes.push(format!(
            "sitemap_entry_limit: kept the first {} URLs",
            limits.max_entries
        ));
        out.remaining.push(format!(
            "sitemap entries beyond the first {} were not read",
            limits.max_entries
        ));
    }
    out
}
