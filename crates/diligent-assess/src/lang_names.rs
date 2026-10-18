//! Names a language-access entry point may carry (`BRW-FED-LANGUAGE-ACCESS`).
//!
//! The list is deliberately short and fixed. It recognises a control by what
//! it says, never by what language its target is in.

/// Language names in their own language, and the generic phrases agencies use.
pub const NAMES: &[&str] = &[
    "Español",
    "Français",
    "中文",
    "Tiếng Việt",
    "한국어",
    "Tagalog",
    "Русский",
    "العربية",
    "Kreyòl Ayisyen",
    "Português",
    "Deutsch",
    "日本語",
    "Italiano",
    "Polski",
    "हिन्दी",
    "Languages",
    "Language assistance",
    "Other languages",
];

/// True for characters of the Latin script blocks that carry the listed names.
fn latin(c: char) -> bool {
    c.is_ascii() || ('\u{00C0}'..='\u{024F}').contains(&c) || ('\u{1E00}'..='\u{1EFF}').contains(&c)
}

fn tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The listed name that `text` contains, if any.
///
/// Latin-script names match whole words, so `Deutsche Bahn` does not match
/// `Deutsch`. Names in other scripts match as substrings, since those scripts
/// do not separate words with spaces.
pub fn find(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    let words = tokens(text);
    NAMES.iter().copied().find(|name| {
        if name.chars().all(|c| latin(c) || c.is_whitespace()) {
            let want = tokens(name);
            !want.is_empty() && words.windows(want.len()).any(|w| w == want.as_slice())
        } else {
            lower.contains(&name.to_lowercase())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::find;

    #[test]
    fn matches_whole_latin_words_only() {
        assert_eq!(find("En español"), Some("Español"));
        assert_eq!(find("Deutsche Bahn"), None);
        assert_eq!(find("Other languages"), Some("Languages"));
        assert_eq!(find("Tiếng Việt"), Some("Tiếng Việt"));
    }

    #[test]
    fn matches_other_scripts_as_substrings() {
        assert_eq!(find("中文版"), Some("中文"));
        assert_eq!(find("한국어 (Korean)"), Some("한국어"));
        assert_eq!(find("Contact us"), None);
    }
}
