//! `SRC-HTML-LANG` (ACT b5c3f8 and bf051a, informs A11Y-3.1.1).
//!
//! Decides: the `html` element has a non-empty `lang` attribute whose value
//! has BCP 47 shape (`^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})*$`). A two-letter
//! primary subtag must be an ISO 639-1 code. A three-letter primary subtag is
//! `needs_review`, because validating it needs the IANA language subtag
//! registry, which this crate does not vendor.
//!
//! `xml:lang` alone does not satisfy the check, as in ACT b5c3f8.
//!
//! Does not decide: whether the language matches the content, region, script
//! or variant subtag validity, or `lang` on other elements (3.1.2).

use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-HTML-LANG",
    act: &["b5c3f8", "bf051a"],
    informs: &["A11Y-3.1.1"],
    summary: "The html element has a well-formed lang attribute",
    absent_reason: "The document has no html element.",
};

const EXPECTED: &str = "html[lang] is present, non-empty and a well-formed BCP 47 tag";

/// ISO 639-1 codes plus the deprecated codes still present in the IANA registry.
const ISO_639_1: &[&str] = &[
    "aa", "ab", "ae", "af", "ak", "am", "an", "ar", "as", "av", "ay", "az", "ba", "be", "bg", "bh",
    "bi", "bm", "bn", "bo", "br", "bs", "ca", "ce", "ch", "co", "cr", "cs", "cu", "cv", "cy", "da",
    "de", "dv", "dz", "ee", "el", "en", "eo", "es", "et", "eu", "fa", "ff", "fi", "fj", "fo", "fr",
    "fy", "ga", "gd", "gl", "gn", "gu", "gv", "ha", "he", "hi", "ho", "hr", "ht", "hu", "hy", "hz",
    "ia", "id", "ie", "ig", "ii", "ik", "io", "is", "it", "iu", "ja", "jv", "ka", "kg", "ki", "kj",
    "kk", "kl", "km", "kn", "ko", "kr", "ks", "ku", "kv", "kw", "ky", "la", "lb", "lg", "li", "ln",
    "lo", "lt", "lu", "lv", "mg", "mh", "mi", "mk", "ml", "mn", "mr", "ms", "mt", "my", "na", "nb",
    "nd", "ne", "ng", "nl", "nn", "no", "nr", "nv", "ny", "oc", "oj", "om", "or", "os", "pa", "pi",
    "pl", "ps", "pt", "qu", "rm", "rn", "ro", "ru", "rw", "sa", "sc", "sd", "se", "sg", "si", "sk",
    "sl", "sm", "sn", "so", "sq", "sr", "ss", "st", "su", "sv", "sw", "ta", "te", "tg", "th", "ti",
    "tk", "tl", "tn", "to", "tr", "ts", "tt", "tw", "ty", "ug", "uk", "ur", "uz", "ve", "vi", "vo",
    "wa", "wo", "xh", "yi", "yo", "za", "zh", "zu", // Deprecated but still registered.
    "in", "iw", "ji", "jw", "mo", "sh",
];

/// Verdict on a `lang` value.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LangVerdict {
    /// Well formed and the primary subtag is known.
    Valid,
    /// Not a BCP 47 shaped tag, or an unknown two-letter primary subtag.
    Invalid(&'static str),
    /// Shape is valid but the primary subtag needs the IANA registry.
    Unverified,
}

pub(crate) fn judge_lang(value: &str) -> LangVerdict {
    let v = value.trim();
    if v.is_empty() {
        return LangVerdict::Invalid("empty");
    }
    let mut parts = v.split('-');
    let primary = parts.next().unwrap_or("");
    if !(2..=3).contains(&primary.len()) || !primary.chars().all(|c| c.is_ascii_alphabetic()) {
        return LangVerdict::Invalid("primary_subtag_shape");
    }
    for sub in parts {
        if !(2..=8).contains(&sub.len()) || !sub.chars().all(|c| c.is_ascii_alphanumeric()) {
            return LangVerdict::Invalid("subtag_shape");
        }
    }
    if primary.len() == 2 {
        if ISO_639_1.contains(&primary.to_ascii_lowercase().as_str()) {
            LangVerdict::Valid
        } else {
            LangVerdict::Invalid("unknown_two_letter_primary_subtag")
        }
    } else {
        LangVerdict::Unverified
    }
}

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let root = em.ctx.root();
    if !(dom::is_html(root) && dom::tag(root) == "html") {
        return;
    }
    let lang = dom::attr(root, "lang");
    let observed = json!({
        "lang": lang,
        "xml_lang": dom::attr(root, "xml:lang"),
    });
    let outcome = match lang {
        None => Outcome::new(
            State::Fail,
            EXPECTED,
            json!({ "lang": null, "xml_lang": dom::attr(root, "xml:lang"), "reason": "missing" }),
        ),
        Some(v) => match judge_lang(v) {
            LangVerdict::Valid => Outcome::new(State::Pass, EXPECTED, observed),
            LangVerdict::Invalid(reason) => Outcome::new(
                State::Fail,
                EXPECTED,
                json!({ "lang": lang, "reason": reason }),
            ),
            LangVerdict::Unverified => Outcome::new(State::NeedsReview, EXPECTED, observed).limit(
                "Three-letter primary language subtag is not checked against the IANA registry.",
            ),
        },
    };
    em.emit_root(&INFO, outcome);
}

#[cfg(test)]
mod tests {
    use super::{judge_lang, LangVerdict};

    #[test]
    fn judges_act_examples() {
        assert_eq!(judge_lang("en"), LangVerdict::Valid);
        assert_eq!(judge_lang("FR"), LangVerdict::Valid);
        assert_eq!(judge_lang("en-US-GB"), LangVerdict::Valid);
        assert!(matches!(judge_lang("em-US"), LangVerdict::Invalid(_)));
        assert!(matches!(judge_lang("#1"), LangVerdict::Invalid(_)));
        assert!(matches!(judge_lang("i-lux"), LangVerdict::Invalid(_)));
        assert!(matches!(judge_lang(" "), LangVerdict::Invalid(_)));
        assert_eq!(judge_lang("eng"), LangVerdict::Unverified);
    }
}
