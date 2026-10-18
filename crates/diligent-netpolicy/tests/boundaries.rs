//! Exhaustive boundary tests for the address policy.
//!
//! Every range boundary listed in the task, plus host-literal edge cases.

use diligent_netpolicy::{forbidden_reason, is_forbidden_host_literal, is_forbidden_ip};
use std::net::IpAddr;

fn forbidden(s: &str) -> bool {
    is_forbidden_ip(s.parse::<IpAddr>().unwrap())
}

// ---- IPv4 boundaries ----

#[test]
fn v4_boundary_9_vs_10() {
    assert!(!forbidden("9.255.255.255"));
    assert!(forbidden("10.0.0.0"));
}

#[test]
fn v4_boundary_172_15_vs_172_16() {
    assert!(!forbidden("172.15.255.255"));
    assert!(forbidden("172.16.0.0"));
}

#[test]
fn v4_boundary_172_31_vs_172_32() {
    assert!(forbidden("172.31.255.255"));
    assert!(!forbidden("172.32.0.0"));
}

#[test]
fn v4_boundary_192_167_vs_192_168() {
    assert!(!forbidden("192.167.255.255"));
    assert!(forbidden("192.168.0.0"));
}

#[test]
fn v4_cgnat_boundaries() {
    assert!(!forbidden("100.63.255.255"));
    assert!(forbidden("100.64.0.0"));
    assert!(forbidden("100.127.255.255"));
    assert!(!forbidden("100.128.0.0"));
}

#[test]
fn v4_link_local_boundary() {
    assert!(!forbidden("169.253.255.255"));
    assert!(forbidden("169.254.0.0"));
}

#[test]
fn v4_zero_loopback_multicast_reserved() {
    assert!(forbidden("0.0.0.0"));
    assert!(forbidden("127.0.0.1"));
    assert!(forbidden("224.0.0.1"));
    assert!(forbidden("240.0.0.1"));
    assert!(forbidden("255.255.255.255"));
}

// ---- IPv6 boundaries ----

#[test]
fn v6_loopback_unspecified() {
    assert!(forbidden("::1"));
    assert!(forbidden("::"));
}

#[test]
fn v6_ipv4_mapped() {
    assert!(forbidden("::ffff:127.0.0.1"));
    assert!(!forbidden("::ffff:8.8.8.8"));
}

#[test]
fn v6_nat64() {
    assert!(forbidden("64:ff9b::7f00:1"));
    assert!(!forbidden("64:ff9b::808:808"));
    assert!(forbidden("64:ff9b:1::1"));
}

#[test]
fn v6_6to4() {
    assert!(forbidden("2002:7f00:1::"));
    assert!(!forbidden("2002:808:808::"));
}

#[test]
fn v6_documentation_and_teredo() {
    assert!(forbidden("2001:db8::1"));
    assert!(forbidden("2001::1")); // Teredo
}

#[test]
fn v6_global_unicast_allowed() {
    assert!(!forbidden("2001:4860:4860::8888"));
    assert!(!forbidden("3fff::1"));
}

#[test]
fn v6_unique_local_link_local_site_local_multicast() {
    assert!(forbidden("fc00::1"));
    assert!(forbidden("fd00::1"));
    assert!(forbidden("fe80::1"));
    assert!(forbidden("fec0::1"));
    assert!(forbidden("ff02::1"));
}

#[test]
fn v6_above_global_unicast_forbidden() {
    assert!(forbidden("4000::1"));
}

// ---- Host literals ----

#[test]
fn host_literal_localhost_variants() {
    assert!(is_forbidden_host_literal("localhost"));
    assert!(is_forbidden_host_literal("LOCALHOST"));
    assert!(is_forbidden_host_literal("foo.localhost"));
    assert!(is_forbidden_host_literal("localhost.localdomain"));
}

#[test]
fn host_literal_ipv6_brackets() {
    assert!(is_forbidden_host_literal("[::1]"));
    assert!(is_forbidden_host_literal("[::ffff:169.254.169.254]"));
}

#[test]
fn host_literal_metadata_ip() {
    assert!(is_forbidden_host_literal("169.254.169.254"));
}

#[test]
fn host_literal_odd_ipv4_forms() {
    // These are non-standard IPv4 representations. is_forbidden_host_literal
    // uses Rust's IpAddr::from_str which does NOT accept hex, decimal, or
    // short forms. They are treated as domain names and NOT recognised as
    // forbidden by this function.
    //
    // NOTE: url::Url DOES parse these forms and normalises them to standard
    // dotted-quad. In the SSRF flow, url::Url parses the redirect target
    // first, so http://127.1/ becomes http://127.0.0.1/ before vet_target
    // sees it. The security property holds via url::Url's normalisation,
    // not via is_forbidden_host_literal. If these forms were passed directly
    // to is_forbidden_host_literal (bypassing url::Url), they would not be
    // caught. Low risk because the normal flow always goes through url::Url.
    assert!(!is_forbidden_host_literal("0x7f000001"));
    assert!(!is_forbidden_host_literal("127.1"));
    assert!(!is_forbidden_host_literal("2130706433"));
}

#[test]
fn forbidden_reason_messages_are_informative() {
    let r = forbidden_reason("127.0.0.1".parse().unwrap());
    assert!(r.is_some());
    assert!(r.unwrap().contains("loopback"));

    let r = forbidden_reason("169.254.169.254".parse().unwrap());
    assert!(r.is_some());
    assert!(r.unwrap().contains("link-local"));

    let r = forbidden_reason("10.0.0.1".parse().unwrap());
    assert!(r.is_some());
    assert!(r.unwrap().contains("RFC 1918"));

    let r = forbidden_reason("::ffff:10.0.0.5".parse().unwrap());
    assert!(r.is_some());

    let r = forbidden_reason("8.8.8.8".parse().unwrap());
    assert!(r.is_none());
}
