# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

BananaTray 是一个跨平台系统托盘应用程序，用于监控 AI 编码助手的配额使用情况。基于 GPUI 框架构建。

## 技术栈

- **语言**: Rust
- **UI 框架**: GPUI (adabraka-gpui)
- **工具链**: Nightly (必需，因为 GPUI 依赖 nightly 特性)
- **异步运行时**: smol v2

## 常用命令

```bash
# 运行开发版本
cargo run

# 构建 release 版本
cargo build --release

# 代码检查
cargo clippy

# 格式化代码
cargo fmt

# 运行测试
cargo test
```

## 环境变量

以下环境变量用于配置 AI Provider:

- `GITHUB_USERNAME` - GitHub Copilot provider 所需
- `GITHUB_TOKEN` - GitHub Copilot provider 所需（需要读取 GitHub API 的权限）

其他 Provider（Claude、Gemini、Codex、Kimi、Amp）的 API key 配置待定。

## 代码规范

- 使用 `cargo fmt` 自动格式化代码
- 使用 `cargo clippy` 检查代码问题
- 提交前运行 `/verify` 技能检查代码

## Provider 开发说明

当前支持的 Provider:

1. **Claude** - mock 数据（待实现真实 API）
2. **Gemini** - mock 数据（待实现真实 API）
3. **GitHub Copilot** - 已实现（需要 GITHUB_USERNAME + GITHUB_TOKEN）
4. **Codex** - mock 数据（待实现真实 API）
5. **Kimi** - mock 数据（待实现真实 API）
6. **Amp** - 已实现（需要安装 `amp` CLI）

添加新 Provider 的步骤:
1. 在 `src/providers/` 创建新文件
2. 实现 `Provider` trait
3. 在 `src/providers/manager.rs` 中注册

## 注意事项

- GPUI 框架目前主要支持 macOS，其他平台的支持仍在开发中
- 项目使用 Rust nightly 工具链，rust-toolchain.toml 已配置
- 托盘图标使用 `src/tray_icon.png`
- 全局热键默认绑定到 `Cmd+Shift+S`
