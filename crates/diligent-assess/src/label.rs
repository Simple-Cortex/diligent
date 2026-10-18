//! Label-in-name helper (TC-02, WCAG 2.5.3 Label in Name).
//!
//! [`normalize_label`] is the string normalization helper the audit specification
//! requires. [`label_in_name`] compares a visible label with an accessible
//! name after normalizing both, and abstains with [`LabelMatch::Ambiguous`]
//! when string comparison cannot answer the question.

use unicode_normalization::UnicodeNormalization;

/// Characters stripped from the end of a label: `. : ! ? …`.
const TRAILING_PUNCTUATION: &[char] = &['.', ':', '!', '?', '\u{2026}'];

/// Quote characters stripped from both ends of a label.
const QUOTES: &[char] = &[
    '"', '\'', '\u{201C}', '\u{201D}', '\u{2018}', '\u{2019}', '\u{201E}', '\u{201A}', '\u{00AB}',
    '\u{00BB}', '\u{2039}', '\u{203A}',
];

/// Normalize a label for comparison.
///
/// Rules, applied in this order:
/// 1. Unicode NFKC. Fullwidth forms, ligatures, non-breaking spaces and the
///    ellipsis character fold to their plain equivalents.
/// 2. Unicode-aware lowercase (`str::to_lowercase`).
/// 3. Every run of Unicode whitespace becomes one ASCII space.
/// 4. Leading and trailing whitespace is trimmed.
/// 5. Surrounding quotes (straight and curly, single and double, guillemets)
///    and trailing `.`, `:`, `!`, `?` and `…` are stripped, repeatedly, until
///    nothing changes. So `"Search."` and `Search:` both become `search`.
///
/// Interior punctuation, symbols and apostrophes are kept: `don't` is
/// unchanged. A trailing apostrophe is treated as a closing quote.
pub fn normalize_label(s: &str) -> String {
    let folded: String = s.nfkc().collect::<String>().to_lowercase();
    let mut current = folded.split_whitespace().collect::<Vec<_>>().join(" ");
    loop {
        let next = current
            .trim_matches(|c: char| c.is_whitespace() || QUOTES.contains(&c))
            .trim_end_matches(|c: char| TRAILING_PUNCTUATION.contains(&c))
            .trim_end()
            .to_string();
        if next == current {
            return current;
        }
        current = next;
    }
}

/// Result of comparing a visible label with an accessible name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelMatch {
    /// Both normalize to the same string.
    Match,
    /// The accessible name starts with the visible label as whole words.
    Prefix,
    /// The visible label appears as a whole-word run inside the accessible name.
    Contained,
    /// The visible label does not appear in the accessible name.
    NoMatch,
    /// String comparison cannot decide. Send to review with this reason.
    Ambiguous(String),
}

/// A script group, for the mixed-script check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Latin,
    Greek,
    Cyrillic,
    Armenian,
    Hebrew,
    Arabic,
    Devanagari,
    Bengali,
    Tamil,
    Thai,
    Georgian,
    Hangul,
    /// Han, Hiragana and Katakana share one group because Japanese text mixes them.
    Cjk,
    Other,
}

fn script_of(c: char) -> Option<Script> {
    if !c.is_alphabetic() {
        return None;
    }
    let u = c as u32;
    Some(match u {
        0x0041..=0x024F
        | 0x1E00..=0x1EFF
        | 0x2C60..=0x2C7F
        | 0xA720..=0xA7FF
        | 0xFF21..=0xFF3A
        | 0xFF41..=0xFF5A => Script::Latin,
        0x0370..=0x03FF | 0x1F00..=0x1FFF => Script::Greek,
        0x0400..=0x052F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F => Script::Cyrillic,
        0x0530..=0x058F => Script::Armenian,
        0x0590..=0x05FF => Script::Hebrew,
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => {
            Script::Arabic
        }
        0x0900..=0x097F => Script::Devanagari,
        0x0980..=0x09FF => Script::Bengali,
        0x0B80..=0x0BFF => Script::Tamil,
        0x0E00..=0x0E7F => Script::Thai,
        0x10A0..=0x10FF => Script::Georgian,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => Script::Hangul,
        0x2E80..=0x2FDF
        | 0x3040..=0x30FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0x20000..=0x2FA1F => Script::Cjk,
        _ => Script::Other,
    })
}

