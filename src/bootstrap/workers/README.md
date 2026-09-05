# src/bootstrap/workers/

Foreground bridges for background work owned by the app shell.

## Files

| File | Responsibility |
|------|----------------|
| `refresh.rs` | Starts the refresh coordinator thread, bridges `RefreshEvent` back to the GPUI foreground executor, and sends startup refresh requests. |
| `custom_provider.rs` | Owns the serial NewAPI / script-provider CRUD worker (including edit loads and save-time settings flush) and returns finished actions to the reducer. |
| `script_test.rs` | Owns the independent serial script Run Test worker and returns `ScriptProviderTestFinished` to the reducer. |
| `linux_dbus.rs` | Linux-only D-Bus snapshot emission after foreground state changes. |

## Boundary

These modules may know GPUI foreground executors and shell-owned handles, but they should not own
domain policy. Refresh scheduling stays in `refresh/`, custom-provider I/O and script test execution
stay in `runtime`, and state changes still enter through `AppAction`. Script tests and CRUD use
independent queues/owners so a long test timeout cannot block save/delete/load. The CRUD queue is
persistent: shutdown closes only its enqueue side, then drains accepted jobs and joins its
worker within the shared 60 ms deadline; overdue workers detach, so unfinished transactions and
late results are not guaranteed to settle. Its worker writes each finished action to the reliable result ledger before
sending a lightweight foreground wake-up; if the foreground pump has stopped, quit settlement still
reduces the ledger. Refresh receives a Shutdown request and script-test uses out-of-band cancellation; both share
the bounded 60 ms join deadline with CRUD.
