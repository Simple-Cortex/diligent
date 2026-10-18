//! Sitemap parsing and traversal against hostile input.

use std::io::Write;

use diligent_inventory::sitemap::{
    collect, decode, parse, Limits, SitemapError, SitemapFetch, SitemapKind, MAX_DEPTH, MAX_FILES,
};
use flate2::write::GzEncoder;
use flate2::Compression;
use url::Url;

fn urlset(n: usize) -> String {
    let mut s = String::from(
        r#"<?xml version="1.0"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">"#,
    );
    for i in 0..n {
        s.push_str(&format!(
            "<url><loc>https://e.gov/p/{i}</loc><lastmod>2026-01-01</lastmod></url>"
        ));
    }
    s.push_str("</urlset>");
    s
}

fn gz(bytes: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(bytes).unwrap();
    enc.finish().unwrap()
}

#[test]
fn urlset_entries_carry_loc_and_lastmod() {
    let p = parse(urlset(3).as_bytes(), 1 << 20, false).unwrap();
    assert_eq!(p.kind, SitemapKind::UrlSet);
    assert_eq!(p.entries.len(), 3);
    assert_eq!(p.entries[1].loc.as_str(), "https://e.gov/p/1");
    assert_eq!(p.entries[1].lastmod.as_deref(), Some("2026-01-01"));
    assert!(!p.truncated);
}

#[test]
fn sitemap_index_is_recognized() {
    let xml = r#"<sitemapindex><sitemap><loc>https://e.gov/a.xml</loc></sitemap><sitemap><loc>https://e.gov/b.xml</loc></sitemap></sitemapindex>"#;
    let p = parse(xml.as_bytes(), 1 << 20, false).unwrap();
    assert_eq!(p.kind, SitemapKind::Index);
    assert_eq!(p.entries.len(), 2);
}

#[test]
fn gzip_is_detected_by_magic_bytes_not_by_name_or_header() {
    let body = gz(urlset(5).as_bytes());
    assert_eq!(&body[..2], &[0x1f, 0x8b]);
    let p = parse(&body, 1 << 20, false).unwrap();
    assert_eq!(p.entries.len(), 5);
}

#[test]
fn gzip_bomb_is_capped_by_the_decoded_size() {
    // 256 MiB of zeros compresses to a few hundred KiB; the decoder must stop at the cap.
    let mut enc = GzEncoder::new(Vec::new(), Compression::best());
    let chunk = vec![0u8; 1 << 20];
    for _ in 0..256 {
        enc.write_all(&chunk).unwrap();
    }
    let bomb = enc.finish().unwrap();
    assert!(bomb.len() < 2 << 20);
    let (out, cut) = decode(&bomb, 1 << 20);
    assert_eq!(out.len(), 1 << 20);
    assert!(cut);
    assert!(parse(&bomb, 1 << 20, false).is_err(), "zeros are not XML");
}

#[test]
fn damaged_gzip_is_reported_not_trusted() {
    let mut body = gz(urlset(50).as_bytes());
    body.truncate(body.len() / 2);
    let (_, cut) = decode(&body, 1 << 20);
    assert!(cut);
}

#[test]
fn ten_megabyte_sitemap_is_cut_at_the_cap_and_repaired() {
    let big = urlset(160_000);
    assert!(big.len() > 10 * 1024 * 1024);
    let p = parse(big.as_bytes(), 2 * 1024 * 1024, false).unwrap();
    assert!(p.truncated);
    assert!(p.entries.len() > 10_000 && p.entries.len() < 160_000);
    // The last kept entry is complete.
    assert!(p
        .entries
        .last()
        .unwrap()
        .loc
        .as_str()
        .starts_with("https://e.gov/p/"));
}

#[test]
fn body_cut_by_the_fetcher_is_repaired_and_flagged() {
    let xml = urlset(100);
    let cut = &xml.as_bytes()[..xml.len() / 2];
    let p = parse(cut, 1 << 20, true).unwrap();
    assert!(p.truncated);
    assert!(!p.entries.is_empty() && p.entries.len() < 100);
}

#[test]
fn malformed_and_foreign_xml_is_an_error() {
    assert!(matches!(
        parse(b"<urlset><url>", 1 << 20, false),
        Err(SitemapError::Malformed(_))
    ));
    assert!(matches!(
        parse(b"not xml at all", 1 << 20, false),
        Err(SitemapError::Malformed(_))
    ));
    assert!(matches!(
        parse(b"<html><body/></html>", 1 << 20, false),
        Err(SitemapError::NotSitemap(_))
    ));
    assert!(matches!(
        parse(b"", 1 << 20, false),
        Err(SitemapError::Malformed(_))
    ));
}

#[test]
fn entity_expansion_and_dtds_are_refused() {
    let lol = r#"<!DOCTYPE lolz [<!ENTITY a "aaaa"><!ENTITY b "&a;&a;&a;&a;">]><urlset><url><loc>&b;</loc></url></urlset>"#;
    assert!(parse(lol.as_bytes(), 1 << 20, false).is_err());
}

