# Banana Tray

A cross-platform system tray application for monitoring AI coding assistant quota usage, built with Rust and GPUI.

## Features

- **System tray integration** — left-click opens a compact quota popover, right-click opens settings
- **15 AI provider integrations** — real-time quota monitoring via APIs, CLIs, and local credential files (14 built-in + YAML custom providers)
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

- **Language**: Rust (stable toolchain)
- **UI Framework**: [GPUI](https://crates.io/crates/adabraka-gpui) (`adabraka-gpui`) + `adabraka-ui` component library
- **Async Runtime**: smol v2 (background refresh coordinator)
- **HTTP Client**: ureq v3
- **Logging**: fern + log (file + stdout, with panic hook)
- **Serialization**: serde + serde_json
- **PTY**: portable-pty (for CLI-based providers)
- **Notifications**: UNUserNotificationCenter (macOS) / notify-rust (Linux)
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

## macOS Bundle & DMG

### App Bundle

```bash
# Build and assemble the macOS .app bundle
bash scripts/bundle.sh

# Use an Apple Developer signing identity when available
export CODESIGN_IDENTITY='Apple Development: you@example.com (TEAMID)'
bash scripts/bundle.sh --skip-build
```

### DMG Creation

```bash
# Build .app and create DMG (recommended)
bash scripts/bundle.sh --dmg

# Use existing .app to create DMG
bash scripts/bundle.sh --dmg --skip-build

# Install create-dmg for better DMG styling
brew install create-dmg
```

**DMG Features**:
- Unified script interface - one script for all packaging needs
- Custom window size and icon layout
- Applications symlink for drag-and-drop installation
- Default background image (auto-generated)
- Optional custom background (`resources/dmg-background.png`)
- Optional license display (`LICENSE`)
- Code signing support (with `CODESIGN_IDENTITY`)
- Automatic dependency checking and fallback

**Notes**:

- If `CODESIGN_IDENTITY` is unset, scripts fall back to ad-hoc signing (`-`) for local testing.
- Before using an Apple Developer certificate, verify that macOS recognizes it as a valid signing identity:

```bash
security find-identity -v -p codesigning
```

- If the expected identity does not appear, check the certificate chain and private key in Keychain Access. A common cause is an outdated Apple WWDR intermediate certificate or a missing private key.

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
  main.rs            — Entry point: Application::run(), CLI dispatch
  lib.rs             — Crate root. `ui` module behind cfg(feature = "app")
  bootstrap.rs       — App initialization (UI setup, refresh, tray events)
  ui/                — GPUI views, settings window, widgets
  application/       — Action-Reducer-Effect pipeline (state, reducer, selectors)
  models/            — Core data types (GPUI-free: ProviderKind, QuotaInfo, AppSettings, etc.)
  providers/         — AiProvider trait + 14 implementations + ProviderManager + YAML custom providers
  platform/          — Platform adaptation layer (assets, auto-launch, logging, notification, paths, single instance)
  tray/              — TrayController, multi-display positioning, icon management
  refresh/           — RefreshCoordinator (background polling thread)
  runtime/           — Effect executor (GPUI bridge)
  icons/             — SVG icon assets (provider + UI icons)
  utils/             — Text/time helpers, log capture
  settings_store.rs  — JSON settings persistence (atomic write)
  i18n.rs            — Locale detection and i18n configuration
  theme.rs           — GPUI color token system (depends on gpui: Hsla, Global, WindowAppearance)
```

Key design decisions:

1. **AppState decomposition** — `AppState` (`ui/bridge.rs`) wraps `AppSession` (`application/state.rs`), which holds `ProviderStore`, `NavigationState`, `SettingsUiState`, `DebugUiState` + `AppSettings`. Sub-states are GPUI-free for testability.
2. **Action-Reducer-Effect** — Elm/Redux-style unidirectional data flow: `AppAction → reduce() → Vec<AppEffect> → runtime/`. Pure reducers and selectors are fully testable.
3. **Provider extensibility** — `AiProvider` trait with `metadata() -> ProviderMetadata`. Adding a new provider requires only implementing the trait and registering via `register_providers!` macro.
4. **GPUI isolation** — `ui` module is behind `cfg(feature = "app")` in `lib.rs` because GPUI proc macros crash test compilation. Pure logic lives in `application/`, `models/`, and `providers/`.
5. **Refresh architecture** — `RefreshCoordinator` runs in a dedicated thread, receives `RefreshRequest` messages, applies cooldown/dedup, spawns concurrent refresh tasks, and sends `RefreshEvent` results back to the UI thread.
