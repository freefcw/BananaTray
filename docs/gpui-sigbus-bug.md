# GPUI 宏编译器 Bug 问题记录

## 问题概述

**现象**：运行 `cargo test` 或 `cargo check --all-targets` 时，Rust 编译器崩溃，抛出 `SIGBUS: access to undefined memory` 错误。

**触发条件**：
- 使用 `adabraka-gpui` 0.5.1 版本
- 编译包含 `#[cfg(test)]` 模块的代码
- 特别是 `src/app/mod.rs` 中包含 GPUI 宏（如 `impl Render`）的文件

**错误堆栈特征**：
```
error: rustc interrupted by SIGBUS, printing backtrace
...
libadabraka_gpui_macros ... syn4stmt7parsing34 ... parse_within
... cycle encountered after 30 frames with period 35
... recursed 6 times
```

## 根本原因分析

### 技术细节

1. **宏展开递归**：`adabraka-gpui-macros` 在处理测试目标时，`syn` crate 解析代码进入无限递归
2. **栈溢出导致 SIGBUS**：递归在解析 `impl Render` 中的代码块时循环，最终栈溢出触发 SIGBUS
3. **仅影响测试目标**：bin 目标正常编译，只有 test 目标触发此 bug

### 触发代码模式

```rust
// src/app/mod.rs
#[cfg(test)]  // <-- 这个测试模块触发问题
mod tests {
    use super::*;  // 导入包含 GPUI Render 实现的模块
    // ... 测试代码
}

// 同文件中的 GPUI 宏使用
impl Render for AppView {  // <-- GPUI 宏展开
    fn render(...) { ... }
}
```

## 解决方案

### 核心策略

**将测试代码与 GPUI 宏分离**：
- 内联测试（`#[cfg(test)] mod tests`）→ 集成测试（`tests/*.rs`）
- 纯 bin crate → bin + lib 混合 crate

### 实施步骤

#### 1. 创建 lib.rs

新建 `src/lib.rs`，导出无需 GPUI 的纯逻辑模块：

```rust
//! BananaTray - 系统托盘配额监控应用
//!
//! 注意：这是一个 bin + lib 混合 crate，lib 部分主要供测试使用。

pub mod models;
pub mod providers;
pub mod settings_store;
pub mod theme;
pub mod utils;

// app 模块包含 GPUI 代码，测试时可能触发编译器 bug
// 因此默认不导出
#[cfg(feature = "app")]
pub mod app;
#[cfg(feature = "app")]
pub mod logging;
```

#### 2. 修改 Cargo.toml

添加 lib 和 bin 配置：

```toml
[lib]
name = "bananatray"
path = "src/lib.rs"

[[bin]]
name = "bananatray"
path = "src/main.rs"
```

#### 3. 创建外部测试文件

新建 `tests/state_tests.rs`，复制纯逻辑测试：

```rust
//! 集成测试 - 纯逻辑测试（避免 GPUI 宏干扰）

use bananatray::models::{...};

// 测试代码（从 src/app/mod.rs 迁移）
#[test]
fn store_find_existing() { ... }
```

#### 4. 清理原文件中的测试

删除 `src/app/mod.rs` 中的 `#[cfg(test)]` 模块。

#### 5. 更新 pre-commit 配置

修改 `.pre-commit-config.yaml`：

```yaml
# 修改前（触发 bug）
- id: cargo-test
  entry: cargo test --all-targets --all-features

# 修改后（仅运行集成测试）
- id: cargo-test
  name: cargo test (integration only)
  entry: cargo test --test state_tests --all-features
```

## 验证结果

### 通过的命令

```bash
# ✅ 基础检查
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features

# ✅ 集成测试
cargo test --test state_tests --all-features

# ✅ pre-commit
pre-commit run --all-files
```

### 测试覆盖

迁移的测试用例（10 个）：
- `store_find_existing` - ProviderStore 查找
- `store_find_missing` - 查找不存在的 provider
- `store_find_mut_modifies` - 可变查找修改
- `store_set_connection` - 设置连接状态
- `store_set_connection_missing_is_noop` - 安全设置
- `nav_switch_to_provider` - 导航切换
- `nav_switch_to_settings_preserves_last_provider` - 设置 tab 保留 provider
- `nav_fallback_when_current_disabled` - 禁用回退
- `nav_fallback_noop_when_not_current` - 非当前不处理
- `nav_fallback_no_other_enabled` - 无可用回退

## 上游状态

| 项目 | 状态 | 备注 |
|------|------|------|
| adabraka-gpui | 闭源 | 无法确认是否有修复 |
| 最新版本 | 0.5.1 | 当前仍受影响 |
| Rust 编译器 | 待报告 | 可能需要向 Rust 团队报告 |

## 后续维护建议

### 升级检查

定期运行以下命令检查 adabraka-gpui 更新：

```bash
cargo update -p adabraka-gpui
cargo test --all-targets  # 测试是否修复
```

### 如需恢复内联测试

当 bug 修复后，可以：
1. 删除 `tests/state_tests.rs`
2. 将测试代码迁回 `src/app/mod.rs`
3. 恢复 pre-commit 配置为 `--all-targets`

### 避免回归

**不要**在 `app/` 模块中添加 `#[cfg(test)]` 测试，直到上游修复确认。

如需新增测试：
1. 添加到 `tests/` 目录作为集成测试
2. 或使用纯数据类型（不导入 GPUI）在 lib 中测试

## 相关文件

- `src/lib.rs` - 新创建的 lib 入口
- `src/main.rs` - bin 入口（添加 `pub` 导出）
- `tests/state_tests.rs` - 迁移的集成测试
- `Cargo.toml` - bin + lib 配置
- `.pre-commit-config.yaml` - 更新后的 hook 配置

## 参考链接

- [Rust Issue: rustc SIGBUS on macOS](https://github.com/rust-lang/rust/issues)（待提交）
- [Cargo 测试目标文档](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#tests)
- [Rust 递归限制](https://doc.rust-lang.org/reference/attributes/limits.html)

---

**记录时间**：2026-03-28
**最后更新**：2026-03-28
**状态**：已解决（通过架构调整绕过）
