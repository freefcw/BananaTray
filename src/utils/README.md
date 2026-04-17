# src/utils/

Shared utility modules used across the application. **No GPUI dependency.**

## Modules

### `text_utils.rs` — Text Processing

- `strip_ansi(text)` — removes ANSI escape sequences (CSI and OSC) via regex

### `time_utils.rs` — Time Parsing and Formatting

Shared time utilities consolidating logic previously duplicated across providers:
- `parse_iso8601_to_epoch(iso)` — parses ISO 8601 timestamps (with/without timezone, with fractional seconds) to Unix epoch seconds
- `epoch_to_iso8601(epoch)` — converts epoch to ISO 8601 UTC string
- `now_epoch_secs()` — current time as epoch seconds
- `format_countdown(delta_secs)` — human-readable countdown (e.g. "Resets in 2d 5h", "Resets in 45m", "Resets soon")
- `format_reset_from_epoch(epoch)` — countdown from epoch seconds
- `is_expired_epoch_secs(expiry)` — token expiry check
- `is_older_than(iso, max_age_secs)` — staleness check

### `log_capture.rs` — Debug Log Capture

Ring buffer for captured log entries, used by the debug panel to show recent log activity.

## Constraints

- These modules are used from background threads — all functions are synchronous and thread-safe.

## Moved Modules

The following modules have been relocated:
- `http_client.rs` → `src/providers/common/http_client.rs`
- `interactive_runner.rs` → `src/providers/common/runner.rs`
- `platform.rs` → `src/platform/system.rs`
