# Banana Tray

一个基于 Rust 和 GPUI 构建的 macOS/Linux 系统托盘应用，用于监控 AI 编程助手配额使用情况。

## 功能特性

- **系统托盘集成** — 左键点击打开紧凑配额弹窗；Linux 采用双模模式：已安装扩展的 GNOME 桌面使用原生 GNOME Shell Extension 弹窗，其他环境回退到 ksni SNI 托盘加右键菜单
- **15 个 AI 提供商条目** — 通过 API、CLI 和本地凭据文件监控配额，外加参考/占位条目（14 个内置 + YAML 自定义提供商）
- **设置窗口** — 独立的桌面设置窗口，用于完整配置（不受托盘面板尺寸限制）
- **自动刷新** — 可配置的轮询间隔，支持按提供商冷却和去重
- **配额告警** — 当使用量低于 10% 或配额耗尽时发送系统通知
- **单实例** — 第二次启动时通过 IPC 聚焦已有窗口
- **登录时启动** — macOS（SMAppService）和 Linux（XDG autostart）
- **全局快捷键** — 固定 `Cmd+Shift+S` 快捷键切换弹窗

## 支持的提供商

| 提供商 | 数据来源 | 能力 | 备注 |
|----------|-------------|------------|-------|
| **Claude** | HTTP API (`api.anthropic.com`) + CLI 回退 | 可监控 | 完整配额刷新 |
| **Gemini** | HTTP API (`googleapis.com`) | 可监控 | 完整配额刷新 |
| **Copilot** | HTTP API (`api.github.com`) | 可监控 | 完整配额刷新 |
| **Codex** | HTTP API (`chatgpt.com`) | 可监控 | 完整配额刷新 |
| **Kimi** | HTTP API (`kimi.com`) | 可监控 | 完整配额刷新 |
| **Amp** | CLI (`amp usage`) | 可监控 | 完整配额刷新 |
| **Cursor** | HTTP API (`cursor.com`) + 本地 SQLite 令牌 | 可监控 | 完整配额刷新 |
| **Antigravity** | 本地语言服务器 API + 本地缓存 | 可监控 | 完整配额刷新 |
| **Windsurf** | 本地语言服务器 API + 本地缓存 | 可监控 | 完整配额刷新 |
| **MiniMax** | HTTP API (`api.minimax.io`) | 可监控 | 完整配额刷新 |
| **Kiro** | CLI (`kiro-cli` 交互式 PTY) | 可监控 | 完整配额刷新 |
| **Kilo** | 仅扩展检测 | 占位 | 在 UI 中显示，但不参与刷新/重试流程 |
| **OpenCode** | 仅 CLI 检测 | 占位 | 在 UI 中显示，但不参与刷新/重试流程 |
| **Vertex AI** | Gemini CLI 配置检测 | 信息参考 | Gemini Vertex AI 认证模式的参考条目 |
| **自定义 YAML** | HTTP / CLI / 占位 | 可监控或占位 | `source: placeholder` 保持为参考条目 |

## 技术栈

