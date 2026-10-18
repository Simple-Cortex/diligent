//! WCAG contrast measurement (TC-01 steps 3 and 4).
//!
//! Measurements return numbers. Mapping a measurement to a result state is a
//! separate function ([`text_contrast_verdict`]) so a caller can apply
//! exceptions (logos, disabled controls, incidental text) before deciding.

/// An opaque sRGB color, 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Srgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

fn linearize(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG 2.x relative luminance: `L = 0.2126 R + 0.7152 G + 0.0722 B` over
/// linearized channels (`c / 12.92` when `c <= 0.04045`, else
/// `((c + 0.055) / 1.055) ^ 2.4`).
pub fn relative_luminance(c: Srgb) -> f64 {
    0.2126 * linearize(c.r) + 0.7152 * linearize(c.g) + 0.0722 * linearize(c.b)
}

/// WCAG contrast ratio `(lighter + 0.05) / (darker + 0.05)`, unrounded.
/// The result is in `1.0..=21.0` and does not depend on argument order.
pub fn contrast_ratio(a: Srgb, b: Srgb) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (lighter, darker) = if la >= lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Round a ratio to two decimals for display. Never use the rounded value to
/// decide a verdict: 4.4999 displays as 4.50 and still fails 4.5:1.
pub fn display_ratio(ratio: f64) -> String {
    format!("{ratio:.2}:1")
}

/// WCAG text size class for contrast thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSize {
    Normal,
    Large,
}

/// CSS pixels per typographic point: 96 px per inch over 72 pt per inch.
///
/// Exactly 4/3, not the rounded 1.333. Using the exact value puts the large
/// text thresholds at 24 px (18 pt) and 56/3 = 18.666... px (14 pt).
pub const PX_PER_PT: f64 = 96.0 / 72.0;

/// WCAG "large scale" text: at least 18 pt, or at least 14 pt and bold.
///
/// The comparison is on the unrounded pixel size. 14 pt is 18.666... px, so
/// 18.66 px bold is still normal text and 18.67 px bold is large. The widely
/// quoted "18.66px" is a truncation of 18.666... and must not be used as the
/// threshold.
pub fn classify_text(font_px: f64, bold: bool) -> TextSize {
    let large_px = 18.0 * PX_PER_PT;
    let large_bold_px = 14.0 * PX_PER_PT;
    if font_px >= large_px || (bold && font_px >= large_bold_px) {
        TextSize::Large
    } else {
        TextSize::Normal
    }
}

/// Conformance level for text contrast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContrastLevel {
    /// WCAG 1.4.3 (Contrast Minimum): 4.5:1, or 3:1 for large text.
    Aa,
    /// WCAG 1.4.6 (Contrast Enhanced): 7:1, or 4.5:1 for large text.
    Aaa,
}

/// Outcome of comparing a measured ratio with a threshold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContrastVerdict {
    Pass,
    Fail { required: f64, actual: f64 },
}

/// The ratio the criterion requires for this text size and level.
pub fn required_ratio(size: TextSize, level: ContrastLevel) -> f64 {
    match (level, size) {
        (ContrastLevel::Aa, TextSize::Normal) => 4.5,
        (ContrastLevel::Aa, TextSize::Large) => 3.0,
        (ContrastLevel::Aaa, TextSize::Normal) => 7.0,
        (ContrastLevel::Aaa, TextSize::Large) => 4.5,
    }
}

/// Map a measured, unrounded ratio to pass or fail. A ratio exactly equal to
/// the threshold passes.
pub fn text_contrast_verdict(ratio: f64, size: TextSize, level: ContrastLevel) -> ContrastVerdict {
    let required = required_ratio(size, level);
    if ratio >= required {
        ContrastVerdict::Pass
    } else {
        ContrastVerdict::Fail {
            required,
            actual: ratio,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Srgb = Srgb {
        r: 255,
        g: 255,
        b: 255,
    };
    const BLACK: Srgb = Srgb { r: 0, g: 0, b: 0 };

    #[test]
    fn black_on_white_is_21() {
        assert!((contrast_ratio(BLACK, WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio(WHITE, WHITE) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn luminance_endpoints() {
        assert_eq!(relative_luminance(BLACK), 0.0);
        assert!((relative_luminance(WHITE) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn ratio_is_symmetric() {
        let a = Srgb {
            r: 118,
            g: 118,
            b: 118,
        };
        assert_eq!(contrast_ratio(a, WHITE), contrast_ratio(WHITE, a));
    }

    #[test]
    fn grey_767676_on_white_is_the_aa_minimum() {
        let ratio = contrast_ratio(
            Srgb {
                r: 118,
                g: 118,
                b: 118,
            },
            WHITE,
        );
        assert!((ratio - 4.5422).abs() < 1e-3, "got {ratio}");
        assert_eq!(
            text_contrast_verdict(ratio, TextSize::Normal, ContrastLevel::Aa),
            ContrastVerdict::Pass
        );
    }

    #[test]
    fn text_size_thresholds() {
        assert_eq!(classify_text(23.99, false), TextSize::Normal);
        assert_eq!(classify_text(24.0, false), TextSize::Large);
        assert_eq!(classify_text(18.65, true), TextSize::Normal);
        assert_eq!(classify_text(18.66, true), TextSize::Normal);
        assert_eq!(classify_text(18.67, true), TextSize::Large);
        assert_eq!(classify_text(18.67, false), TextSize::Normal);
        assert_eq!(classify_text(14.0 * PX_PER_PT, true), TextSize::Large);
    }

    #[test]
    fn display_rounding_does_not_change_the_verdict() {
        assert_eq!(display_ratio(4.4999), "4.50:1");
        assert!(matches!(
            text_contrast_verdict(4.4999, TextSize::Normal, ContrastLevel::Aa),
            ContrastVerdict::Fail { required, .. } if required == 4.5
        ));
        assert_eq!(
            text_contrast_verdict(4.5, TextSize::Normal, ContrastLevel::Aa),
            ContrastVerdict::Pass
        );
    }

    #[test]
    fn required_ratios() {
        use ContrastLevel::*;
        use TextSize::*;
        assert_eq!(required_ratio(Normal, Aa), 4.5);
        assert_eq!(required_ratio(Large, Aa), 3.0);
        assert_eq!(required_ratio(Normal, Aaa), 7.0);
        assert_eq!(required_ratio(Large, Aaa), 4.5);
    }
}
