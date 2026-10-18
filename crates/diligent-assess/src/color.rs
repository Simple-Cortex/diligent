//! CSS color parsing for contrast checks.
//!
//! Supported syntaxes: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()` and
//! `rgba()` (comma or space separated, optional `/ alpha`, numbers or
//! percentages), the 148 CSS Color 4 named colors and `transparent`.
//! Anything else (`hsl()`, `color()`, `currentcolor`, system colors, `var()`)
//! returns `None`, and the caller must treat that as "needs review".

use crate::contrast::Srgb;

/// A color with alpha. `a` is in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

/// A parsed CSS color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Color {
    /// Alpha is exactly 1.
    Opaque(Srgb),
    /// Alpha is below 1. The caller must composite it over a known opaque
    /// background with [`composite_over`], or report `needs_review`.
    Translucent(Rgba),
}

/// Composite `fg` over an opaque `bg` in sRGB space.
///
/// Each channel is `fg * a + bg * (1 - a)`, rounded half away from zero to the
/// nearest integer. This matches how browsers blend in gamma-encoded sRGB.
pub fn composite_over(fg: Rgba, bg: Srgb) -> Srgb {
    let a = fg.a.clamp(0.0, 1.0);
    let mix = |f: u8, b: u8| -> u8 {
        let v = f64::from(f) * a + f64::from(b) * (1.0 - a);
        v.round().clamp(0.0, 255.0) as u8
    };
    Srgb {
        r: mix(fg.r, bg.r),
        g: mix(fg.g, bg.g),
        b: mix(fg.b, bg.b),
    }
}

fn opaque_or_translucent(r: u8, g: u8, b: u8, a: f64) -> Color {
    if a >= 1.0 {
        Color::Opaque(Srgb { r, g, b })
    } else {
        Color::Translucent(Rgba {
            r,
            g,
            b,
            a: a.clamp(0.0, 1.0),
        })
    }
}

/// Parse a CSS color string. See the module docs for the supported syntaxes.
pub fn parse_css_color(input: &str) -> Option<Color> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    let lower = s.to_ascii_lowercase();
    if lower == "transparent" {
        return Some(Color::Translucent(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0.0,
        }));
    }
    if let Some(inner) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return parse_rgb_function(inner);
    }
    named_color(&lower).map(|(r, g, b)| Color::Opaque(Srgb { r, g, b }))
}

fn parse_hex(hex: &str) -> Option<Color> {
    if !hex.is_ascii() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let nibble = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok().map(|n| n * 17);
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        3 => Some(opaque_or_translucent(
            nibble(0)?,
            nibble(1)?,
            nibble(2)?,
            1.0,
        )),
        4 => Some(opaque_or_translucent(
            nibble(0)?,
            nibble(1)?,
            nibble(2)?,
            f64::from(nibble(3)?) / 255.0,
        )),
        6 => Some(opaque_or_translucent(byte(0)?, byte(2)?, byte(4)?, 1.0)),
        8 => Some(opaque_or_translucent(
            byte(0)?,
            byte(2)?,
            byte(4)?,
            f64::from(byte(6)?) / 255.0,
        )),
        _ => None,
    }
}

/// Parse a number or percentage. Returns `(value, was_percent)`.
fn number_or_percent(token: &str) -> Option<(f64, bool)> {
    let (digits, percent) = match token.strip_suffix('%') {
        Some(d) => (d, true),
        None => (token, false),
    };
    if digits.is_empty() || digits.eq_ignore_ascii_case("nan") || digits.contains("inf") {
        return None;
    }
    digits
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| (v, percent))
}

