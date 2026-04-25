# AGENTS.md

BananaTray — macOS/Linux system tray app for monitoring AI coding assistant quota usage. Rust (stable) + GPUI.

## RULES

### 文档同步

**核心规则**：每次任务结束前必须执行"文档影响评估"——不是机械匹配某个清单，而是做一次主动判断：

> "本次改动引入 / 修改 / 移除的信息里，有没有任何一条是**未来的维护者或用户会期望从文档里读到**的？如果有，对应文档现在是否仍然准确？"

如果答案是"有 / 不准确"，更新文档是任务的一部分，不是后续可选项。

下列文档位置是常见落点（**不是穷举，也不是判断依据**，只是提醒哪里容易被遗漏）：

- `AGENTS.md` Architecture Map — 顶层模块结构 / 责任边界
- 各子模块的 `README.md` — 该模块的公共 API、内部数据流、对外契约
- `docs/providers.md` — provider 行为契约，含错误处理 / fallback / 降级语义等用户可观察行为
- `docs/architecture.md` — 跨模块架构决策
- 新建 `src/` 下子目录时配套新建 `README.md`

判断不清时的倾向：**"是不是用户会问的问题，或接手的人会踩的坑？"** 只要答案偏是，就更新。

### 环境与构建

- **Read `AGENTS_local.md` first** — if the file exists in the project root, read it before running any commands. It contains machine-specific environment config (e.g. tool paths) and is git-ignored.
- **Lockfile before blame** — if build/test/check fails after dependency-related changes (especially patched git crates, new upstream APIs, or code that clearly expects newer dependency behavior), first run `cargo update` or targeted `cargo update -p <crate>` to refresh `Cargo.lock`, then judge whether the failure is a real source issue.

## Commands

```bash
cargo run                  # dev
cargo build --release      # release
cargo test --lib                        # tests (MUST use --lib, see below)
cargo clippy               # lint
cargo fmt                  # format
```

> **`cargo test --lib` is the standard test command.** 默认支持的应用构建路径始终带 `app` feature。该 feature 现在同时隔离托盘壳的运行时依赖（GPUI / adabraka-ui / 单实例 / 通知 / 自启动等）；`--no-default-features` 只保留给 `lib` 层局部验证，不是受支持的 app 构建契约。All GPUI glob imports (`use gpui::*`) are banned via CI check, so SIGBUS regressions are prevented.

> **History:** Before commit `2e36981` (2026-04-13), `use gpui::*` in files with `#[test]` caused rustc SIGBUS (stack overflow via syn recursive parsing). The glob import ban fully resolved this.

## Architecture Map

```
src/
  main.rs / bootstrap.rs — App entry, startup wiring, background bridge setup (`main.rs` requires `app` feature)
  lib.rs                 — Crate root; `runtime` / `tray` / `ui` / `theme` and app-only platform adapters compiled behind `cfg(feature = "app")`
  application/           — Action-Reducer-Effect pipeline, pure app-domain logic, NewAPI 状态操作
  models/                — Core data types and settings domain models (GPUI-free)
                           settings/            — User preferences with nested sub-structures
  ui/                    — GPUI views, settings window, reusable widgets, AppState bridge
  runtime/               — Effect executor, shared AppState, GPUI/context bridge, NewAPI 文件 I/O 适配
                           effects/             — GPUI-free CommonEffect executors by domain
  providers/             — AiProvider trait, built-in/custom providers, ProviderManager
  refresh/               — Background refresh coordinator and scheduling
  tray/                  — Tray controller, icon management, multi-display positioning
  platform/              — OS integration; `paths` / `system` / log readers stay lib-safe, `assets` / `single_instance` / `notification` / `auto_launch` are app-only
  icons/                 — SVG assets
  utils/                 — Shared text/time/log helpers
  i18n.rs                — Locale detection and i18n configuration
  settings_store.rs      — Settings JSON persistence
  theme.rs               — GPUI theme tokens and window-appearance integration (`app` feature only)
  theme_tests.rs         — Theme parsing unit tests (`app` feature only)
```

This map is intentionally high-level. File-level structure and public APIs live in each module's `README.md` and in `docs/architecture.md`.

## Key Constraints

1. **GPUI isolation** — GPUI proc macros crash `cargo test`. Pure logic lives in GPUI-free modules (`application/state.rs`, `models/`). The app shell and its runtime-only dependencies are behind `cfg(feature = "app")`; `--no-default-features` only exists for `lib`-layer checks/tests.
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
