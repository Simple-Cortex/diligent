//! Plain-language content metrics (research doc section 4.4).
//!
//! Everything here is a heuristic over English prose. The functions are pure
//! and deterministic, and every rule is documented next to the code so a
//! reviewer can see why a number came out the way it did. None of these
//! numbers is a pass or fail on its own: they are leads for a human reviewer.

use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------

/// Split text into words.
///
/// A word is a run of alphanumeric characters. An apostrophe (straight or
/// curly) or hyphen joins two runs when it sits between alphanumerics, so
/// `don't` and `well-known` are one word each. A `.` or `,` joins two runs
/// only between ASCII digits, so `3.5` and `1,200` are one word each.
pub fn words(text: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].1.is_alphanumeric() {
            i += 1;
            continue;
        }
        let start = chars[i].0;
        let mut j = i;
        while j < chars.len() {
            let c = chars[j].1;
            if c.is_alphanumeric() {
                j += 1;
                continue;
            }
            let joins = j + 1 < chars.len() && j > i && {
                let next = chars[j + 1].1;
                let prev = chars[j - 1].1;
                match c {
                    '\'' | '\u{2019}' | '-' => next.is_alphanumeric(),
                    '.' | ',' => prev.is_ascii_digit() && next.is_ascii_digit(),
                    _ => false,
                }
            };
            if joins {
                j += 1;
            } else {
                break;
            }
        }
        let end = if j < chars.len() {
            chars[j].0
        } else {
            text.len()
        };
        out.push(&text[start..end]);
        i = j;
    }
    out
}

// ---------------------------------------------------------------------------
// Sentences
// ---------------------------------------------------------------------------

/// Abbreviations whose final period never ends a sentence, compared without
/// case. `etc.` is on the list because it usually closes a list inside a
/// sentence, which means a sentence that really ends in `etc.` and is followed
/// by a capital letter is merged with the next one.
const ABBREVIATIONS: &[&str] = &[
    "e.g.", "i.e.", "u.s.", "dr.", "mr.", "ms.", "mrs.", "prof.", "no.", "vs.", "etc.", "st.",
    "jr.", "sr.", "inc.", "ltd.", "co.", "u.s.c.", "c.f.r.", "fig.", "approx.",
];

fn is_opening(c: char) -> bool {
    matches!(c, '"' | '\'' | '\u{201C}' | '\u{2018}' | '(' | '[')
}

fn is_closing(c: char) -> bool {
    matches!(c, '"' | '\'' | '\u{201D}' | '\u{2019}' | ')' | ']')
}

/// True when the period at byte `pos` closes an abbreviation, a single
/// capital initial or a numbered list marker rather than a sentence.
fn period_is_not_terminal(text: &str, pos: usize) -> bool {
    let before = &text[..pos];
    let token_start = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '.')
        .last()
        .map_or(pos, |(i, _)| i);
    let token = &text[token_start..pos];
    if token.is_empty() {
        return false;
    }
    let with_dot = format!("{}.", token.to_lowercase());
    if ABBREVIATIONS.contains(&with_dot.as_str()) {
        return true;
    }
    let mut chars = token.chars();
    let first = chars.next();
    // Single capital initial, as in `J. Smith`.
    if token.chars().count() == 1 && first.is_some_and(char::is_uppercase) {
        return true;
    }
    // List marker, as in `1. Apply`.
    token.chars().all(|c| c.is_ascii_digit())
}