fn parse_rgb_function(inner: &str) -> Option<Color> {
    // Split an optional alpha: `a, b, c, alpha` or `a b c / alpha`.
    let (color_part, alpha_part) = if let Some((left, right)) = inner.split_once('/') {
        (left, Some(right))
    } else {
        (inner, None)
    };
    let tokens: Vec<&str> = color_part
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .collect();
    let (channels, alpha_token) = match (tokens.len(), alpha_part) {
        (3, None) => (&tokens[..3], None),
        (4, None) => (&tokens[..3], Some(tokens[3])),
        (3, Some(a)) => (&tokens[..3], Some(a.trim())),
        _ => return None,
    };
    let channel = |t: &str| -> Option<u8> {
        let (v, pct) = number_or_percent(t)?;
        let scaled = if pct { v * 255.0 / 100.0 } else { v };
        Some(scaled.round().clamp(0.0, 255.0) as u8)
    };
    let (r, g, b) = (
        channel(channels[0])?,
        channel(channels[1])?,
        channel(channels[2])?,
    );
    let alpha = match alpha_token {
        None => 1.0,
        Some(t) => {
            let (v, pct) = number_or_percent(t)?;
            (if pct { v / 100.0 } else { v }).clamp(0.0, 1.0)
        }
    };
    Some(opaque_or_translucent(r, g, b, alpha))
}

/// Look up a lowercase CSS named color.
pub fn named_color(name: &str) -> Option<(u8, u8, u8)> {
    NAMED_COLORS
        .binary_search_by(|(n, ..)| (*n).cmp(name))
        .ok()
        .map(|i| {
            let (_, r, g, b) = NAMED_COLORS[i];
            (r, g, b)
        })
}

