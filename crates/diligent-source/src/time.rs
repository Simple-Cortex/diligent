//! Clock helpers: RFC 3339 timestamps and run identifiers without a date crate.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::sha256::sha256_hex;

/// Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Formats a Unix time in milliseconds as `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn format_unix_millis(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Current UTC time as an RFC 3339 string with millisecond precision.
pub fn now_rfc3339() -> String {
    format_unix_millis(now_millis())
}

/// A fresh run identifier matching `^run-\d{4}-\d{2}-\d{2}-[0-9a-f]{6}$`.
pub fn new_run_id(seed: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stamp = format_unix_millis(now_millis());
    let digest = sha256_hex(format!("{seed}|{now}").as_bytes());
    format!("run-{}-{}", &stamp[..10], &digest[..6])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_instants() {
        assert_eq!(format_unix_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            format_unix_millis(1_792_195_200_123),
            "2026-10-17T00:00:00.123Z"
        );
    }

    #[test]
    fn run_id_matches_schema_pattern() {
        let id = new_run_id("x");
        assert_eq!(id.len(), "run-2026-10-17-abcdef".len());
        assert!(id.starts_with("run-"));
        assert!(id[15..].chars().all(|c| c.is_ascii_hexdigit()));
    }
}