/// Split text into sentences.
///
/// A sentence ends at one or more of `.`, `!` or `?`, plus any closing quotes
/// or brackets, when the next thing is whitespace followed by an uppercase
/// letter (after any opening quote or bracket) or the end of the text. A blank
/// line also ends a sentence, so headings and list items without final
/// punctuation do not run together. A `.` does not end a sentence after an
/// entry in the abbreviation list (`e.g.`, `i.e.`, `U.S.`, `Dr.`, `Mr.`,
/// `Ms.`, `No.`, `vs.`, `etc.` and a few more), after a single capital initial,
/// or after a bare number such as a list marker. Returned slices are trimmed
/// and non-empty.
///
/// Known failures: a sentence that ends in an abbreviation from the list and
/// is followed by a capital is merged with the next one, and text with no
/// capitals after full stops (all lowercase, or all caps) is not split.
pub fn sentences(text: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let byte_at = |idx: usize| if idx < n { chars[idx].0 } else { text.len() };
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    fn push<'a>(out: &mut Vec<&'a str>, text: &'a str, from: usize, to: usize) {
        let s = text[from..to].trim();
        if !s.is_empty() {
            out.push(s);
        }
    }
    while i < n {
        let (pos, c) = chars[i];
        if c == '\n' {
            // Blank line: only whitespace between this newline and the next.
            let mut k = i + 1;
            while k < n && chars[k].1.is_whitespace() && chars[k].1 != '\n' {
                k += 1;
            }
            if k < n && chars[k].1 == '\n' {
                push(&mut out, text, start, pos);
                while k < n && chars[k].1.is_whitespace() {
                    k += 1;
                }
                start = byte_at(k);
                i = k;
                continue;
            }
        }
        if matches!(c, '.' | '!' | '?') {
            let mut j = i + 1;
            while j < n && (matches!(chars[j].1, '.' | '!' | '?') || is_closing(chars[j].1)) {
                j += 1;
            }
            let end = byte_at(j);
            if j >= n {
                push(&mut out, text, start, end);
                start = text.len();
                break;
            }
            if !chars[j].1.is_whitespace() {
                i = j;
                continue;
            }
            let single_period = c == '.' && j == i + 1;
            if single_period && period_is_not_terminal(text, pos) {
                i = j;
                continue;
            }
            let mut k = j;
            while k < n && chars[k].1.is_whitespace() {
                k += 1;
            }
            let mut m = k;
            while m < n && is_opening(chars[m].1) {
                m += 1;
            }
            if m >= n || chars[m].1.is_uppercase() {
                push(&mut out, text, start, end);
                start = byte_at(k);
                i = k;
                continue;
            }
            i = j;
            continue;
        }
        i += 1;
    }
    if start < text.len() {
        push(&mut out, text, start, text.len());
    }
    out
}

// ---------------------------------------------------------------------------
// Syllables
// ---------------------------------------------------------------------------

const VOWELS: &str = "aeiouy\u{e0}\u{e1}\u{e2}\u{e3}\u{e4}\u{e5}\u{e8}\u{e9}\u{ea}\u{eb}\u{ec}\u{ed}\u{ee}\u{ef}\u{f2}\u{f3}\u{f4}\u{f5}\u{f6}\u{f9}\u{fa}\u{fb}\u{fc}\u{fd}";

/// Words the vowel-group rules get wrong often enough to list by hand.
const SYLLABLE_EXCEPTIONS: &[(&str, usize)] = &[
    ("being", 2),
    ("idea", 3),
    ("area", 3),
    ("science", 2),
    ("create", 2),
    ("created", 3),
    ("poem", 2),
    ("poet", 2),
    ("lion", 2),
    ("violin", 3),
    ("everyone", 3),
    ("someone", 2),
];

fn is_vowel(c: char) -> bool {
    VOWELS.contains(c)
}

