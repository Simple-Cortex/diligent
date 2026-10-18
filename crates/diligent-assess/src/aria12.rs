//! Embedded WAI-ARIA 1.2 role, state and property tables.
//!
//! Source: WAI-ARIA 1.2, W3C Recommendation 6 June 2023,
//! <https://www.w3.org/TR/wai-aria-1.2/>, section 5 (characteristics of roles)
//! and section 6 (supported states and properties). Authored by hand from the
//! specification on 2026-10-18; no network was used.
//!
//! Additions beyond 1.2, all permissive so a newer-but-real page does not
//! fail: the Digital Publishing roles (`doc-*`) and Graphics roles
//! (`graphics-*`) accept any `aria-*` attribute; the ARIA 1.3 draft roles
//! `comment`, `mark` and `suggestion` and the 1.3 attributes
//! `aria-braillelabel`, `aria-brailleroledescription`, `aria-colindextext` and
//! `aria-rowindextext` are accepted as valid.
//!
//! Conservatism, because a false fail is worse than a miss:
//!
//! * Global attributes include the ones 1.2 deprecated as global
//!   (`aria-disabled`, `aria-errormessage`, `aria-haspopup`, `aria-invalid`,
//!   `aria-dropeffect`, `aria-grabbed`).
//! * Required states are limited to the ones where the requirement is clear
//!   and no host-language default exists: `aria-checked` on checkbox, radio,
//!   switch and the menu item checkbox and radio roles; `aria-expanded` on
//!   combobox; `aria-valuenow` on slider, meter and scrollbar; `aria-selected`
//!   on option. The 1.2 requirements for `aria-controls` on combobox and
//!   scrollbar, and `aria-level` on heading, are not enforced.
//! * UNSURE and therefore permitted: `aria-readonly`, `aria-required` and
//!   `aria-expanded` on the checkbox family; every state and property on the
//!   `doc-*`, `graphics-*` and 1.3 roles; `aria-activedescendant` on every
//!   composite role.
//! * The role table is the set of roles an author may use. Abstract roles are
//!   listed separately because using one is an authoring error.

/// One concrete role.
#[derive(Debug)]
pub struct RoleDef {
    /// Role token.
    pub name: &'static str,
    /// Supported states and properties beyond the global ones. `"*"` allows every attribute.
    pub props: &'static [&'static str],
    /// States and properties the role requires when no host-language default supplies them.
    pub required: &'static [&'static str],
}

/// Abstract roles. Authors must not use them.
pub const ABSTRACT_ROLES: [&str; 12] = [
    "command",
    "composite",
    "input",
    "landmark",
    "range",
    "roletype",
    "section",
    "sectionhead",
    "select",
    "structure",
    "widget",
    "window",
];

/// Roles whose children are presentational (ARIA 1.2 "Children Presentational: True").
pub const PRESENTATIONAL_CHILDREN_ROLES: [&str; 14] = [
    "button",
    "checkbox",
    "img",
    "math",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "progressbar",
    "radio",
    "scrollbar",
    "separator",
    "slider",
    "switch",
    "tab",
];

const ANY: &[&str] = &["*"];
const NONE: &[&str] = &[];

const fn role(
    name: &'static str,
    props: &'static [&'static str],
    required: &'static [&'static str],
) -> RoleDef {
    RoleDef {
        name,
        props,
        required,
    }
}

