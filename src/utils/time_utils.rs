//! Shared time utilities for providers.
//!
//! This module consolidates ISO 8601 parsing, epoch conversion, and
//! human-readable countdown formatting that were previously duplicated
//! across `gemini.rs`, `codex.rs`, and `kimi.rs`.

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use rust_i18n::t;

/// Parse an ISO 8601 timestamp (e.g. "2025-03-25T12:00:00Z" or with offset)
/// into Unix epoch seconds.  Returns `None` on malformed input.
///
/// When a timezone offset is present (e.g. `+08:00`), the result is the true
/// UTC epoch — i.e. `"2025-01-01T08:00:00+08:00"` equals `"2025-01-01T00:00:00Z"`.
/// Naive timestamps (no offset) are assumed to be UTC.
pub fn parse_iso8601_to_epoch(iso: &str) -> Option<i64> {
    // Try parsing with timezone info first (e.g. "2025-01-01T08:00:00+08:00" or "...Z")
    if let Ok(dt) = DateTime::<FixedOffset>::parse_from_rfc3339(iso) {
        return Some(dt.timestamp());
    }

    // Try as naive datetime without timezone suffix — assume UTC
    let formats = [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ];
    for fmt in &formats {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(iso, fmt) {
            return Some(ndt.and_utc().timestamp());
        }
    }

    None
}

/// Convert Unix epoch seconds to an ISO 8601 UTC string (e.g. `"2025-01-01T00:00:00.000Z"`).
pub fn epoch_to_iso8601(epoch: u64) -> String {
    chrono::DateTime::from_timestamp(epoch as i64, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
        .unwrap_or_default()
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
        return t!("time.resets_soon").to_string();
    }

    let days = delta_secs / 86400;
    let hours = (delta_secs % 86400) / 3600;
    let mins = (delta_secs % 3600) / 60;

    if days > 0 {
        if hours > 0 {
            t!("time.resets_in_days_hours", d = days, h = hours).to_string()
        } else {
            t!("time.resets_in_days", d = days).to_string()
        }
    } else if hours > 0 {
        if mins > 0 {
            t!("time.resets_in_hours_mins", h = hours, m = mins).to_string()
        } else {
            t!("time.resets_in_hours", h = hours).to_string()
        }
    } else {
        t!("time.resets_in_mins", m = mins.max(1)).to_string()
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
        // "2025-01-01T08:00:00+08:00" is midnight UTC → same as "2025-01-01T00:00:00Z"
        let a = parse_iso8601_to_epoch("2025-01-01T08:00:00+08:00").unwrap();
        let b = parse_iso8601_to_epoch("2025-01-01T00:00:00Z").unwrap();
        assert_eq!(a, b);
        assert_eq!(a, 1735689600); // 2025-01-01T00:00:00Z
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
        let _locale_guard = crate::i18n::test_locale_guard("en");
        assert_eq!(format_countdown(0), "⏱ Resets soon");
        assert_eq!(format_countdown(-100), "⏱ Resets soon");
        assert_eq!(format_countdown(60), "⏱ Resets in 1m");
        assert_eq!(format_countdown(3661), "⏱ Resets in 1h 1m");
        assert_eq!(format_countdown(3600), "⏱ Resets in 1h");
        assert_eq!(format_countdown(90000), "⏱ Resets in 1d 1h");
        assert_eq!(format_countdown(86400), "⏱ Resets in 1d");
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

    #[test]
    fn test_epoch_to_iso8601() {
        assert_eq!(epoch_to_iso8601(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(epoch_to_iso8601(1735689600), "2025-01-01T00:00:00.000Z");
    }

    #[test]
    fn test_epoch_round_trip() {
        let epoch: u64 = 1735689600;
        let iso = epoch_to_iso8601(epoch);
        let parsed = parse_iso8601_to_epoch(&iso).unwrap();
        assert_eq!(parsed, epoch as i64);
    }

    #[test]
    fn test_parse_iso8601_naive_without_tz() {
        // No timezone suffix — assumed UTC
        let epoch = parse_iso8601_to_epoch("2025-01-01T00:00:00").unwrap();
        assert_eq!(epoch, 1735689600);
    }

    #[test]
    fn test_parse_iso8601_minute_precision() {
        let epoch = parse_iso8601_to_epoch("2025-01-01T00:00").unwrap();
        assert_eq!(epoch, 1735689600);
    }
}
