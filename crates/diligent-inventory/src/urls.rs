//! URL helpers: normalization, origin keys and document detection.
//!
//! Every URL that enters the inventory goes through [`normalize`], so duplicates
//! that differ only by fragment, host case, default port, credentials or an
//! undeclared query key collapse to one entry. Path case is significant and kept.

use url::Url;

/// Document kinds the inventory records without fetching.
const DOC_EXTENSIONS: [(&str, &str); 4] = [
    ("pdf", "pdf"),
    ("docx", "docx"),
    ("xlsx", "xlsx"),
    ("pptx", "pptx"),
];

/// Returns `url` with the fragment and credentials removed and the query reduced to
/// the keys in `query_keys` (sorted by key then value). With `keep_all_query` the
/// whole query is kept (operator-supplied URLs such as the start URL).
pub fn normalize(url: &Url, query_keys: &[String], keep_all_query: bool) -> Url {
    let mut out = url.clone();
    out.set_fragment(None);
    let _ = out.set_username("");
    let _ = out.set_password(None);
    if !keep_all_query {
        let mut kept: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| query_keys.iter().any(|q| q == k.as_ref()))
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        kept.sort();
        if kept.is_empty() {
            out.set_query(None);
        } else {
            out.query_pairs_mut()
                .clear()
                .extend_pairs(kept.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        }
    }
    out
}

/// `scheme://host[:port]` in ASCII, lowercase, default port elided.
pub fn origin_key(url: &Url) -> String {
    url.origin().ascii_serialization()
}

/// Parses an origin given by an operator (`https://host` or a full URL) to its key.
pub fn parse_origin(text: &str) -> Option<String> {
    let url = Url::parse(text.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    Some(origin_key(&url))
}

/// The document kind named by the URL's path extension, if any.
pub fn doc_kind_from_path(url: &Url) -> Option<&'static str> {
    let last = url.path().rsplit('/').next().unwrap_or("");
    let ext = last.rsplit_once('.')?.1.to_ascii_lowercase();
    DOC_EXTENSIONS
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, k)| *k)
}

/// The document kind named by a `Content-Type` header value, if any.
pub fn doc_kind_from_content_type(value: &str) -> Option<&'static str> {
    let mime = value
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match mime.as_str() {
        "application/pdf" => Some("pdf"),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => Some("docx"),
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => Some("xlsx"),
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => Some("pptx"),
        _ => None,
    }
}

/// Host of `url` as written for vendor records (lowercase ASCII, IPv6 without brackets).
pub fn host_string(url: &Url) -> Option<String> {
    match url.host()? {
        url::Host::Domain(d) => Some(d.to_ascii_lowercase()),
        url::Host::Ipv4(ip) => Some(ip.to_string()),
        url::Host::Ipv6(ip) => Some(ip.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn fragment_credentials_and_undeclared_query_are_dropped() {
        let keys = vec!["page".to_string()];
        let n = normalize(
            &u("https://User:pw@EXAMPLE.gov:443/a?utm=1&page=2#top"),
            &keys,
            false,
        );
        assert_eq!(n.as_str(), "https://example.gov/a?page=2");
    }

    #[test]
    fn start_url_keeps_its_query() {
        let n = normalize(&u("https://example.gov/?lang=en#x"), &[], true);
        assert_eq!(n.as_str(), "https://example.gov/?lang=en");
    }

    #[test]
    fn document_kinds() {
        assert_eq!(
            doc_kind_from_path(&u("https://e.gov/a/Report.PDF")),
            Some("pdf")
        );
        assert_eq!(doc_kind_from_path(&u("https://e.gov/a/report.html")), None);
        assert_eq!(
            doc_kind_from_content_type("application/pdf; charset=binary"),
            Some("pdf")
        );
    }
}
