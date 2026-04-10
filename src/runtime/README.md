# src/runtime/

Effect 执行层（GPUI 桥接），将 reducer 产出的 `AppEffect` 转化为真实的 side effects。

## 核心函数

### Dispatch 入口（3 个 GPUI 上下文变体）

| 函数 | GPUI 上下文 | 使用场景 |
|------|-------------|---------|
| `dispatch_in_context<V>()` | `Context<V>` | View render 回调中（如按钮点击） |
| `dispatch_in_window()` | `Window + App` | 窗口级事件处理（如设置窗口操作） |
| `dispatch_in_app()` | `App` | 全局事件（如后台刷新事件泵） |

所有 dispatch 函数共享同一流程：
1. 借用 `AppState`，调用 `reduce(&mut session, action)` 得到 `Vec<AppEffect>`
2. 释放借用
3. 逐个执行 effects

### 重入保护

`dispatch_effects()` 使用 `thread_local!` RAII guard 检测重入，防止 effect handler 中再次 dispatch 导致 `RefCell` 重入 panic。需要延迟分派的场景使用 `schedule_*` 系列函数。

### Effect 执行

- **上下文相关 effects**：`Render` / `OpenSettingsWindow` / `OpenUrl` / `ApplyTrayIcon` / `QuitApp` — 根据可用的 GPUI 上下文执行或降级
- **通用 effects**（`run_common_effect`）：`PersistSettings` / `SendRefreshRequest` / `SyncAutoLaunch` / `ApplyLocale` / `UpdateLogLevel` / 通知 / 剪贴板 / 自定义 Provider YAML 操作 等

## 约束

- 本模块在 `cfg(feature = "app")` 下编译，依赖 GPUI
- Effect handler 中**不得**调用 `dispatch_*()` — 使用 `schedule_*` 延迟到下一轮事件循环
- 通知发送在独立线程中执行，防止 macOS 系统事件导致 GPUI RefCell 重入
