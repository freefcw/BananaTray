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

## Feature Contract

- 默认受支持的产品路径是 `app` feature 开启的托盘应用构建。
- `bananatray` 二进制目标通过 Cargo `required-features = ["app"]` 显式要求 `app` feature。
- `--no-default-features` 不再表示“完整 app 可构建”；它只保留给 GPUI-free `lib` 层的本地检查/测试。

## AppState Decomposition

`AppState` (in `runtime/app_state.rs`) is a composition container holding:

- `session: AppSession` — pure-logic session state (see below)
- `manager: ProviderManagerHandle` — shared provider registry handle; foreground UI and background refresh both read snapshots from it, and hot-reload swaps the inner `Arc<ProviderManager>` atomically
- `refresh_tx` — channel to RefreshCoordinator
- `settings_writer` — debounced settings persistence executor
- `log_path` — log file path for Debug tab

Persisted `AppSettings` are loaded in `bootstrap.rs` and injected into `AppState::new(...)`. `AppState` now lives in `runtime`, so both `runtime` and `ui` depend on a shared state container instead of `runtime` depending on `ui::AppState`.

Important boundary: `AppState` no longer stores GPUI view handles such as `WeakEntity<AppView>`. Popup-view notification and settings-window view creation are registered into `runtime` through `runtime/ui_hooks.rs`, keeping shared state free of UI handle ownership.

`AppSession` (in `application/state.rs`, GPUI-free) holds:

- `ProviderStore` — provider status list + find/mutate/sync methods
- `NavigationState` — active tab + last provider id + generation counter
- `SettingsUiState` — settings window tab + provider management UI state
- `DebugUiState` — debug tab state (selected provider, refresh status)
- `AppSettings` — persisted user preferences
- `alert_tracker` — quota notification state machine (`application/quota_alert.rs`)
- `popup_visible` — popup visibility flag (deferred dynamic icon updates)

Access: `state.session.provider_store.providers`, `state.session.nav.active_tab`, etc.

## Layering and Ownership

The current architecture is organized around four layers:

1. **`application/`** — pure Action → Reducer → Effect pipeline
   - owns domain state transitions
   - owns selector-side presentation formatting (`application/selectors/format.rs`)
   - emits `AppEffect` values only
   - must stay GPUI-free
2. **`runtime/`** — effect execution and foreground integration
   - owns `AppState`, dispatch entrypoints, settings persistence, window-opening orchestration
   - bridges reducer output into platform/UI side effects
3. **`ui/`** — GPUI views and rendering
   - owns `AppView`, `SettingsView`, widgets, and view-local state
   - registers UI hooks into `runtime` during bootstrap
4. **platform / refresh / providers** — infrastructure services
   - `refresh/` handles background scheduling and event production
   - `providers/` owns provider implementations and runtime registry
   - `platform/` owns OS integration such as notifications, autostart, paths, URL open, and shared macOS display/mouse geometry helpers

Target dependency direction:

```text
ui ───────────────▶ runtime ─────────────▶ application
 │                    │                     │
 │                    ├────────────────────▶ refresh / providers / platform
 │                    │
 └─ register hooks ───┘
```

This is intentionally not a fully inverted ports-and-adapters design. Instead, BananaTray uses a pragmatic boundary: shared state and effect orchestration live in `runtime`, while concrete GPUI view types remain in `ui` and are exposed to `runtime` only through a narrow hook registration layer.

## Action → Effect → Runtime Flow

The main foreground path is:

1. UI or background event produces an `AppAction`
2. `runtime::dispatch_*()` borrows `AppState.session`
3. `application::reduce(&mut session, action)` returns `Vec<AppEffect>`
4. `runtime` executes each effect through the appropriate runner

`AppEffect` is split into two sub-enums:

- `ContextEffect`
  - requires a GPUI-capable foreground context
  - examples: `Render`, `OpenSettingsWindow`, `OpenUrl`, `ApplyTrayIcon`, `QuitApp`
