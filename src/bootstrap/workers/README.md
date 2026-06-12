# src/bootstrap/workers/

Foreground bridges for background work owned by the app shell.

## Files

| File | Responsibility |
|------|----------------|
| `refresh.rs` | Starts the refresh coordinator thread, bridges `RefreshEvent` back to the GPUI foreground executor, and sends startup refresh requests. |
| `script_test.rs` | Runs Settings UI script-provider tests on a background thread and returns `ScriptProviderTestFinished` to the reducer. |
| `linux_dbus.rs` | Linux-only D-Bus snapshot emission after foreground state changes. |

## Boundary

These modules may know GPUI foreground executors and shell-owned handles, but they should not own
domain policy. Refresh scheduling stays in `refresh/`, script test execution stays in `runtime`,
and state changes still enter through `AppAction`.
