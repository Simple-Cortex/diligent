//! Address policy: which hosts a scan must never contact (loopback, private
//! networks, link-local and cloud metadata, other special-use ranges).
//!
//! `std` only. IPv4 uses a deny list. IPv6 is deny-by-default: only global
//! unicast `2000::/3` is allowed, minus special-use carve-outs, and transition
//! forms are judged by the IPv4 address they embed.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Why `ip` is forbidden, or `None` when it is allowed. Suitable for limitation text.
pub fn forbidden_reason(ip: IpAddr) -> Option<&'static str> {
    match ip {
        IpAddr::V4(v4) => reason_v4(v4),
        IpAddr::V6(v6) => reason_v6(v6),
    }
}

/// The first reason any of `addrs` is forbidden, or `None` when all are allowed.
pub fn is_forbidden_addrs(addrs: &[IpAddr]) -> Option<&'static str> {
    addrs.iter().find_map(|a| forbidden_reason(*a))
}

/// True when a scan must not connect to `ip`.
pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    forbidden_reason(ip).is_some()
}

/// True when `host` (a URL host as written: a name, an IPv4 literal, or an IPv6
/// literal with or without brackets) is forbidden without any DNS lookup: an
/// IP literal in a forbidden range, `localhost`, `*.localhost` or
/// `localhost.localdomain`.
pub fn is_forbidden_host_literal(host: &str) -> bool {
    let mut h = host.trim();
    if let Some(inner) = h.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        h = inner;
    }
    let h = h.trim_end_matches('.').to_ascii_lowercase();
    if h.is_empty() {
        return true;
    }
    if h == "localhost" || h.ends_with(".localhost") || h == "localhost.localdomain" {
        return true;
    }
    match h.parse::<IpAddr>() {
        Ok(ip) => is_forbidden_ip(ip),
        Err(_) => false,
    }
}

fn reason_v4(ip: Ipv4Addr) -> Option<&'static str> {
    let o = ip.octets();
    if o[0] == 0 {
        Some("unspecified or 0.0.0.0/8 address")
    } else if ip.is_loopback() {
        Some("loopback address")
    } else if ip.is_private() {
        Some("private (RFC 1918) address")
    } else if ip.is_link_local() {
        Some("link-local address (includes cloud metadata 169.254.169.254)")
    } else if o[0] == 100 && (o[1] & 0xc0) == 64 {
        Some("carrier-grade NAT address (100.64.0.0/10)")
    } else if o[0] == 192 && o[1] == 0 && o[2] == 0 {
        Some("IETF protocol assignment address (192.0.0.0/24)")
    } else if o[0] == 198 && (o[1] & 0xfe) == 18 {
        Some("benchmarking address (198.18.0.0/15)")
    } else if ip.is_multicast() {
        Some("multicast address")
    } else if o[0] >= 240 {
        Some("reserved or broadcast address (240.0.0.0/4)")
    } else {
        None
    }
}

fn v4_from(a: u16, b: u16) -> Ipv4Addr {
    Ipv4Addr::new((a >> 8) as u8, a as u8, (b >> 8) as u8, b as u8)
}

