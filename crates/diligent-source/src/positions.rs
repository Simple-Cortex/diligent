//! Source positions for parsed elements.
//!
//! The HTML parser does not report where an element began. This module lexes
//! the start tags of the text on its own, tolerantly, and then lines the lexer's
//! tags up with the parsed tree in document order. The match is by lowercase
//! tag name, so elements the parser implied (`html`, `head`, `body`, `tbody`)
//! that never appear in the text get no position and do not consume a tag.
//!
//! The lexer follows the HTML tokenizer where it matters for finding tags:
//! comments, `<![CDATA[`, declarations, quoted attribute values, and the raw
//! text of `script`, `style` and similar elements are skipped, so a `<` inside
//! any of them never starts a tag.

use std::collections::HashMap;

/// A start tag found in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartTag {
    /// Lowercase tag name.
    pub name: String,
    /// Byte offset of the `<`.
    pub offset: usize,
    /// 1-based line.
    pub line: u32,
    /// 1-based column, counted in characters.
    pub column: u32,
}

/// Maps byte offsets to 1-based line and column.
///
/// A line ends at `\n`, `\r\n`, or a lone `\r`.
#[derive(Debug, Clone)]
pub struct LineIndex<'t> {
    text: &'t str,
    starts: Vec<usize>,
}

