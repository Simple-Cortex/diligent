//! Which axe-core rules corroborate which deterministic check.
//!
//! The table is built from the rule list of the pinned axe-core bundle (see
//! `fixtures/a11y/axe-rules-<version>.json`), matching on ACT rule id first and
//! then on documented rule semantics. A check with an empty rule list has no
//! counterpart in axe-core; findings of that check are recorded as
//! `no_coverage`. A unit test asserts every id below appears in the committed
//! rule listing, so a rule that a later axe-core release drops fails CI.

/// Check id to the axe-core rule ids that cover it. Every `BRW-*` check has an entry.
pub const MAP: &[(&str, &[&str])] = &[
    ("BRW-TEXT-CONTRAST", &["color-contrast"]),
    ("BRW-FOCUS-VISIBLE", &[]),
    ("BRW-FOCUS-CYCLE", &[]),
    ("BRW-ARIA-HIDDEN-FOCUSABLE", &["aria-hidden-focus"]),
    ("BRW-LABEL-IN-NAME", &["label-content-name-mismatch"]),
    (
        "BRW-ARIA-ATTR-VALID",
        &[
            "aria-allowed-attr",
            "aria-required-attr",
            "aria-roles",
            "aria-valid-attr",
            "aria-valid-attr-value",
        ],
    ),
    ("BRW-PRESENTATIONAL-CHILDREN", &["nested-interactive"]),
    ("BRW-BYPASS-BLOCKS", &["bypass"]),
    ("BRW-LINK-SAME-NAME", &["identical-links-same-purpose"]),
    ("BRW-TEXT-SPACING-STYLE", &["avoid-inline-spacing"]),
    ("BRW-REFLOW-320", &[]),
    ("BRW-TARGET-SIZE", &["target-size"]),
    ("BRW-FOCUS-OBSCURED", &[]),
    ("BRW-NON-TEXT-CONTRAST", &[]),
    (
        "BRW-AX-NAME-REQUIRED",
        &[
            "aria-command-name",
            "empty-heading",
            "input-image-alt",
            "object-alt",
        ],
    ),
    ("BRW-ELEMENT-LANG-VALID", &["valid-lang"]),
    (
        "BRW-TABLE-HEADER-ASSOC",
        &["td-headers-attr", "th-has-data-cells"],
    ),
    ("BRW-FED-BANNER", &[]),
    ("BRW-FED-IDENTIFIER", &[]),
    ("BRW-FED-SEARCH", &[]),
    ("BRW-FED-DAP", &[]),
    ("BRW-FED-PRIVACY-LINK", &[]),
    ("BRW-FED-ACCESSIBILITY-STATEMENT", &[]),
    ("BRW-FED-TITLE-CRITERIA", &[]),
    ("BRW-FED-META-DESCRIPTION-CRITERIA", &[]),
    ("BRW-FED-CONTACT", &[]),
    ("BRW-FED-LANGUAGE-ACCESS", &[]),
    ("BRW-FED-MOBILE-VIEWPORT", &[]),
];

/// The axe-core rule ids that corroborate `check_id`; empty when none do or the check is unknown.
pub fn rules_for(check_id: &str) -> &'static [&'static str] {
    MAP.iter()
        .find(|(id, _)| *id == check_id)
        .map(|(_, rules)| *rules)
        .unwrap_or(&[])
}

/// Whether any check maps `rule_id`.
pub fn is_mapped_rule(rule_id: &str) -> bool {
    MAP.iter().any(|(_, rules)| rules.contains(&rule_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_check_has_exactly_one_entry() {
        for info in crate::checks::all() {
            let n = MAP.iter().filter(|(id, _)| *id == info.id).count();
            assert_eq!(n, 1, "{} needs exactly one MAP entry", info.id);
        }
        for (id, _) in MAP {
            assert!(
                crate::checks::all().iter().any(|i| i.id == *id),
                "{id} is not a check"
            );
        }
    }

    #[test]
    fn rule_ids_are_sorted_and_unique_per_check() {
        for (id, rules) in MAP {
            let mut sorted = rules.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(&sorted[..], *rules, "{id}: rules must be sorted and unique");
        }
    }

    #[test]
    fn a_rule_maps_to_one_check_only() {
        for (a, ra) in MAP {
            for (b, rb) in MAP {
                if a != b {
                    for r in *ra {
                        assert!(!rb.contains(r), "{r} maps to both {a} and {b}");
                    }
                }
            }
        }
    }

    #[test]
    fn every_mapped_rule_is_in_the_committed_listing() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/a11y");
        let mut listings: Vec<_> = std::fs::read_dir(&dir)
            .expect("fixtures/a11y exists")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("axe-rules-") && n.ends_with(".json"))
            })
            .collect();
        listings.sort();
        assert_eq!(
            listings.len(),
            1,
            "expected one axe-rules-<version>.json, found {listings:?}"
        );
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&listings[0]).unwrap()).unwrap();
        let listed: Vec<&str> = doc["rules"]
            .as_array()
            .expect("listing has rules")
            .iter()
            .filter_map(|r| r["id"].as_str())
            .collect();
        assert!(!listed.is_empty(), "listing has no rules");
        for (check, rules) in MAP {
            for r in *rules {
                assert!(
                    listed.contains(r),
                    "{check}: axe rule `{r}` is not in {:?}",
                    listings[0]
                );
            }
        }
    }
}
