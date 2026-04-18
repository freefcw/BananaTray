# src/application/

Action-Reducer-Effect 架构层，实现类 Elm/Redux 的单向数据流。**核心逻辑不依赖 GPUI**，全部可测试。

## 模块结构

### `state.rs` — 纯逻辑应用状态

包含所有 GPUI-free 的状态定义和计算逻辑：

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

测试文件：`state_tests.rs`

### `action.rs` — 动作定义

- **`AppAction`** — 所有用户交互和系统事件的枚举（导航、设置变更、Provider 操作、调试等）
- **`SettingChange`** — 设置变更子枚举
- **`DebugNotificationKind`** — 调试通知类型

### `reducer.rs` — 纯函数状态变换

- **`reduce(session, action) → Vec<AppEffect>`** — 核心 reducer，将 action 转换为状态变更 + side effects
- **`build_config_sync_request()`** — 构建配置同步请求
- 内部函数：`apply_setting_change()` / `toggle_provider()` / `apply_refresh_event()` / `process_refresh_outcome()` / `cleanup_dangling_refs()`
- **自定义 Provider 自动注册**：`SubmitNewApi` 保存时通过 `models::newapi_provider_id()` 计算 ID 并预注册到 `enabled_providers` + `sidebar_providers`；YAML 生成和文件写入委托给 `SaveNewApiProvider` effect；`EditNewApi` 的磁盘读取委托给 `LoadNewApiConfig` effect
- **NewAPI 删除流**：`DeleteNewApi` 会先清空 `confirming_delete_newapi`，然后委托 `DeleteNewApiProvider` effect 执行磁盘删除

测试文件：`reducer_tests.rs`

### `effect.rs` — 副作用声明

- **`AppEffect`** — 两级副作用枚举（`Context(ContextEffect)` / `Common(CommonEffect)`）
  - `ContextEffect` — 需要 GPUI 上下文的 effect（Render / OpenSettingsWindow / OpenUrl / ApplyTrayIcon / QuitApp）
  - `CommonEffect` — GPUI-free 的 effect（PersistSettings / SendRefreshRequest / 通知 / SaveNewApiProvider / DeleteNewApiProvider / LoadNewApiConfig 等）
  - `From<ContextEffect>` / `From<CommonEffect>` trait impl — reducer 使用 `SubEnum::Variant.into()` 风格构造
- **`TrayIconRequest`** — 托盘图标请求类型（Static/DynamicStatus）

### `quota_alert.rs` — 配额告警领域状态机

- **`QuotaAlertTracker`** — 追踪各 Provider 的 quota 状态转换，产出告警事件
- **`QuotaAlert`** — 告警领域事件（LowQuota / Exhausted / Recovered）
- 该模块只表达“应该发什么告警”，不关心 OS 通知如何发送

### `newapi_ops.rs` — NewAPI 保存操作纯函数

从 `runtime/mod.rs` 的 `SaveNewApiProvider` handler 中提取的状态操作逻辑：

- **`rollback_newapi_edit()`** — 编辑模式失败回滚：从 config 重建 `NewApiEditData` 回填表单
- **`rollback_newapi_create()`** — 新增模式失败回滚：从 `enabled_providers` + `sidebar_providers` 中移除预注册 ID（而非写回 disabled）+ 恢复空表单 + 回退 `selected_provider`
- **`newapi_save_notification_keys()`** — 根据保存结果选择通知 i18n key（partial / edit_success / save_success）

本模块为纯函数，不包含 I/O 或 GPUI 依赖。生产构建中它只在 `app` feature 开启时参与编译；无 `app` 的 `lib` 本地测试场景仍会编译该模块以保留单元测试覆盖。

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

`application/mod.rs` 只 re-export 当前 UI/运行时直接依赖的 selector API，避免把仅供 selector 内部或测试使用的类型持续暴露在根模块 facade 上。

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
- **不可导入 `providers/`** — 避免 application → providers 的反向依赖。NewAPI 纯数据类型位于 `models/newapi.rs`。
- **不可导入 `platform/notification` 承载业务规则** — quota 告警状态机留在 application，platform 只负责通知发送适配。
- Reducer 必须是**纯函数**（给定 state + action → 确定的 effects），便于测试。
- 部分 CommonEffect handler（如 `LoadNewApiConfig`、`StartDebugRefresh`）会直接修改 `AppSession` 状态，这是异步 I/O 回填的必要 tradeoff。
- Effect handler 不得在执行期间再次调用 `dispatch_*()`（重入保护）。
