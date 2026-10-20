//! Template flavours: expectations beyond the manifest (source lines through
//! the pre-pass, opacity of components, fragments, and downgrades).

use std::fs;
use std::path::{Path, PathBuf};

use diligent_source::checks::DOCUMENT_LEVEL;
use diligent_source::{scan_text, Finding, Flavor, SourceRef, State};

fn read(rel: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source/templates")
        .join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn scan_file(rel: &str) -> Vec<Finding> {
    let flavor = Flavor::from_path(Path::new(rel)).expect("known extension");
    scan_text(&read(rel), &SourceRef::new(rel), flavor)
}

fn scan(text: &str, flavor: Flavor) -> Vec<Finding> {
    scan_text(text, &SourceRef::new("t"), flavor)
}

fn of<'f>(findings: &'f [Finding], check: &str) -> Vec<&'f Finding> {
    findings
        .iter()
        .filter(|f| f.result.check_id == check)
        .collect()
}

fn states(findings: &[Finding], check: &str) -> Vec<State> {
    of(findings, check)
        .iter()
        .map(|f| f.result.status)
        .collect()
}

fn lines(findings: &[Finding], check: &str) -> Vec<Option<u64>> {
    of(findings, check)
        .iter()
        .map(|f| f.evidence.context["line"].as_u64())
        .collect()
}

const IMG: &str = "SRC-IMG-ALT-PRESENT";

#[test]
fn lines_are_source_lines_in_every_flavour() {
    let jsx = scan_file("jsx/expressions.jsx");
    assert_eq!(lines(&jsx, IMG), vec![Some(7), Some(8), Some(9)]);
    assert_eq!(
        states(&jsx, IMG),
        vec![State::NeedsReview, State::Fail, State::NeedsReview]
    );

    let vue = scan_file("vue/gallery.vue");
    assert_eq!(lines(&vue, IMG), vec![Some(10), Some(13), Some(14)]);

    let svelte = scan_file("svelte/list.svelte");
    assert_eq!(lines(&svelte, IMG), vec![Some(7), Some(16)]);

    let astro = scan_file("astro/gallery.astro");
    assert_eq!(lines(&astro, IMG), vec![Some(7), Some(8)]);
    assert_eq!(lines(&astro, "SRC-LINK-NAME"), vec![Some(9)]);

    let twig = scan_file("twig/page.twig");
    assert_eq!(lines(&twig, IMG), vec![Some(4), Some(10)]);

    let liquid = scan_file("liquid/loop.liquid");
    assert_eq!(lines(&liquid, IMG), vec![Some(5)]);
    assert_eq!(lines(&liquid, "SRC-CONTROL-NAME"), vec![Some(6)]);
}

#[test]
fn byte_offsets_point_at_the_tag_in_the_original_text() {
    for rel in [
        "jsx/expressions.jsx",
        "vue/gallery.vue",
        "svelte/list.svelte",
        "astro/gallery.astro",
        "twig/page.twig",
        "liquid/loop.liquid",
    ] {
        let text = read(rel);
        for f in scan_file(rel) {
            let ctx = &f.evidence.context;
            let (Some(off), Some(tag)) = (ctx["byte_offset"].as_u64(), ctx["tag"].as_str()) else {
                continue;
            };
            let at = &text[off as usize..];
            assert!(
                at.starts_with(&format!("<{tag}")),
                "{rel}: offset {off} does not start <{tag}: {:?}",
                &at[..at.len().min(20)]
            );
        }
    }
}

#[test]
fn crlf_template_keeps_lines() {
    let text = read("vue/gallery.vue").replace('\n', "\r\n");
    let findings = scan(&text, Flavor::Vue);
    assert_eq!(lines(&findings, IMG), vec![Some(10), Some(13), Some(14)]);
}

#[test]
fn components_are_opaque_but_children_are_scanned() {
    for rel in [
        "jsx/expressions.jsx",
        "vue/gallery.vue",
        "svelte/list.svelte",
        "astro/gallery.astro",
    ] {
        for f in scan_file(rel) {
            assert_ne!(f.evidence.context["tag"], "dl-component", "{rel}");
        }
    }
    // The wrapper component reports nothing; its child still does.
    let findings = scan("<Layout><img src=\"/a.png\"></Layout>", Flavor::Jsx);
    assert_eq!(states(&findings, IMG), vec![State::Fail]);
    // A component with a role=button and no name is not judged.
    let findings = scan("<p><Widget role=\"button\" /></p>", Flavor::Jsx);
    assert_eq!(states(&findings, "SRC-BUTTON-NAME"), vec![State::NotTested]);
}

#[test]
fn fragments_record_one_not_tested_per_document_level_check() {
    for rel in ["jsx/clean.jsx", "vue/clean.vue", "astro/clean.astro"] {
        let findings = scan_file(rel);
        for id in DOCUMENT_LEVEL {
            let rows = of(&findings, id);
            assert_eq!(rows.len(), 1, "{rel} {id}");
            assert_eq!(rows[0].result.status, State::NotTested, "{rel} {id}");
            assert!(
                rows[0].result.observed["reason"]
                    .as_str()
                    .unwrap()
                    .contains("fragment"),
                "{rel} {id}"
            );
        }
    }
    // A template with an html tag is a document: its checks run.
    let findings = scan_file("twig/base.twig");
    assert_eq!(states(&findings, "SRC-HTML-LANG"), vec![State::NeedsReview]);
}