/// Every concrete role, with its supported non-global states and properties.
pub static ROLES: &[RoleDef] = &[
    role("alert", NONE, NONE),
    role("alertdialog", NONE, NONE),
    role(
        "application",
        &["aria-activedescendant", "aria-expanded"],
        NONE,
    ),
    role("article", &["aria-posinset", "aria-setsize"], NONE),
    role("banner", NONE, NONE),
    role("blockquote", NONE, NONE),
    role("button", &["aria-expanded", "aria-pressed"], NONE),
    role("caption", NONE, NONE),
    role(
        "cell",
        &[
            "aria-colindex",
            "aria-colspan",
            "aria-rowindex",
            "aria-rowspan",
        ],
        NONE,
    ),
    role(
        "checkbox",
        &[
            "aria-checked",
            "aria-expanded",
            "aria-readonly",
            "aria-required",
        ],
        &["aria-checked"],
    ),
    role("code", NONE, NONE),
    role(
        "columnheader",
        &[
            "aria-colindex",
            "aria-colspan",
            "aria-expanded",
            "aria-readonly",
            "aria-required",
            "aria-rowindex",
            "aria-rowspan",
            "aria-selected",
            "aria-sort",
        ],
        NONE,
    ),
    role(
        "combobox",
        &[
            "aria-activedescendant",
            "aria-autocomplete",
            "aria-expanded",
            "aria-readonly",
            "aria-required",
        ],
        &["aria-expanded"],
    ),
    role("complementary", NONE, NONE),
    role("contentinfo", NONE, NONE),
    role("definition", NONE, NONE),
    role("deletion", NONE, NONE),
    role("dialog", &["aria-modal"], NONE),
    role("directory", NONE, NONE),
    role("document", NONE, NONE),
    role("emphasis", NONE, NONE),
    role("feed", NONE, NONE),
    role("figure", NONE, NONE),
    role("form", NONE, NONE),
    role("generic", NONE, NONE),
    role(
        "grid",
        &[
            "aria-activedescendant",
            "aria-colcount",
            "aria-multiselectable",
            "aria-readonly",
            "aria-rowcount",
        ],
        NONE,
    ),
    role(
        "gridcell",
        &[
            "aria-colindex",
            "aria-colspan",
            "aria-expanded",
            "aria-readonly",
            "aria-required",
            "aria-rowindex",
            "aria-rowspan",
            "aria-selected",
        ],
        NONE,
    ),
    role("group", &["aria-activedescendant"], NONE),
    role("heading", &["aria-level"], NONE),
    role("img", NONE, NONE),
    role("insertion", NONE, NONE),
    role("link", &["aria-expanded"], NONE),
    role("list", NONE, NONE),
    role(
        "listbox",
        &[
            "aria-activedescendant",
            "aria-expanded",
            "aria-multiselectable",
            "aria-orientation",
            "aria-readonly",
            "aria-required",
        ],
        NONE,
    ),
    role(
        "listitem",
        &["aria-level", "aria-posinset", "aria-setsize"],
        NONE,
    ),
    role("log", NONE, NONE),
    role("main", NONE, NONE),
    role("marquee", NONE, NONE),
    role("math", NONE, NONE),
    role(
        "meter",
        &[
            "aria-valuemax",
            "aria-valuemin",
            "aria-valuenow",
            "aria-valuetext",
        ],
        &["aria-valuenow"],
    ),
    role("menu", &["aria-activedescendant", "aria-orientation"], NONE),
    role(
        "menubar",
        &["aria-activedescendant", "aria-orientation"],
        NONE,
    ),
    role(
        "menuitem",
        &["aria-expanded", "aria-posinset", "aria-setsize"],
        NONE,
    ),
    role(
        "menuitemcheckbox",
        &[
            "aria-checked",
            "aria-expanded",
            "aria-posinset",
            "aria-readonly",
            "aria-required",
            "aria-setsize",
        ],
        &["aria-checked"],
    ),
    role(
        "menuitemradio",
        &[
            "aria-checked",
            "aria-expanded",
            "aria-posinset",
            "aria-readonly",
            "aria-required",
            "aria-setsize",
        ],
        &["aria-checked"],
    ),
    role("navigation", NONE, NONE),
    role("none", NONE, NONE),
    role("note", NONE, NONE),
    role(
        "option",
        &[
            "aria-checked",
            "aria-posinset",
            "aria-selected",
            "aria-setsize",
        ],
        &["aria-selected"],
    ),
    role("paragraph", NONE, NONE),
    role("presentation", NONE, NONE),
    role(
        "progressbar",
        &[
            "aria-valuemax",
            "aria-valuemin",
            "aria-valuenow",
            "aria-valuetext",
        ],
        NONE,
    ),
    role(
        "radio",
        &["aria-checked", "aria-posinset", "aria-setsize"],
        &["aria-checked"],
    ),
    role(
        "radiogroup",
        &["aria-activedescendant", "aria-readonly", "aria-required"],
        NONE,
    ),
    role("region", NONE, NONE),
    role(
        "row",
        &[
            "aria-colindex",
            "aria-expanded",
            "aria-level",
            "aria-posinset",
            "aria-rowindex",
            "aria-selected",
            "aria-setsize",
        ],
        NONE,
    ),
    role("rowgroup", NONE, NONE),
    role(
        "rowheader",
        &[
            "aria-colindex",
            "aria-colspan",
            "aria-expanded",
            "aria-readonly",
            "aria-required",
            "aria-rowindex",
            "aria-rowspan",
            "aria-selected",
            "aria-sort",
        ],
        NONE,
    ),
    role(
        "scrollbar",
        &[
            "aria-controls",
            "aria-orientation",
            "aria-valuemax",
            "aria-valuemin",
            "aria-valuenow",
            "aria-valuetext",
        ],
        &["aria-valuenow"],
    ),
    role("search", NONE, NONE),
    role(
        "searchbox",
        &[
            "aria-activedescendant",
            "aria-autocomplete",
            "aria-multiline",
            "aria-placeholder",
            "aria-readonly",
            "aria-required",
        ],
        NONE,
    ),
    role(
        "separator",
        &[
            "aria-orientation",
            "aria-valuemax",
            "aria-valuemin",
            "aria-valuenow",
            "aria-valuetext",
        ],
        NONE,
    ),
    role(
        "slider",
        &[
            "aria-orientation",
            "aria-readonly",
            "aria-valuemax",
            "aria-valuemin",
            "aria-valuenow",
            "aria-valuetext",
        ],
        &["aria-valuenow"],
    ),
    role(
        "spinbutton",
        &[
            "aria-activedescendant",
            "aria-readonly",
            "aria-required",
            "aria-valuemax",
            "aria-valuemin",
            "aria-valuenow",
            "aria-valuetext",
        ],
        NONE,
    ),
    role("status", NONE, NONE),
    role("strong", NONE, NONE),
    role("subscript", NONE, NONE),
    role("superscript", NONE, NONE),
    role(
        "switch",
        &[
            "aria-checked",
            "aria-expanded",
            "aria-readonly",
            "aria-required",
        ],
        &["aria-checked"],
    ),
    role(
        "tab",
        &[
            "aria-expanded",
            "aria-posinset",
            "aria-selected",
            "aria-setsize",
        ],
        NONE,
    ),
    role("table", &["aria-colcount", "aria-rowcount"], NONE),
    role(
        "tablist",
        &[
            "aria-activedescendant",
            "aria-level",
            "aria-multiselectable",
            "aria-orientation",
        ],
        NONE,
    ),
    role("tabpanel", NONE, NONE),
    role("term", NONE, NONE),
    role(
        "textbox",
        &[
            "aria-activedescendant",
            "aria-autocomplete",
            "aria-multiline",
            "aria-placeholder",
            "aria-readonly",
            "aria-required",
        ],
        NONE,
    ),
    role("time", NONE, NONE),
    role("timer", NONE, NONE),
    role(
        "toolbar",
        &["aria-activedescendant", "aria-orientation"],
        NONE,
    ),
    role("tooltip", NONE, NONE),
    role(
        "tree",
        &[
            "aria-activedescendant",
            "aria-multiselectable",
            "aria-orientation",
            "aria-required",
        ],
        NONE,
    ),
    role(
        "treegrid",
        &[
            "aria-activedescendant",
            "aria-colcount",
            "aria-multiselectable",
            "aria-orientation",
            "aria-readonly",
            "aria-required",
            "aria-rowcount",
        ],
        NONE,
    ),
    role(
        "treeitem",
        &[
            "aria-checked",
            "aria-expanded",
            "aria-level",
            "aria-posinset",
            "aria-selected",
            "aria-setsize",
        ],
        NONE,
    ),
    // ARIA 1.3 draft roles, accepted permissively.
    role("comment", ANY, NONE),
    role("mark", ANY, NONE),
    role("suggestion", ANY, NONE),
    // Digital Publishing WAI-ARIA Module 1.1 roles.
    role("doc-abstract", ANY, NONE),
    role("doc-acknowledgments", ANY, NONE),
    role("doc-afterword", ANY, NONE),
    role("doc-appendix", ANY, NONE),
    role("doc-backlink", ANY, NONE),
    role("doc-biblioentry", ANY, NONE),
    role("doc-bibliography", ANY, NONE),
    role("doc-biblioref", ANY, NONE),
    role("doc-chapter", ANY, NONE),
    role("doc-colophon", ANY, NONE),
    role("doc-conclusion", ANY, NONE),
    role("doc-cover", ANY, NONE),
    role("doc-credit", ANY, NONE),
    role("doc-credits", ANY, NONE),
    role("doc-dedication", ANY, NONE),
    role("doc-endnote", ANY, NONE),
    role("doc-endnotes", ANY, NONE),
    role("doc-epigraph", ANY, NONE),
    role("doc-epilogue", ANY, NONE),
    role("doc-errata", ANY, NONE),
    role("doc-example", ANY, NONE),
    role("doc-footnote", ANY, NONE),
    role("doc-foreword", ANY, NONE),
    role("doc-glossary", ANY, NONE),
    role("doc-glossref", ANY, NONE),
    role("doc-index", ANY, NONE),
    role("doc-introduction", ANY, NONE),
    role("doc-noteref", ANY, NONE),
    role("doc-notice", ANY, NONE),
    role("doc-pagebreak", ANY, NONE),
    role("doc-pagelist", ANY, NONE),
    role("doc-part", ANY, NONE),
    role("doc-preface", ANY, NONE),
    role("doc-prologue", ANY, NONE),
    role("doc-pullquote", ANY, NONE),
    role("doc-qna", ANY, NONE),
    role("doc-subtitle", ANY, NONE),
    role("doc-tip", ANY, NONE),
    role("doc-toc", ANY, NONE),
    // Graphics WAI-ARIA Module 1.0 roles.
    role("graphics-document", ANY, NONE),
    role("graphics-object", ANY, NONE),
    role("graphics-symbol", ANY, NONE),
];