impl<'t> LineIndex<'t> {
    /// Indexes the line starts of `text`.
    pub fn new(text: &'t str) -> Self {
        let b = text.as_bytes();
        let mut starts = vec![0];
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'\n' => starts.push(i + 1),
                b'\r' if b.get(i + 1) != Some(&b'\n') => starts.push(i + 1),
                _ => {}
            }
            i += 1;
        }
        LineIndex { text, starts }
    }

    /// Line and column of a byte offset. An offset past the end clamps to it.
    pub fn locate(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.text.len());
        let line = self.starts.partition_point(|s| *s <= offset).max(1);
        let start = self.starts[line - 1];
        let column = match self.text.get(start..offset) {
            Some(s) => s.chars().count(),
            None => offset - start,
        } + 1;
        (to_u32(line), to_u32(column))
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn find(b: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || from >= b.len() {
        return None;
    }
    b[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn find_byte(b: &[u8], from: usize, byte: u8) -> Option<usize> {
    b.get(from..)?
        .iter()
        .position(|c| *c == byte)
        .map(|p| p + from)
}

/// Index just past the end of a `<!` or `<?` construct that starts at `i`.
fn skip_declaration(b: &[u8], i: usize) -> usize {
    if b[i..].starts_with(b"<!--") {
        let body = i + 4;
        if b.get(body) == Some(&b'>') {
            return body + 1;
        }
        if b[body.min(b.len())..].starts_with(b"->") {
            return body + 2;
        }
        return find(b, body, b"-->").map_or(b.len(), |p| p + 3);
    }
    if b[i..].starts_with(b"<![CDATA[") {
        if let Some(p) = find(b, i + 9, b"]]>") {
            return p + 3;
        }
    }
    find_byte(b, i, b'>').map_or(b.len(), |p| p + 1)
}

const RAW_TEXT: &[&str] = &[
    "script", "style", "textarea", "title", "xmp", "iframe", "noembed", "noframes",
];

/// Index of the `<` of the end tag that closes raw text `name`, from `from`.
fn raw_text_end(b: &[u8], from: usize, name: &str) -> usize {
    let n = name.as_bytes();
    let mut i = from;
    while let Some(p) = find_byte(b, i, b'<') {
        let after = p + 2 + n.len();
        if b.get(p + 1) == Some(&b'/')
            && b.len() >= after
            && b[p + 2..after].eq_ignore_ascii_case(n)
            && b.get(after)
                .is_none_or(|c| is_ws(*c) || *c == b'/' || *c == b'>')
        {
            return p;
        }
        i = p + 1;
    }
    b.len()
}

/// Lexes every start tag in `text`, in document order.
///
/// A tag whose attributes run to the end of the text without a closing `>` is
/// not reported, because the parser drops it too.
pub fn start_tags(text: &str) -> Vec<StartTag> {
    let b = text.as_bytes();
    let index = LineIndex::new(text);
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(lt) = find_byte(b, i, b'<') {
        i = lt;
        match b.get(i + 1) {
            Some(b'!' | b'?') => i = skip_declaration(b, i),
            Some(b'/') => i = find_byte(b, i, b'>').map_or(b.len(), |p| p + 1),
            Some(c) if c.is_ascii_alphabetic() => {
                let name_end = b[i + 1..]
                    .iter()
                    .position(|c| is_ws(*c) || *c == b'/' || *c == b'>')
                    .map_or(b.len(), |p| p + i + 1);
                let Some(end) = skip_attributes(b, name_end) else {
                    break;
                };
                let name = text[i + 1..name_end].to_ascii_lowercase();
                let (line, column) = index.locate(i);
                let raw = RAW_TEXT.contains(&name.as_str());
                out.push(StartTag {
                    name: name.clone(),
                    offset: i,
                    line,
                    column,
                });
                i = if raw {
                    raw_text_end(b, end, &name)
                } else {
                    end
                };
            }
            _ => i += 1,
        }
    }
    out
}

/// Skips attributes from `j` (just after the tag name) to just past the `>`.
/// `None` when the text ends first.
fn skip_attributes(b: &[u8], mut j: usize) -> Option<usize> {
    loop {
        while j < b.len() && (is_ws(b[j]) || b[j] == b'/') {
            j += 1;
        }
        match b.get(j) {
            None => return None,
            Some(b'>') => return Some(j + 1),
            _ => {}
        }
        let start = j;
        if b[j] == b'=' {
            j += 1;
        }
        while j < b.len() && !is_ws(b[j]) && !matches!(b[j], b'/' | b'>' | b'=') {
            j += 1;
        }
        if j == start {
            j += 1;
        }
        while j < b.len() && is_ws(b[j]) {
            j += 1;
        }
        if b.get(j) == Some(&b'=') {
            j += 1;
            while j < b.len() && is_ws(b[j]) {
                j += 1;
            }
            match b.get(j) {
                Some(q @ (b'"' | b'\'')) => {
                    let close = find_byte(b, j + 1, *q)?;
                    j = close + 1;
                }
                Some(b'>') | None => {}
                Some(_) => {
                    while j < b.len() && !is_ws(b[j]) && b[j] != b'>' {
                        j += 1;
                    }
                }
            }
        }
    }
}

/// Lines elements up with lexed tags.
///
/// `names` yields each parsed element's tag name in document order. The result
/// has one entry per element: the index into `tags` of the tag it came from,
/// or `None` when no tag of that name remains after the previous match.
pub fn align<'n>(names: impl Iterator<Item = &'n str>, tags: &[StartTag]) -> Vec<Option<usize>> {
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, t) in tags.iter().enumerate() {
        by_name.entry(t.name.as_str()).or_default().push(i);
    }
    let mut cursor = 0usize;
    names
        .map(|name| {
            let lower = name.to_ascii_lowercase();
            let list = by_name.get(lower.as_str())?;
            let at = list.partition_point(|idx| *idx < cursor);
            let idx = *list.get(at)?;
            cursor = idx + 1;
            Some(idx)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<(String, u32)> {
        start_tags(text)
            .into_iter()
            .map(|t| (t.name, t.line))
            .collect()
    }

    #[test]
    fn lines_and_columns() {
        let tags = start_tags("<p>a</p>\n  <b>x</b>");
        assert_eq!((tags[0].line, tags[0].column), (1, 1));
        assert_eq!((tags[1].line, tags[1].column), (2, 3));
    }

    #[test]
    fn crlf_and_lone_cr_each_count_once() {
        let tags = start_tags("<a>\r\n<b>\r<c>\n<d>");
        let lines: Vec<u32> = tags.iter().map(|t| t.line).collect();
        assert_eq!(lines, vec![1, 2, 3, 4]);
    }

    #[test]
    fn skips_comments_script_and_text_angles() {
        let t = "<!-- <x> --><p>1 < 2 and a<3</p><script>var s='</div><i>';</script><u>";
        let got: Vec<String> = names(t).into_iter().map(|n| n.0).collect();
        assert_eq!(got, vec!["p", "script", "u"]);
    }

    #[test]
    fn quoted_attribute_may_contain_angle_and_gt() {
        let got = names("<a title=\"x > <b>\" href=y><i>");
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].0, "i");
    }

    #[test]
    fn unterminated_tag_is_dropped() {
        assert!(start_tags("<a title=\"oops").is_empty());
        assert!(start_tags("<a href").is_empty());
    }

    #[test]
    fn cdata_and_declarations() {
        let got: Vec<String> = names("<!DOCTYPE html><svg><![CDATA[ <x> ]]><g>")
            .into_iter()
            .map(|n| n.0)
            .collect();
        assert_eq!(got, vec!["svg", "g"]);
    }

    #[test]
    fn align_skips_implied_elements() {
        let tags = start_tags("<p>a</p><p>b</p>");
        let got = align(["html", "head", "body", "p", "p"].into_iter(), &tags);
        assert_eq!(got, vec![None, None, None, Some(0), Some(1)]);
    }

    #[test]
    fn align_is_case_insensitive() {
        let tags = start_tags("<svg><foreignObject>");
        let got = align(["svg", "foreignObject"].into_iter(), &tags);
        assert_eq!(got, vec![Some(0), Some(1)]);
    }

    #[test]
    fn multibyte_columns_count_characters() {
        let tags = start_tags("é é <b>");
        assert_eq!(tags[0].column, 5);
    }

    #[test]
    fn locate_clamps_past_end() {
        let idx = LineIndex::new("ab");
        assert_eq!(idx.locate(99), (1, 3));
    }
}
