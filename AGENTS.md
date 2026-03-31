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
  main.rs            — Entry: TrayController, Application::run()
  lib.rs             — Crate root. `app` module behind cfg(feature = "app")
  app/               — GPUI views, settings window, widgets
  app_state.rs       — Pure-logic sub-states (GPUI-free, testable)
  models/            — Core data types (GPUI-free)
  providers/         — AiProvider trait + 12 implementations + ProviderManager
  refresh.rs         — RefreshCoordinator (background polling thread)
  utils/             — HTTP client, PTY runner, text/time helpers
```

Each `src/` subdirectory has its own `README.md` with detailed documentation.

## Key Constraints

1. **GPUI isolation** — GPUI proc macros crash `cargo test`. Pure logic lives in GPUI-free modules (`app_state.rs`, `app/provider_logic.rs`, `models/`). The `app` module is `cfg(feature = "app")` gated.
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