fn syllables_of_part(part: &str) -> usize {
    let w: Vec<char> = part
        .chars()
        .filter(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect();
    if w.is_empty() {
        return 0;
    }
    let s: String = w.iter().collect();
    if let Some((_, n)) = SYLLABLE_EXCEPTIONS.iter().find(|(word, _)| *word == s) {
        return *n;
    }
    // Vowel letters, treating a leading `y` and the `u` of `qu` as consonants.
    let vowel_at: Vec<bool> = w
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            if !is_vowel(c) {
                return false;
            }
            if i == 0 && c == 'y' && w.get(1).is_some_and(|&n| is_vowel(n)) {
                return false;
            }
            if c == 'u' && i > 0 && w[i - 1] == 'q' {
                return false;
            }
            true
        })
        .collect();
    let mut count: isize = 0;
    let mut prev = false;
    for &v in &vowel_at {
        if v && !prev {
            count += 1;
        }
        prev = v;
    }
    let len = w.len();
    let consonant_before = |idx: usize| idx > 0 && !vowel_at[idx - 1];
    if count > 1 {
        // Silent final `e` after a consonant, except consonant + `le`.
        if s.ends_with('e') && len >= 2 && consonant_before(len - 1) {
            let le_after_consonant = s.ends_with("le") && len >= 3 && consonant_before(len - 2);
            if !le_after_consonant {
                count -= 1;
            }
        } else if s.ends_with("ed") && len >= 3 && consonant_before(len - 2) {
            // `-ed` is silent unless it follows `t` or `d`.
            if !matches!(w[len - 3], 't' | 'd') {
                count -= 1;
            }
        } else if s.ends_with("es") && len >= 3 && consonant_before(len - 2) {
            // `-es` is silent unless it follows a sibilant.
            let sibilant = matches!(w[len - 3], 's' | 'x' | 'z' | 'c' | 'g')
                || s.ends_with("ches")
                || s.ends_with("shes");
            if !sibilant {
                count -= 1;
            }
        }
    }
    // Vowel pairs that usually split into two syllables.
    let mut extra: isize = 0;
    let ious_or_eous = s.ends_with("ious") || s.ends_with("eous");
    for idx in 0..len.saturating_sub(1) {
        // The suffix rule below owns the pair inside `-ious` and `-eous`.
        if ious_or_eous && idx + 4 >= len {
            continue;
        }
        let pair: String = [w[idx], w[idx + 1]].iter().collect();
        let before = if idx > 0 { Some(w[idx - 1]) } else { None };
        let after = w.get(idx + 2).copied();
        match pair.as_str() {
            "ia" if !matches!(before, Some('c' | 't' | 's' | 'g')) => extra += 1,
            "ua" if !matches!(before, Some('q' | 'g')) => extra += 1,
            "uo" if before != Some('q') => extra += 1,
            "io" if !matches!(before, Some('t' | 'c' | 's' | 'x' | 'g' | 'l')) => extra += 1,
            "ie" if after == Some('t') && !matches!(before, Some('t' | 'c')) => extra += 1,
            _ => {}
        }
    }
    if ious_or_eous {
        let stem_end = len - 4;
        if stem_end == 0 || !matches!(w[stem_end - 1], 'c' | 't' | 'g' | 'x') {
            extra += 1;
        }
    }
    if s.ends_with("ism") || s.ends_with("isms") {
        extra += 1;
    }
    (count + extra).max(1) as usize
}

/// Estimate the number of syllables in one word.
///
/// Rules: count groups of vowel letters (`a e i o u y`, with a leading `y` and
/// the `u` in `qu` treated as consonants), then subtract one for a silent
/// final `e` (not consonant + `le`), a silent `-ed` (not after `t` or `d`) and
/// a silent `-es` (not after a sibilant), add one for vowel pairs that usually
/// split (`ia`, `ua`, `uo`, `io` and `ie` before `t`, with common exceptions
/// such as `-tion`, `-cial`, `-gua-`) and for `-ious`, `-eous` and `-ism`, and
/// finally apply a short exception table. Hyphenated words are summed over
/// their parts, and apostrophes are ignored. Any word with letters counts at
/// least one syllable, and a number counts as one.
///
/// Known failure classes: loanwords and proper nouns, acronyms and initialisms
/// (`NASA`, `HTTP`), numerals read aloud (`2024`), words with unusual vowel
/// splits (`poem`, `being` are in the exception table, others are not), and
/// `-ed` after a vowel. Expect an error of a few percent on running text,
/// which shifts Flesch scores by about a point.
pub fn syllables(word: &str) -> usize {
    let total: usize = word.split('-').map(syllables_of_part).sum();
    if total == 0 && word.chars().any(|c| c.is_ascii_digit()) {
        return 1;
    }
    total
}

// ---------------------------------------------------------------------------
// Passive voice
// ---------------------------------------------------------------------------

const BE_FORMS: &[&str] = &[
    "am", "is", "are", "was", "were", "be", "been", "being", "isn't", "aren't", "wasn't", "weren't",
];

