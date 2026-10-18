//! Host literals the request handler fails regardless of resource type.

use diligent_netpolicy::is_forbidden_host_literal;

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .unwrap()
        .host_str()
        .unwrap()
        .to_string()
}

#[test]
fn metadata_address_is_a_forbidden_host_literal() {
    assert!(is_forbidden_host_literal(&host_of(
        "http://169.254.169.254/x"
    )));
}

#[test]
fn ipv6_loopback_is_a_forbidden_host_literal() {
    assert!(is_forbidden_host_literal(&host_of("http://[::1]/x")));
}

#[test]
fn public_hosts_are_not() {
    assert!(!is_forbidden_host_literal(&host_of(
        "https://example.com/x"
    )));
}
