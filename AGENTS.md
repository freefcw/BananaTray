# AGENTS.md

BananaTray — cross-platform system tray app for monitoring AI coding assistant quota usage. Rust (stable) + GPUI.

## RULES

**MUST**: after update source, should update related docs

1. If module structure changes (add/remove/rename files) → update `AGENTS.md` Module Map section
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
  application/       — Action-Reducer-Effect pipeline
    state.rs         — Pure-logic session state (GPUI-free, testable)
    reducer.rs       — Pure state transitions (reducer_tests.rs separated)
    action.rs        — Action enum definitions
    effect.rs        — Side-effect declarations
    selectors/       — ViewModel derivation from AppSession
  models/            — Core data types (GPUI-free)
    settings/        — User preferences (4 sub-structs + migration + domain methods)
    test_helpers.rs  — Test fixture constructors for ProviderStatus, QuotaInfo, etc.
  icons/             — SVG icon assets (provider + UI icons)
  providers/         — AiProvider trait + 14 implementations + ProviderManager
    common/          — Shared provider infra (CLI, JWT, HTTP client, PTY runner)
    custom/          — YAML declarative custom provider system
    codeium_family/  — Shared Codeium-family logic (Windsurf + Antigravity)
    error_presenter.rs — Provider error → user-facing message mapping
  platform/          — Platform adaptation layer
    assets.rs        — GPUI asset loading (multi-platform path resolution)
    auto_launch.rs   — Launch at login (macOS SMAppService / Linux XDG)
    logging.rs       — Log system init (fern + panic hook)
    notification.rs  — System notifications + quota alert state machine
    paths.rs         — Canonical config/custom-provider paths + macOS legacy fallback
    system.rs          — Platform utils (open URL, clipboard, system info)
    single_instance.rs — Single instance detection (IPC local socket)
  tray/              — TrayController, multi-display positioning, icon management
  refresh/           — RefreshCoordinator (background polling thread)
  runtime/           — Effect executor (GPUI bridge)
  utils/             — Text/time helpers, log capture
  i18n.rs            — Locale detection and i18n configuration
  settings_store.rs  — Settings JSON persistence (atomic write)
  theme.rs           — YAML-based theme system (GPUI-free)
  theme_tests.rs     — Theme parsing unit tests
```

Each `src/` subdirectory has its own `README.md` with detailed documentation.

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
