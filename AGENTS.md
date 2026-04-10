# AGENTS.md

BananaTray — cross-platform system tray app for monitoring AI coding assistant quota usage. Rust (nightly) + GPUI.

## Commands

```bash
cargo run                  # dev
cargo build --release      # release
cargo test --lib           # tests (MUST use --lib, see below)
cargo clippy               # lint
cargo fmt                  # format
```

> **`cargo test --lib` is mandatory.** `cargo test` without `--lib` will fail — the binary target pulls in GPUI which requires a Metal GPU context.

## Module Map

```
src/
  main.rs            — Entry: Application::run(), CLI dispatch
  lib.rs             — Crate root. `ui` module behind cfg(feature = "app")
  bootstrap.rs       — App initialization (UI, refresh, tray events)
  ui/                — GPUI views, settings window, widgets
    bridge.rs        — AppState (GPUI wrapper over AppSession)
    views/           — AppView, nav, provider panel, tray settings
    settings_window/ — Settings window tabs and provider management
    widgets/         — Reusable UI components
  app_state.rs       — Pure-logic sub-states (GPUI-free, testable)
  application/       — Action-Reducer-Effect pipeline (reducer_tests.rs separated)
  models/            — Core data types (GPUI-free)
  providers/         — AiProvider trait + 14 implementations + ProviderManager
    common/          — Shared provider infra (CLI, JWT, HTTP client, PTY runner)
    error_presenter.rs — Provider error → user-facing message mapping
  tray/              — TrayController, multi-display positioning, icon management
  refresh/           — RefreshCoordinator (background polling thread)
  runtime/           — Effect executor (GPUI bridge)
  utils/             — Text/time helpers, log capture, platform utils
```

Each `src/` subdirectory has its own `README.md` with detailed documentation.

## Key Constraints

1. **GPUI isolation** — GPUI proc macros crash `cargo test`. Pure logic lives in GPUI-free modules (`app_state.rs`, `models/`). The `ui` module is `cfg(feature = "app")` gated.
2. **Pure logic modules must NOT import `gpui`** — this is the testability boundary.
3. **`#![recursion_limit = "512"]`** is required in `main.rs` (GPUI macro expansion).

## Code Conventions

- `cargo fmt` + `cargo clippy`
- Comments in Chinese for domain-specific logic
- Providers return `ProviderError` variants (not raw strings)
- Log targets: `"app"`, `"tray"`, `"refresh"`, `"providers"`, `"settings"`

## Reference Docs

Detailed guides live in `docs/`:
- [docs/providers.md](docs/providers.md) — Provider table, AiProvider trait, step-by-step guide for adding a new provider
- [docs/architecture.md](docs/architecture.md) — AppState decomposition, refresh architecture, env vars, settings paths, testing coverage
