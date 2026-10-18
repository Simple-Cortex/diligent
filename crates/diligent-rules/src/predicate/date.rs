//! Calendar dates without a date crate: strict `YYYY-MM-DD` parsing and UTC clocks.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// A proleptic Gregorian calendar date. Field order makes the derived
/// ordering chronological.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub y: i32,
    pub m: u8,
    pub d: u8,
}

impl Date {
    /// Parse exactly `YYYY-MM-DD` (four-digit year, valid calendar day).
    pub fn parse(text: &str) -> Option<Date> {
        let b = text.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let digits = |range: std::ops::Range<usize>| -> Option<u32> {
            let mut n = 0u32;
            for &c in &b[range] {
                if !c.is_ascii_digit() {
                    return None;
                }
                n = n * 10 + u32::from(c - b'0');
            }
            Some(n)
        };
        let y = digits(0..4)? as i32;
        let m = digits(5..7)? as u8;
        let d = digits(8..10)? as u8;
        if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
            return None;
        }
        Some(Date { y, m, d })
    }

    /// Today's date in UTC, from the system clock.
    pub fn today_utc() -> Date {
        let (date, _) = utc_now_parts();
        date
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.y, self.m, self.d)
    }
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i32, m: u8) -> u8 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

/// Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> Date {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let y = if m <= 2 { y + 1 } else { y };
    Date { y: y as i32, m, d }
}

fn utc_now_parts() -> (Date, (u8, u8, u8)) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    (
        civil_from_days(days),
        (
            (rem / 3600) as u8,
            ((rem % 3600) / 60) as u8,
            (rem % 60) as u8,
        ),
    )
}

/// The current UTC time as RFC 3339 with a `Z` suffix, second precision.
pub fn rfc3339_now() -> String {
    let (date, (h, mi, s)) = utc_now_parts();
    format!("{date}T{h:02}:{mi:02}:{s:02}Z")
}

/// Shape check for an RFC 3339 date-time: `YYYY-MM-DDTHH:MM:SS[.frac](Z|+HH:MM|-HH:MM)`.
pub fn is_rfc3339(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() < 20 || Date::parse(&text[..10]).is_none() || !matches!(b[10], b'T' | b't') {
        return false;
    }
    let num = |i: usize, max: u8| -> bool {
        b.len() > i + 1
            && b[i].is_ascii_digit()
            && b[i + 1].is_ascii_digit()
            && (b[i] - b'0') * 10 + (b[i + 1] - b'0') <= max
    };
    if !(num(11, 23) && b[13] == b':' && num(14, 59) && b[16] == b':' && num(17, 60)) {
        return false;
    }
    let mut i = 19;
    if b[i] == b'.' {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    match b.get(i) {
        Some(b'Z') | Some(b'z') => i + 1 == b.len(),
        Some(b'+') | Some(b'-') => {
            b.len() == i + 6 && num(i + 1, 23) && b[i + 3] == b':' && num(i + 4, 59)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_and_rejects_invalid() {
        assert_eq!(
            Date::parse("2026-10-17"),
            Some(Date {
                y: 2026,
                m: 10,
                d: 17
            })
        );
        assert!(Date::parse("2024-02-29").is_some());
        assert!(Date::parse("2025-02-29").is_none());
        assert!(Date::parse("2026-13-01").is_none());
        assert!(Date::parse("2026-1-01").is_none());
        assert!(Date::parse("soon").is_none());
        assert!(Date::parse("2026-10-17T00:00:00Z").is_none());
    }

    #[test]
    fn orders_chronologically() {
        assert!(Date::parse("2026-01-31") < Date::parse("2026-02-01"));
        assert!(Date::parse("2025-12-31") < Date::parse("2026-01-01"));
    }

    #[test]
    fn civil_conversion_matches_known_dates() {
        assert_eq!(civil_from_days(0).to_string(), "1970-01-01");
        assert_eq!(civil_from_days(19_723).to_string(), "2024-01-01");
        assert_eq!(civil_from_days(11_016).to_string(), "2000-02-29");
    }

    #[test]
    fn rfc3339_shape() {
        assert!(is_rfc3339("2026-10-17T12:00:00Z"));
        assert!(is_rfc3339("2026-10-17T12:00:00.123+05:30"));
        assert!(!is_rfc3339("2026-10-17"));
        assert!(!is_rfc3339("2026-10-17T12:00:00"));
        assert!(is_rfc3339(&rfc3339_now()));
    }
}
