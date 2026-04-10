# Architecture

## Tech Stack

- **Language**: Rust (nightly — required by GPUI)
- **UI**: GPUI (`adabraka-gpui` v0.5.x) + `adabraka-ui` v0.3.x
- **Async**: smol v2 (background refresh coordinator)
- **HTTP**: ureq v3 (blocking, used from async via `smol::unblock`)
- **Logging**: fern + log (dual output: stdout + file)
- **PTY**: portable-pty (CLI-based providers)
- **Notifications**: notify-rust
- **Single Instance**: interprocess (local sockets)
- **Auto-launch**: smappservice-rs (macOS) / XDG autostart (Linux)

## AppState Decomposition

`AppState` (in `app/gpui_bridge.rs`) is a composition container holding:

- `ProviderStore` — provider status list + find/mutate methods
- `NavigationState` — active tab + last provider kind
- `SettingsUiState` — settings window tab + dropdown state
- `AppSettings` — persisted user preferences
- `refresh_tx` — channel to RefreshCoordinator
- `alert_tracker` — quota notification state machine
- `view_entity` — weak ref to AppView for UI updates

Sub-state structs live in `src/app_state.rs` (GPUI-free). Access: `state.session.provider_store.providers`, `state.session.nav.active_tab`, etc.

## Refresh Architecture

`RefreshCoordinator` runs in a dedicated thread:

- Receives `RefreshRequest` via `smol::channel`
- Applies cooldown (half interval, min 30s) and in-flight dedup
- Spawns concurrent refresh tasks per provider
- Sends `RefreshEvent` results to foreground executor for UI update

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

## Testing

644 unit tests, run with `cargo test --lib --no-default-features`. Coverage:

- `models/` — ProviderKind, QuotaInfo, AppSettings, PopupLayout
- `app_state.rs` — ProviderStore, NavigationState, SettingsUiState
- `application/reducer_tests.rs` — all Action-Reducer-Effect tests
- `app/provider_logic.rs` — formatting and display logic
- `providers/` — ProviderError, ProviderManager, individual provider parsers
- `tray/icon.rs` — tray icon data and template mode tests
- `utils/` — HTTP header parsing, text stripping, time parsing
- `notification.rs` — QuotaAlertTracker state machine
- `auto_launch.rs` — platform-specific launch-at-login
- `assets.rs` — asset resolution fallback chain