/// Looks up a concrete role by token (ASCII case-insensitive).
pub fn role_def(name: &str) -> Option<&'static RoleDef> {
    ROLES.iter().find(|r| r.name.eq_ignore_ascii_case(name))
}

/// True for abstract roles.
pub fn is_abstract_role(name: &str) -> bool {
    ABSTRACT_ROLES.iter().any(|r| r.eq_ignore_ascii_case(name))
}

/// The declared value type of a state or property.
#[derive(Debug)]
pub enum ValueType {
    /// `true` or `false`.
    TrueFalse,
    /// `true`, `false` or `undefined`.
    TrueFalseUndefined,
    /// `true`, `false`, `mixed` or `undefined`.
    Tristate,
    /// An integer.
    Integer,
    /// A real number.
    Number,
    /// One element id.
    IdRef,
    /// One or more element ids.
    IdRefList,
    /// Free text.
    Text,
    /// One token from the list.
    Token(&'static [&'static str]),
    /// One or more tokens from the list.
    TokenList(&'static [&'static str]),
}

/// One `aria-*` attribute.
#[derive(Debug)]
pub struct AttrDef {
    /// Attribute name.
    pub name: &'static str,
    /// Value type.
    pub kind: ValueType,
    /// True when every role supports it.
    pub global: bool,
}

