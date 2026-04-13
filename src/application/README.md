# src/application/

Action-Reducer-Effect 架构层，实现类 Elm/Redux 的单向数据流。**核心逻辑不依赖 GPUI**，全部可测试。

## 模块结构

### `state.rs` — 纯逻辑应用状态

从 `src/app_state.rs` 迁入。包含所有 GPUI-free 的状态定义和计算逻辑：

- **`AppSession`** — 顶层会话状态，组合各子状态
- **`ProviderStore`** — Provider 数据存储，提供 `find_by_id()` / `sync_custom_providers()` 等查询方法
- **`NavigationState`** — 导航状态（当前 tab、动画 generation）
- **`SettingsUiState`** — 设置窗口的临时 UI 状态
- **`DebugUiState`** — Debug Tab 状态
- **`SettingsTab`** — 设置窗口 Tab 枚举
- **`HeaderStatusKind`** — 头部状态徽章类型（Synced/Syncing/Stale/Offline）
- **`provider_panel_flags()`** — 面板可见性规则（单一真理来源）
- **`compute_popup_height()`** — 弹窗高度计算
- **`compute_header_status()`** — 头部状态文本计算

测试文件：`state_tests.rs`（854 行，覆盖全部子状态和计算逻辑）

### `action.rs` — 动作定义

- **`AppAction`** — 所有用户交互和系统事件的枚举（导航、设置变更、Provider 操作、调试等）
- **`SettingChange`** — 设置变更子枚举
- **`DebugNotificationKind`** — 调试通知类型

### `reducer.rs` — 纯函数状态变换

- **`reduce(session, action) → Vec<AppEffect>`** — 核心 reducer，将 action 转换为状态变更 + side effects
- **`build_config_sync_request()`** — 构建配置同步请求
- 内部函数：`apply_setting_change()` / `toggle_provider()` / `apply_refresh_event()` / `process_refresh_outcome()` / `sanitize_stale_refs()`

测试文件：`reducer_tests.rs`（1100+ 行，覆盖所有 action 分支）

### `effect.rs` — 副作用声明

- **`AppEffect`** — 两级副作用枚举（`Context(ContextEffect)` / `Common(CommonEffect)`）
  - `ContextEffect` — 需要 GPUI 上下文的 effect（Render / OpenSettingsWindow / OpenUrl / ApplyTrayIcon / QuitApp）
  - `CommonEffect` — GPUI-free 的 effect（PersistSettings / SendRefreshRequest / 通知 / 文件操作等）
  - `From<ContextEffect>` / `From<CommonEffect>` trait impl — reducer 使用 `SubEnum::Variant.into()` 风格构造
- **`TrayIconRequest`** — 托盘图标请求类型（Static/DynamicStatus）

### `selectors/` — 视图状态选择器

从 `AppSession` 中派生 ViewModel，供 UI 渲染使用：

| 文件 | 职责 |
|------|------|
| `mod.rs` | ViewModel 类型定义（含 `OverviewQuotaItem`）+ 公共 re-export |
| `tray.rs` | 弹窗面板 ViewModel（header / provider detail / nav / global actions） |
| `settings.rs` | 设置窗口 ViewModel（provider list / detail / available providers） |
| `debug.rs` | Debug Tab ViewModel（系统信息、日志捕获、调试刷新） |
| `format.rs` | 共享格式化函数（时间、百分比、quota 文本） |
| `*_tests.rs` | 各 selector 的单元测试 |

## 数据流

```
User Event / Background Event
  → AppAction
    → reduce(&mut AppSession, action)
      → Vec<AppEffect>
        → runtime/ 执行 effects
```

## 约束

- **不可导入 `gpui`** — 这是最核心的测试边界。所有类型必须是纯 Rust。
- Reducer 必须是**纯函数**（给定 state + action → 确定的 effects），便于测试。
- Effect handler 不得在执行期间再次调用 `dispatch_*()`（重入保护）。