fn has_mixed_scripts(s: &str) -> bool {
    let mut first: Option<Script> = None;
    for script in s.chars().filter_map(script_of) {
        match first {
            None => first = Some(script),
            Some(f) if f != script => return true,
            Some(_) => {}
        }
    }
    false
}

fn is_cjk(c: char) -> bool {
    script_of(c) == Some(Script::Cjk)
}

/// A character that continues a word. CJK characters never do, because those
/// scripts do not separate words with spaces.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() && !is_cjk(c)
}

/// Where `needle` occurs in `haystack` on word boundaries.
/// Returns `(whole_word_at_start, whole_word_anywhere, appears_inside_a_word)`.
fn find_whole_word(haystack: &str, needle: &str) -> (bool, bool, bool) {
    let mut at_start = false;
    let mut anywhere = false;
    let mut inside = false;
    let first = needle.chars().next();
    let last = needle.chars().next_back();
    for (index, _) in haystack.match_indices(needle) {
        let before = haystack[..index].chars().next_back();
        let after = haystack[index + needle.len()..].chars().next();
        let left_ok = before.is_none_or(|b| !is_word_char(b) || first.is_some_and(is_cjk));
        let right_ok = after.is_none_or(|a| !is_word_char(a) || last.is_some_and(is_cjk));
        if left_ok && right_ok {
            anywhere = true;
            if index == 0 {
                at_start = true;
            }
        } else {
            inside = true;
        }
    }
    (at_start, anywhere, inside)
}

