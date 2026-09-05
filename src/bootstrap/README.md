# src/bootstrap/

App shell composition root. This layer wires concrete GPUI windows, tray callbacks,
D-Bus bridges, background workers, and full-context runtime dispatch. It stays behind
the `app` feature and may depend on GPUI.

## Module Structure

| File | Responsibility |
|------|----------------|
| `../bootstrap.rs` | Thin module root; declares bootstrap submodules and re-exports the stable shell entry points used by `main`, `ui`, `tray`, and `dbus`. |
| `capabilities.rs` | Implements `WindowShellCaps` / `AppShellCaps` and exposes `dispatch_in_window()` / `dispatch_in_app()` as the full-context runtime dispatch facade. |
| `settings_window.rs` | Holds the shell hook registry for popup/settings view factories and owns settings window open, reuse, display targeting, and activation workarounds. |
| `ui_bootstrap.rs` | Initializes locale, UI toolkit, idle GPU cache trim, initial tray icon/tooltip, macOS tray panel mode, and notification authorization. |
| `workers/` | Background worker bootstrap and foreground event pumps: refresh coordinator, independent custom-provider I/O and script-test pumps, and Linux D-Bus snapshot emission. |
| `event_sources/` | External app event registration: app shutdown, tray callbacks/menu fallback, startup global hotkey, and secondary instance SHOW bridge. |

## Boundary

- `bootstrap` is the shell composition root, not a domain layer.
- `runtime` owns reducer/effect execution and capability traits, but it must not know concrete
  `SettingsView`, tray controller, D-Bus handles, or GPUI window handles.
- `ui` provides concrete views and hook factories; `bootstrap` performs the shell-side registration.
- `tray`, `platform`, and `dbus` own their local protocols/adapters. The GPUI foreground wiring
  that combines them with `AppState` stays here.

Do not add a generic shell manager to `runtime`, and do not move GPUI foreground executor bridges
into lower-level modules just to shrink this package.

## Startup Flow

`lib.rs::run_app()` intentionally keeps the high-level startup order visible, while `main.rs`
remains a thin binary entry point:

1. load settings and initialize UI/tray shell
2. start refresh, custom-provider I/O, and script-test channels and worker threads
3. construct the shared `AppState`, then inject it into `TrayController`
4. register the app shutdown hook for owned runtime resources
5. start Linux D-Bus service when applicable
6. start foreground event pumps
7. send initial refresh requests
8. register the remaining external event sources: tray, hotkey, secondary instance

Submodules own each step's implementation details. `main.rs` should not need to know callback
registration internals, channel bridge loops, or settings window lifecycle workarounds.

## Workarounds Owned Here

- `settings_window.rs`: 10 ms delayed settings open after popup close, plus the `+1px` resize nudge
  after creating the settings window.
- `ui_bootstrap.rs`: macOS `set_tray_panel_mode(true)` and idle GPU cache trim observer registration.
- `event_sources/shutdown.rs`: closes the custom-provider CRUD enqueue side (the worker drains accepted jobs within the shared 60ms join deadline and detaches if overdue), requests shutdown for refresh/script-test workers and joins them within a shared 60ms bound, then settles already received ledger results before scheduling the final settings snapshot. Linux D-Bus has a separate 20ms bounded drop hook; the settings writer's explicit final flush and launch-at-login state complete before the quit observer returns.
- `event_sources/tray.rs`: Linux tray menu fallback for tray hosts that do not consistently forward click
  activation events.