- `CommonEffect`
  - does not require a concrete GPUI context
  - examples: `PersistSettings`, `SendRefreshRequest`, notifications, YAML I/O

This split keeps reducer output explicit while letting `runtime` centralize the imperative work.

## Localization Boundary

Provider / refresh / reducer layers now persist only stable semantics instead of locale-bound strings:

- `QuotaInfo` stores `stable_key`, `QuotaLabelSpec`, `QuotaDetailSpec`
- `ProviderStatus` stores `last_failure: Option<ProviderFailure>`
- `ProviderErrorPresenter` converts provider-layer errors into stable failure payloads
- `application/selectors/format.rs` performs the final i18n string generation on read

Consequence:

- switching language does **not** require refreshing provider data
- cached/offline provider data can still fully switch display language
- state no longer needs to “wash away” old localized strings by forcing refresh

## Runtime Dispatch and UI Hooks

`runtime/mod.rs` owns three dispatch entrypoints:

- `dispatch_in_context<V>()` — for view callbacks running under `Context<V>`
- `dispatch_in_window()` — for handlers that have `Window + App`
- `dispatch_in_app()` — for global app callbacks and refresh-event delivery

These share the same reducer pipeline and differ only in what capabilities are available during effect execution.

`runtime/ui_hooks.rs` provides the remaining bridge points that need UI participation:

- notify the current popup view to rerender
- clear popup-view registration when the popup closes
- construct the settings-window view entity

Hooks are registered from `bootstrap::bootstrap_ui()` via `ui::settings_window::register_runtime_hooks()`. This keeps:

- popup-view weak references inside `ui`
- settings-window view construction inside `ui`
- effect routing and open-window orchestration inside `runtime`

## Window Ownership

BananaTray has two foreground window surfaces with different responsibilities:

- **Tray popup**
  - opened and owned by `tray/controller.rs`
  - content view is `ui::AppView`
  - popup lifecycle and auto-hide are tray concerns; macOS display geometry comes from `platform/core_graphics.rs`
- **Settings window**
  - open/reuse scheduling is owned by `runtime/settings_window_opener.rs`
  - content view is `ui::settings_window::SettingsView`
  - cross-display reopen and delayed creation live in `runtime`, while mouse-display detection reuses `platform/core_graphics.rs`

Why the delayed settings-window open exists:

- some actions emit `OpenSettingsWindow` while the popup is still borrowing `Rc<RefCell<AppState>>`
- opening a window immediately from the same call stack risks `RefCell` reentrancy
- `schedule_open_settings_window()` delays creation to the next foreground turn, avoiding this class of panic

## Refresh Architecture

`RefreshCoordinator` runs in a dedicated `std::thread`:

- Receives `RefreshRequest` via `smol::channel`
- Delegates scheduling decisions to `RefreshScheduler` (cooldown, eligibility, deadline)
- Uses absolute-deadline timers to avoid timer reset on request receipt
- Reads the current `ProviderManager` snapshot from `ProviderManagerHandle`
- Spawns concurrent refresh tasks via `smol` blocking pool, collecting results in completion order
- Wraps each provider refresh with a coordinator-side timeout guard so one hung provider cannot wedge result collection forever
- Sends `RefreshEvent` results to foreground executor for UI update
- Supports `ReloadProviders` for custom provider hot-reload and atomically replaces the shared registry snapshot so UI and refresh stay on the same manager instance

Timeout model:

- shared HTTP requests use a global `ureq` timeout in `providers/common/http_client.rs`
- non-interactive CLI commands use a polled child-process timeout in `providers/common/cli.rs`
- coordinator timeout is the final safety net that clears in-flight state even if provider code still blocks internally

Foreground integration path:

