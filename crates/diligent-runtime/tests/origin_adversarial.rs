//! Adversarial tests for `origin_of`: userinfo tricks, encoding, IDN, control
//! characters, IPv6, and default-port normalisation.

use diligent_runtime::util::origin_of;

#[test]
fn userinfo_with_backslash_at() {
    // http://evil.com\@good.gov/ — backslash is rejected outright.
    assert_eq!(origin_of("http://evil.com\\@good.gov/"), None);
}

#[test]
fn userinfo_with_space_at() {
    // http://good.gov @evil.com/ — space is not in a valid URL; url::Url may
    // encode it or reject it. Assert actual behaviour.
    let r = origin_of("http://good.gov @evil.com/");
    // The space makes this either None (parse error) or some origin. Assert
    // whatever the engine does; the important thing is it does not return
    // good.gov's origin if the host is evil.com.
    if let Some(ref o) = r {
        assert!(
            !o.contains("good.gov"),
            "space in URL should not resolve to good.gov: {o}"
        );
    }
}

#[test]
fn userinfo_with_encoded_slash() {
    // http://good.gov%2F@evil.com/ — %2F in userinfo. url::Url treats the
    // host as evil.com with username "good.gov%2F". origin_of rejects URLs
    // with non-empty username.
    assert_eq!(origin_of("http://good.gov%2F@evil.com/"), None);
}

#[test]
fn trailing_dot_in_host() {
    // http://good.gov./ — trailing dot is a valid FQDN. url::Url preserves it
    // in host_str but origin serialisation may drop it. Assert actual behaviour.
    let r = origin_of("http://good.gov./");
    // The origin should normalise to good.gov (trailing dot dropped) or keep it.
    // Either way it should be Some and contain "good.gov".
    assert!(r.is_some(), "trailing dot host should parse");
    assert!(r.as_deref().unwrap().contains("good.gov"));
}

#[test]
fn uppercase_host_normalised() {
    assert_eq!(
        origin_of("http://GOOD.GOV:80/"),
        Some("http://good.gov".to_string())
    );
}

#[test]
fn https_default_port_dropped() {
    assert_eq!(
        origin_of("https://good.gov:443/"),
        Some("https://good.gov".to_string())
    );
}

#[test]
fn https_non_default_port_kept() {
    assert_eq!(
        origin_of("https://good.gov:8443/"),
        Some("https://good.gov:8443".to_string())
    );
}

#[test]
fn http_on_https_port_differs_from_https() {
    // http://good.gov:443/ is NOT the same origin as https://good.gov/
    let http_origin = origin_of("http://good.gov:443/");
    let https_origin = origin_of("https://good.gov/");
    assert_ne!(http_origin, https_origin);
}

#[test]
fn idn_cyrillic_o() {
    // IDN with Cyrillic о (U+043E) instead of Latin o — different origin.
    let idn = "http://g\u{043E}od.gov/";
    let r = origin_of(idn);
    // The url crate will either punycode-encode it or reject it. Assert it
    // does NOT equal the ASCII origin.
    if let Some(ref o) = r {
        assert_ne!(
            o, "http://good.gov",
            "Cyrillic о must not normalise to Latin o"
        );
    }
}

#[test]
fn nul_in_url() {
    assert_eq!(origin_of("http://good\u{0}evil.com/"), None);
}

#[test]
fn tab_in_url() {
    assert_eq!(origin_of("http://good\tevil.com/"), None);
}

#[test]
fn newline_in_url() {
    assert_eq!(origin_of("http://good\nevil.com/"), None);
}

#[test]
fn ipv6_loopback_with_port() {
    assert_eq!(
        origin_of("http://[::1]:80/"),
        Some("http://[::1]".to_string())
    );
}
