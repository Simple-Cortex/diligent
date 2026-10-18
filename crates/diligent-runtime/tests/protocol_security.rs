//! `origin_of` must reject anything that is not a plain http(s) origin.

use diligent_runtime::util::origin_of;

#[test]
fn userinfo_is_rejected() {
    assert_eq!(origin_of("http://user:pw@example.com/"), None);
    assert_eq!(origin_of("https://user@example.com/"), None);
    assert_eq!(origin_of("https://example.com@evil.test/"), None);
}

#[test]
fn backslash_is_rejected() {
    assert_eq!(origin_of("https://example.com\\@evil.test/"), None);
    assert_eq!(origin_of("https://example.com/a\\b"), None);
}

#[test]
fn control_characters_are_rejected() {
    assert_eq!(origin_of("https://exa\tmple.com/"), None);
    assert_eq!(origin_of("https://example.com/\n"), None);
    assert_eq!(origin_of("https://example.com/\u{0}"), None);
    assert_eq!(origin_of("https://example.com/\u{7f}"), None);
}

#[test]
fn host_is_lowercased() {
    assert_eq!(
        origin_of("HTTPS://EXAMPLE.Com/Path").as_deref(),
        Some("https://example.com")
    );
}

#[test]
fn default_ports_are_dropped_and_explicit_ports_kept() {
    assert_eq!(
        origin_of("http://example.com:80/").as_deref(),
        Some("http://example.com")
    );
    assert_eq!(
        origin_of("https://example.com:443/").as_deref(),
        Some("https://example.com")
    );
    assert_eq!(
        origin_of("https://example.com:8443/").as_deref(),
        Some("https://example.com:8443")
    );
    assert_eq!(
        origin_of("http://example.com:443/").as_deref(),
        Some("http://example.com:443")
    );
    assert_eq!(
        origin_of("https://example.com:80/").as_deref(),
        Some("https://example.com:80")
    );
}

#[test]
fn non_http_schemes_are_rejected() {
    assert_eq!(origin_of("javascript:alert(1)"), None);
    assert_eq!(origin_of("file:///etc/passwd"), None);
    assert_eq!(origin_of("data:text/html,<p>x</p>"), None);
    assert_eq!(origin_of("about:blank"), None);
    assert_eq!(origin_of("ftp://example.com/"), None);
}

#[test]
fn equivalent_spellings_share_one_origin() {
    assert_eq!(
        origin_of("https://example.com"),
        origin_of("https://EXAMPLE.com:443/x?y#z")
    );
    assert_eq!(
        origin_of("http://[::1]:80/").as_deref(),
        Some("http://[::1]")
    );
}
