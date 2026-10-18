//! Template signatures: a URL path shape plus, for fetched pages, a DOM shape.

use scraper::{ElementRef, Html};
use sha2::{Digest, Sha256};
use url::Url;

/// Landmark and structure tags recorded in document order.
const LANDMARKS: [&str; 6] = ["header", "nav", "main", "aside", "footer", "form"];
/// Most tables inspected for header cells (bounds work on pages with thousands of nested tables).
const MAX_TABLES_CHECKED: usize = 64;
/// Longest landmark sequence kept in a DOM shape.
const MAX_LANDMARKS: usize = 32;

/// Structural facts about one parsed page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomFacts {
    /// Landmark tags in document order (capped).
    pub landmarks: Vec<&'static str>,
    /// Number of `h1` elements.
    pub h1: usize,
    /// Number of `h2` elements.
    pub h2: usize,
    /// A `form` with a submit control exists.
    pub submit_form: bool,
    /// A `table` with header cells exists.
    pub data_table: bool,
    /// Any `form` element exists.
    pub any_form: bool,
}

impl DomFacts {
    /// Reads the facts from a parsed document.
    pub fn from_html(html: &Html) -> Self {
        let mut landmarks = Vec::new();
        let (mut h1, mut h2) = (0usize, 0usize);
        let mut any_form = false;
        let mut submit_form = false;
        let mut data_table = false;
        let mut tables_checked = 0usize;
        for el in html
            .root_element()
            .descendants()
            .filter_map(ElementRef::wrap)
        {
            let name = el.value().name();
            if let Some(l) = LANDMARKS.iter().find(|l| **l == name) {
                if landmarks.len() < MAX_LANDMARKS {
                    landmarks.push(*l);
                }
            }
            match name {
                "h1" => h1 += 1,
                "h2" => h2 += 1,
                "form" => {
                    any_form = true;
                    if !submit_form && form_has_submit(el) {
                        submit_form = true;
                    }
                }
                "table" if !data_table && tables_checked < MAX_TABLES_CHECKED => {
                    tables_checked += 1;
                    data_table = el
                        .descendants()
                        .filter_map(ElementRef::wrap)
                        .any(|c| matches!(c.value().name(), "th" | "thead"));
                }
                _ => {}
            }
        }
        DomFacts {
            landmarks,
            h1,
            h2,
            submit_form,
            data_table,
            any_form,
        }
    }

    /// The stable string form used in signatures and fixtures.
    pub fn shape(&self) -> String {
        format!(
            "L={};h1={};h2={};submit_form={};data_table={}",
            self.landmarks.join(","),
            bucket(self.h1),
            bucket(self.h2),
            u8::from(self.submit_form),
            u8::from(self.data_table)
        )
    }
}

fn form_has_submit(form: ElementRef<'_>) -> bool {
    form.descendants().filter_map(ElementRef::wrap).any(|c| {
        let name = c.value().name();
        let ty = c.value().attr("type").map(str::to_ascii_lowercase);
        match name {
            "button" => ty.as_deref().is_none_or(|t| t == "submit"),
            "input" => matches!(ty.as_deref(), Some("submit") | Some("image")),
            _ => false,
        }
    })
}

fn bucket(n: usize) -> &'static str {
    match n {
        0 => "0",
        1 => "1",
        2..=4 => "2-4",
        _ => "5+",
    }
}

/// A template signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Normalized path shape, for example `/news/{slug}`.
    pub path_shape: String,
    /// DOM shape for fetched pages; `None` otherwise.
    pub dom_shape: Option<String>,
    /// First 12 hex characters of `sha256(path_shape + "\n" + dom_shape)`.
    pub cluster_id: String,
}

/// The signature of `url`. `dom` is `Some` only for pages that were fetched and parsed.
/// Only query keys in `query_keys` appear in the shape, without their values.
pub fn signature(url: &Url, dom: Option<&DomFacts>, query_keys: &[String]) -> Signature {
    let path_shape = path_shape(url, query_keys);
    let dom_shape = dom.map(DomFacts::shape);
    let mut hasher = Sha256::new();
    hasher.update(path_shape.as_bytes());
    hasher.update(b"\n");
    hasher.update(dom_shape.as_deref().unwrap_or("").as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    Signature {
        path_shape,
        dom_shape,
        cluster_id: hex[..12].to_string(),
    }
}

/// The path shape of `url`.
pub fn path_shape(url: &Url, query_keys: &[String]) -> String {
    let segments: Vec<String> = url
        .path()
        .split('/')
        .filter(|s| !s.is_empty())
        .map(segment_shape)
        .collect();
    let mut shape = format!("/{}", segments.join("/"));
    let mut keys: Vec<String> = url
        .query_pairs()
        .map(|(k, _)| k.into_owned())
        .filter(|k| query_keys.iter().any(|q| q == k))
        .collect();
    keys.sort();
    keys.dedup();
    if !keys.is_empty() {
        shape.push('?');
        shape.push_str(&keys.join("&"));
    }
    shape
}

fn segment_shape(seg: &str) -> String {
    let lower = seg.to_ascii_lowercase();
    if !lower.is_empty() && lower.bytes().all(|b| b.is_ascii_digit()) {
        "{n}".to_string()
    } else if is_uuid(&lower) {
        "{uuid}".to_string()
    } else if lower.len() >= 16 && lower.bytes().all(|b| b.is_ascii_hexdigit()) {
        "{hex}".to_string()
    } else if lower.bytes().any(|b| b.is_ascii_digit()) {
        "{slug}".to_string()
    } else {
        lower
    }
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(u: &str) -> String {
        path_shape(&Url::parse(u).unwrap(), &["page".to_string()])
    }

    #[test]
    fn path_shapes() {
        assert_eq!(shape("https://e.gov/"), "/");
        assert_eq!(shape("https://e.gov/news/2024/"), "/news/{n}");
        assert_eq!(shape("https://e.gov/News/my-post-2"), "/news/{slug}");
        assert_eq!(shape("https://e.gov/about/team"), "/about/team");
        assert_eq!(
            shape("https://e.gov/a/123e4567-e89b-12d3-a456-426614174000"),
            "/a/{uuid}"
        );
        assert_eq!(shape("https://e.gov/a/abcdefabcdefabcdef"), "/a/{hex}");
        assert_eq!(shape("https://e.gov/list?page=3&utm=1"), "/list?page");
    }

    #[test]
    fn trailing_slash_does_not_change_the_shape() {
        assert_eq!(shape("https://e.gov/about"), shape("https://e.gov/about/"));
    }
}
