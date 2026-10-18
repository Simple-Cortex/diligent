//! Adversarial tests for diligent-assess: boundary values, edge cases, and
//! inputs designed to expose off-by-one errors, panics, or incorrect behavior.

use diligent_assess::*;

// ---------------------------------------------------------------------------
// Contrast boundaries
// ---------------------------------------------------------------------------

const WHITE: Srgb = Srgb {
    r: 255,
    g: 255,
    b: 255,
};
const GREY_767676: Srgb = Srgb {
    r: 118,
    g: 118,
    b: 118,
};

#[test]
fn ratio_exactly_at_4_5_passes_aa_normal() {
    let verdict = text_contrast_verdict(4.5, TextSize::Normal, ContrastLevel::Aa);
    assert_eq!(
        verdict,
        ContrastVerdict::Pass,
        "ratio exactly at 4.5 must pass AA normal"
    );
}

#[test]
fn ratio_exactly_at_3_0_passes_aa_large() {
    let verdict = text_contrast_verdict(3.0, TextSize::Large, ContrastLevel::Aa);
    assert_eq!(
        verdict,
        ContrastVerdict::Pass,
        "ratio exactly at 3.0 must pass AA large"
    );
}

#[test]
fn ratio_1e9_below_4_5_fails_aa_normal() {
    let verdict = text_contrast_verdict(4.5 - 1e-9, TextSize::Normal, ContrastLevel::Aa);
    assert!(
        matches!(verdict, ContrastVerdict::Fail { .. }),
        "ratio 1e-9 below 4.5 must fail AA normal"
    );
}

#[test]
fn ratio_1e9_below_3_0_fails_aa_large() {
    let verdict = text_contrast_verdict(3.0 - 1e-9, TextSize::Large, ContrastLevel::Aa);
    assert!(
        matches!(verdict, ContrastVerdict::Fail { .. }),
        "ratio 1e-9 below 3.0 must fail AA large"
    );
}

#[test]
fn grey_767676_on_white_passes_aa_normal() {
    let ratio = contrast_ratio(GREY_767676, WHITE);
    assert!(
        ratio >= 4.5,
        "#767676 on #ffffff ratio {ratio} must be >= 4.5"
    );
    let verdict = text_contrast_verdict(ratio, TextSize::Normal, ContrastLevel::Aa);
    assert_eq!(
        verdict,
        ContrastVerdict::Pass,
        "#767676 on #ffffff must pass AA normal"
    );
}

#[test]
fn display_ratio_does_not_change_verdict() {
    let ratio_raw = 4.4999999;
    let displayed = display_ratio(ratio_raw);
    assert_eq!(displayed, "4.50:1", "4.4999999 displays as 4.50:1");
    let verdict_from_raw = text_contrast_verdict(ratio_raw, TextSize::Normal, ContrastLevel::Aa);
    assert!(
        matches!(verdict_from_raw, ContrastVerdict::Fail { .. }),
        "raw ratio 4.4999999 must fail even though display rounds to 4.50:1"
    );
}

// ---------------------------------------------------------------------------
// Text classification boundaries
// ---------------------------------------------------------------------------

#[test]
fn classify_text_23_999_not_bold_is_normal() {
    assert_eq!(
        classify_text(23.999, false),
        TextSize::Normal,
        "23.999px not bold must be Normal"
    );
}

#[test]
fn classify_text_24_0_not_bold_is_large() {
    assert_eq!(
        classify_text(24.0, false),
        TextSize::Large,
        "24.0px not bold must be Large"
    );
}

#[test]
fn classify_text_18_66_bold_is_normal() {
    assert_eq!(
        classify_text(18.66, true),
        TextSize::Normal,
        "18.66px bold must be Normal (threshold is 18.666...)"
    );
}

#[test]
fn classify_text_18_67_bold_is_large() {
    assert_eq!(
        classify_text(18.67, true),
        TextSize::Large,
        "18.67px bold must be Large"
    );
}

// ---------------------------------------------------------------------------
// CSS color parsing
// ---------------------------------------------------------------------------

#[test]
fn parse_css_color_rejects_garbage() {
    assert!(parse_css_color("not a color").is_none());
    assert!(parse_css_color("rgb(a, b, c)").is_none());
    assert!(parse_css_color("#gg").is_none());
    assert!(parse_css_color("").is_none());
    assert!(parse_css_color("hsl(0, 0%, 0%)").is_none());
}

#[test]
fn parse_css_color_handles_uppercase_hex() {
    let result = parse_css_color("#ABCDEF");
    assert!(
        matches!(result, Some(Color::Opaque(c)) if c.r == 0xAB && c.g == 0xCD && c.b == 0xEF),
        "uppercase hex #ABCDEF must parse"
    );
}

#[test]
fn parse_css_color_handles_rgb_with_spaces() {
    let result = parse_css_color("rgb(10 20 30)");
    assert!(
        matches!(result, Some(Color::Opaque(c)) if c.r == 10 && c.g == 20 && c.b == 30),
        "rgb(10 20 30) with spaces must parse"
    );
}