const fn attr(name: &'static str, kind: ValueType, global: bool) -> AttrDef {
    AttrDef { name, kind, global }
}

/// Every `aria-*` state and property.
pub static ATTRS: &[AttrDef] = &[
    attr("aria-activedescendant", ValueType::IdRef, false),
    attr("aria-atomic", ValueType::TrueFalse, true),
    attr(
        "aria-autocomplete",
        ValueType::Token(&["inline", "list", "both", "none"]),
        false,
    ),
    attr("aria-braillelabel", ValueType::Text, true),
    attr("aria-brailleroledescription", ValueType::Text, true),
    attr("aria-busy", ValueType::TrueFalse, true),
    attr("aria-checked", ValueType::Tristate, false),
    attr("aria-colcount", ValueType::Integer, false),
    attr("aria-colindex", ValueType::Integer, false),
    attr("aria-colindextext", ValueType::Text, false),
    attr("aria-colspan", ValueType::Integer, false),
    attr("aria-controls", ValueType::IdRefList, true),
    attr(
        "aria-current",
        ValueType::Token(&["page", "step", "location", "date", "time", "true", "false"]),
        true,
    ),
    attr("aria-describedby", ValueType::IdRefList, true),
    attr("aria-description", ValueType::Text, true),
    attr("aria-details", ValueType::IdRefList, true),
    attr("aria-disabled", ValueType::TrueFalse, true),
    attr(
        "aria-dropeffect",
        ValueType::TokenList(&["copy", "execute", "link", "move", "none", "popup"]),
        true,
    ),
    attr("aria-errormessage", ValueType::IdRef, true),
    attr("aria-expanded", ValueType::TrueFalseUndefined, false),
    attr("aria-flowto", ValueType::IdRefList, true),
    attr("aria-grabbed", ValueType::TrueFalseUndefined, true),
    attr(
        "aria-haspopup",
        ValueType::Token(&["false", "true", "menu", "listbox", "tree", "grid", "dialog"]),
        true,
    ),
    attr("aria-hidden", ValueType::TrueFalseUndefined, true),
    attr(
        "aria-invalid",
        ValueType::Token(&["grammar", "false", "spelling", "true"]),
        true,
    ),
    attr("aria-keyshortcuts", ValueType::Text, true),
    attr("aria-label", ValueType::Text, true),
    attr("aria-labelledby", ValueType::IdRefList, true),
    attr("aria-level", ValueType::Integer, false),
    attr(
        "aria-live",
        ValueType::Token(&["assertive", "off", "polite"]),
        true,
    ),
    attr("aria-modal", ValueType::TrueFalse, false),
    attr("aria-multiline", ValueType::TrueFalse, false),
    attr("aria-multiselectable", ValueType::TrueFalse, false),
    attr(
        "aria-orientation",
        ValueType::Token(&["horizontal", "undefined", "vertical"]),
        false,
    ),
    attr("aria-owns", ValueType::IdRefList, true),
    attr("aria-placeholder", ValueType::Text, false),
    attr("aria-posinset", ValueType::Integer, false),
    attr("aria-pressed", ValueType::Tristate, false),
    attr("aria-readonly", ValueType::TrueFalse, false),
    attr(
        "aria-relevant",
        ValueType::TokenList(&["additions", "all", "removals", "text"]),
        true,
    ),
    attr("aria-required", ValueType::TrueFalse, false),
    attr("aria-roledescription", ValueType::Text, true),
    attr("aria-rowcount", ValueType::Integer, false),
    attr("aria-rowindex", ValueType::Integer, false),
    attr("aria-rowindextext", ValueType::Text, false),
    attr("aria-rowspan", ValueType::Integer, false),
    attr("aria-selected", ValueType::TrueFalseUndefined, false),
    attr("aria-setsize", ValueType::Integer, false),
    attr(
        "aria-sort",
        ValueType::Token(&["ascending", "descending", "none", "other"]),
        false,
    ),
    attr("aria-valuemax", ValueType::Number, false),
    attr("aria-valuemin", ValueType::Number, false),
    attr("aria-valuenow", ValueType::Number, false),
    attr("aria-valuetext", ValueType::Text, false),
];

