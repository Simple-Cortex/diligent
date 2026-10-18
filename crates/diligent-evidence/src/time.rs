//! RFC 3339 timestamps without a date-time dependency.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current time as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix_seconds(secs as i64)
}

/// Formats Unix seconds as an RFC 3339 UTC timestamp.
pub fn format_unix_seconds(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
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

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parses an RFC 3339 timestamp into `(unix seconds, nanoseconds)`.
///
/// Accepts `Z` and `+hh:mm` / `-hh:mm` offsets and a fractional second.
/// Returns `None` for anything else, including impossible dates.
pub fn parse_rfc3339(text: &str) -> Option<(i64, u32)> {
    let b = text.as_bytes();
    if b.len() < 20 || !text.is_ascii() {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        let part = &text[from..to];
        if part.bytes().all(|c| c.is_ascii_digit()) {
            part.parse().ok()
        } else {
            None
        }
    };
    if b[4] != b'-'
        || b[7] != b'-'
        || !(b[10] == b'T' || b[10] == b't')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut index = 19;
    let mut nanos: u32 = 0;
    if b[index] == b'.' {
        let start = index + 1;
        let mut end = start;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
        if end == start {
            return None;
        }
        let mut digits = text[start..end].to_string();
        digits.truncate(9);
        while digits.len() < 9 {
            digits.push('0');
        }
        nanos = digits.parse().ok()?;
        index = end;
    }
    let offset_seconds = match &text[index..] {
        "Z" | "z" => 0,
        rest if rest.len() == 6
            && (rest.starts_with('+') || rest.starts_with('-'))
            && &rest[3..4] == ":" =>
        {
            let hh: i64 = rest[1..3].parse().ok()?;
            let mm: i64 = rest[4..6].parse().ok()?;
            if hh > 23 || mm > 59 {
                return None;
            }
            let total = hh * 3600 + mm * 60;
            if rest.starts_with('-') {
                -total
            } else {
                total
            }
        }
        _ => return None,
    };
    let days = days_from_civil(year, month, day);
    // Reject 31 February and the like by round-tripping the date.
    if !format_unix_seconds(days * 86_400).starts_with(&text[0..10]) {
        return None;
    }
    Some((
        days * 86_400 + hour * 3600 + minute * 60 + second - offset_seconds,
        nanos,
    ))
}
