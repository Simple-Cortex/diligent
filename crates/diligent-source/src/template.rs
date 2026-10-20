//! Template flavours: a dependency-free, tolerant pre-pass that turns a
//! component or template file into text the HTML parser can read.
//!
//! The pass never evaluates anything. It rewrites template syntax into plain
//! HTML plus a few markers the checks recognise:
//!
//! - [`MARK`] stands in for an expression, in a text node or in an attribute
//!   value. A check that meets it cannot know the runtime value and reports
//!   `needs_review`.
//! - Components (`<Foo>`, `<Foo.Bar>`) become [`COMPONENT_TAG`] elements. They
//!   are opaque: no check reports on them, and their names read as unknown text.
//! - An attribute spread (`{...props}`, `v-bind="obj"`) becomes
//!   [`SPREAD_ATTR`], which downgrades a `fail` on that element to `needs_review`.
//! - Conditionals and loops set [`Preprocessed::has_conditionals`].
//! - Script, style, frontmatter, comment and statement blocks are blanked with
//!   spaces (line breaks are kept).
//!
//! Output bytes map back to source bytes through [`Preprocessed::map`].
//! JavaScript outside JSX in `.jsx` and `.tsx` files is not masked: it stays as
//! text nodes, which is noise but never a finding.

use std::path::Path;

/// Stands in for a template expression in text and attribute values.
pub const MARK: char = '\u{1}';
/// Tag name that replaces a component element.
pub const COMPONENT_TAG: &str = "dl-component";
/// Attribute that records an attribute spread on an element.
pub const SPREAD_ATTR: &str = "data-dl-spread";
/// Tag name that replaces a JSX fragment (`<>`).
pub const FRAGMENT_TAG: &str = "dl-fragment";
/// Tag name that replaces a Vue `template` element.
pub const TEMPLATE_TAG: &str = "dl-template";

const MAX_NEST: usize = 48;

/// The kind of file being scanned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Flavor {
    /// Plain HTML: scanned as is.
    Html,
    /// JSX or TSX.
    Jsx,
    /// Vue single-file component.
    Vue,
    /// Svelte component.
    Svelte,
    /// Astro component.
    Astro,
    /// Twig template.
    Twig,
    /// Liquid template.
    Liquid,
}

impl Flavor {
    /// The flavour for a file extension, without the dot.
    pub fn from_extension(ext: &str) -> Option<Flavor> {
        match ext.to_ascii_lowercase().as_str() {
            "html" | "htm" => Some(Flavor::Html),
            "jsx" | "tsx" => Some(Flavor::Jsx),
            "vue" => Some(Flavor::Vue),
            "svelte" => Some(Flavor::Svelte),
            "astro" => Some(Flavor::Astro),
            "twig" => Some(Flavor::Twig),
            "liquid" => Some(Flavor::Liquid),
            _ => None,
        }
    }

    /// The flavour for a path, by extension.
    pub fn from_path(path: &Path) -> Option<Flavor> {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(Flavor::from_extension)
    }

    /// Lowercase name, as written in evidence.
    pub fn name(self) -> &'static str {
        match self {
            Flavor::Html => "html",
            Flavor::Jsx => "jsx",
            Flavor::Vue => "vue",
            Flavor::Svelte => "svelte",
            Flavor::Astro => "astro",
            Flavor::Twig => "twig",
            Flavor::Liquid => "liquid",
        }
    }

    fn has_components(self) -> bool {
        matches!(
            self,
            Flavor::Jsx | Flavor::Vue | Flavor::Svelte | Flavor::Astro
        )
    }

    fn has_braces(self) -> bool {
        matches!(self, Flavor::Jsx | Flavor::Svelte | Flavor::Astro)
    }

    fn has_fragments(self) -> bool {
        matches!(self, Flavor::Jsx | Flavor::Astro)
    }
}

/// Text ready for the HTML parser, with a map back to the source.
#[derive(Debug, Clone)]
pub struct Preprocessed {
    /// The rewritten text.
    pub html: String,
    /// Source byte for each output byte. Empty means the output is the source.
    pub map: Vec<u32>,
    /// True when the file holds a conditional or a loop.
    pub has_conditionals: bool,
}

