# src/platform/

Platform integration layer. This module owns OS adapters and filesystem locations, but not application-domain state.

## Module Boundaries

| File | Feature Boundary | Responsibility |
|------|------------------|----------------|
| `mod.rs` | mixed | Defines app identity constants and gates app-only adapters behind `feature = "app"`. |
| `atomic_file.rs` | lib-safe | Atomically replaces private settings/credential files and prepares exclusive private temp files for caller-owned multi-file transactions, with Unix `0600`, flush-before-rename, permission hardening, and error cleanup. |
| `paths.rs` | lib-safe | Resolves settings, custom provider, and custom script directories. |
| `system.rs` | lib-safe | Small system helpers: open URL/path without blocking the UI while a background monitor checks exit status, clipboard fallback, OS info, file-size formatting, dark-mode detection. |
| `logging.rs` | mixed | App logger initialization behind `feature = "app"` plus test/lib-safe log-tail helpers. |
| `assets.rs` | app-only | GPUI `AssetSource`; resolves resources from `BANANATRAY_RESOURCES`, app bundles, Linux system install paths, then dev root. |
| `notification.rs` | app-only | OS notification adapter. Domain alert decisions stay in `application/quota_alert.rs`. |
| `auto_launch.rs` | app-only | Launch-at-login integration: macOS `SMAppService`, Linux XDG autostart desktop entry; requests run on one background worker, coalesce to the latest desired state, and expose an exit-time completion barrier. |
| `single_instance.rs` | app-only | Single-instance IPC via local sockets; secondary launches send `SHOW` to the primary instance. |
| `gnome_detect.rs` | Linux + app-only | Detects when the native GNOME Shell Extension path should replace KSNI/AppIndicator fallback. |

## Stable Paths

- Settings file:
  - macOS: `~/Library/Application Support/BananaTray/settings.json`
  - Linux: `$XDG_CONFIG_HOME/bananatray/settings.json`
- Custom provider directory:
  - macOS: `~/Library/Application Support/BananaTray/providers/`
  - Linux: `$XDG_CONFIG_HOME/bananatray/providers/`
- Custom script directory:
  - macOS: `~/Library/Application Support/BananaTray/scripts/`
  - Linux: `$XDG_CONFIG_HOME/bananatray/scripts/`
- Default log file:
  - macOS: `~/Library/Logs/bananatray/bananatray.log`
  - Linux: `$XDG_STATE_HOME/bananatray/bananatray.log`

## Key Constraints

- Keep `paths`, `system`, and log-reading helpers usable from lib/no-default-feature checks.
- Route private settings/credential writes through `atomic_file`; multi-file callers may own commit/rollback orchestration but must reuse its exclusive private temp-file and permission-hardening primitives.
- Keep GPUI, notification, single-instance, auto-launch, and asset loading dependencies behind `feature = "app"`.
- Do not put quota alert policy in `notification.rs`; platform code only sends already-decided notifications.
- Do not add `notify-rust` back to the macOS dependency path. macOS uses `UNUserNotificationCenter` in app bundles and `osascript` for development fallback to avoid `mac-notification-sys` Launch Services side effects.
- `BANANATRAY_FORCE_GNOME_EXTENSION` and `BANANATRAY_SINGLE_INSTANCE_SUFFIX` are development/debug controls used by nested GNOME workflows, not user-facing configuration.