/// Irregular past participles, vendored. `read`, `put`, `set` and similar
/// verbs that look like their base form are included on purpose.
pub static IRREGULAR_PARTICIPLES: &[&str] = &[
    "arisen",
    "awoken",
    "been",
    "beaten",
    "become",
    "begun",
    "bent",
    "bet",
    "bitten",
    "bled",
    "blown",
    "borne",
    "born",
    "bought",
    "bound",
    "bred",
    "broken",
    "brought",
    "built",
    "burnt",
    "burst",
    "cast",
    "caught",
    "chosen",
    "clung",
    "come",
    "cost",
    "crept",
    "cut",
    "dealt",
    "done",
    "drawn",
    "driven",
    "drunk",
    "dug",
    "eaten",
    "fallen",
    "fed",
    "felt",
    "fled",
    "flown",
    "flung",
    "forbidden",
    "foreseen",
    "forgiven",
    "forgotten",
    "forsaken",
    "fought",
    "found",
    "frozen",
    "given",
    "gone",
    "gotten",
    "ground",
    "grown",
    "heard",
    "held",
    "hidden",
    "hit",
    "hung",
    "hurt",
    "kept",
    "known",
    "laid",
    "lain",
    "led",
    "left",
    "lent",
    "let",
    "lit",
    "lost",
    "made",
    "meant",
    "met",
    "mistaken",
    "outgrown",
    "overcome",
    "overdone",
    "overtaken",
    "paid",
    "put",
    "quit",
    "read",
    "rebuilt",
    "redone",
    "reset",
    "retold",
    "rewritten",
    "ridden",
    "risen",
    "run",
    "rung",
    "said",
    "seen",
    "sent",
    "set",
    "sewn",
    "shaken",
    "shed",
    "shone",
    "shot",
    "shown",
    "shrunk",
    "shut",
    "slain",
    "slept",
    "slid",
    "sold",
    "sought",
    "spent",
    "split",
    "spoken",
    "spread",
    "sprung",
    "spun",
    "stolen",
    "stood",
    "struck",
    "strung",
    "stuck",
    "stung",
    "sung",
    "sunk",
    "swept",
    "sworn",
    "swum",
    "swung",
    "taken",
    "taught",
    "thought",
    "thrown",
    "told",
    "torn",
    "undergone",
    "understood",
    "undone",
    "unwritten",
    "upheld",
    "withdrawn",
    "withheld",
    "woken",
    "won",
    "worn",
    "woven",
    "wound",
    "written",
];

/// Words ending in `-ed` that are not past participles.
const ED_NOT_PARTICIPLES: &[&str] = &[
    "need", "speed", "shed", "hundred", "indeed", "sacred", "naked", "wicked", "kindred", "hatred",
    "infrared", "embed", "bleed", "breed", "proceed", "succeed", "exceed", "feed", "seed", "weed",
    "greed", "creed", "deed", "heed", "reed", "steed", "freed",
];

/// Words ending in `-en` that are not past participles.
const EN_NOT_PARTICIPLES: &[&str] = &[
    "often",
    "even",
    "open",
    "then",
    "when",
    "children",
    "women",
    "garden",
    "token",
    "happen",
    "listen",
    "kitchen",
    "dozen",
    "eleven",
    "seven",
    "golden",
    "wooden",
    "woolen",
    "citizen",
    "siren",
    "sudden",
    "linen",
    "oxygen",
    "specimen",
    "omen",
    "regimen",
    "ten",
    "men",
    "hen",
    "pen",
    "den",
    "ben",
    "tyren",
    "again",
    "hyphen",
    "women's",
    "eighteen",
    "nineteen",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "maiden",
    "burden",
    "pattern",
    "lessen",
    "widen",
    "harden",
    "soften",
    "weaken",
    "lengthen",
    "strengthen",
    "brighten",
    "shorten",
];

fn is_participle(token: &str) -> bool {
    if IRREGULAR_PARTICIPLES.contains(&token) {
        return true;
    }
    let len = token.chars().count();
    if len >= 4 && token.ends_with("ed") && !token.ends_with("eed") {
        return !ED_NOT_PARTICIPLES.contains(&token);
    }
    if len >= 5 && token.ends_with("en") && !token.ends_with("een") {
        return !EN_NOT_PARTICIPLES.contains(&token);
    }
    false
}

