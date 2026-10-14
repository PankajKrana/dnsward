//! Minimal time helpers (local calendar date, human-friendly durations)
//! without pulling in a date/time library.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Local calendar date (`YYYY-MM-DD`) for a Unix timestamp.
pub fn local_date(unix: u64) -> String {
    let t = unix as libc::time_t;
    // SAFETY: `localtime_r` writes into the provided `tm` and is thread-safe.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
    if !ok {
        return utc_date(unix);
    }
    format!(
        "{:04}-{:02}-{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday
    )
}

/// UTC calendar date, using the civil-from-days algorithm (H. Hinnant).
pub fn utc_date(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// "4 minutes ago", "2 hours ago", "never".
pub fn ago(then: Option<u64>, now: u64) -> String {
    let Some(then) = then else {
        return "never".into();
    };
    if then > now {
        return "just now".into();
    }
    let secs = now - then;
    let plural = |n: u64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match secs {
        0..=59 => "just now".into(),
        60..=3_599 => plural(secs / 60, "minute"),
        3_600..=86_399 => plural(secs / 3_600, "hour"),
        _ => plural(secs / 86_400, "day"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(951_782_400), "2000-02-29");
        assert_eq!(utc_date(1_790_000_000), "2026-09-21");
        assert_eq!(local_date(1_790_000_000).len(), 10);
    }

    #[test]
    fn durations() {
        assert_eq!(ago(None, 10), "never");
        assert_eq!(ago(Some(100), 100 + 240), "4 minutes ago");
        assert_eq!(ago(Some(0), 3_600), "1 hour ago");
    }
}