#[test]
fn attribute_expressions_never_pass_or_fail() {
    let cases: [(Flavor, &str); 5] = [
        (Flavor::Jsx, "<p><img alt={t} /></p>"),
        (Flavor::Vue, "<template><p><img :alt=\"t\"></p></template>"),
        (Flavor::Svelte, "<p><img alt={t}></p>"),
        (Flavor::Astro, "<p><img alt={t} /></p>"),
        (Flavor::Twig, "<p><img alt=\"{{ t }}\"></p>"),
    ];
    for (flavor, text) in cases {
        let findings = scan(text, flavor);
        assert_eq!(
            states(&findings, IMG),
            vec![State::NeedsReview],
            "{flavor:?}"
        );
    }
    // A literal string in braces is a literal.
    let findings = scan("<p><img alt={\"\"} /></p>", Flavor::Jsx);
    assert_eq!(states(&findings, IMG), vec![State::Pass]);
    // The same markup in a plain html file keeps its verdict.
    let findings = scan("<p><img alt=\"{t}\"></p>", Flavor::Html);
    assert_eq!(states(&findings, IMG), vec![State::NeedsReview]);
}

#[test]
fn a_spread_downgrades_a_fail() {
    let findings = scan("<p><img {...props} /></p>", Flavor::Jsx);
    assert_eq!(states(&findings, IMG), vec![State::NeedsReview]);
    let findings = scan(
        "<template><p><img v-bind=\"props\"></p></template>",
        Flavor::Vue,
    );
    assert_eq!(states(&findings, IMG), vec![State::NeedsReview]);
    let findings = scan("<p><img {...props}></p>", Flavor::Svelte);
    assert_eq!(states(&findings, IMG), vec![State::NeedsReview]);
    let findings = scan("<p><img src=\"/a.png\" /></p>", Flavor::Jsx);
    assert_eq!(states(&findings, IMG), vec![State::Fail]);
}

#[test]
fn text_expressions_make_names_needs_review() {
    let findings = scan("<p><a href=\"/x\">{label}</a></p>", Flavor::Jsx);
    assert_eq!(states(&findings, "SRC-LINK-NAME"), vec![State::NeedsReview]);
    let findings = scan("<p><button>{{ label }}</button></p>", Flavor::Liquid);
    assert_eq!(
        states(&findings, "SRC-BUTTON-NAME"),
        vec![State::NeedsReview]
    );
    let findings = scan("<p><a href=\"/x\"><Icon /></a></p>", Flavor::Jsx);
    assert_eq!(states(&findings, "SRC-LINK-NAME"), vec![State::NeedsReview]);
}

#[test]
fn conditionals_turn_duplicate_ids_into_needs_review() {
    let twig = "{% if a %}<p id=\"x\">1</p>{% else %}<p id=\"x\">2</p>{% endif %}";
    assert_eq!(
        states(&scan(twig, Flavor::Twig), "SRC-DUP-ID"),
        vec![State::NeedsReview]
    );
    let vue = "<template><p v-if=\"a\" id=\"x\">1</p><p v-else id=\"x\">2</p></template>";
    assert_eq!(
        states(&scan(vue, Flavor::Vue), "SRC-DUP-ID"),
        vec![State::NeedsReview]
    );
    // Without a conditional, the same duplicate is a failure.
    let plain = "<p id=\"x\">1</p><p id=\"x\">2</p>";
    assert_eq!(
        states(&scan(plain, Flavor::Twig), "SRC-DUP-ID"),
        vec![State::Fail]
    );
}

#[test]
fn expression_ids_do_not_collide() {
    let findings = scan("<p><i id={a}></i><i id={b}></i></p>", Flavor::Jsx);
    assert!(!states(&findings, "SRC-DUP-ID").contains(&State::Fail));
}

#[test]
fn evidence_shows_expressions_without_control_characters() {
    let findings = scan_file("jsx/expressions.jsx");
    let f = findings
        .iter()
        .find(|f| f.result.check_id == IMG && f.evidence.context["line"] == 7)
        .unwrap();
    let text = serde_json::to_string(&f.evidence).unwrap();
    assert!(!text.contains("\\u0001"), "{text}");
    assert!(f.evidence.context["outer_html"]
        .as_str()
        .unwrap()
        .contains("{expr}"));
    assert!(f
        .evidence
        .limitations
        .iter()
        .any(|l| l.contains("Template file")));
}

#[test]
fn every_template_fixture_is_in_the_manifest() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/source");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();
    let listed: std::collections::BTreeSet<&str> = manifest["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    let mut flavours = std::collections::BTreeMap::<String, usize>::new();
    let mut stack = vec![root.join("templates")];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let rel = p
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            assert!(listed.contains(rel.as_str()), "{rel} missing from manifest");
            let flavour = p
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            *flavours.entry(flavour).or_default() += 1;
        }
    }
    assert_eq!(flavours.len(), 6, "{flavours:?}");
    for (flavour, n) in flavours {
        assert!(n >= 2, "{flavour} has {n} fixture files");
    }
}

#[test]
fn syntax_soup_never_panics() {
    let alphabet: Vec<&str> = vec![
        "<", ">", "/", "{", "}", "%", "#", "\"", "'", "`", "=", "$", "-", "!", "a", "B", " ", "\n",
        "é", "{{", "}}", "{%", "%}", "<!--", "-->", "</", "/>", "...", "script", "v-if", ":x", "@",
        "---",
    ];
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let flavors = [
        Flavor::Html,
        Flavor::Jsx,
        Flavor::Vue,
        Flavor::Svelte,
        Flavor::Astro,
        Flavor::Twig,
        Flavor::Liquid,
    ];
    for _ in 0..600 {
        let len = (next() % 40) as usize;
        let text: String = (0..len)
            .map(|_| alphabet[(next() % alphabet.len() as u64) as usize])
            .collect();
        for flavor in flavors {
            let findings = scan(&text, flavor);
            assert!(!findings.is_empty(), "{flavor:?} {text:?}");
        }
    }
}
