//! Small helpers: content hashing, UTC timestamps and origin parsing.

use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest.iter() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Milliseconds since the Unix epoch.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Format Unix milliseconds as an RFC 3339 UTC timestamp.
pub fn rfc3339_from_millis(ms: u64) -> String {
    let secs = ms / 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil-from-days algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Current time as an RFC 3339 UTC timestamp.
pub fn now_rfc3339() -> String {
    rfc3339_from_millis(now_millis())
}

/// WHATWG origin (`scheme://host[:port]`, host ASCII-serialised, default port
/// dropped) of an absolute http or https URL. `None` for any other scheme and
/// for URLs carrying userinfo, backslashes or ASCII control characters.
/// Origins are compared by this serialisation.
pub fn origin_of(url: &str) -> Option<String> {
    if url.chars().any(|c| c == '\\' || c.is_ascii_control()) {
        return None;
    }
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return None;
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    parsed.host_str()?;
    let origin = parsed.origin();
    if !origin.is_tuple() {
        return None;
    }
    Some(origin.ascii_serialization())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn rfc3339_epoch_and_leap() {
        assert_eq!(rfc3339_from_millis(0), "1970-01-01T00:00:00Z");
        assert_eq!(
            rfc3339_from_millis(1_709_164_800_000),
            "2024-02-29T00:00:00Z"
        );
    }

    #[test]
    fn origins_normalize() {
        assert_eq!(
            origin_of("HTTP://Example.com:80/a?b").as_deref(),
            Some("http://example.com")
        );
        assert_eq!(
            origin_of("https://x.org:8443/").as_deref(),
            Some("https://x.org:8443")
        );
        assert_eq!(origin_of("http://u:p@h.test/x"), None);
        assert_eq!(origin_of("file:///etc/passwd"), None);
        assert_eq!(origin_of("javascript:alert(1)"), None);
    }
}
