# Banana Tray

A macOS/Linux system tray application for monitoring AI coding assistant quota usage, built with Rust and GPUI.

## Features

- **System tray integration** — left-click opens a compact quota popover, right-click opens settings
- **15 AI provider integrations** — real-time quota monitoring via APIs, CLIs, and local credential files (14 built-in + YAML custom providers)
- **Settings window** — separate desktop window for full configuration (not constrained by tray panel size)
- **Auto-refresh** — configurable polling interval with per-provider cooldown and deduplication
- **Quota alerts** — system notifications when usage drops below 10% or is exhausted
- **Single instance** — second launch focuses the existing window via IPC
- **Launch at login** — macOS (SMAppService) and Linux (XDG autostart)
- **Global hotkey** — fixed `Cmd+Shift+S` shortcut toggles the popover

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

# Run tests (standard command)
cargo test --lib

# Optional local verification of the GPUI-free lib surface only
cargo test --lib --no-default-features

# Lint
cargo clippy

# Format
cargo fmt
```

Feature contract:

- Default build enables `app` and is the supported application path for `cargo run` / `cargo build`.
- `--no-default-features` is **not** a supported app build mode. It is kept only for GPUI-free `lib` checks/tests.
- The `bananatray` binary target explicitly requires the `app` feature.

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

High-level module boundaries:

- `application/` — Action-Reducer-Effect pipeline and selectors
- `models/` — core data types and persisted settings (GPUI-free)
- `runtime/` — shared foreground state, effect execution, settings-window orchestration
- `ui/` — GPUI views and widgets
- `refresh/` — background scheduling and refresh execution
- `providers/` — built-in/custom providers and `ProviderManager`
- `platform/` / `tray/` — OS integration and tray lifecycle

Key design decisions:

1. **Action-Reducer-Effect** — UI and background events become `AppAction`, reducers emit `AppEffect`, and `runtime/` executes effects.
2. **GPUI isolation** — core state and domain logic stay in GPUI-free modules; the app shell lives behind `feature = "app"`.
3. **Provider extensibility** — providers expose identity, availability, refresh semantics, and optional settings capability through `AiProvider`.
4. **Background refresh** — refresh runs off the UI thread and reports stable result semantics back to the foreground.

For current architecture details, see [docs/architecture.md](docs/architecture.md) and the module `README.md` files under `src/`.
