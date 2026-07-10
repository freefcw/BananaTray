# src/bootstrap/event_sources/

Registration points for external app events.

## Files

| File | Responsibility |
|------|----------------|
| `shutdown.rs` | Registers the GPUI app-quit observer and waits for the settings writer's final flush and thread exit. |
| `tray.rs` | Registers GPUI tray click callbacks and the Linux tray menu fallback, then maps them to `TrayController` commands. |
| `hotkey.rs` | Performs startup global hotkey registration, legacy value canonicalization, invalid-config recovery, error state backfill, and hotkey callback wiring. |
| `secondary_instance.rs` | Bridges `platform::single_instance` SHOW requests from `std::sync::mpsc` into the GPUI foreground executor. |

## Boundary

These modules adapt external events into existing shell commands. They should stay thin: no
provider refresh policy, no settings-window lifecycle logic, and no generic event bus.