impl Preprocessed {
    /// Source byte offset for an output byte offset.
    pub fn source_offset(&self, out: usize, source_len: usize) -> usize {
        if self.map.is_empty() {
            return out.min(source_len);
        }
        self.map
            .get(out)
            .map_or(source_len, |s| usize::try_from(*s).unwrap_or(source_len))
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Rewrites `text` for the HTML parser.
pub fn preprocess(text: &str, flavor: Flavor) -> Preprocessed {
    if flavor == Flavor::Html {
        return Preprocessed {
            html: text.to_string(),
            map: Vec::new(),
            has_conditionals: false,
        };
    }
    let mut sc = Scanner {
        s: text,
        b: text.as_bytes(),
        flavor,
        o: Out {
            src: text,
            html: String::with_capacity(text.len()),
            map: Vec::with_capacity(text.len()),
            conditional: false,
        },
    };
    let mut start = 0;
    if flavor == Flavor::Astro {
        start = astro_frontmatter_end(text);
        sc.o.mask(0, start);
    }
    sc.scan(start, Kind::Top, 0);
    Preprocessed {
        html: sc.o.html,
        map: sc.o.map,
        has_conditionals: sc.o.conditional,
    }
}

/// End of a leading `---` frontmatter block, or 0 when there is none.
fn astro_frontmatter_end(text: &str) -> usize {
    let mut pos = 0;
    let mut opened = false;
    for line in text.split_inclusive('\n') {
        let t = line.trim_end();
        let body = t.trim_start_matches('\u{feff}');
        if !opened {
            if body.trim().is_empty() {
                pos += line.len();
                continue;
            }
            if body.trim() == "---" {
                opened = true;
                pos += line.len();
                continue;
            }
            return 0;
        }
        pos += line.len();
        if body.trim() == "---" {
            return pos;
        }
    }
    0
}

struct Out<'s> {
    src: &'s str,
    html: String,
    map: Vec<u32>,
    conditional: bool,
}

impl Out<'_> {
    fn copy(&mut self, from: usize, to: usize) {
        if let Some(t) = self.src.get(from..to) {
            self.html.push_str(t);
            self.map.extend((from..to).map(to_u32));
        }
    }

    /// Blank `from..to` with spaces, keeping line breaks so lines still line up.
    fn mask(&mut self, from: usize, to: usize) {
        let Some(t) = self.src.get(from..to) else {
            return;
        };
        let mut at = from;
        for ch in t.chars() {
            let n = ch.len_utf8();
            if ch == '\n' || ch == '\r' {
                self.html.push(ch);
            } else {
                self.html.extend(std::iter::repeat_n(' ', n));
            }
            self.map.extend((at..at + n).map(to_u32));
            at += n;
        }
    }