/// Compare a visible label with an accessible name (WCAG 2.5.3).
///
/// Both strings are normalized with [`normalize_label`] first. Abstains with
/// [`LabelMatch::Ambiguous`] when:
/// * the visible label is empty, which usually means an icon-only control;
/// * the visible label has no letters or digits (symbols or emoji only);
/// * either string mixes characters from more than one script. Han, Hiragana
///   and Katakana count as one script;
/// * the visible label appears only inside a longer word of the accessible
///   name, for example `search` inside `researching`.
///
/// An empty accessible name with a real visible label is [`LabelMatch::NoMatch`].
pub fn label_in_name(visible: &str, accessible: &str) -> LabelMatch {
    let visible_folded: String = visible.nfkc().collect();
    if visible_folded.trim().is_empty() {
        return LabelMatch::Ambiguous("visible label is empty".to_string());
    }
    if !visible_folded.chars().any(char::is_alphanumeric) {
        return LabelMatch::Ambiguous(
            "visible label has no letters or digits (symbols or emoji only)".to_string(),
        );
    }
    let v = normalize_label(visible);
    let a = normalize_label(accessible);
    if v.is_empty() {
        return LabelMatch::Ambiguous("visible label is empty after normalization".to_string());
    }
    if has_mixed_scripts(&v) {
        return LabelMatch::Ambiguous("visible label mixes more than one script".to_string());
    }
    if has_mixed_scripts(&a) {
        return LabelMatch::Ambiguous("accessible name mixes more than one script".to_string());
    }
    if v == a {
        return LabelMatch::Match;
    }
    let (at_start, anywhere, inside) = find_whole_word(&a, &v);
    if at_start {
        LabelMatch::Prefix
    } else if anywhere {
        LabelMatch::Contained
    } else if inside {
        LabelMatch::Ambiguous(
            "visible label appears only inside a longer word of the accessible name".to_string(),
        )
    } else {
        LabelMatch::NoMatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use LabelMatch::*;

    #[test]
    fn normalization_rules() {
        assert_eq!(normalize_label("  Search  "), "search");
        assert_eq!(normalize_label("Search:"), "search");
        assert_eq!(normalize_label("Search\u{2026}"), "search");
        assert_eq!(normalize_label("\"Search.\""), "search");
        assert_eq!(normalize_label("\u{201C}Sign   in!\u{201D}"), "sign in");
        assert_eq!(normalize_label("Sign\u{00A0}\tin\n"), "sign in");
        assert_eq!(
            normalize_label("\u{FF33}\u{FF45}\u{FF41}\u{FF52}\u{FF43}\u{FF48}"),
            "search"
        );
        assert_eq!(normalize_label("STRASSE"), "strasse");
        assert_eq!(normalize_label("Don't"), "don't");
        assert_eq!(normalize_label("Why?!"), "why");
        assert_eq!(normalize_label(""), "");
    }

    #[test]
    fn exact_and_punctuation_differences_match() {
        assert_eq!(label_in_name("Search", "Search"), Match);
        assert_eq!(label_in_name("Search:", "search"), Match);
        assert_eq!(label_in_name("Email address", "  EMAIL   address "), Match);
    }

    #[test]
    fn prefix_and_contained() {
        assert_eq!(label_in_name("Search", "Search the site"), Prefix);
        assert_eq!(label_in_name("Search", "Site search"), Contained);
        assert_eq!(label_in_name("sign in", "Please sign in now"), Contained);
    }

    #[test]
    fn different_words_do_not_match() {
        assert_eq!(label_in_name("Submit", "Send"), NoMatch);
        assert_eq!(label_in_name("Submit", ""), NoMatch);
    }

    #[test]
    fn icon_buttons_and_symbols_are_ambiguous() {
        assert!(matches!(label_in_name("", "Search"), Ambiguous(_)));
        assert!(matches!(label_in_name("   ", "Search"), Ambiguous(_)));
        assert!(matches!(label_in_name("\u{1F50D}", "Search"), Ambiguous(_)));
        assert!(matches!(label_in_name("\u{00D7}", "Close"), Ambiguous(_)));
        assert!(matches!(label_in_name("...", "More"), Ambiguous(_)));
    }

    #[test]
    fn mixed_script_is_ambiguous() {
        assert!(matches!(
            label_in_name("Search \u{043F}\u{043E}\u{0438}\u{0441}\u{043A}", "Search"),
            Ambiguous(_)
        ));
        assert!(matches!(
            label_in_name("Search", "Search \u{0628}\u{062D}\u{062B}"),
            Ambiguous(_)
        ));
        // A Cyrillic look-alike inside a Latin word is the classic spoof.
        assert!(matches!(
            label_in_name("\u{0441}ontact", "contact"),
            Ambiguous(_)
        ));
    }

    #[test]
    fn single_non_latin_script_compares_normally() {
        assert_eq!(
            label_in_name(
                "\u{043F}\u{043E}\u{0438}\u{0441}\u{043A}",
                "\u{043F}\u{043E}\u{0438}\u{0441}\u{043A} \u{043F}\u{043E} \u{0441}\u{0430}\u{0439}\u{0442}\u{0443}"
            ),
            Prefix
        );
    }

    #[test]
    fn japanese_kana_and_han_are_one_script() {
        assert_eq!(
            label_in_name("\u{691C}\u{7D22}", "\u{691C}\u{7D22}\u{3059}\u{308B}"),
            Prefix
        );
    }

    #[test]
    fn partial_word_is_ambiguous() {
        assert!(matches!(
            label_in_name("search", "Researching topics"),
            Ambiguous(_)
        ));
        assert!(matches!(label_in_name("Submit", "Resubmit"), Ambiguous(_)));
    }

    #[test]
    fn digits_count_as_letters() {
        assert_eq!(label_in_name("3", "3 items"), Prefix);
        assert_eq!(label_in_name("Page 2", "Go to page 2"), Contained);
    }

    #[test]
    fn fixture_cases() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/content/label-in-name.json");
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let cases = doc["cases"].as_array().unwrap();
        assert!(cases.len() >= 12);
        for case in cases {
            let id = case["id"].as_str().unwrap();
            let got = label_in_name(
                case["visible"].as_str().unwrap(),
                case["accessible"].as_str().unwrap(),
            );
            let want = case["expected"].as_str().unwrap();
            let got_name = match got {
                Match => "match",
                Prefix => "prefix",
                Contained => "contained",
                NoMatch => "no_match",
                Ambiguous(_) => "ambiguous",
            };
            assert_eq!(got_name, want, "{id}");
        }
    }
}
