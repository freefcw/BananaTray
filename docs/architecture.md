# Architecture

## Tech Stack

- **Language**: Rust (nightly — required by GPUI)
- **UI**: GPUI (`adabraka-gpui` v0.5.x) + `adabraka-ui` v0.3.x
- **Async**: smol v2 (background refresh coordinator)
- **HTTP**: ureq v3 (blocking, used from async via `smol::unblock`)
- **Logging**: fern + log (dual output: stdout + file)
- **PTY**: portable-pty (CLI-based providers)
- **Notifications**: notify-rust (Linux) / UNUserNotificationCenter (macOS)
- **Single Instance**: interprocess (local sockets)
- **Auto-launch**: smappservice-rs (macOS) / XDG autostart (Linux)

## AppState Decomposition

`AppState` (in `ui/bridge.rs`) is a composition container holding:

- `session: AppSession` — pure-logic session state (see below)
- `refresh_tx` — channel to RefreshCoordinator
- `view_entity` — weak ref to AppView for UI updates
- `log_path` — log file path for Debug tab

Persisted `AppSettings` are loaded in `bootstrap.rs` and injected into `AppState::new(...)`, so the UI runtime container no longer performs settings I/O during construction.

`AppSession` (in `application/state.rs`, GPUI-free) holds:

- `ProviderStore` — provider status list + find/mutate/sync methods
- `NavigationState` — active tab + last provider id + generation counter
- `SettingsUiState` — settings window tab + provider management UI state
- `DebugUiState` — debug tab state (selected provider, refresh status)
- `AppSettings` — persisted user preferences
- `alert_tracker` — quota notification state machine
- `popup_visible` — popup visibility flag (deferred dynamic icon updates)

Access: `state.session.provider_store.providers`, `state.session.nav.active_tab`, etc.

## Refresh Architecture

`RefreshCoordinator` runs in a dedicated `std::thread`:

- Receives `RefreshRequest` via `smol::channel`
- Delegates scheduling decisions to `RefreshScheduler` (cooldown, eligibility, deadline)
- Uses absolute-deadline timers to avoid timer reset on request receipt
- Spawns concurrent refresh tasks per provider via `std::thread`
- Sends `RefreshEvent` results to foreground executor for UI update
- Supports `ReloadProviders` for custom provider hot-reload

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `RUST_LOG` | Log level (`trace`, `debug`, `info`, `warn`, `error`) |
| `BANANATRAY_LOG_DIR` | Override log file directory |
| `BANANATRAY_RESOURCES` | Override asset directory (AppImage packaging) |

Provider credentials are read from local config files or CLI tools, except Copilot which reads a GitHub token from settings.

## Settings Storage

- **macOS**: `~/Library/Application Support/BananaTray/settings.json`
- **Linux**: `$XDG_CONFIG_HOME/bananatray/settings.json`

## Custom Provider Storage

- **Canonical directory**:
  - macOS: `~/Library/Application Support/BananaTray/providers/`
  - Linux: `$XDG_CONFIG_HOME/bananatray/providers/`
- **Compatibility**:
  - On startup, macOS legacy lowercase directory `~/Library/Application Support/bananatray/providers/` is migrated into the canonical directory
  - After migration, runtime reads and writes only use the canonical directory

## Custom Provider Auto-Registration

Custom providers are automatically registered in `settings.json` through three layers:

1. **Startup** (`AppSession::new`): YAML files that exist on disk but have no corresponding entry in `enabled_providers` are auto-enabled and added to the sidebar
2. **Save** (`SubmitNewApi` reducer): The provider ID is pre-registered in `enabled_providers` + `sidebar_providers` before the YAML file is written, so it's immediately visible after hot-reload
3. **Hot-reload** (`ProvidersReloaded` reducer): Newly discovered custom providers (e.g. manually dropped YAML files) are auto-enabled and added to the sidebar

## Testing

run with `cargo test --lib`. Coverage:

- `models/` — ProviderKind, QuotaInfo, AppSettings, PopupLayout
- `app_state.rs` — ProviderStore, NavigationState, SettingsUiState
- `application/reducer_tests.rs` — all Action-Reducer-Effect tests
- `application/selectors/` — format, tray, settings, debug selector tests
- `providers/` — ProviderError, ProviderManager, individual provider parsers
- `tray/icon.rs` — tray icon data and template mode tests
- `utils/` — text stripping, time parsing, log capture
- `platform/notification.rs` — QuotaAlertTracker state machine
- `platform/auto_launch.rs` — platform-specific launch-at-login
- `platform/assets.rs` — asset resolution fallback chain
- `refresh/` — coordinator and scheduler tests
- `theme.rs` — YAML theme parsing
- `settings_store.rs` — settings persistence round-trip

## GPUI Import Discipline

- `src/` forbids `use gpui::*;` because glob imports hide the actual GPUI dependency surface and were previously correlated with GPUI test/SIGBUS failure investigation.
- Enforcement is automated by `scripts/check-gpui-imports.sh`, wired into CI, and exposed through `.pre-commit-config.yaml` for local commits.
- UI files should use explicit GPUI imports plus explicit extension traits. In practice the most common trait imports are `Styled`, `ParentElement`, `InteractiveElement`, `StatefulInteractiveElement`, `IntoElement`, `AnimationExt`, and `AppContext`.
- This keeps GPUI-heavy modules readable, makes stateful-builder transitions visible in code review, and reduces the chance of reintroducing the same failure pattern.