    /// Write synthesised text; every byte maps to `at`.
    fn put(&mut self, text: &str, at: usize) {
        self.html.push_str(text);
        self.map.extend(std::iter::repeat_n(to_u32(at), text.len()));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Top,
    Braces,
}

enum Val<'s> {
    Quoted(&'s str),
    Unquoted(&'s str),
    Expr(&'s str),
}

enum Action {
    Keep,
    Drop,
    Set(String, Option<String>),
}

struct Scanner<'s> {
    s: &'s str,
    b: &'s [u8],
    flavor: Flavor,
    o: Out<'s>,
}

impl<'s> Scanner<'s> {
    fn at(&self, i: usize) -> Option<u8> {
        self.b.get(i).copied()
    }

    fn find(&self, from: usize, needle: &str) -> Option<usize> {
        let n = needle.as_bytes();
        self.b
            .get(from..)?
            .windows(n.len())
            .position(|w| w == n)
            .map(|p| p + from)
    }

    fn find_byte(&self, from: usize, byte: u8) -> Option<usize> {
        self.b
            .get(from..)?
            .iter()
            .position(|c| *c == byte)
            .map(|p| p + from)
    }

    /// Index just past a JavaScript string or template literal that starts at `i`.
    fn string_end(&self, i: usize, depth: usize) -> usize {
        let len = self.b.len();
        let q = self.b[i];
        let mut j = i + 1;
        while j < len {
            match self.b[j] {
                b'\\' => j += 2,
                b'\n' if q != b'`' => return j,
                c if c == q => return j + 1,
                b'$' if q == b'`' && self.at(j + 1) == Some(b'{') => {
                    j = self.brace_end(j + 1, depth + 1) + 1;
                }
                _ => j += 1,
            }
        }
        len
    }

    /// Index of the `}` that closes the `{` at `i`, or the text length.
    fn brace_end(&self, i: usize, depth: usize) -> usize {
        let len = self.b.len();
        if depth > MAX_NEST {
            return len;
        }
        let mut level = 0usize;
        let mut j = i;
        while j < len {
            match self.b[j] {
                b'{' => level += 1,
                b'}' => {
                    level = level.saturating_sub(1);
                    if level == 0 {
                        return j;
                    }
                }
                b'\'' | b'"' | b'`' => {
                    j = self.string_end(j, depth);
                    continue;
                }
                _ => {}
            }
            j += 1;
        }
        len
    }

    fn is_component(&self, name: &str) -> bool {
        if !self.flavor.has_components() {
            return false;
        }
        name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            || name.contains('.')
            || (self.flavor == Flavor::Svelte && name.starts_with("svelte:"))
    }

    fn rename(&self, name: &str) -> Option<&'static str> {
        if self.is_component(name) {
            Some(COMPONENT_TAG)
        } else if self.flavor == Flavor::Vue && name.eq_ignore_ascii_case("template") {
            Some(TEMPLATE_TAG)
        } else {
            None
        }
    }

    /// True when a `<` at `i` begins a tag in markup.
    fn tag_start(&self, i: usize) -> bool {
        match self.at(i + 1) {
            Some(c) if c.is_ascii_alphabetic() => true,
            Some(b'>') => self.flavor.has_fragments(),
            _ => false,
        }
    }

    /// True when the text before `i` lets a JSX element start there.
    fn jsx_allowed_before(&self, i: usize) -> bool {
        let mut j = i;
        while j > 0 && is_ws(self.b[j - 1]) {
            j -= 1;
        }
        let Some(&p) = j.checked_sub(1).and_then(|k| self.b.get(k)) else {
            return true;
        };
        if p.is_ascii_alphanumeric() || p == b'_' || p == b'$' {
            let mut k = j;
            while k > 0
                && (self.b[k - 1].is_ascii_alphanumeric() || matches!(self.b[k - 1], b'_' | b'$'))
            {
                k -= 1;
            }
            return matches!(
                &self.s[k..j],
                "return"
                    | "default"
                    | "case"
                    | "yield"
                    | "await"
                    | "in"
                    | "of"
                    | "else"
                    | "do"
                    | "typeof"
                    | "void"
                    | "throw"
            );
        }
        !matches!(p, b')' | b']' | b'.')
    }

    /// Scans markup or JavaScript from `start`. Returns the index it stopped at
    /// (the closing `}` for [`Kind::Braces`], else the end) and whether a tag
    /// was seen.
    fn scan(&mut self, start: usize, kind: Kind, nest: usize) -> (usize, bool) {
        let len = self.b.len();
        let mut i = start;
        let mut run = start;
        let mut elems = 0usize;
        let mut brace = 0usize;
        let mut saw_tag = false;
        let mut pending: Option<usize> = None;
        while i < len {
            let in_markup = elems > 0 || (kind == Kind::Top && self.flavor != Flavor::Jsx);
            let mark_mode = !in_markup && kind == Kind::Braces;
            let c = self.b[i];
            if in_markup {
                match c {
                    b'<' => {
                        if self.s[i..].starts_with("<!--") {
                            self.flush(&mut run, &mut pending, i, false);
                            let end = self.find(i + 4, "-->").map_or(len, |p| p + 3);
                            self.o.copy(i, end);
                            i = end;
                            run = i;
                        } else if self.at(i + 1) == Some(b'/') {
                            self.flush(&mut run, &mut pending, i, false);
                            i = self.end_tag(i);
                            elems = elems.saturating_sub(1);
                            run = i;
                        } else if self.tag_start(i) {
                            self.flush(&mut run, &mut pending, i, false);
                            saw_tag = true;
                            let (next, opens) = self.tag(i);
                            if opens {
                                elems += 1;
                            }
                            i = next;
                            run = i;
                        } else if matches!(self.at(i + 1), Some(b'!' | b'?')) {
                            self.flush(&mut run, &mut pending, i, false);
                            let end = self.find_byte(i, b'>').map_or(len, |p| p + 1);
                            self.o.copy(i, end);
                            i = end;
                            run = i;
                        } else {
                            i += 1;
                        }
                    }
                    b'{' => {
                        let next =
                            self.brace_in_markup(i, nest, &mut run, &mut pending, &mut saw_tag);
                        i = next;
                    }
                    _ => i += 1,
                }
                continue;
            }
            match c {
                b'\'' | b'"' | b'`' => {
                    if mark_mode && pending.is_none() {
                        pending = Some(i);
                    }
                    i = self.string_end(i, nest);
                }
                b'/' if self.at(i + 1) == Some(b'/') => {
                    i = self.find_byte(i, b'\n').unwrap_or(len);
                }
                b'/' if self.at(i + 1) == Some(b'*') => {
                    i = self.find(i + 2, "*/").map_or(len, |p| p + 2);
                }
                b'{' => {
                    brace += 1;
                    if mark_mode && pending.is_none() {
                        pending = Some(i);
                    }
                    i += 1;
                }
                b'}' => {
                    if kind == Kind::Braces {
                        if brace == 0 {
                            self.flush(&mut run, &mut pending, i, true);
                            return (i, saw_tag);
                        }
                        brace -= 1;
                    }
                    if mark_mode && pending.is_none() {
                        pending = Some(i);
                    }
                    i += 1;
                }
                b'<' if self.tag_start(i) && self.jsx_allowed_before(i) => {
                    self.flush(&mut run, &mut pending, i, mark_mode);
                    saw_tag = true;
                    let (next, opens) = self.tag(i);
                    if opens {
                        elems += 1;
                    }
                    i = next;
                    run = i;
                }
                b'<' if !mark_mode => {
                    // A `<` in script text must not start an element.
                    self.flush(&mut run, &mut pending, i, false);
                    self.o.put("&lt;", i);
                    i += 1;
                    run = i;
                }
                c if is_ws(c) => i += 1,
                _ => {
                    if mark_mode && pending.is_none() {
                        pending = Some(i);
                    }
                    i += 1;
                }
            }
        }
        self.flush(
            &mut run,
            &mut pending,
            len,
            kind == Kind::Braces && elems == 0,
        );
        (len, saw_tag)
    }

    /// Emits pending plain bytes before a construct at `upto`.
    fn flush(
        &mut self,
        run: &mut usize,
        pending: &mut Option<usize>,
        upto: usize,
        mark_mode: bool,
    ) {
        if mark_mode {
            if let Some(at) = pending.take() {
                self.o.put(&MARK.to_string(), at);
            }
        } else {
            self.o.copy(*run, upto);
        }
        *run = upto;
    }

    /// A `{` met in markup. Returns the index to continue from.
    fn brace_in_markup(
        &mut self,
        i: usize,
        nest: usize,
        run: &mut usize,
        pending: &mut Option<usize>,
        saw_tag: &mut bool,
    ) -> usize {
        let len = self.b.len();
        let next = self.at(i + 1);
        match self.flavor {
            Flavor::Vue if next == Some(b'{') => {
                self.flush(run, pending, i, false);
                let end = self.find(i + 2, "}}").map_or(len, |p| p + 2);
                self.o.put(&MARK.to_string(), i);
                *run = end;
                end
            }
            Flavor::Twig | Flavor::Liquid if matches!(next, Some(b'{' | b'%' | b'#')) => {
                self.flush(run, pending, i, false);
                let end = self.template_construct(i);
                *run = end;
                end
            }
            Flavor::Svelte if matches!(next, Some(b'#' | b':' | b'/' | b'@')) => {
                self.flush(run, pending, i, false);
                let end = self.brace_end(i, nest);
                let inner = self.s.get(i + 1..end).unwrap_or("");
                if inner.starts_with("@html") || inner.starts_with("@render") {
                    self.o.put(&MARK.to_string(), i);
                } else {
                    if ["#if", "#each", "#await", ":else", ":then", ":catch"]
                        .iter()
                        .any(|k| inner.starts_with(k))
                    {
                        self.o.conditional = true;
                    }
                    self.o.mask(i, (end + 1).min(len));
                }
                let after = (end + 1).min(len);
                *run = after;
                after
            }
            f if f.has_braces() && nest < MAX_NEST => {
                self.flush(run, pending, i, false);
                let (end, saw) = self.scan(i + 1, Kind::Braces, nest + 1);
                if saw {
                    *saw_tag = true;
                    self.o.conditional = true;
                }
                let after = (end + 1).min(len);
                *run = after;
                after
            }
            _ => i + 1,
        }
    }

    /// Handles `{{ }}`, `{% %}` and `{# #}` at `i`. Returns the index after it.
    fn template_construct(&mut self, i: usize) -> usize {
        let len = self.b.len();
        match self.at(i + 1) {
            Some(b'{') => {
                let end = self.find(i + 2, "}}").map_or(len, |p| p + 2);
                self.o.put(&MARK.to_string(), i);
                end
            }
            Some(b'%') => {
                let (end, word) = self.statement(i);
                if word == "comment" {
                    let mut at = end;
                    while let Some(p) = self.find(at, "{%") {
                        let (e2, w2) = self.statement(p);
                        at = e2;
                        if w2 == "endcomment" {
                            break;
                        }
                    }
                    let stop = if at == end { len } else { at };
                    self.o.mask(i, stop);
                    return stop;
                }
                if is_conditional_word(&word) {
                    self.o.conditional = true;
                }
                self.o.mask(i, end);
                end
            }
            _ => {
                let end = self.find(i + 2, "#}").map_or(len, |p| p + 2);
                self.o.mask(i, end);
                end
            }
        }
    }

    /// For a `{%` at `i`: the index after `%}` and the first word inside.
    fn statement(&self, i: usize) -> (usize, String) {
        let len = self.b.len();
        let close = self.find(i + 2, "%}");
        let inner_end = close.unwrap_or(len);
        let inner = self.s.get(i + 2..inner_end).unwrap_or("");
        let word: String = inner
            .trim_start_matches(|c: char| c == '-' || c == '~' || c.is_whitespace())
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        (close.map_or(len, |p| p + 2), word)
    }

    /// Rewrites an end tag at `i`. Returns the index after it.
    fn end_tag(&mut self, i: usize) -> usize {
        let len = self.b.len();
        let close = self.find_byte(i, b'>');
        let after = close.map_or(len, |p| p + 1);
        let name_end = (i + 2..close.unwrap_or(len))
            .find(|k| is_ws(self.b[*k]))
            .or(close)
            .unwrap_or(len);
        let name = self.s.get(i + 2..name_end).unwrap_or("");
        if name.is_empty() && self.flavor.has_fragments() {
            self.o.put(&format!("</{FRAGMENT_TAG}>"), i);
        } else if let Some(new) = self.rename(name) {
            self.o.put(&format!("</{new}>"), i);
        } else {
            self.o.copy(i, after);
        }
        after
    }

    /// Rewrites the start tag at `i`. Returns the index after it and whether
    /// it opens an element that needs a closing tag.
    fn tag(&mut self, i: usize) -> (usize, bool) {
        let len = self.b.len();
        if self.at(i + 1) == Some(b'>') {
            self.o.put(&format!("<{FRAGMENT_TAG}>"), i);
            return (i + 2, true);
        }
        let ns = i + 1;
        let mut ne = ns;
        while ne < len && !is_ws(self.b[ne]) && self.b[ne] != b'/' && self.b[ne] != b'>' {
            ne += 1;
        }
        let name = self.s.get(ns..ne).unwrap_or("");
        let valid = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'));
        if !valid {
            self.o.copy(i, i + 1);
            return (i + 1, false);
        }
        let lower = name.to_ascii_lowercase();
        let comp = self.is_component(name);
        let is_void = VOID.contains(&lower.as_str()) && !comp;
        if matches!(self.flavor, Flavor::Vue | Flavor::Svelte)
            && (lower == "script" || lower == "style")
        {
            let end = self.raw_end(ne, &lower);
            let after = self.find_byte(end, b'>').map_or(len, |p| p + 1);
            self.o.mask(i, after);
            return (after, false);
        }
        self.o.copy(i, ns);
        match self.rename(name) {
            Some(new) => self.o.put(new, ns),
            None => self.o.copy(ns, ne),
        }
        if comp {
            self.o.put(&format!(" data-dl-component=\"{name}\""), ns);
        }
        let mut j = ne;
        let mut self_close = false;
        loop {
            let ws = j;
            while j < len && is_ws(self.b[j]) {
                j += 1;
            }
            self.o.copy(ws, j);
            let Some(c) = self.at(j) else { break };
            if c == b'>' {
                self.o.copy(j, j + 1);
                j += 1;
                break;
            }
            if c == b'/' {
                if self.at(j + 1) == Some(b'>') {
                    // The HTML parser ignores `/>` on non-void elements, so a
                    // self-closing component or `<div />` gets an explicit end tag.
                    if is_void {
                        self.o.copy(j, j + 2);
                    } else {
                        let shown = self.rename(name).unwrap_or(name);
                        self.o.put(&format!("></{shown}>"), j);
                    }
                    j += 2;
                    self_close = true;
                    break;
                }
                self.o.copy(j, j + 1);
                j += 1;
                continue;
            }
            if c == b'{' {
                if self.flavor.has_braces() {
                    j = self.brace_attribute(j);
                    continue;
                }
                if matches!(self.flavor, Flavor::Twig | Flavor::Liquid) {
                    j = self.template_attribute(j);
                    continue;
                }
            }
            j = self.attribute(j);
        }
        let opens = !self_close && !is_void;
        if opens && !comp && (lower == "script" || lower == "style") {
            let end = self.raw_end(j, &lower);
            self.o.copy(j, end);
            j = end;
        }
        (j, opens)
    }

    /// Index of the `<` that closes raw text `name`, from `from`.
    fn raw_end(&self, from: usize, name: &str) -> usize {
        let n = name.as_bytes();
        let mut at = from;
        while let Some(p) = self.find_byte(at, b'<') {
            let after = p + 2 + n.len();
            if self.at(p + 1) == Some(b'/')
                && self
                    .b
                    .get(p + 2..after)
                    .is_some_and(|w| w.eq_ignore_ascii_case(n))
                && self
                    .at(after)
                    .is_none_or(|c| is_ws(c) || c == b'/' || c == b'>')
            {
                return p;
            }
            at = p + 1;
        }
        self.b.len()
    }

    /// An attribute-position `{...}` in JSX, Astro or Svelte.
    fn brace_attribute(&mut self, j: usize) -> usize {
        let len = self.b.len();
        let end = self.brace_end(j, 0);
        let inner = self.s.get(j + 1..end).unwrap_or("").trim();
        if !inner.starts_with("...") && is_ident(inner) {
            let text = format!("{inner}=\"{MARK}\"");
            self.o.put(&text, j);
        } else {
            self.o.put(SPREAD_ATTR, j);
        }
        (end + 1).min(len)
    }

    /// An attribute-position `{{ }}`, `{% %}` or `{# #}` in Twig or Liquid.
    fn template_attribute(&mut self, j: usize) -> usize {
        match self.at(j + 1) {
            Some(b'{') => {
                let end = self.find(j + 2, "}}").map_or(self.b.len(), |p| p + 2);
                self.o.put(SPREAD_ATTR, j);
                end
            }
            Some(b'%' | b'#') => self.template_construct(j),
            _ => self.attribute(j),
        }
    }

    /// One ordinary attribute starting at `j`. Returns the index after it.
    fn attribute(&mut self, j: usize) -> usize {
        let len = self.b.len();
        let mut ne = j;
        if self.b[ne] == b'=' {
            ne += 1;
        }
        loop {
            match self.b.get(ne) {
                None => break,
                Some(c) if is_ws(*c) || matches!(c, b'=' | b'>') => break,
                Some(b'/') if self.at(ne + 1) == Some(b'>') => break,
                Some(_) => ne += 1,
            }
        }
        if ne == j {
            ne += 1;
        }
        let raw_name = self.s.get(j..ne).unwrap_or("");
        let mut k = ne;
        while k < len && is_ws(self.b[k]) {
            k += 1;
        }
        let mut after = ne;
        let mut value: Option<Val<'s>> = None;
        if self.at(k) == Some(b'=') {
            k += 1;
            while k < len && is_ws(self.b[k]) {
                k += 1;
            }
            match self.at(k) {
                Some(q @ (b'"' | b'\'')) => {
                    let close = self.find_byte(k + 1, q).unwrap_or(len);
                    value = Some(Val::Quoted(self.s.get(k + 1..close).unwrap_or("")));
                    after = (close + 1).min(len);
                }
                Some(b'{') if self.flavor.has_braces() => {
                    let e = self.brace_end(k, 0);
                    value = Some(Val::Expr(self.s.get(k + 1..e).unwrap_or("")));
                    after = (e + 1).min(len);
                }
                Some(b'{') if self.at(k + 1) == Some(b'{') => {
                    let e = self.find(k + 2, "}}").map_or(len, |p| p + 2);
                    value = Some(Val::Quoted(self.s.get(k..e).unwrap_or("")));
                    after = e;
                }
                Some(_) => {
                    let mut e = k;
                    while e < len && !is_ws(self.b[e]) && self.b[e] != b'>' {
                        e += 1;
                    }
                    value = Some(Val::Unquoted(self.s.get(k..e).unwrap_or("")));
                    after = e;
                }
                None => after = len,
            }
        }
        match self.attr_action(raw_name, value.as_ref()) {
            Action::Keep => self.o.copy(j, after),
            Action::Drop => {}
            Action::Set(name, Some(v)) => {
                let text = format!("{name}=\"{}\"", v.replace('"', "&quot;"));
                self.o.put(&text, j);
            }
            Action::Set(name, None) => self.o.put(&name, j),
        }
        after.max(j + 1)
    }

    fn attr_action(&mut self, name: &str, value: Option<&Val<'_>>) -> Action {
        match self.flavor {
            Flavor::Jsx | Flavor::Astro | Flavor::Svelte => {
                let mapped = if self.flavor == Flavor::Svelte {
                    name
                } else {
                    jsx_name(name)
                };
                match value {
                    Some(Val::Expr(e)) => Action::Set(mapped.to_string(), Some(expr_value(e))),
                    Some(Val::Quoted(v)) if self.flavor == Flavor::Svelte && v.contains('{') => {
                        Action::Set(mapped.to_string(), Some(self.interpolate(v)))
                    }
                    Some(Val::Quoted(v) | Val::Unquoted(v)) if mapped != name => {
                        Action::Set(mapped.to_string(), Some((*v).to_string()))
                    }
                    None if mapped != name => Action::Set(mapped.to_string(), None),
                    _ => Action::Keep,
                }
            }
            Flavor::Vue => self.vue_action(name, value),
            Flavor::Twig | Flavor::Liquid => match value {
                Some(Val::Quoted(v) | Val::Unquoted(v))
                    if v.contains("{{") || v.contains("{%") || v.contains("{#") =>
                {
                    Action::Set(name.to_string(), Some(self.interpolate(v)))
                }
                _ => Action::Keep,
            },
            Flavor::Html => Action::Keep,
        }
    }

    fn vue_action(&mut self, name: &str, value: Option<&Val<'_>>) -> Action {
        let text = match value {
            Some(Val::Quoted(v) | Val::Unquoted(v) | Val::Expr(v)) => Some(*v),
            None => None,
        };
        let bound = name
            .strip_prefix(':')
            .or_else(|| name.strip_prefix("v-bind:"));
        if let Some(arg) = bound {
            let arg = arg.split('.').next().unwrap_or("");
            if arg.is_empty() || arg.starts_with('[') {
                return Action::Set(SPREAD_ATTR.to_string(), None);
            }
            return Action::Set(
                arg.to_string(),
                Some(text.map_or_else(|| MARK.to_string(), expr_value)),
            );
        }
        match name {
            "v-bind" => Action::Set(SPREAD_ATTR.to_string(), None),
            "v-if" | "v-else-if" | "v-else" | "v-for" => {
                self.o.conditional = true;
                Action::Drop
            }
            n if n.starts_with('@') || n.starts_with("v-") || n.starts_with('#') => Action::Drop,
            _ => Action::Keep,
        }
    }

    /// Replaces interpolations inside a quoted attribute value with [`MARK`].
    fn interpolate(&mut self, v: &str) -> String {
        let mut out = String::with_capacity(v.len());
        let mut rest = v;
        loop {
            let hit = match self.flavor {
                Flavor::Svelte => rest.find('{').map(|p| (p, "{", "}")),
                _ => ["{{", "{%", "{#"]
                    .iter()
                    .filter_map(|o| rest.find(o).map(|p| (p, *o, "")))
                    .min_by_key(|h| h.0),
            };
            let Some((p, open, _)) = hit else {
                out.push_str(rest);
                return out;
            };
            out.push_str(&rest[..p]);
            let after_open = &rest[p + open.len()..];
            let (close, drop) = match open {
                "{%" => ("%}", true),
                "{#" => ("#}", true),
                "{{" => ("}}", false),
                _ => ("}", false),
            };
            let end = after_open.find(close);
            if drop {
                let inner = &after_open[..end.unwrap_or(after_open.len())];
                let word: String = inner
                    .trim_start_matches(|c: char| c == '-' || c == '~' || c.is_whitespace())
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if open == "{%" && is_conditional_word(&word) {
                    self.o.conditional = true;
                }
            } else {
                out.push(MARK);
            }
            match end {
                Some(e) => rest = &after_open[e + close.len()..],
                None => return out,
            }
        }
    }
}

fn is_conditional_word(w: &str) -> bool {
    matches!(
        w,
        "if" | "elseif"
            | "elsif"
            | "else"
            | "unless"
            | "for"
            | "case"
            | "when"
            | "endif"
            | "endfor"
            | "endunless"
            | "endcase"
    )
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn jsx_name(name: &str) -> &str {
    match name {
        "className" => "class",
        "htmlFor" => "for",
        "httpEquiv" => "http-equiv",
        "acceptCharset" => "accept-charset",
        other => other,
    }
}

/// The attribute value an expression stands for: its literal text when the
/// expression is a plain string, otherwise [`MARK`].
fn expr_value(expr: &str) -> String {
    let e = expr.trim();
    let b = e.as_bytes();
    if b.len() >= 2 {
        let q = b[0];
        if matches!(q, b'"' | b'\'' | b'`') && b[b.len() - 1] == q {
            let inner = &e[1..e.len() - 1];
            let dynamic = inner.contains(q as char)
                || inner.contains('\\')
                || (q == b'`' && inner.contains("${"));
            if !dynamic {
                return inner.to_string();
            }
        }
    }
    MARK.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, f: Flavor) -> Preprocessed {
        preprocess(text, f)
    }

    #[test]
    fn extension_table() {
        for (e, f) in [
            ("html", Flavor::Html),
            ("HTM", Flavor::Html),
            ("tsx", Flavor::Jsx),
            ("jsx", Flavor::Jsx),
            ("vue", Flavor::Vue),
            ("svelte", Flavor::Svelte),
            ("astro", Flavor::Astro),
            ("twig", Flavor::Twig),
            ("liquid", Flavor::Liquid),
        ] {
            assert_eq!(Flavor::from_extension(e), Some(f), "{e}");
        }
        assert_eq!(Flavor::from_extension("rs"), None);
    }

    #[test]
    fn map_covers_every_output_byte() {
        let src = "<div className=\"a\" {...p}>{x}<Foo/>é</div>";
        let p = run(src, Flavor::Jsx);
        assert_eq!(p.map.len(), p.html.len());
        assert!(p.map.iter().all(|m| (*m as usize) <= src.len()));
    }

    #[test]
    fn jsx_attribute_and_text_expressions() {
        let p = run(
            "<p><img alt={a} className=\"x\" htmlFor={\"lit\"} {...rest}/>{n}</p>",
            Flavor::Jsx,
        );
        assert!(p.html.contains("alt=\"\u{1}\""), "{}", p.html);
        assert!(p.html.contains("class=\"x\""));
        assert!(p.html.contains("for=\"lit\""));
        assert!(p.html.contains(SPREAD_ATTR));
        assert!(p.html.ends_with("\u{1}</p>"));
    }

    #[test]
    fn jsx_components_fragments_and_generics() {
        let p = run(
            "const x = useState<string>('a');\nreturn (<><Foo.Bar a=\"1\"><b>t</b></Foo.Bar></>);",
            Flavor::Jsx,
        );
        assert!(p.html.contains("<dl-fragment>"));
        assert!(p.html.contains("</dl-fragment>"));
        assert!(p
            .html
            .contains("<dl-component data-dl-component=\"Foo.Bar\" a=\"1\">"));
        assert!(p.html.contains("</dl-component>"));
        assert!(!p.html.contains("<string"), "{}", p.html);
    }

    #[test]
    fn self_closing_elements_get_an_end_tag_but_voids_do_not() {
        let p = run(
            "<p><Icon name=\"a\"/><div /><img alt=\"x\"/></p>",
            Flavor::Jsx,
        );
        assert!(p.html.contains("></dl-component>"), "{}", p.html);
        assert!(p.html.contains("></div>"), "{}", p.html);
        assert!(p.html.contains("<img alt=\"x\"/>"), "{}", p.html);
    }

    #[test]
    fn jsx_conditional_markup_inside_braces() {
        let p = run("<ul>{ok && <li>Hi</li>}</ul>", Flavor::Jsx);
        assert!(p.has_conditionals);
        assert!(p.html.contains("<li>Hi</li>"), "{}", p.html);
    }

    #[test]
    fn vue_template_script_and_directives() {
        let src = "<script setup>\nconst a = '</div>'\n</script>\n<template>\n<img :alt=\"t\" v-if=\"ok\" @click=\"go\">\n</template>\n<style>a{}</style>";
        let p = run(src, Flavor::Vue);
        assert!(p.has_conditionals);
        assert!(p.html.contains("<dl-template>"));
        assert!(p.html.contains("alt=\"\u{1}\""));
        assert!(!p.html.contains("v-if"));
        assert!(!p.html.contains("const"));
        assert_eq!(p.html.matches('\n').count(), src.matches('\n').count());
    }

    #[test]
    fn svelte_blocks_and_shorthand() {
        let p = run(
            "{#each items as i}<img {alt}>{/each}<p>{name}</p><script>let x = 1;</script>",
            Flavor::Svelte,
        );
        assert!(p.has_conditionals);
        assert!(p.html.contains("alt=\"\u{1}\""));
        assert!(!p.html.contains("each"));
        assert!(!p.html.contains("let x"));
    }

    #[test]
    fn astro_frontmatter_is_masked() {
        let src = "---\nconst a = '<b>';\n---\n<Layout><h1>T</h1></Layout>\n";
        let p = run(src, Flavor::Astro);
        assert!(!p.html.contains("const"));
        assert!(p.html.contains("<h1>T</h1>"));
        assert!(p.html.contains("dl-component"));
    }

    #[test]
    fn twig_and_liquid_statements() {
        let t = run(
            "{# note #}{% if a %}<a href=\"{{ u }}\" {% if b %}x{% endif %}>t</a>{% endif %}",
            Flavor::Twig,
        );
        assert!(t.has_conditionals);
        assert!(t.html.contains("href=\"\u{1}\""));
        assert!(!t.html.contains("note"));
        let l = run(
            "{% comment %}<b>x</b>{% endcomment %}{% for i in l %}<li>{{ i }}</li>{% endfor %}",
            Flavor::Liquid,
        );
        assert!(l.has_conditionals);
        assert!(!l.html.contains("<b>"));
        assert!(l.html.contains("<li>\u{1}</li>"));
    }

    #[test]
    fn unterminated_input_does_not_panic() {
        for f in [
            Flavor::Jsx,
            Flavor::Vue,
            Flavor::Svelte,
            Flavor::Astro,
            Flavor::Twig,
            Flavor::Liquid,
        ] {
            for s in [
                "<a href={",
                "<a {...",
                "{#if",
                "{{",
                "{%",
                "<",
                "</",
                "<a b=\"",
                "<script>",
                "---\nx",
                "{`${`${",
                "<A.B c={`x",
            ] {
                let p = run(s, f);
                assert_eq!(p.map.len(), p.html.len(), "{s:?} {f:?}");
            }
        }
    }

    #[test]
    fn deep_brace_nesting_is_bounded() {
        let s = format!("<a b={}{}>", "{".repeat(5000), "}".repeat(5000));
        let _ = run(&s, Flavor::Jsx);
        let t = format!("{}x{}", "{".repeat(5000), "}".repeat(5000));
        let _ = run(&t, Flavor::Svelte);
    }
}