// ---------------------------------------------------------------------------
// Composite over
// ---------------------------------------------------------------------------

#[test]
fn composite_over_with_alpha_zero_returns_background() {
    let fg = Rgba {
        r: 255,
        g: 0,
        b: 0,
        a: 0.0,
    };
    let bg = Srgb {
        r: 10,
        g: 20,
        b: 30,
    };
    let result = composite_over(fg, bg);
    assert_eq!(result, bg, "alpha 0 must return the background unchanged");
}

// ---------------------------------------------------------------------------
// Readability
// ---------------------------------------------------------------------------

#[test]
fn readability_of_empty_string_does_not_panic() {
    let r = readability("");
    assert_eq!(r.sentences, 0, "empty string has zero sentences");
    assert_eq!(r.words, 0);
    assert_eq!(r.flesch_reading_ease, 0.0);
}

#[test]
fn readability_of_single_word_does_not_panic() {
    let r = readability("Hello");
    assert_eq!(r.words, 1);
    assert_eq!(r.sentences, 1, "single word is one sentence");
}

// ---------------------------------------------------------------------------
// Sentences
// ---------------------------------------------------------------------------

#[test]
fn sentences_handles_e_g_without_splitting() {
    let result = sentences("Use e.g. a form, i.e. paper, vs. online. Then wait.");
    assert_eq!(
        result.len(),
        2,
        "e.g., i.e., vs. must not split sentences; expected 2 got {}: {:?}",
        result.len(),
        result
    );
}

#[test]
fn sentences_handles_u_s_without_splitting() {
    let result = sentences("Dr. Smith met Mr. Jones in the U.S. Department of State.");
    assert_eq!(
        result.len(),
        1,
        "U.S. must not split; expected 1 got {}: {:?}",
        result.len(),
        result
    );
}

// ---------------------------------------------------------------------------
// Passive voice
// ---------------------------------------------------------------------------

#[test]
fn passive_detects_was_approved_by() {
    let r = readability("The form was approved by the manager.");
    assert!(
        r.passive_voice_count >= 1,
        "'was approved by' must be detected as passive; got passive_voice_count={}",
        r.passive_voice_count
    );
}

#[test]
fn passive_does_not_flag_is_happy() {
    let r = readability("She is happy.");
    assert_eq!(
        r.passive_voice_count, 0,
        "'is happy' must not be flagged as passive; got passive_voice_count={}",
        r.passive_voice_count
    );
}

// ---------------------------------------------------------------------------
// Link text quality
// ---------------------------------------------------------------------------

#[test]
fn link_text_quality_click_here_is_generic() {
    let result = link_text_quality("Click here");
    assert!(
        matches!(result, LinkTextQuality::Generic(_)),
        "'Click here' must be Generic, got {:?}",
        result
    );
}

#[test]
fn link_text_quality_bare_url_is_bare_url() {
    let result = link_text_quality("https://example.gov/apply");
    assert_eq!(
        result,
        LinkTextQuality::BareUrl,
        "bare URL must be BareUrl, got {:?}",
        result
    );
}

// ---------------------------------------------------------------------------
// Acronyms undefined
// ---------------------------------------------------------------------------

#[test]
fn acronyms_undefined_does_not_flag_us_or_faq() {
    let result = acronyms_undefined("The US FAQ is available.");
    assert!(
        !result.contains(&"US".to_string()),
        "US must be allowlisted"
    );
    assert!(
        !result.contains(&"FAQ".to_string()),
        "FAQ must be allowlisted"
    );
}

#[test]
fn acronyms_undefined_flags_ict_when_undefined() {
    let result = acronyms_undefined("The ICT requirements are listed.");
    assert!(
        result.contains(&"ICT".to_string()),
        "ICT must be flagged when undefined; got {:?}",
        result
    );
}

// ---------------------------------------------------------------------------
// Normalize label
// ---------------------------------------------------------------------------

#[test]
fn normalize_label_strips_trailing_colon() {
    assert_eq!(
        normalize_label("Search:"),
        "search",
        "trailing colon must be stripped"
    );
}

#[test]
fn normalize_label_collapses_whitespace() {
    assert_eq!(
        normalize_label("  Search   the   site  "),
        "search the site",
        "whitespace must be collapsed and trimmed"
    );
}

// ---------------------------------------------------------------------------
// Label in name
// ---------------------------------------------------------------------------

#[test]
fn label_in_name_search_prefix_of_search_the_site() {
    let result = label_in_name("Search", "Search the site");
    assert_eq!(
        result,
        LabelMatch::Prefix,
        "'Search' must be Prefix of 'Search the site', got {:?}",
        result
    );
}

#[test]
fn label_in_name_emoji_only_visible_is_ambiguous() {
    let result = label_in_name("\u{1F50D}", "Search");
    assert!(
        matches!(result, LabelMatch::Ambiguous(_)),
        "emoji-only visible text must be Ambiguous, got {:?}",
        result
    );
}