/// Looks up an `aria-*` attribute (ASCII case-insensitive).
pub fn attr_def(name: &str) -> Option<&'static AttrDef> {
    ATTRS.iter().find(|a| a.name.eq_ignore_ascii_case(name))
}

/// True when `role` supports attribute `name`, either globally or as a specific property.
pub fn role_supports(role: &RoleDef, name: &str) -> bool {
    if attr_def(name).is_some_and(|a| a.global) {
        return true;
    }
    role.props
        .iter()
        .any(|p| *p == "*" || p.eq_ignore_ascii_case(name))
}

fn is_integer(s: &str) -> bool {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit())
}

fn is_number(s: &str) -> bool {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    if body.is_empty()
        || !body
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'))
    {
        return false;
    }
    body.bytes().any(|b| b.is_ascii_digit()) && body.parse::<f64>().is_ok_and(f64::is_finite)
}

/// Checks `value` against `kind`. `id_exists` answers whether an element id is in the snapshot.
///
/// An empty value is accepted for every type except integer and number, where
/// the spec gives it no meaning. Token comparison is ASCII case-insensitive
/// and ignores surrounding whitespace.
pub fn validate_value(
    value: &str,
    kind: &ValueType,
    id_exists: &dyn Fn(&str) -> bool,
) -> Result<(), String> {
    let v = value.trim();
    let lower = v.to_ascii_lowercase();
    match kind {
        ValueType::Integer => {
            if is_integer(v) {
                Ok(())
            } else {
                Err(format!("`{value}` is not an integer"))
            }
        }
        ValueType::Number => {
            if is_number(v) {
                Ok(())
            } else {
                Err(format!("`{value}` is not a number"))
            }
        }
        _ if v.is_empty() => Ok(()),
        ValueType::TrueFalse => match lower.as_str() {
            "true" | "false" => Ok(()),
            _ => Err(format!("`{value}` is not true or false")),
        },
        ValueType::TrueFalseUndefined => match lower.as_str() {
            "true" | "false" | "undefined" => Ok(()),
            _ => Err(format!("`{value}` is not true, false or undefined")),
        },
        ValueType::Tristate => match lower.as_str() {
            "true" | "false" | "mixed" | "undefined" => Ok(()),
            _ => Err(format!("`{value}` is not true, false, mixed or undefined")),
        },
        ValueType::IdRef => {
            if v.split_ascii_whitespace().count() == 1 && id_exists(v) {
                Ok(())
            } else {
                Err(format!(
                    "`{value}` is not the id of one element in the document"
                ))
            }
        }
        ValueType::IdRefList => {
            if v.split_ascii_whitespace().any(id_exists) {
                Ok(())
            } else {
                Err(format!("no element in the document has an id in `{value}`"))
            }
        }
        ValueType::Text => Ok(()),
        ValueType::Token(allowed) => {
            if allowed.contains(&lower.as_str()) {
                Ok(())
            } else {
                Err(format!("`{value}` is not one of {}", allowed.join(", ")))
            }
        }
        ValueType::TokenList(allowed) => {
            if lower.split_ascii_whitespace().all(|t| allowed.contains(&t)) {
                Ok(())
            } else {
                Err(format!(
                    "`{value}` has a token outside {}",
                    allowed.join(", ")
                ))
            }
        }
    }
}

