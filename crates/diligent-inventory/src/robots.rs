//! robots.txt: group selection for `*` and the `diligent` token, longest-match
//! Allow/Disallow per RFC 9309, and `Crawl-delay` as a rate hint.

use url::Url;

/// The product token the crawler matches robots groups against.
pub const AGENT: &str = "diligent";

const MAX_LINES: usize = 50_000;
const MAX_PATTERN: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    allow: bool,
    pattern: String,
}

/// Parsed rules for one crawler.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Robots {
    rules: Vec<Rule>,
    /// `Crawl-delay` in seconds from the selected groups (the largest value), if any.
    pub crawl_delay: Option<f64>,
    /// True when a group for the `diligent` token was used instead of `*`.
    pub specific_group: bool,
}

#[derive(Default)]
struct Group {
    agents: Vec<String>,
    rules: Vec<Rule>,
    delay: Option<f64>,
}

impl Robots {
    /// A file that allows everything (missing or unreadable robots.txt).
    pub fn allow_all() -> Self {
        Robots::default()
    }

    /// Parses `text` for crawler token `agent` (case-insensitive).
    pub fn parse(text: &str, agent: &str) -> Self {
        let agent = agent.to_ascii_lowercase();
        let mut groups: Vec<Group> = Vec::new();
        let mut in_agents = false;
        for raw in text.trim_start_matches('\u{feff}').lines().take(MAX_LINES) {
            let line = raw.split('#').next().unwrap_or("").trim();
            let Some((field, value)) = line.split_once(':') else {
                continue;
            };
            let field = field.trim().to_ascii_lowercase();
            let value = value.trim();
            match field.as_str() {
                "user-agent" => {
                    if !in_agents || groups.is_empty() {
                        groups.push(Group::default());
                    }
                    in_agents = true;
                    if let Some(g) = groups.last_mut() {
                        g.agents.push(value.to_ascii_lowercase());
                    }
                }
                "allow" | "disallow" => {
                    in_agents = false;
                    if let Some(g) = groups.last_mut() {
                        if !value.is_empty() && value.len() <= MAX_PATTERN {
                            g.rules.push(Rule {
                                allow: field == "allow",
                                pattern: value.to_string(),
                            });
                        }
                    }
                }
                "crawl-delay" => {
                    in_agents = false;
                    if let Some(g) = groups.last_mut() {
                        if let Ok(d) = value.parse::<f64>() {
                            if d.is_finite() && d > 0.0 {
                                g.delay = Some(g.delay.map_or(d, |old| old.max(d)));
                            }
                        }
                    }
                }
                _ => {
                    // sitemap and unknown fields do not end a group's agent list in practice,
                    // but they never add rules.
                }
            }
        }
        let specific: Vec<&Group> = groups
            .iter()
            .filter(|g| g.agents.contains(&agent))
            .collect();
        let (selected, specific_group) = if specific.is_empty() {
            (
                groups
                    .iter()
                    .filter(|g| g.agents.iter().any(|a| a == "*"))
                    .collect::<Vec<_>>(),
                false,
            )
        } else {
            (specific, true)
        };
        let mut rules = Vec::new();
        let mut delay: Option<f64> = None;
        for g in selected {
            rules.extend(g.rules.iter().cloned());
            if let Some(d) = g.delay {
                delay = Some(delay.map_or(d, |old: f64| old.max(d)));
            }
        }
        Robots {
            rules,
            crawl_delay: delay,
            specific_group,
        }
    }

    /// Whether the crawler may fetch `url` (path plus query is matched).
    pub fn allowed(&self, url: &Url) -> bool {
        let mut target = url.path().to_string();
        if let Some(q) = url.query() {
            target.push('?');
            target.push_str(q);
        }
        self.allowed_path(&target)
    }

    /// Whether the crawler may fetch the given path (and query).
    pub fn allowed_path(&self, target: &str) -> bool {
        let mut best: Option<&Rule> = None;
        for rule in &self.rules {
            if !matches(&rule.pattern, target) {
                continue;
            }
            best = match best {
                None => Some(rule),
                Some(b) if rule.pattern.len() > b.pattern.len() => Some(rule),
                Some(b) if rule.pattern.len() == b.pattern.len() && rule.allow && !b.allow => {
                    Some(rule)
                }
                keep => keep,
            };
        }
        best.is_none_or(|r| r.allow)
    }
}

/// Matches a robots pattern (`*` wildcard, trailing `$` anchor) as a prefix of `path`.
/// Linear in the sizes of both inputs.
pub fn matches(pattern: &str, path: &str) -> bool {
    let (pat, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 {
        return if anchored {
            path == parts[0]
        } else {
            path.starts_with(parts[0])
        };
    }
    if !path.starts_with(parts[0]) {
        return false;
    }
    let mut pos = parts[0].len();
    let last = parts.len() - 1;
    for (i, seg) in parts.iter().enumerate().skip(1) {
        if i == last && anchored {
            return path.len() >= pos + seg.len() && path.ends_with(seg);
        }
        if seg.is_empty() {
            continue;
        }
        match path[pos..].find(seg) {
            Some(k) => pos += k + seg.len(),
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_and_anchor() {
        assert!(matches("/private", "/private/x"));
        assert!(matches("/*.pdf$", "/a/b.pdf"));
        assert!(!matches("/*.pdf$", "/a/b.pdf?x=1"));
        assert!(matches("/a*b*c", "/aXbYc/z"));
        assert!(!matches("/a*b*c", "/aXcYb"));
    }
}
