# src/runtime/

前台运行时与 effect 执行层。负责持有共享运行时状态、调用 reducer、执行 effects，并编排与 GPUI / tray / refresh / settings persistence 相关的前台行为。

## Responsibilities

- 持有共享状态 `AppState`
- 提供 `dispatch_*()` 入口，把 `AppAction` 送入 reducer
- 执行 `AppEffect`，把声明式 effect 转成真实副作用
- 管理设置窗口打开/复用调度
- 串行化设置持久化写入
- 通过 `ui_hooks` 与具体 UI 视图交互

## Boundaries

- `application/` 只声明状态变化与 effects，不执行副作用
- `runtime/` 执行副作用，但不再把 UI 句柄存进 `AppState`
- `ui/` 持有具体 GPUI 视图类型，并在启动时把必要 hooks 注册给 `runtime`

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
- **`AppCaps`** — `App` 的适配器，支持大部分能力，Render 实现为通过 `ui_hooks` 请求当前 popup view 刷新。

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

### `app_state.rs` — 共享运行时状态容器

定义 `AppState`，作为 `runtime` 与 `ui` 共同使用的组合状态：

- `session: AppSession` — 纯状态树
- `manager: Arc<ProviderManager>` — provider 运行时注册表
- `refresh_tx` — 后台刷新请求通道
- `settings_writer: SettingsWriter` — 设置持久化串行写入器
- `log_path` — Debug 页展示的日志路径

`AppState` 已从 `ui` 模块迁出到 `runtime`，这样 `runtime` 不再依赖 `ui::AppState`，`ui` 改为消费 `runtime::AppState`。弹窗视图弱引用与设置窗口构造入口通过 `ui_hooks` 注册到 `runtime`，避免把 UI 句柄直接存进 `AppState`。

### `ui_hooks.rs` — UI 注册边界

定义 `runtime` 与 `ui` 之间剩余的窄桥接层：

- 请求当前 popup view 重新渲染
- 清理 popup view 注册
- 构造 settings window 的 view entity

这些 hook 在 `bootstrap::bootstrap_ui()` 阶段由 `ui` 模块注册。

### `settings_window_opener.rs` — 设置窗口打开与复用

封装设置窗口的异步调度、窗口复用、多显示器选择与前台激活逻辑：

- **`schedule_open_settings_window()`** — 延迟到下一帧打开设置窗口，避免 effect handler 中立即建窗导致 `RefCell` 重入
- 内部维护 `SETTINGS_WINDOW` 句柄，优先复用现有窗口
- 跨显示器时自动关闭旧窗口并在目标显示器重建
- 通过 GPUI `tray_icon_anchor()` 获取托盘图标所在显示器
- 通过 `ui_hooks` 请求 UI 构造 settings view，`runtime` 不再依赖 UI 的窗口管理函数

### `settings_writer.rs` — 设置文件 Debounce 写入器

合并短时间内的多次 `PersistSettings` 请求，避免快速操作（拖拽排序、连续切换）时频繁写盘。

- **`SettingsWriter::spawn()`** — 启动后台写入线程，返回句柄（存放在 `AppState` 上）
- **`schedule(settings)`** — 异步 debounce 写入，500ms 窗口内合并多次调用，只写最后一份
- **`flush(settings)`** — 同步写入，立即落盘并返回结果，会打断未落盘的 debounce 窗口
- 所有写入（schedule 和 flush）通过同一个后台线程串行化，避免乱序覆盖

### `newapi_io.rs` — NewAPI YAML 文件 I/O

封装 `SaveNewApiProvider` 的磁盘写入操作：

- **`save_newapi_yaml(config, filename) → Result<PathBuf, String>`** — YAML 生成 + 目录创建 + 文件写入

回滚和通知逻辑位于 `application/newapi_ops.rs`（纯函数，可测试），本模块仅负责 I/O。

## 约束

- 本模块在 `cfg(feature = "app")` 下编译，依赖 GPUI
- Effect handler 中**不得**调用 `dispatch_*()` — 使用 `schedule_*` 延迟到下一轮事件循环
- 通知线程切换统一由 `platform::notification` 负责，runtime 只触发通知 effect，避免重复 `spawn`

## Data Flow

```text
AppAction
  -> dispatch_*()
  -> application::reduce(&mut AppState.session, action)
  -> Vec<AppEffect>
  -> run_context_effect / run_common_effect
  -> GPUI / tray / refresh / settings_store / providers side effects
```