/// The first token of a `role` attribute that is a concrete role, if any.
pub fn first_valid_role(value: &str) -> Option<&'static RoleDef> {
    value.split_ascii_whitespace().find_map(role_def)
}

/// The implicit ARIA role of an HTML element, or `None` when it depends on
/// context this module does not model (a `header` may be a banner or generic,
/// a `section` a region or generic) or the element has no role.
///
/// `attr` reads an attribute of the element.
pub fn implicit_role(tag: &str, attr: &dyn Fn(&str) -> Option<String>) -> Option<&'static str> {
    let role = match tag {
        "a" | "area" => {
            if attr("href").is_some() {
                "link"
            } else {
                "generic"
            }
        }
        "article" => "article",
        "aside" => "complementary",
        "blockquote" => "blockquote",
        "button" => "button",
        "caption" => "caption",
        "code" => "code",
        "datalist" => "listbox",
        "dd" => "definition",
        "del" => "deletion",
        "address" | "details" | "fieldset" | "optgroup" => "group",
        "dfn" | "dt" => "term",
        "dialog" => "dialog",
        "em" => "emphasis",
        "figure" => "figure",
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
        "hr" => "separator",
        "html" => "document",
        "img" => match attr("alt") {
            Some(alt) if alt.is_empty() => return None,
            _ => "img",
        },
        "input" => {
            let kind = attr("type").unwrap_or_default().trim().to_ascii_lowercase();
            match kind.as_str() {
                "checkbox" => "checkbox",
                "radio" => "radio",
                "button" | "submit" | "reset" | "image" => "button",
                "range" => "slider",
                "number" => "spinbutton",
                "search" => {
                    if attr("list").is_some() {
                        return None;
                    }
                    "searchbox"
                }
                "" | "text" | "email" | "tel" | "url" => {
                    if attr("list").is_some() {
                        "combobox"
                    } else {
                        "textbox"
                    }
                }
                _ => return None,
            }
        }
        "ins" => "insertion",
        "li" => "listitem",
        "main" => "main",
        "math" => "math",
        "menu" | "ol" | "ul" => "list",
        "meter" => "meter",
        "nav" => "navigation",
        "option" => "option",
        "output" => "status",
        "p" => "paragraph",
        "progress" => "progressbar",
        "select" => {
            let multiple = attr("multiple").is_some();
            let size = attr("size")
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(0);
            if multiple || size > 1 {
                "listbox"
            } else {
                "combobox"
            }
        }
        "strong" => "strong",
        "sub" => "subscript",
        "sup" => "superscript",
        "table" => "table",
        "tbody" | "thead" | "tfoot" => "rowgroup",
        "textarea" => "textbox",
        "time" => "time",
        "tr" => "row",
        "abbr" | "b" | "bdi" | "bdo" | "body" | "cite" | "data" | "div" | "i" | "kbd" | "label"
        | "legend" | "pre" | "q" | "s" | "samp" | "small" | "span" | "u" | "var" => "generic",
        _ => return None,
    };
    Some(role)
}