- `bootstrap::start_event_pump()` receives `RefreshEvent`
- it forwards each event onto the GPUI foreground executor
- `runtime::dispatch_in_app()` turns the event into state updates and follow-up effects
- UI redraw is requested through the registered runtime UI hook when needed

## Settings Persistence

Settings persistence is intentionally centralized in `runtime/settings_writer.rs`:

- `CommonEffect::PersistSettings` schedules a debounced write
- all writes are serialized through a dedicated background thread
- `flush()` is used when a synchronous “write now” boundary is required
- persistence uses `settings_store` directly rather than routing through `ui`

This avoids the old architectural smell where runtime-owned logic reached back into `ui` just to save settings.

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `RUST_LOG` | Log level (`trace`, `debug`, `info`, `warn`, `error`) |
| `BANANATRAY_LOG_DIR` | Override log file directory |
| `BANANATRAY_RESOURCES` | Override asset directory (AppImage packaging) |

Provider credentials may come from local config files, CLI tools, environment variables, or BananaTray-managed provider settings. For providers using `TokenInputCapability`, BananaTray persists only its own token overrides under `provider.credentials` in `settings.json`; this is not the full source of truth for every provider's auth state.

## Settings Storage

- **macOS**: `~/Library/Application Support/BananaTray/settings.json`
- **Linux**: `$XDG_CONFIG_HOME/bananatray/settings.json`

`settings.json` keeps provider preferences and BananaTray-managed provider token overrides together under `provider`. External provider auth state (for example CLI login sessions, provider-owned config files, or env vars) is resolved separately at runtime and is not mirrored into `settings.json`.

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

标准测试命令是 `cargo test --lib`。如果只想验证纯逻辑层，也可以本地运行 `cargo test --lib --no-default-features` 或 `cargo check --lib --no-default-features`。这类命令只覆盖 `lib` 面，不代表 BananaTray app shell 支持在无 `app` feature 下构建。

Coverage:

- `models/` — ProviderKind, QuotaInfo, AppSettings, PopupLayout
- `application/state.rs` — ProviderStore, NavigationState, SettingsUiState
- `application/reducer_tests.rs` — all Action-Reducer-Effect tests
- `application/selectors/` — format, tray, settings, debug selector tests
- `providers/` — ProviderError, ProviderManager, individual provider parsers
- `tray/icon.rs` — tray icon data and template mode tests
- `utils/` — text stripping, time parsing, log capture
- `application/quota_alert.rs` — QuotaAlertTracker state machine
- `platform/auto_launch.rs` — platform-specific launch-at-login
- `platform/assets.rs` — asset resolution fallback chain
- `refresh/` — coordinator and scheduler tests
- `theme.rs` — GPUI color token system (depends on gpui; not GPUI-free)
- `settings_store.rs` — settings persistence round-trip

Architectural testability notes:

- `application/` and `models/` remain GPUI-free and are the primary unit-test surface
- `runtime/settings_writer.rs` is tested directly because it is thread-based but GPUI-free in behavior
- `runtime` itself still compiles only under the `app` feature, but its shared state and execution responsibilities are now more isolated from concrete UI storage than before
- `application/newapi_ops.rs` is intentionally compiled only when `app` is enabled or when unit tests need it, because its production caller is the app runtime

## GPUI Import Discipline

- `src/` forbids `use gpui::*;` because glob imports hide the actual GPUI dependency surface and were previously correlated with GPUI test/SIGBUS failure investigation.
- Enforcement is automated by `scripts/check-gpui-imports.sh`, wired into CI, and exposed through `.pre-commit-config.yaml` for local commits.
- UI files should use explicit GPUI imports plus explicit extension traits. In practice the most common trait imports are `Styled`, `ParentElement`, `InteractiveElement`, `StatefulInteractiveElement`, `IntoElement`, `AnimationExt`, and `AppContext`.
- This keeps GPUI-heavy modules readable, makes stateful-builder transitions visible in code review, and reduces the chance of reintroducing the same failure pattern.