/// The 148 CSS Color 4 named colors, sorted by name for binary search.
/// Source: the `color-name` package, cross-checked against Pillow's table.
pub static NAMED_COLORS: [(&str, u8, u8, u8); 148] = [
    ("aliceblue", 240, 248, 255),
    ("antiquewhite", 250, 235, 215),
    ("aqua", 0, 255, 255),
    ("aquamarine", 127, 255, 212),
    ("azure", 240, 255, 255),
    ("beige", 245, 245, 220),
    ("bisque", 255, 228, 196),
    ("black", 0, 0, 0),
    ("blanchedalmond", 255, 235, 205),
    ("blue", 0, 0, 255),
    ("blueviolet", 138, 43, 226),
    ("brown", 165, 42, 42),
    ("burlywood", 222, 184, 135),
    ("cadetblue", 95, 158, 160),
    ("chartreuse", 127, 255, 0),
    ("chocolate", 210, 105, 30),
    ("coral", 255, 127, 80),
    ("cornflowerblue", 100, 149, 237),
    ("cornsilk", 255, 248, 220),
    ("crimson", 220, 20, 60),
    ("cyan", 0, 255, 255),
    ("darkblue", 0, 0, 139),
    ("darkcyan", 0, 139, 139),
    ("darkgoldenrod", 184, 134, 11),
    ("darkgray", 169, 169, 169),
    ("darkgreen", 0, 100, 0),
    ("darkgrey", 169, 169, 169),
    ("darkkhaki", 189, 183, 107),
    ("darkmagenta", 139, 0, 139),
    ("darkolivegreen", 85, 107, 47),
    ("darkorange", 255, 140, 0),
    ("darkorchid", 153, 50, 204),
    ("darkred", 139, 0, 0),
    ("darksalmon", 233, 150, 122),
    ("darkseagreen", 143, 188, 143),
    ("darkslateblue", 72, 61, 139),
    ("darkslategray", 47, 79, 79),
    ("darkslategrey", 47, 79, 79),
    ("darkturquoise", 0, 206, 209),
    ("darkviolet", 148, 0, 211),
    ("deeppink", 255, 20, 147),
    ("deepskyblue", 0, 191, 255),
    ("dimgray", 105, 105, 105),
    ("dimgrey", 105, 105, 105),
    ("dodgerblue", 30, 144, 255),
    ("firebrick", 178, 34, 34),
    ("floralwhite", 255, 250, 240),
    ("forestgreen", 34, 139, 34),
    ("fuchsia", 255, 0, 255),
    ("gainsboro", 220, 220, 220),
    ("ghostwhite", 248, 248, 255),
    ("gold", 255, 215, 0),
    ("goldenrod", 218, 165, 32),
    ("gray", 128, 128, 128),
    ("green", 0, 128, 0),
    ("greenyellow", 173, 255, 47),
    ("grey", 128, 128, 128),
    ("honeydew", 240, 255, 240),
    ("hotpink", 255, 105, 180),
    ("indianred", 205, 92, 92),
    ("indigo", 75, 0, 130),
    ("ivory", 255, 255, 240),
    ("khaki", 240, 230, 140),
    ("lavender", 230, 230, 250),
    ("lavenderblush", 255, 240, 245),
    ("lawngreen", 124, 252, 0),
    ("lemonchiffon", 255, 250, 205),
    ("lightblue", 173, 216, 230),
    ("lightcoral", 240, 128, 128),
    ("lightcyan", 224, 255, 255),
    ("lightgoldenrodyellow", 250, 250, 210),
    ("lightgray", 211, 211, 211),
    ("lightgreen", 144, 238, 144),
    ("lightgrey", 211, 211, 211),
    ("lightpink", 255, 182, 193),
    ("lightsalmon", 255, 160, 122),
    ("lightseagreen", 32, 178, 170),
    ("lightskyblue", 135, 206, 250),
    ("lightslategray", 119, 136, 153),
    ("lightslategrey", 119, 136, 153),
    ("lightsteelblue", 176, 196, 222),
    ("lightyellow", 255, 255, 224),
    ("lime", 0, 255, 0),
    ("limegreen", 50, 205, 50),
    ("linen", 250, 240, 230),
    ("magenta", 255, 0, 255),
    ("maroon", 128, 0, 0),
    ("mediumaquamarine", 102, 205, 170),
    ("mediumblue", 0, 0, 205),
    ("mediumorchid", 186, 85, 211),
    ("mediumpurple", 147, 112, 219),
    ("mediumseagreen", 60, 179, 113),
    ("mediumslateblue", 123, 104, 238),
    ("mediumspringgreen", 0, 250, 154),
    ("mediumturquoise", 72, 209, 204),
    ("mediumvioletred", 199, 21, 133),
    ("midnightblue", 25, 25, 112),
    ("mintcream", 245, 255, 250),
    ("mistyrose", 255, 228, 225),
    ("moccasin", 255, 228, 181),
    ("navajowhite", 255, 222, 173),
    ("navy", 0, 0, 128),
    ("oldlace", 253, 245, 230),
    ("olive", 128, 128, 0),
    ("olivedrab", 107, 142, 35),
    ("orange", 255, 165, 0),
    ("orangered", 255, 69, 0),
    ("orchid", 218, 112, 214),
    ("palegoldenrod", 238, 232, 170),
    ("palegreen", 152, 251, 152),
    ("paleturquoise", 175, 238, 238),
    ("palevioletred", 219, 112, 147),
    ("papayawhip", 255, 239, 213),
    ("peachpuff", 255, 218, 185),
    ("peru", 205, 133, 63),
    ("pink", 255, 192, 203),
    ("plum", 221, 160, 221),
    ("powderblue", 176, 224, 230),
    ("purple", 128, 0, 128),
    ("rebeccapurple", 102, 51, 153),
    ("red", 255, 0, 0),
    ("rosybrown", 188, 143, 143),
    ("royalblue", 65, 105, 225),
    ("saddlebrown", 139, 69, 19),
    ("salmon", 250, 128, 114),
    ("sandybrown", 244, 164, 96),
    ("seagreen", 46, 139, 87),
    ("seashell", 255, 245, 238),
    ("sienna", 160, 82, 45),
    ("silver", 192, 192, 192),
    ("skyblue", 135, 206, 235),
    ("slateblue", 106, 90, 205),
    ("slategray", 112, 128, 144),
    ("slategrey", 112, 128, 144),
    ("snow", 255, 250, 250),
    ("springgreen", 0, 255, 127),
    ("steelblue", 70, 130, 180),
    ("tan", 210, 180, 140),
    ("teal", 0, 128, 128),
    ("thistle", 216, 191, 216),
    ("tomato", 255, 99, 71),
    ("turquoise", 64, 224, 208),
    ("violet", 238, 130, 238),
    ("wheat", 245, 222, 179),
    ("white", 255, 255, 255),
    ("whitesmoke", 245, 245, 245),
    ("yellow", 255, 255, 0),
    ("yellowgreen", 154, 205, 50),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque(r: u8, g: u8, b: u8) -> Option<Color> {
        Some(Color::Opaque(Srgb { r, g, b }))
    }

    #[test]
    fn named_table_has_148_sorted_unique_names() {
        assert_eq!(NAMED_COLORS.len(), 148);
        assert!(NAMED_COLORS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn hex_forms_parse() {
        assert_eq!(parse_css_color("#fff"), opaque(255, 255, 255));
        assert_eq!(parse_css_color("#767676"), opaque(118, 118, 118));
        assert_eq!(parse_css_color("  #ABCDEF "), opaque(0xAB, 0xCD, 0xEF));
        assert_eq!(parse_css_color("#12345"), None);
        assert_eq!(parse_css_color("#gggggg"), None);
    }

    #[test]
    fn hex_alpha_is_translucent() {
        match parse_css_color("#00000080") {
            Some(Color::Translucent(c)) => {
                assert_eq!((c.r, c.g, c.b), (0, 0, 0));
                assert!((c.a - 128.0 / 255.0).abs() < 1e-12);
            }
            other => panic!("expected translucent, got {other:?}"),
        }
        assert_eq!(parse_css_color("#000000ff"), opaque(0, 0, 0));
    }

    #[test]
    fn rgb_functions_parse() {
        assert_eq!(parse_css_color("rgb(10, 20, 30)"), opaque(10, 20, 30));
        assert_eq!(parse_css_color("RGB(10 20 30)"), opaque(10, 20, 30));
        assert_eq!(parse_css_color("rgb(100%, 0%, 50%)"), opaque(255, 0, 128));
        assert_eq!(parse_css_color("rgba(0,0,0,1)"), opaque(0, 0, 0));
        assert_eq!(parse_css_color("rgb(300, -5, 0)"), opaque(255, 0, 0));
        assert!(matches!(
            parse_css_color("rgba(0, 0, 0, 0.5)"),
            Some(Color::Translucent(c)) if (c.a - 0.5).abs() < 1e-12
        ));
        assert!(matches!(
            parse_css_color("rgb(0 0 0 / 25%)"),
            Some(Color::Translucent(c)) if (c.a - 0.25).abs() < 1e-12
        ));
        assert_eq!(parse_css_color("rgb(1, 2)"), None);
        assert_eq!(parse_css_color("rgb(a, b, c)"), None);
    }

    #[test]
    fn names_and_keywords() {
        assert_eq!(parse_css_color("White"), opaque(255, 255, 255));
        assert_eq!(parse_css_color("rebeccapurple"), opaque(102, 51, 153));
        assert!(matches!(
            parse_css_color("transparent"),
            Some(Color::Translucent(c)) if c.a == 0.0
        ));
        assert_eq!(parse_css_color("currentcolor"), None);
        assert_eq!(parse_css_color("hsl(0, 0%, 0%)"), None);
        assert_eq!(parse_css_color(""), None);
    }

    #[test]
    fn compositing_rounds_half_away_from_zero() {
        let fg = Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0.5,
        };
        let out = composite_over(
            fg,
            Srgb {
                r: 255,
                g: 255,
                b: 255,
            },
        );
        assert_eq!(
            out,
            Srgb {
                r: 128,
                g: 128,
                b: 128
            }
        );
        let full = Rgba {
            r: 9,
            g: 9,
            b: 9,
            a: 1.0,
        };
        assert_eq!(
            composite_over(full, Srgb { r: 1, g: 2, b: 3 }),
            Srgb { r: 9, g: 9, b: 9 }
        );
    }
}