- **语言**: Rust（稳定工具链）
- **UI 框架**: [GPUI](https://crates.io/crates/adabraka-gpui)（`adabraka-gpui`）+ `adabraka-ui` 组件库
- **异步运行时**: smol v2（后台刷新协调器）
- **HTTP 客户端**: ureq v3
- **日志**: fern + log（文件 + 标准输出，含 panic 钩子）
- **序列化**: serde + serde_json
- **PTY**: portable-pty（用于基于 CLI 的提供商）
- **通知**: UNUserNotificationCenter（macOS）/ notify-rust（Linux）
- **单实例**: interprocess（本地套接字）
- **自启动**: smappservice-rs（macOS）/ XDG 桌面文件（Linux）
- **D-Bus**: zbus v5（async-io，兼容 smol）用于 GNOME Shell Extension 进程间通信（仅 Linux）
- **GNOME Shell Extension**: GJS（GNOME JavaScript）— 原生顶部栏弹窗，通过 D-Bus 代理与主应用通信

## 快速开始

```bash
# 运行开发构建
cargo run

# 构建发布版本
cargo build --release

# 运行测试（标准命令）
cargo test --lib

# 可选：仅本地验证无 GPUI 的 lib 层
cargo test --lib --no-default-features

# Lint
cargo clippy

# 格式化
cargo fmt
```

功能契约：

- 默认构建启用 `app` 功能，是 `cargo run` / `cargo build` 支持的应用程序构建路径。
- `--no-default-features` **不是**受支持的应用程序构建模式。仅保留用于无 GPUI 的 `lib` 检查/测试。
- `bananatray` 二进制目标明确需要 `app` 功能。

## macOS Bundle 与 DMG

### App Bundle

```bash
# 构建并组装 macOS .app 包
bash scripts/bundle.sh

# 在有 Apple Developer 签名身份时使用
export CODESIGN_IDENTITY='Apple Development: you@example.com (TEAMID)'
bash scripts/bundle.sh --skip-build
```

### DMG 创建

```bash
# 构建 .app 并创建 DMG（推荐）
bash scripts/bundle.sh --dmg

# 使用现有 .app 创建 DMG
bash scripts/bundle.sh --dmg --skip-build

# 安装 create-dmg 以获得更好的 DMG 样式
brew install create-dmg
```

**DMG 特性**：
- 统一脚本接口 — 一个脚本满足所有打包需求
- 自定义窗口大小和图标布局
- Applications 符号链接，支持拖拽安装
- 默认背景图片（自动生成）
- 可选自定义背景（`resources/dmg-background.png`）
- 可选许可证显示（`LICENSE`）
- 代码签名支持（使用 `CODESIGN_IDENTITY`）
- 自动依赖检查与回退

**注意事项**：

- 如果未设置 `CODESIGN_IDENTITY`，脚本会回退到临时签名（`-`）用于本地测试。
- 在使用 Apple Developer 证书之前，请确认 macOS 将其识别为有效的签名身份：

```bash
security find-identity -v -p codesigning
```

- 如果预期身份未出现，请在 Keychain Access 中检查证书链和私钥。常见原因是 Apple WWDR 中间证书过期或缺少私钥。

## 配置

设置以 JSON 格式持久化存储：

- **macOS**: `~/Library/Application Support/BananaTray/settings.json`
- **Linux**: `$XDG_CONFIG_HOME/bananatray/settings.json`（默认 `~/.config/bananatray/settings.json`）

## 日志

运行时日志使用 `fern`，双输出（标准输出 + 文件）：

- **macOS**: `~/Library/Logs/bananatray/bananatray.log`
- **Linux**: `$XDG_STATE_HOME/bananatray/bananatray.log`（默认 `~/.local/state/bananatray/bananatray.log`）
- **覆盖**: 设置 `BANANATRAY_LOG_DIR=/path/to/dir` 可将日志写入自定义目录
- **日志级别**: 由 `RUST_LOG` 控制（默认：`info`）
- **格式**: `timestamp [LEVEL] target     message`

## 架构

高层模块边界：

- `application/` — Action-Reducer-Effect 管道及选择器
- `models/` — 核心数据类型和持久化设置（无 GPUI 依赖）
- `runtime/` — 共享前台状态、Effect 执行、设置窗口编排
- `ui/` — GPUI 视图和组件
- `refresh/` — 后台调度和刷新执行
- `providers/` — 内置/自定义提供商和 `ProviderManager`
- `dbus/` — D-Bus 服务，用于 GNOME Shell Extension（仅 Linux）；zbus 接口 + 信号桥接
- `platform/` / `tray/` — 操作系统集成和托盘生命周期
- `gnome-shell-extension/`（项目根目录）— GNOME Shell Extension（GJS）：PanelMenu.Button + D-Bus 代理 + 配额弹窗

关键设计决策：

1. **Action-Reducer-Effect** — UI 和后台事件变成 `AppAction`，Reducer 产生 `AppEffect`，由 `runtime/` 执行 Effect。
2. **GPUI 隔离** — 核心状态和领域逻辑保持在无 GPUI 的模块中；应用外壳位于 `feature = "app"` 之后。
3. **提供商可扩展性** — 提供商通过 `AiProvider` trait 公开身份、能力等级、可用性、刷新语义和可选的设置功能。
4. **后台刷新** — 刷新在 UI 线程之外运行，将稳定的结果语义报告回前台。

有关当前架构详情，请参阅 [docs/architecture.md](docs/architecture.md) 和 `src/` 下的模块 `README.md` 文件。
