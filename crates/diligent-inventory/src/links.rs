//! Reading a fetched HTML page: title, structure and same-origin `<a href>` links.
//!
//! Page content is untrusted data. Nothing here changes scope, rate or permissions.

use scraper::{Html, Selector};
use url::Url;

use crate::signature::DomFacts;
use crate::urls::normalize;

/// Most `<a href>` values read from one page.
pub const MAX_HREFS_PER_PAGE: usize = 10_000;
/// Longest title kept, in characters.
const MAX_TITLE_CHARS: usize = 200;

/// What was read from one HTML page.
#[derive(Debug, Clone)]
pub struct PageFacts {
    /// Whitespace-collapsed `<title>`, cut to 200 characters.
    pub title: Option<String>,
    /// Raw `href` values in document order (capped).
    pub hrefs: Vec<String>,
    /// True when the href cap was hit.
    pub hrefs_capped: bool,
    /// The first `<base href>`, if present.
    pub base_href: Option<String>,
    /// Structural facts for the DOM shape.
    pub dom: DomFacts,
}

/// Parses `html` once and returns title, hrefs and structure.
pub fn analyze(html: &str) -> PageFacts {
    let doc = Html::parse_document(html);
    let title = Selector::parse("title")
        .ok()
        .and_then(|s| doc.select(&s).next())
        .map(|t| collapse(&t.text().collect::<String>()))
        .filter(|t| !t.is_empty())
        .map(|t| t.chars().take(MAX_TITLE_CHARS).collect());
    let base_href = Selector::parse("base[href]")
        .ok()
        .and_then(|s| doc.select(&s).next())
        .and_then(|b| b.value().attr("href"))
        .map(str::to_string);
    let mut hrefs = Vec::new();
    let mut hrefs_capped = false;
    if let Ok(sel) = Selector::parse("a[href]") {
        for a in doc.select(&sel) {
            if hrefs.len() >= MAX_HREFS_PER_PAGE {
                hrefs_capped = true;
                break;
            }
            if let Some(h) = a.value().attr("href") {
                hrefs.push(h.to_string());
            }
        }
    }
    PageFacts {
        title,
        hrefs,
        hrefs_capped,
        base_href,
        dom: DomFacts::from_html(&doc),
    }
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Resolves the page's hrefs against `final_url` (and a usable `<base href>`), keeps only
/// http(s) targets, drops fragments and undeclared query keys, and removes duplicates while
/// keeping document order. `rel=nofollow` links are ordinary links.
pub fn resolve_links(facts: &PageFacts, final_url: &Url, query_keys: &[String]) -> Vec<Url> {
    let base = facts
        .base_href
        .as_deref()
        .and_then(|b| final_url.join(b.trim()).ok())
        .filter(|b| matches!(b.scheme(), "http" | "https"))
        .unwrap_or_else(|| final_url.clone());
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for href in &facts.hrefs {
        let href = href.trim();
        if href.is_empty() || href.starts_with('#') {
            continue;
        }
        let Ok(url) = base.join(href) else { continue };
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            continue;
        }
        let url = normalize(&url, query_keys, false);
        if seen.insert(url.as_str().to_string()) {
            out.push(url);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_resolve_and_dangerous_schemes_are_dropped() {
        let html = r##"<html><head><title> My
            page </title></head><body>
            <a href="/a#x">a</a><a href="/a">a again</a><a href="javascript:alert(1)">j</a>
            <a href="data:text/html,hi">d</a><a href="mailto:x@y.gov">m</a>
            <a rel="nofollow" href="b?utm=1&page=2">b</a><a href="#top">t</a>
            <a href="https://other.example/x">o</a></body></html>"##;
        let facts = analyze(html);
        assert_eq!(facts.title.as_deref(), Some("My page"));
        let base = Url::parse("https://e.gov/dir/").unwrap();
        let links: Vec<String> = resolve_links(&facts, &base, &["page".to_string()])
            .iter()
            .map(Url::to_string)
            .collect();
        assert_eq!(
            links,
            vec![
                "https://e.gov/a",
                "https://e.gov/dir/b?page=2",
                "https://other.example/x"
            ]
        );
    }
}
