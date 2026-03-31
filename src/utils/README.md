# src/utils/

Shared utility modules used by providers and other parts of the application. **No GPUI dependency.**

## Modules

### `http_client.rs` — HTTP Request Helpers

Thin wrapper around a shared `ureq::Agent` (lazy-initialized, connection-pooled):
- `get(url, headers)` — GET returning body string (errors on 4xx+)
- `get_with_headers(url, headers)` — GET returning raw HTTP response (status + headers + body)
- `get_with_status(url, headers)` — GET returning `(body, status_code)`
- `post_json(url, headers, body)` — POST with `Content-Type: application/json`
- `post_form(url, headers, body)` — POST with `Content-Type: application/x-www-form-urlencoded`

Headers are passed as raw strings (e.g. `"Authorization: Bearer xxx"`) and parsed via `split_once(':')`.

**Note**: The ureq agent is configured with `http_status_as_error(false)` so that 4xx/5xx responses are handled by the caller logic rather than ureq's default error behavior.

### `interactive_runner.rs` — PTY-based CLI Runner

Executes CLI commands in a pseudo-terminal for providers that require interactive sessions (Claude, Kiro):
- `InteractiveRunner::run(binary, input, options)` — spawns a process in a PTY, sends input, captures output
- `InteractiveOptions` — configurable timeout, idle timeout, working directory, arguments, auto-responses to prompts, environment exclusions
- Auto-response: maps prompt text to responses (normalized matching ignores whitespace/case)
- Output is cleaned via `text_utils::strip_ansi()`

Uses `which` crate for binary resolution with fallback to common paths (`/opt/homebrew/bin`, `/usr/local/bin`).

### `text_utils.rs` — Text Processing

- `strip_ansi(text)` — removes ANSI escape sequences (CSI and OSC) via regex

### `time_utils.rs` — Time Parsing and Formatting

Shared time utilities consolidating logic previously duplicated across providers:
- `parse_iso8601_to_epoch(iso)` — parses ISO 8601 timestamps (with/without timezone, with fractional seconds) to Unix epoch seconds
- `epoch_to_iso8601(epoch)` — converts epoch to ISO 8601 UTC string
- `now_epoch_secs()` — current time as epoch seconds
- `format_countdown(delta_secs)` — human-readable countdown (e.g. "Resets in 2d 5h", "Resets in 45m", "Resets soon")
- `format_reset_countdown(iso)` — combines parse + countdown
- `format_reset_from_epoch(epoch)` — countdown from epoch seconds
- `is_expired_epoch_secs(expiry)` — token expiry check
- `is_older_than(iso, max_age_secs)` — staleness check

## Constraints

- These modules are used from background threads — all functions are synchronous and thread-safe.
- `http_client` uses blocking I/O (ureq). Providers call it from within `smol::unblock()`.
- `interactive_runner` spawns OS threads for PTY I/O — it is inherently blocking.
