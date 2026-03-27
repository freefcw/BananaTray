//! Shared time utilities for providers.
//!
//! This module consolidates ISO 8601 parsing, epoch conversion, and
//! human-readable countdown formatting that were previously duplicated
//! across `gemini.rs`, `codex.rs`, and `kimi.rs`.

/// Parse an ISO 8601 timestamp (e.g. "2025-03-25T12:00:00Z" or with offset)
/// into Unix epoch seconds.  Returns `None` on malformed input.
pub fn parse_iso8601_to_epoch(iso: &str) -> Option<i64> {
    // Strip trailing 'Z'
    let clean = iso.trim_end_matches('Z');

    // Strip timezone offset like +08:00 / -05:00 (only after the date part)
    let clean = if let Some(pos) = clean.rfind('+') {
        if pos > 10 {
            &clean[..pos]
        } else {
            clean
        }
    } else if let Some(pos) = clean.rfind('-') {
        if pos > 10 {
            &clean[..pos]
        } else {
            clean
        }
    } else {
        clean
    };

    // Strip fractional seconds (e.g. ".000")
    let clean = if let Some(dot_pos) = clean.rfind('.') {
        if dot_pos > 10 {
            &clean[..dot_pos]
        } else {
            clean
        }
    } else {
        clean
    };

    let parts: Vec<&str> = clean.split('T').collect();
    if parts.len() != 2 {
        return None;
    }

    let date_parts: Vec<&str> = parts[0].split('-').collect();
    let time_parts: Vec<&str> = parts[1].split(':').collect();
    if date_parts.len() != 3 || time_parts.len() < 2 {
        return None;
    }

    let year: i64 = date_parts[0].parse().ok()?;
    let month: i64 = date_parts[1].parse().ok()?;
    let day: i64 = date_parts[2].parse().ok()?;
    let hour: i64 = time_parts[0].parse().ok()?;
    let min: i64 = time_parts[1].parse().ok()?;
    let sec: i64 = time_parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    let days_in_month = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut total_days: i64 = 0;

    for y in 1970..year {
        total_days += if is_leap_year(y) { 366 } else { 365 };
    }

    let leap = is_leap_year(year);
    for m in 1..month {
        total_days += days_in_month[m as usize];
        if m == 2 && leap {
            total_days += 1;
        }
    }
    total_days += day - 1;

    Some(total_days * 86400 + hour * 3600 + min * 60 + sec)
}

fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Return the current time as Unix epoch seconds.
pub fn now_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Format a duration (in seconds) as a human-readable countdown string.
///
/// Examples: "Resets in 2d 5h", "Resets in 3h 12m", "Resets in 45m", "Resets soon".
pub fn format_countdown(delta_secs: i64) -> String {
    if delta_secs <= 0 {
        return "Resets soon".to_string();
    }

    let days = delta_secs / 86400;
    let hours = (delta_secs % 86400) / 3600;
    let mins = (delta_secs % 3600) / 60;

    if days > 0 {
        if hours > 0 {
            format!("Resets in {}d {}h", days, hours)
        } else {
            format!("Resets in {}d", days)
        }
    } else if hours > 0 {
        if mins > 0 {
            format!("Resets in {}h {}m", hours, mins)
        } else {
            format!("Resets in {}h", hours)
        }
    } else {
        format!("Resets in {}m", mins.max(1))
    }
}

/// Parse an ISO 8601 timestamp and return a human-readable countdown string.
///
/// This is the primary entry point used by most providers.
pub fn format_reset_countdown(iso: &str) -> Option<String> {
    let reset_epoch = parse_iso8601_to_epoch(iso)?;
    let delta = reset_epoch - now_epoch_secs();
    Some(format_countdown(delta))
}

/// Convert a Unix timestamp (seconds) to a human-readable countdown string.
pub fn format_reset_from_epoch(epoch_secs: i64) -> String {
    let delta = epoch_secs - now_epoch_secs();
    format_countdown(delta)
}

/// Check if a token with the given expiry (epoch seconds as f64) has expired.
pub fn is_expired_epoch_secs(expiry_secs: f64) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    expiry_secs < now
}

/// Check if an ISO 8601 timestamp is older than the given number of seconds.
///
/// Used by Codex to detect tokens older than 8 days.
pub fn is_older_than(iso: &str, max_age_secs: i64) -> bool {
    let Some(then) = parse_iso8601_to_epoch(iso) else {
        return true; // Can't parse → assume stale
    };
    let now = now_epoch_secs();
    (now - then) > max_age_secs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_iso8601_basic() {
        // 2025-01-01T00:00:00Z
        let epoch = parse_iso8601_to_epoch("2025-01-01T00:00:00Z").unwrap();
        // 2025-01-01 = 55 years from 1970
        assert!(epoch > 0);
        // Known: 2025-01-01T00:00:00Z = 1735689600
        assert_eq!(epoch, 1735689600);
    }

    #[test]
    fn test_parse_iso8601_with_offset() {
        // Offset should be stripped; result is the naive datetime as-if UTC
        let a = parse_iso8601_to_epoch("2025-01-01T08:00:00+08:00").unwrap();
        let b = parse_iso8601_to_epoch("2025-01-01T08:00:00Z").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_parse_iso8601_with_fractional() {
        let a = parse_iso8601_to_epoch("2025-01-15T10:00:00.000Z").unwrap();
        let b = parse_iso8601_to_epoch("2025-01-15T10:00:00Z").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_parse_iso8601_invalid() {
        assert!(parse_iso8601_to_epoch("not-a-date").is_none());
        assert!(parse_iso8601_to_epoch("").is_none());
    }

    #[test]
    fn test_format_countdown_values() {
        assert_eq!(format_countdown(0), "Resets soon");
        assert_eq!(format_countdown(-100), "Resets soon");
        assert_eq!(format_countdown(60), "Resets in 1m");
        assert_eq!(format_countdown(3661), "Resets in 1h 1m");
        assert_eq!(format_countdown(3600), "Resets in 1h");
        assert_eq!(format_countdown(90000), "Resets in 1d 1h");
        assert_eq!(format_countdown(86400), "Resets in 1d");
    }

    #[test]
    fn test_is_older_than() {
        // A date far in the past should be "older than" 1 day
        assert!(is_older_than("2020-01-01T00:00:00Z", 86400));
    }

    #[test]
    fn test_is_expired() {
        // An expiry in the past
        assert!(is_expired_epoch_secs(1000.0));
        // An expiry far in the future
        assert!(!is_expired_epoch_secs(9999999999.0));
    }
}