fn reason_v6(ip: Ipv6Addr) -> Option<&'static str> {
    let s = ip.segments();
    // Transition forms outside 2000::/3 are judged by the IPv4 address they embed.
    if s[..5] == [0, 0, 0, 0, 0] && s[5] == 0xffff {
        // ::ffff:0:0/96 IPv4-mapped.
        return reason_v4(v4_from(s[6], s[7]));
    }
    if s[0] == 0x64 && s[1] == 0xff9b {
        if s[2..6] == [0, 0, 0, 0] {
            // 64:ff9b::/96 NAT64.
            return reason_v4(v4_from(s[6], s[7]));
        }
        if s[2] == 1 {
            // 64:ff9b:1::/48 local-use NAT64: the prefix length is deployment-specific,
            // so the embedded IPv4 address cannot be decoded reliably. Deny outright.
            return Some("local-use NAT64 prefix (64:ff9b:1::/48)");
        }
    }
    if (s[0] & 0xffc0) == 0xfec0 {
        return Some("deprecated site-local address (fec0::/10)");
    }
    if (s[0] & 0xfe00) == 0xfc00 {
        return Some("unique-local address (fc00::/7)");
    }
    if (s[0] & 0xe000) != 0x2000 {
        return Some("not global unicast (outside 2000::/3)");
    }
    if s[0] == 0x2001 && s[1] < 0x0200 {
        return Some("special-use address (2001::/23, includes Teredo)");
    }
    if s[0] == 0x2001 && s[1] == 0x0db8 {
        return Some("documentation address (2001:db8::/32)");
    }
    if s[0] == 0x2002 {
        // 6to4: judged by the embedded IPv4 address.
        return reason_v4(v4_from(s[1], s[2]));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn forbidden(s: &str) -> bool {
        is_forbidden_ip(ip(s))
    }

    #[test]
    fn v4_zero_and_loopback_boundaries() {
        assert!(forbidden("0.0.0.0"));
        assert!(forbidden("0.255.255.255"));
        assert!(!forbidden("1.0.0.0"));
        assert!(forbidden("127.0.0.1"));
        assert!(forbidden("127.255.255.255"));
        assert!(!forbidden("126.255.255.255"));
        assert!(!forbidden("128.0.0.0"));
    }

    #[test]
    fn v4_rfc1918_boundaries() {
        assert!(forbidden("10.0.0.0") && forbidden("10.255.255.255"));
        assert!(!forbidden("9.255.255.255") && !forbidden("11.0.0.0"));
        assert!(forbidden("172.16.0.0") && forbidden("172.31.255.255"));
        assert!(!forbidden("172.15.255.255") && !forbidden("172.32.0.0"));
        assert!(forbidden("192.168.0.0") && forbidden("192.168.255.255"));
        assert!(!forbidden("192.167.255.255") && !forbidden("192.169.0.0"));
    }

    #[test]
    fn v4_link_local_and_metadata() {
        assert!(forbidden("169.254.0.0"));
        assert!(forbidden("169.254.169.254"));
        assert!(forbidden("169.254.255.255"));
        assert!(!forbidden("169.253.255.255") && !forbidden("169.255.0.0"));
    }

    #[test]
    fn v4_cgnat_boundaries() {
        assert!(!forbidden("100.63.255.255"));
        assert!(forbidden("100.64.0.0") && forbidden("100.127.255.255"));
        assert!(!forbidden("100.128.0.0"));
    }

    #[test]
    fn v4_special_use_boundaries() {
        assert!(forbidden("192.0.0.0") && forbidden("192.0.0.255"));
        assert!(!forbidden("192.0.1.0"));
        assert!(!forbidden("198.17.255.255"));
        assert!(forbidden("198.18.0.0") && forbidden("198.19.255.255"));
        assert!(!forbidden("198.20.0.0"));
        assert!(!forbidden("223.255.255.255"));
        assert!(forbidden("224.0.0.0") && forbidden("239.255.255.255"));
        assert!(forbidden("240.0.0.0") && forbidden("255.255.255.255"));
    }

    #[test]
    fn v4_public_addresses_are_allowed() {
        assert!(!forbidden("8.8.8.8"));
        assert!(!forbidden("93.184.216.34"));
    }

    #[test]
    fn v6_outside_global_unicast_is_denied() {
        assert!(forbidden("::"));
        assert!(forbidden("::1"));
        assert!(forbidden("::2"));
        assert!(forbidden("1fff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"));
        assert!(forbidden("4000::"));
        assert!(forbidden("ff02::1"));
        // IPv4-compatible (deprecated) forms are denied, not unwrapped.
        assert!(forbidden("::8.8.8.8"));
    }

    #[test]
    fn v6_global_unicast_boundaries() {
        assert!(!forbidden("2000::1"));
        assert!(!forbidden("2606:4700:4700::1111"));
        assert!(!forbidden("2a00:1450:4001::1"));
        assert!(forbidden("4000::1"));
    }

    #[test]
    fn v6_unique_local_and_link_local() {
        assert!(forbidden("fbff:ffff::1"));
        assert!(forbidden("fc00::") && forbidden("fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"));
        assert!(forbidden("fe00::1"));
        assert!(forbidden("fe80::1") && forbidden("febf::1"));
        assert!(forbidden("fec0::1") && forbidden("feff::1"));
    }

    #[test]
    fn v6_special_use_2001_boundaries() {
        assert!(!forbidden("2000:ffff::1"));
        assert!(forbidden("2001::") && forbidden("2001:0:4136:e378:8000:63bf:3fff:fdd2"));
        assert!(forbidden("2001:1ff:ffff:ffff:ffff:ffff:ffff:ffff"));
        assert!(!forbidden("2001:200::1"));
        assert!(!forbidden("2001:db7:ffff::1"));
        assert!(forbidden("2001:db8::") && forbidden("2001:db8:ffff:ffff:ffff:ffff:ffff:ffff"));
        assert!(!forbidden("2001:db9::1"));
    }

    #[test]
    fn v6_6to4_uses_embedded_v4() {
        assert!(forbidden("2002:7f00:1::")); // 127.0.0.1
        assert!(forbidden("2002:a9fe:a9fe::")); // 169.254.169.254
        assert!(forbidden("2002:0a00:0005::")); // 10.0.0.5
        assert!(!forbidden("2002:0808:0808::")); // 8.8.8.8
    }

    #[test]
    fn v6_ipv4_mapped_uses_embedded_v4() {
        assert!(forbidden("::ffff:127.0.0.1"));
        assert!(forbidden("::ffff:10.0.0.5"));
        assert!(forbidden("::ffff:169.254.169.254"));
        assert!(!forbidden("::ffff:8.8.8.8"));
        assert!(forbidden("::fffe:8.8.8.8"));
    }

    #[test]
    fn v6_nat64_uses_embedded_v4() {
        assert!(forbidden("64:ff9b::7f00:1"));
        assert!(forbidden("64:ff9b::a9fe:a9fe"));
        assert!(!forbidden("64:ff9b::808:808"));
        assert!(forbidden("64:ff9a::808:808"));
        assert!(forbidden("64:ff9b:2::808:808"));
        // Local-use 64:ff9b:1::/48 is denied outright, even around a public IPv4.
        assert!(forbidden("64:ff9b:1::"));
        assert!(forbidden("64:ff9b:1:808:0:800::"));
        assert!(forbidden("64:ff9b:1:ffff:ffff:ffff:ffff:ffff"));
    }

    #[test]
    fn host_literals() {
        assert!(is_forbidden_host_literal("169.254.169.254"));
        assert!(is_forbidden_host_literal("[::1]"));
        assert!(is_forbidden_host_literal("::1"));
        assert!(is_forbidden_host_literal("[fc00::1]"));
        assert!(is_forbidden_host_literal("10.0.0.5"));
        assert!(is_forbidden_host_literal("localhost"));
        assert!(is_forbidden_host_literal("LOCALHOST."));
        assert!(is_forbidden_host_literal("app.localhost"));
        assert!(is_forbidden_host_literal("localhost.localdomain"));
        assert!(is_forbidden_host_literal(""));
        assert!(!is_forbidden_host_literal("example.com"));
        assert!(!is_forbidden_host_literal("notlocalhost.example"));
        assert!(!is_forbidden_host_literal("8.8.8.8"));
        assert!(!is_forbidden_host_literal("[2606:4700:4700::1111]"));
    }

    #[test]
    fn reasons_are_reported() {
        assert!(forbidden_reason(ip("127.0.0.1")).is_some());
        assert!(forbidden_reason(ip("8.8.8.8")).is_none());
        assert!(forbidden_reason(ip("fe80::1")).is_some());
    }
}
