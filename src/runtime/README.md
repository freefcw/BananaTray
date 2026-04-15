# src/runtime/

Effect 执行层（GPUI 桥接），将 reducer 产出的 `AppEffect` 转化为真实的 side effects。

## 两级路由架构

`AppEffect` 由两个子枚举组成：

| 子枚举 | 职责 | 新增时改动 |
|--------|------|-----------|
| `ContextEffect` | 需要 GPUI 上下文（Render, OpenSettingsWindow, OpenUrl, ApplyTrayIcon, QuitApp） | effect.rs 定义 + `run_context_effect`（必要时补 adapter override） |
| `CommonEffect` | GPUI-free（PersistSettings, SendRefreshRequest, 通知, 文件操作等） | effect.rs 定义 + `run_common_effect` 实现 |

三路 dispatcher 统一使用两级路由。`CommonEffect` 委托给 `run_common_effect` 处理；`ContextEffect` 则由于各入口能力差异，使用 **Capability Trait** 模式进行收敛。

### ContextCapabilities 模式

为了实现 `ContextEffect` 执行逻辑的收敛，定义了 `ContextCapabilities` trait 及其三个 Adapter 实现：

- **`ContextCapabilities`** — 抽象能力（Render, OpenSettingsWindow, OpenUrl, ApplyTrayIcon, QuitApp）。不支持的能力默认提供 `warn!` 告警实现。
- **`ViewCaps`** — `Context<V>` 的适配器，显式实现 `render`；其余能力走 trait 默认实现（`open_url` 可直接复用默认平台调用，其它能力记录告警）。
- **`WindowCaps`** — `Window + App` 的适配器，覆盖窗口场景需要的完整能力，`render` 实现为 `window.refresh()`。
- **`AppCaps`** — `App` 的适配器，支持大部分能力，Render 实现为触发 view entity 事件通知。

在这种模式下，新增一个 `ContextEffect` 变体只需在 `run_context_effect()` 的单一 `match` 中增加分支；如果不同入口存在行为差异，再在 trait 默认实现或 adapter override 中补齐。

### Dispatch 入口

| 函数 | GPUI 上下文 | 使用场景 |
|------|-------------|---------|
| `dispatch_in_context<V>()` | `Context<V>` | View render 回调中（如按钮点击） |
| `dispatch_in_window()` | `Window + App` | 窗口级事件处理（如设置窗口操作） |
| `dispatch_in_app()` | `App` | 全局事件（如后台刷新事件泵） |

所有 dispatch 函数共享同一流程：
1. 借用 `AppState`，调用 `reduce(&mut session, action)` 得到 `Vec<AppEffect>`
2. 释放借用
3. 将相应上下文包装进 Adapter
4. 逐个执行 effects (通过 `run_context_effect` 或 `run_common_effect`)

### 重入保护

`dispatch_effects()` 使用 `thread_local!` RAII guard 检测重入，防止 effect handler 中再次 dispatch 导致 `RefCell` 重入 panic。需要延迟分派的场景使用 `schedule_*` 系列函数。

## 子模块

### `newapi_io.rs` — NewAPI YAML 文件 I/O

封装 `SaveNewApiProvider` 的磁盘写入操作：

- **`save_newapi_yaml(config, filename) → Result<PathBuf, String>`** — YAML 生成 + 目录创建 + 文件写入

回滚和通知逻辑位于 `application/newapi_ops.rs`（纯函数，可测试），本模块仅负责 I/O。

## 约束

- 本模块在 `cfg(feature = "app")` 下编译，依赖 GPUI
- Effect handler 中**不得**调用 `dispatch_*()` — 使用 `schedule_*` 延迟到下一轮事件循环
- 通知发送在独立线程中执行，防止 macOS 系统事件导致 GPUI RefCell 重入
