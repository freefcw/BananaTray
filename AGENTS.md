# AGENTS.md

BananaTray — cross-platform system tray app for monitoring AI coding assistant quota usage. Rust (stable) + GPUI.

## RULES

**MUST**: after update source, should update related docs

1. If high-level module structure or ownership changes (top-level modules, major responsibilities, key entry files) → update `AGENTS.md` Architecture Map section
2. If a module's public API or architecture changes → update that module's `README.md`
3. If provider-related changes → update `docs/providers.md`
4. If architecture-level changes → update `docs/architecture.md`
5. When adding a new subdirectory under `src/`, create a `README.md` for it
6. **Read `AGENTS_local.md` first** — if the file exists in the project root, read it before running any commands. It contains machine-specific environment config (e.g. tool paths) and is git-ignored.
7. **Task completion check** — after finishing a task, review whether any documentation needs updating based on rules 1–5 above.

## Commands

```bash
cargo run                  # dev
cargo build --release      # release
cargo test --lib                        # tests (MUST use --lib, see below)
cargo clippy               # lint
cargo fmt                  # format
```

> **`cargo test --lib` is the standard test command.** Plain `cargo test` also works but is slower (compiles bin target too). All GPUI glob imports (`use gpui::*`) are banned via CI check, so SIGBUS regressions are prevented.

> **History:** Before commit `2e36981` (2026-04-13), `use gpui::*` in files with `#[test]` caused rustc SIGBUS (stack overflow via syn recursive parsing). The glob import ban fully resolved this.

## Architecture Map

```
src/
  main.rs / bootstrap.rs — App entry, startup wiring, background bridge setup
  lib.rs                 — Crate root; `ui` compiled behind `cfg(feature = "app")`
  application/           — Action-Reducer-Effect pipeline, pure app-domain logic, NewAPI 状态操作
  models/                — Core data types and settings domain models (GPUI-free)
  ui/                    — GPUI views, settings window, reusable widgets, AppState bridge
  runtime/               — Effect executor, GPUI/context bridge, NewAPI 文件 I/O 适配
  providers/             — AiProvider trait, built-in/custom providers, ProviderManager
  refresh/               — Background refresh coordinator and scheduling
  tray/                  — Tray controller, icon management, multi-display positioning
  platform/              — OS integration (assets, auto-launch, notifications, paths, system)
  icons/                 — SVG assets
  utils/                 — Shared text/time/log helpers
  i18n.rs                — Locale detection and i18n configuration
  settings_store.rs      — Settings JSON persistence
  theme.rs               — YAML-based theme system (GPUI-free)
  theme_tests.rs         — Theme parsing unit tests
```

This map is intentionally high-level. File-level structure and public APIs live in each module's `README.md` and in `docs/architecture.md`.

## Key Constraints

1. **GPUI isolation** — GPUI proc macros crash `cargo test`. Pure logic lives in GPUI-free modules (`application/state.rs`, `models/`). The `ui` module is `cfg(feature = "app")` gated.
2. **Pure logic modules must NOT import `gpui`** — this is the testability boundary.
3. **`#![recursion_limit = "512"]`** is required in `main.rs` and `lib.rs` (GPUI macro expansion).

## Code Conventions

- `cargo fmt` + `cargo clippy`
- Comments in Chinese for domain-specific logic
- Providers return `ProviderError` variants (not raw strings)
- Log targets: `"app"`, `"tray"`, `"refresh"`, `"providers"`, `"settings"`

## Reference Docs

Detailed guides live in `docs/`:
- [docs/providers.md](docs/providers.md) — Provider table, AiProvider trait, step-by-step guide for adding a new provider
- [docs/architecture.md](docs/architecture.md) — AppState decomposition, refresh architecture, env vars, settings paths, testing coverage
