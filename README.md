# Banana Tray

A cross-platform system tray application for monitoring AI coding assistant quota usage, built with Rust and GPUI.

## Features

- **System tray integration** — left-click opens a compact quota popover, right-click opens settings
- **14 AI provider integrations** — real-time quota monitoring via APIs, CLIs, and local credential files
- **Settings window** — separate desktop window for full configuration (not constrained by tray panel size)
- **Auto-refresh** — configurable polling interval with per-provider cooldown and deduplication
- **Quota alerts** — system notifications when usage drops below 10% or is exhausted
- **Single instance** — second launch focuses the existing window via IPC
- **Launch at login** — macOS (SMAppService) and Linux (XDG autostart)
- **Global hotkey** — `Cmd+Shift+S` toggles the popover

## Supported Providers

| Provider | Data Source | Status |
|----------|-----------|--------|
| **Claude** | HTTP API (`api.anthropic.com`) + CLI fallback | Implemented |
| **Gemini** | HTTP API (`googleapis.com`) | Implemented |
| **Copilot** | HTTP API (`api.github.com`) | Implemented |
| **Codex** | HTTP API (`chatgpt.com`) | Implemented |
| **Kimi** | HTTP API (`kimi.com`) | Implemented |
| **Amp** | CLI (`amp usage`) | Implemented |
| **Cursor** | HTTP API (`cursor.com`) + local SQLite token | Implemented |
| **Antigravity** | Local language server API + local cache | Implemented |
| **Windsurf** | Local language server API + local cache | Implemented |
| **MiniMax** | HTTP API (`api.minimax.io`) | Implemented |
| **Kiro** | CLI (`kiro-cli` interactive PTY) | Implemented |
| **Kilo** | — | Placeholder (no public API) |
| **OpenCode** | — | Placeholder (no public API) |
| **Vertex AI** | — | Placeholder (redirects to Gemini) |

## Tech Stack

- **Language**: Rust (nightly toolchain, required by GPUI)
- **UI Framework**: [GPUI](https://crates.io/crates/adabraka-gpui) (`adabraka-gpui`) + `adabraka-ui` component library
- **Async Runtime**: smol v2 (background refresh coordinator)
- **HTTP Client**: ureq v3
- **Logging**: fern + log (file + stdout, with panic hook)
- **Serialization**: serde + serde_json
- **PTY**: portable-pty (for CLI-based providers)
- **Notifications**: notify-rust
- **Single Instance**: interprocess (local sockets)
- **Auto-launch**: smappservice-rs (macOS) / XDG desktop files (Linux)

## Getting Started

```bash
# Run development build
cargo run

# Build release
cargo build --release

# Run tests (lib only — binary tests require Metal/GPUI context)
cargo test --lib

# Lint
cargo clippy

# Format
cargo fmt
```

## Configuration

Settings are persisted as JSON:

- **macOS**: `~/Library/Application Support/BananaTray/settings.json`
- **Linux**: `$XDG_CONFIG_HOME/bananatray/settings.json` (default `~/.config/bananatray/settings.json`)

## Logging

Runtime logs use `fern` with dual output (stdout + file):

- **macOS**: `~/Library/Logs/bananatray/bananatray.log`
- **Linux**: `$XDG_STATE_HOME/bananatray/bananatray.log` (default `~/.local/state/bananatray/bananatray.log`)
- **Override**: set `BANANATRAY_LOG_DIR=/path/to/dir` to write logs to a custom directory
- **Log level**: controlled by `RUST_LOG` (default: `info`)
- **Format**: `timestamp [LEVEL] target     message`

## Architecture

```
src/
  main.rs              — Entry point: Application::run(), CLI dispatch
  lib.rs               — Crate root for lib target (testing entrypoint)
  bootstrap.rs         — App initialization (UI setup, refresh, tray events)
  app/                 — GPUI views, settings window, widgets (behind `app` feature)
  app_state.rs         — Pure-logic sub-states (ProviderStore, NavigationState, SettingsUiState)
  application/         — Action-Reducer-Effect pipeline (reducer + separated tests)
  models/              — Core data types (ProviderKind, QuotaInfo, AppSettings, etc.)
  providers/           — AiProvider trait + 14 provider implementations + ProviderManager
  tray/                — TrayController, multi-display positioning, icon management
  refresh.rs           — RefreshCoordinator: background event loop for quota polling
  runtime/             — Effect executor (GPUI bridge for dispatch_in_app)
  settings_store.rs    — JSON settings persistence (load/save)
  notification.rs      — Quota alert state machine + system notifications
  auto_launch.rs       — Platform-specific launch-at-login
  single_instance.rs   — IPC-based single instance enforcement
  logging.rs           — fern logger initialization + panic hook
  assets.rs            — GPUI AssetSource (bundle / system / dev fallback)
  theme.rs             — Light/dark color theme definitions
  icons/               — SVG icon assets
  utils/               — Shared utilities (HTTP client, PTY runner, text/time helpers)
```

Key design decisions:

1. **AppState decomposition** — `AppState` is a composition container with 3 sub-states (`ProviderStore`, `NavigationState`, `SettingsUiState`) + `AppSettings`. Sub-states are GPUI-free for testability.
2. **Provider extensibility** — `AiProvider` trait with `metadata() -> ProviderMetadata`. Adding a new provider requires only implementing the trait and registering via `register_providers!` macro.
3. **GPUI isolation** — `app` module is behind `cfg(feature = "app")` in `lib.rs` because GPUI proc macros crash test compilation. Pure logic lives in `app_state.rs`, `models/`, and `app/provider_logic.rs`.
4. **Refresh architecture** — `RefreshCoordinator` runs in a dedicated thread, receives `RefreshRequest` messages, applies cooldown/dedup, spawns concurrent refresh tasks, and sends `RefreshEvent` results back to the UI thread.