#[test]
fn absurd_nesting_is_refused_without_overflowing_the_stack() {
    let mut s = String::from("<urlset>");
    for _ in 0..1_000_000 {
        s.push_str("<a>");
    }
    assert!(matches!(
        parse(s.as_bytes(), usize::MAX, false),
        Err(SitemapError::Malformed(_))
    ));
}

#[test]
fn unusable_locs_are_skipped_and_counted() {
    let xml = r#"<urlset>
        <url><loc>javascript:alert(1)</loc></url>
        <url><loc>ftp://e.gov/x</loc></url>
        <url><loc>   </loc></url>
        <url><lastmod>2026-01-01</lastmod></url>
        <url><loc>  https://e.gov/ok  </loc><lastmod>this is far too long to be a date value at all</lastmod></url>
        </urlset>"#;
    let p = parse(xml.as_bytes(), 1 << 20, false).unwrap();
    assert_eq!(p.entries.len(), 1);
    assert_eq!(p.skipped_invalid, 4);
    assert_eq!(p.entries[0].lastmod, None);
}

fn limits() -> Limits {
    Limits {
        max_depth: MAX_DEPTH,
        max_files: MAX_FILES,
        max_body_bytes: 1 << 20,
        max_entries: 10_000,
    }
}

fn index_of(urls: &[String]) -> String {
    let items: String = urls
        .iter()
        .map(|u| format!("<sitemap><loc>{u}</loc></sitemap>"))
        .collect();
    format!("<sitemapindex>{items}</sitemapindex>")
}

#[test]
fn thousand_deep_index_chain_stops_at_the_depth_limit() {
    let root = Url::parse("https://e.gov/s0.xml").unwrap();
    let mut fetched = 0usize;
    let out = collect(&[(root, "sitemap")], &limits(), &mut |u: &Url| {
        fetched += 1;
        let n: usize = u
            .path()
            .trim_start_matches("/s")
            .trim_end_matches(".xml")
            .parse()
            .unwrap();
        SitemapFetch::Body {
            body: index_of(&[format!("https://e.gov/s{}.xml", n + 1)]).into_bytes(),
            truncated: false,
        }
    });
    assert_eq!(fetched, MAX_DEPTH + 1);
    assert!(out
        .notes
        .iter()
        .any(|n| n.starts_with("sitemap_depth_limit")));
    assert!(!out.remaining.is_empty());
}

#[test]
fn thousand_wide_index_stops_at_the_file_limit() {
    let root = Url::parse("https://e.gov/index.xml").unwrap();
    let children: Vec<String> = (0..1_000)
        .map(|i| format!("https://e.gov/c{i}.xml"))
        .collect();
    let mut fetched = 0usize;
    let out = collect(&[(root, "sitemap")], &limits(), &mut |u: &Url| {
        fetched += 1;
        if u.path() == "/index.xml" {
            SitemapFetch::Body {
                body: index_of(&children).into_bytes(),
                truncated: false,
            }
        } else {
            SitemapFetch::Body {
                body: urlset(1).into_bytes(),
                truncated: false,
            }
        }
    });
    assert_eq!(fetched, MAX_FILES);
    assert_eq!(out.entries.len(), MAX_FILES - 1);
    assert!(out
        .notes
        .iter()
        .any(|n| n.starts_with("sitemap_file_limit")));
}

#[test]
fn index_loops_visit_each_file_once_and_skips_are_noted() {
    let root = Url::parse("https://e.gov/a.xml").unwrap();
    let mut seen = Vec::new();
    let out = collect(&[(root, "robots_sitemap")], &limits(), &mut |u: &Url| {
        seen.push(u.path().to_string());
        match u.path() {
            "/a.xml" => SitemapFetch::Body {
                body: index_of(&[
                    "https://e.gov/b.xml".into(),
                    "https://e.gov/a.xml".into(),
                    "https://e.gov/gone.xml".into(),
                ])
                .into_bytes(),
                truncated: false,
            },
            "/b.xml" => SitemapFetch::Body {
                body: index_of(&["https://e.gov/a.xml".into()]).into_bytes(),
                truncated: false,
            },
            _ => SitemapFetch::Skip("status 404".to_string()),
        }
    });
    assert_eq!(seen, vec!["/a.xml", "/b.xml", "/gone.xml"]);
    assert!(out
        .notes
        .iter()
        .any(|n| n.contains("gone.xml") && n.contains("status 404")));
}

#[test]
fn entries_keep_the_label_of_their_root_and_respect_the_entry_cap() {
    let root = Url::parse("https://e.gov/s.xml").unwrap();
    let mut l = limits();
    l.max_entries = 5;
    let out = collect(&[(root, "robots_sitemap")], &l, &mut |_: &Url| {
        SitemapFetch::Body {
            body: urlset(50).into_bytes(),
            truncated: false,
        }
    });
    assert_eq!(out.entries.len(), 5);
    assert!(out
        .entries
        .iter()
        .all(|(_, label)| *label == "robots_sitemap"));
    assert!(out
        .notes
        .iter()
        .any(|n| n.starts_with("sitemap_entry_limit")));
}