fn is_passive_sentence(sentence: &str) -> bool {
    let tokens: Vec<String> = words(sentence)
        .into_iter()
        .map(|w| w.replace('\u{2019}', "'").to_lowercase())
        .collect();
    for (i, token) in tokens.iter().enumerate() {
        if BE_FORMS.contains(&token.as_str()) {
            for k in 1..=2 {
                if tokens.get(i + k).is_some_and(|t| is_participle(t)) {
                    return true;
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Readability
// ---------------------------------------------------------------------------

/// Readability metrics for a passage. All values are zero for empty text.
#[derive(Debug, Clone, PartialEq)]
pub struct Readability {
    pub words: usize,
    pub sentences: usize,
    pub syllables: usize,
    /// Flesch Reading Ease: `206.835 - 1.015 (words/sentences) - 84.6 (syllables/words)`.
    pub flesch_reading_ease: f64,
    /// Flesch-Kincaid grade: `0.39 (words/sentences) + 11.8 (syllables/words) - 15.59`.
    pub flesch_kincaid_grade: f64,
    pub mean_sentence_length: f64,
    /// Sentences with more than 20 words.
    pub long_sentence_count: usize,
    /// Sentences with at least one passive construction.
    pub passive_voice_count: usize,
    /// `passive_voice_count / sentences`.
    pub passive_voice_share: f64,
}

/// Compute readability metrics.
///
/// Sentences come from [`sentences`], words from [`words`] and syllables from
/// [`syllables`]. A passive construction is a form of "to be" (`am is are was
/// were be been being`, plus `isn't aren't wasn't weren't`) followed within
/// two tokens by a past participle: a word in the vendored irregular list
/// ([`IRREGULAR_PARTICIPLES`]), a regular `-ed` word, or a regular `-en`
/// word, minus short stop lists. This is the identification rule on
/// plainlanguage.gov, and it has false positives: adjectives that look like
/// participles (`is tired`, `is concerned`) and `-en` words that are not
/// participles and are missing from the stop list. It also misses passives
/// with more than one word between the verb forms.
pub fn readability(text: &str) -> Readability {
    let sents = sentences(text);
    let mut word_count = 0usize;
    let mut syllable_count = 0usize;
    let mut long = 0usize;
    let mut passive = 0usize;
    for sentence in &sents {
        let ws = words(sentence);
        word_count += ws.len();
        syllable_count += ws.iter().map(|w| syllables(w)).sum::<usize>();
        if ws.len() > 20 {
            long += 1;
        }
        if is_passive_sentence(sentence) {
            passive += 1;
        }
    }
    let sentence_count = sents.len();
    if word_count == 0 || sentence_count == 0 {
        return Readability {
            words: word_count,
            sentences: sentence_count,
            syllables: syllable_count,
            flesch_reading_ease: 0.0,
            flesch_kincaid_grade: 0.0,
            mean_sentence_length: 0.0,
            long_sentence_count: long,
            passive_voice_count: passive,
            passive_voice_share: 0.0,
        };
    }
    let w = word_count as f64;
    let s = sentence_count as f64;
    let y = syllable_count as f64;
    Readability {
        words: word_count,
        sentences: sentence_count,
        syllables: syllable_count,
        flesch_reading_ease: 206.835 - 1.015 * (w / s) - 84.6 * (y / w),
        flesch_kincaid_grade: 0.39 * (w / s) + 11.8 * (y / w) - 15.59,
        mean_sentence_length: w / s,
        long_sentence_count: long,
        passive_voice_count: passive,
        passive_voice_share: passive as f64 / s,
    }
}

// ---------------------------------------------------------------------------
// Link text
// ---------------------------------------------------------------------------

/// Link text quality, judged from the text alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTextQuality {
    Ok,
    /// Matches a generic phrase. Carries the matched phrase.
    Generic(String),
    /// The text is a URL or a bare domain.
    BareUrl,
    /// No letters or digits.
    Empty,
}

const GENERIC_LINK_PHRASES: &[&str] = &[
    "click here",
    "here",
    "read more",
    "more",
    "learn more",
    "link",
    "this page",
    "this link",
    "continue",
    "go",
];

const URL_TLDS: &[&str] = &[
    "com", "org", "net", "gov", "edu", "mil", "us", "io", "co", "info", "biz", "app", "dev", "uk",
    "ca", "int",
];

fn looks_like_url(lower: &str) -> bool {
    if lower.contains(char::is_whitespace) {
        return false;
    }
    if lower.contains("://") || lower.starts_with("www.") {
        return true;
    }
    let host = lower.split(['/', '?', '#']).next().unwrap_or("");
    if !host.contains('.') {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let tld_ok = labels.last().is_some_and(|t| URL_TLDS.contains(t));
    tld_ok
        && labels
            .iter()
            .all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
}

/// Judge link text on its own, without surrounding context.
///
/// The text is trimmed, whitespace is collapsed and it is lowercased. It is
/// `Empty` when it has no letters or digits, `BareUrl` when it is a URL or a
/// domain ending in a common top-level domain, `Generic` when it equals one of
/// `click here`, `here`, `read more`, `more`, `learn more`, `link`, `this
/// page`, `this link`, `continue` or `go` (leading and trailing symbols such
/// as `»` or `…` are ignored), and `Ok` otherwise. Only an exact match is
/// generic, so `Read more about parking permits` is `Ok`. WCAG 2.4.4 allows
/// generic text when the surrounding context names the purpose, so `Generic`
/// is a lead, not a failure.
pub fn link_text_quality(text: &str) -> LinkTextQuality {
    let collapsed = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if !collapsed.chars().any(char::is_alphanumeric) {
        return LinkTextQuality::Empty;
    }
    if looks_like_url(&collapsed) {
        return LinkTextQuality::BareUrl;
    }
    let core = collapsed.trim_matches(|c: char| !c.is_alphanumeric());
    match GENERIC_LINK_PHRASES.iter().find(|p| **p == core) {
        Some(phrase) => LinkTextQuality::Generic((*phrase).to_string()),
        None => LinkTextQuality::Ok,
    }
}

// ---------------------------------------------------------------------------
// Acronyms
// ---------------------------------------------------------------------------

const ACRONYM_ALLOWLIST: &[&str] = &[
    "US", "USA", "FAQ", "PDF", "HTML", "URL", "ID", "ZIP", "AM", "PM", "UTC",
];

/// Acronyms used without a nearby expansion.
///
/// An acronym is a word of 2 to 6 uppercase ASCII letters. It counts as
/// defined from the first place where it is either written in parentheses
/// right after its expansion (`Federal Communications Commission (FCC)`) or
/// followed by a parenthetical expansion (`FCC (Federal Communications
/// Commission)`). Uses before that point are undefined, so use-before-define
/// is reported. Allowlisted acronyms (US, USA, FAQ, PDF, HTML, URL, ID, ZIP,
/// AM, PM, UTC) are skipped, and so are all-caps lines such as headings.
/// Each undefined acronym is reported once, in order of first use.
///
/// Known failures: the expansion is not checked against the letters, roman
/// numerals (`II`, `IV`) and all-caps words in running text are reported, and
/// an acronym with a trailing `s` (`FCCs`) is not recognized.
pub fn acronyms_undefined(text: &str) -> Vec<String> {
    let allow: HashSet<&str> = ACRONYM_ALLOWLIST.iter().copied().collect();
    let mut defined: HashSet<String> = HashSet::new();
    let mut reported: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for line in text.split_inclusive('\n') {
        let line_has_lowercase = line.chars().any(char::is_lowercase);
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let mut i = 0;
        while i < chars.len() {
            if !chars[i].1.is_alphanumeric() {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < chars.len() && chars[j].1.is_alphanumeric() {
                j += 1;
            }
            let start = chars[i].0;
            let end = if j < chars.len() {
                chars[j].0
            } else {
                line.len()
            };
            let token = &line[start..end];
            i = j;
            if !(2..=6).contains(&token.len()) || !token.chars().all(|c| c.is_ascii_uppercase()) {
                continue;
            }
            let before = &line[..start];
            let after = &line[end..];
            let enclosed = before.ends_with('(') && after.starts_with(')');
            let followed = {
                let rest = after.trim_start_matches([' ', '\t']);
                rest.starts_with('(')
                    && rest[1..]
                        .find(')')
                        .is_some_and(|close| !rest[1..1 + close].trim().is_empty())
            };
            if enclosed || followed {
                defined.insert(token.to_string());
                continue;
            }
            if defined.contains(token) || allow.contains(token) || !line_has_lowercase {
                continue;
            }
            if reported.insert(token.to_string()) {
                out.push(token.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/content")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    #[test]
    fn words_handle_contractions_hyphens_and_numbers() {
        assert_eq!(
            words("Don't use well-known 3.5 or 1,200 items, OK?"),
            vec![
                "Don't",
                "use",
                "well-known",
                "3.5",
                "or",
                "1,200",
                "items",
                "OK"
            ]
        );
        assert_eq!(words("  ...  "), Vec::<&str>::new());
        assert_eq!(words("it\u{2019}s fine"), vec!["it\u{2019}s", "fine"]);
    }

    #[test]
    fn sentences_split_on_terminators_followed_by_capitals() {
        assert_eq!(
            sentences("Apply online. It takes five minutes! Do you qualify? Check below."),
            vec![
                "Apply online.",
                "It takes five minutes!",
                "Do you qualify?",
                "Check below."
            ]
        );
    }

    #[test]
    fn sentences_respect_abbreviations_numbers_and_initials() {
        assert_eq!(
            sentences("Dr. Smith met Mr. Jones in the U.S. Department of State."),
            vec!["Dr. Smith met Mr. Jones in the U.S. Department of State."]
        );
        assert_eq!(
            sentences("Use e.g. a form, i.e. paper, vs. online. Then wait."),
            vec!["Use e.g. a form, i.e. paper, vs. online.", "Then wait."]
        );
        assert_eq!(
            sentences("The rate is 3.5 percent. See No. 5 for details."),
            vec!["The rate is 3.5 percent.", "See No. 5 for details."]
        );
        assert_eq!(
            sentences("Contact J. Smith today."),
            vec!["Contact J. Smith today."]
        );
        assert_eq!(
            sentences("Steps: 1. Apply 2. Wait."),
            vec!["Steps: 1. Apply 2. Wait."]
        );
    }

    #[test]
    fn sentences_handle_quotes_ellipses_and_blank_lines() {
        assert_eq!(
            sentences("He said \"Stop.\" Then he left."),
            vec!["He said \"Stop.\"", "Then he left."]
        );
        assert_eq!(sentences("Wait... Then go."), vec!["Wait...", "Then go."]);
        assert_eq!(
            sentences("Title without period\n\nBody starts here. More text"),
            vec!["Title without period", "Body starts here.", "More text"]
        );
        assert_eq!(sentences(""), Vec::<&str>::new());
        assert_eq!(
            sentences("no capitals. here at all"),
            vec!["no capitals. here at all"]
        );
    }

    #[test]
    fn syllable_estimates_for_common_words() {
        let table: &[(&str, usize)] = &[
            ("the", 1),
            ("a", 1),
            ("cat", 1),
            ("make", 1),
            ("table", 2),
            ("people", 2),
            ("water", 2),
            ("computer", 3),
            ("beautiful", 3),
            ("government", 3),
            ("agency", 3),
            ("information", 4),
            ("accessible", 4),
            ("application", 4),
            ("agreed", 2),
            ("wanted", 2),
            ("needed", 2),
            ("walked", 1),
            ("places", 2),
            ("makes", 1),
            ("boxes", 2),
            ("judge", 1),
            ("complete", 2),
            ("above", 2),
            ("quiet", 2),
            ("actual", 3),
            ("media", 3),
            ("social", 2),
            ("serious", 3),
            ("various", 3),
            ("delicious", 3),
            ("capitalism", 5),
            ("education", 4),
            ("radio", 3),
            ("year", 1),
            ("you", 1),
            ("every", 3),
            ("different", 3),
            ("business", 3),
            ("science", 2),
            ("idea", 3),
            ("being", 2),
            ("don't", 1),
            ("well-known", 2),
            ("2024", 1),
            ("", 0),
            ("--", 0),
        ];
        for (word, expected) in table {
            assert_eq!(syllables(word), *expected, "word {word:?}");
        }
    }

    #[test]
    fn irregular_participle_list_is_large_and_unique() {
        assert!(
            IRREGULAR_PARTICIPLES.len() >= 120,
            "{}",
            IRREGULAR_PARTICIPLES.len()
        );
        let unique: HashSet<&&str> = IRREGULAR_PARTICIPLES.iter().collect();
        assert_eq!(unique.len(), IRREGULAR_PARTICIPLES.len());
        assert!(IRREGULAR_PARTICIPLES.iter().all(|w| *w == w.to_lowercase()));
    }

    #[test]
    fn passive_detection() {
        let passive = [
            "The form was completed by the applicant.",
            "Fees are not refunded.",
            "The notice has been sent.",
            "The decision is often made late.",
            "Records were kept for ten years.",
            "It is being reviewed.",
            "The letter wasn't written yet.",
        ];
        for s in passive {
            assert!(is_passive_sentence(s), "{s}");
        }
        let active = [
            "We reviewed the form.",
            "The office is open on Monday.",
            "It is often late.",
            "She was happy.",
            "You will need a photo ID.",
            "Even children can apply.",
        ];
        for s in active {
            assert!(!is_passive_sentence(s), "{s}");
        }
    }

    #[test]
    fn readability_of_empty_text_is_zero() {
        let r = readability("");
        assert_eq!((r.words, r.sentences, r.syllables), (0, 0, 0));
        assert_eq!(r.flesch_reading_ease, 0.0);
        assert_eq!(r.passive_voice_share, 0.0);
    }

    #[test]
    fn readability_formulas_on_a_known_passage() {
        // 6 words, 1 sentence, 6 syllables: FRE = 206.835 - 6.09 - 84.6 = 116.145
        let r = readability("The cat sat on the mat.");
        assert_eq!((r.words, r.sentences, r.syllables), (6, 1, 6));
        assert!((r.flesch_reading_ease - 116.145).abs() < 1e-9);
        assert!((r.flesch_kincaid_grade - (0.39 * 6.0 + 11.8 - 15.59)).abs() < 1e-9);
        assert_eq!(r.mean_sentence_length, 6.0);
        assert_eq!(r.long_sentence_count, 0);
    }

    #[test]
    fn long_sentences_have_more_than_twenty_words() {
        let twenty = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty.";
        assert_eq!(readability(twenty).long_sentence_count, 0);
        let twenty_one = format!("{} twentyone.", twenty.trim_end_matches('.'));
        assert_eq!(readability(&twenty_one).long_sentence_count, 1);
    }

    #[test]
    fn link_text_classes() {
        use LinkTextQuality::*;
        assert_eq!(
            link_text_quality("Click here"),
            Generic("click here".into())
        );
        assert_eq!(
            link_text_quality("  READ   MORE \u{00BB}"),
            Generic("read more".into())
        );
        assert_eq!(
            link_text_quality("Learn more..."),
            Generic("learn more".into())
        );
        assert_eq!(link_text_quality("here"), Generic("here".into()));
        assert_eq!(link_text_quality("Read more about parking permits"), Ok);
        assert_eq!(link_text_quality("https://example.gov/apply"), BareUrl);
        assert_eq!(link_text_quality("www.example.gov"), BareUrl);
        assert_eq!(link_text_quality("example.org/forms"), BareUrl);
        assert_eq!(link_text_quality("README.md"), Ok);
        assert_eq!(link_text_quality(""), Empty);
        assert_eq!(link_text_quality(" \u{00BB} "), Empty);
        assert_eq!(link_text_quality("Apply for a permit"), Ok);
    }

    #[test]
    fn acronym_definitions_and_allowlist() {
        let text = "The Federal Communications Commission (FCC) sets rules. FCC staff review them.\nThe DOJ (Department of Justice) agrees. The IRS agrees. Use a PDF or the URL.";
        assert_eq!(acronyms_undefined(text), vec!["IRS"]);
        assert_eq!(
            acronyms_undefined("FCC rules. The Federal Communications Commission (FCC) agrees."),
            vec!["FCC"]
        );
        assert_eq!(
            acronyms_undefined("IMPORTANT NOTICE\nSee the FAQ."),
            Vec::<String>::new()
        );
        assert_eq!(
            acronyms_undefined("H2O and FCCs and A are not acronyms."),
            Vec::<String>::new()
        );
        assert_eq!(
            acronyms_undefined("Ask NASA. Ask NASA again. Ask ESA."),
            vec!["NASA", "ESA"]
        );
    }

    #[test]
    fn fixtures_match_expected_metrics() {
        let expected: serde_json::Value = serde_json::from_str(&fixture("expected.json")).unwrap();
        let cases = expected["cases"].as_object().expect("cases object");
        assert!(cases.len() >= 4);
        for (file, want) in cases {
            let text = fixture(file);
            let r = readability(&text);
            let exact = |key: &str, got: usize| {
                if let Some(v) = want.get(key) {
                    assert_eq!(v.as_u64().unwrap() as usize, got, "{file}: {key}");
                }
            };
            exact("words", r.words);
            exact("sentences", r.sentences);
            exact("long_sentence_count", r.long_sentence_count);
            exact("passive_voice_count", r.passive_voice_count);
            let ranged = |key: &str, got: f64| {
                if let Some(range) = want.get(key) {
                    let lo = range[0].as_f64().unwrap();
                    let hi = range[1].as_f64().unwrap();
                    assert!(
                        got >= lo && got <= hi,
                        "{file}: {key} = {got}, expected {lo}..={hi}"
                    );
                }
            };
            ranged("flesch_kincaid_grade", r.flesch_kincaid_grade);
            ranged("flesch_reading_ease", r.flesch_reading_ease);
            ranged("passive_voice_share", r.passive_voice_share);
            ranged("mean_sentence_length", r.mean_sentence_length);
            if let Some(list) = want.get("acronyms_undefined") {
                let want_list: Vec<String> = list
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect();
                assert_eq!(acronyms_undefined(&text), want_list, "{file}: acronyms");
            }
        }
    }
}
