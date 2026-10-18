//! robots.txt parsing: group selection, longest match, wildcards, crawl delay, hostile input.

use diligent_inventory::robots::{matches, Robots, AGENT};
use url::Url;

fn allowed(r: &Robots, url: &str) -> bool {
    r.allowed(&Url::parse(url).unwrap())
}

#[test]
fn star_group_applies_when_no_specific_group_exists() {
    let r = Robots::parse("User-agent: *\nDisallow: /private/\n", AGENT);
    assert!(!allowed(&r, "https://e.gov/private/x"));
    assert!(allowed(&r, "https://e.gov/public"));
    assert!(!r.specific_group);
}

#[test]
fn diligent_group_replaces_star_group() {
    let text = "User-agent: *\nDisallow: /\n\nUser-agent: Diligent\nDisallow: /admin\n";
    let r = Robots::parse(text, AGENT);
    assert!(r.specific_group);
    assert!(allowed(&r, "https://e.gov/anything"));
    assert!(!allowed(&r, "https://e.gov/admin/users"));
}

#[test]
fn other_agents_groups_do_not_apply() {
    let r = Robots::parse("User-agent: badbot\nDisallow: /\n", AGENT);
    assert!(allowed(&r, "https://e.gov/"));
}

#[test]
fn consecutive_user_agent_lines_share_a_group() {
    let r = Robots::parse("User-agent: a\nUser-agent: diligent\nDisallow: /x\n", AGENT);
    assert!(!allowed(&r, "https://e.gov/x"));
}

#[test]
fn longest_match_wins_and_allow_wins_ties() {
    let text =
        "User-agent: *\nDisallow: /private/\nAllow: /private/faq\nDisallow: /same\nAllow: /same\n";
    let r = Robots::parse(text, AGENT);
    assert!(allowed(&r, "https://e.gov/private/faq"));
    assert!(allowed(&r, "https://e.gov/private/faq/1"));
    assert!(!allowed(&r, "https://e.gov/private/other"));
    assert!(allowed(&r, "https://e.gov/same"));
}

#[test]
fn wildcards_anchors_and_query_strings() {
    let r = Robots::parse(
        "User-agent: *\nDisallow: /*.json$\nDisallow: /search?*q=\n",
        AGENT,
    );
    assert!(!allowed(&r, "https://e.gov/a/b.json"));
    assert!(allowed(&r, "https://e.gov/a/b.json?x=1"));
    assert!(!allowed(&r, "https://e.gov/search?page=2&q=cats"));
    assert!(allowed(&r, "https://e.gov/search"));
}

#[test]
fn empty_disallow_allows_everything() {
    let r = Robots::parse("User-agent: *\nDisallow:\n", AGENT);
    assert!(allowed(&r, "https://e.gov/anything"));
}

#[test]
fn crawl_delay_is_read_from_the_selected_group_only() {
    let text = "User-agent: other\nCrawl-delay: 99\n\nUser-agent: *\nCrawl-delay: 2.5\n";
    assert_eq!(Robots::parse(text, AGENT).crawl_delay, Some(2.5));
    let bad = "User-agent: *\nCrawl-delay: -1\nCrawl-delay: nan\nCrawl-delay: soon\n";
    assert_eq!(Robots::parse(bad, AGENT).crawl_delay, None);
}

#[test]
fn comments_bom_crlf_and_unknown_fields_are_tolerated() {
    let text = "\u{feff}# hello\r\nUser-agent: * # everyone\r\nDisallow: /x # nope\r\nFoo: bar\r\nnot a field line\r\nSitemap: https://e.gov/s.xml\r\n";
    let r = Robots::parse(text, AGENT);
    assert!(!allowed(&r, "https://e.gov/x"));
    assert!(allowed(&r, "https://e.gov/y"));
}

#[test]
fn rules_before_any_user_agent_are_ignored() {
    let r = Robots::parse("Disallow: /\nUser-agent: *\nAllow: /\n", AGENT);
    assert!(allowed(&r, "https://e.gov/x"));
}

#[test]
fn matcher_edge_cases() {
    assert!(matches("/", "/anything"));
    assert!(matches("/a*", "/a"));
    assert!(matches("/*", "/"));
    assert!(matches("/a$", "/a"));
    assert!(!matches("/a$", "/ab"));
    assert!(matches("/a**b", "/aXXb"));
    assert!(matches("/*$", "/x/y"));
    assert!(!matches("/a*b$", "/ab/c"));
    assert!(matches("/é", "/é/x"));
}

#[test]
fn hostile_inputs_finish_quickly_and_do_not_panic() {
    // A million-line file, a pattern with thousands of wildcards, and binary garbage.
    let mut big = String::from("User-agent: *\n");
    for i in 0..1_000_000 {
        big.push_str(&format!("Disallow: /d{i}\n"));
    }
    let r = Robots::parse(&big, AGENT);
    assert!(allowed(&r, "https://e.gov/zzz"));

    let stars = format!("User-agent: *\nDisallow: /{}x\n", "*a".repeat(1_000));
    let r = Robots::parse(&stars, AGENT);
    let long_path = format!("https://e.gov/{}", "a".repeat(100_000));
    assert!(allowed(&r, &long_path));

    let garbage: String = (0..10_000u32)
        .map(|i| char::from_u32(0x20 + (i * 7919) % 0x2000).unwrap_or('?'))
        .collect();
    let _ = Robots::parse(&garbage, AGENT);

    let long_pattern = format!("User-agent: *\nDisallow: /{}\n", "a".repeat(100_000));
    let r = Robots::parse(&long_pattern, AGENT);
    assert!(
        allowed(&r, "https://e.gov/aaaa"),
        "patterns over the length cap are ignored"
    );
}
