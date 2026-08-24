# src/application/

Action-Reducer-Effect 架构层，实现类 Elm/Redux 的单向数据流。**核心逻辑不依赖 GPUI**，全部可测试。

## 模块结构

### `state.rs` — 纯逻辑应用状态

包含所有 GPUI-free 的状态定义和计算逻辑：

- **`AppSession`** — 顶层会话状态，组合各子状态
  - `overview_expanded` — Overview 面板中展开显示全部配额的 provider id_key 集合。默认折叠，只活在本次进程内（不写 settings.json）；放在 session 而非 `AppView`，因为 macOS 每次关闭弹窗都会销毁 view。`is_overview_expanded()` / `toggle_overview_expanded()`
- **`ProviderStore`** — Provider 数据存储，提供 `find_by_id()` / `sync_custom_providers()` / `enabled_providers()` 等查询方法
  - `enabled_providers(&self, settings)` — 按设置顺序迭代所有已启用的 Provider，集中了 "custom_ids → ordered → filter enabled → find_by_id" 的公共遍历模式，供 `overview_view_state`、`AppSession::overview_card_rows`、`DBusQuotaSnapshot::from_session` 等多处复用。Overview 的窗口高度和窗口内容都必须走这里，否则两边对"谁被启用"的口径一旦分叉，就会出现死空白或提前滚动
- **`NavigationState`** — 导航状态（当前 tab、动画 generation）
- **`SettingsUiState`** — 设置窗口的临时 UI 状态（含 cadence dropdown、token 编辑目标、modal 状态机、脚本测试异步状态、全局热键错误及候选值回填）
- **`SettingsModalState`** — 设置页右侧面板的互斥模态状态机。把"添加 Provider 选择列表 / NewAPI 新增 / NewAPI 编辑回填 / 脚本 Provider 新增 / 脚本 Provider 编辑 / 移除二次确认 / 删除二次确认"这些原本散落的 bool/Option 字段折叠成单一 enum：
  - `Idle`、`AddingProvider`、`AddingNewApi`、`EditingNewApi(NewApiEditData)`、`AddingScriptProvider`、`EditingScriptProvider(ScriptProviderEditData)`、`ConfirmingRemoveProvider`、`ConfirmingDeleteNewApi`、`ConfirmingDeleteScriptProvider`
  - helper：`is_newapi_form()` / `is_script_provider_form()` / `is_adding_provider()` / `is_confirming_remove_provider()` / `is_confirming_delete_newapi()` / `is_confirming_delete_script_provider()` / `newapi_edit_data()` / `script_provider_edit_data()` / `form_identity()`
  - 互斥关系上升到类型层，reducer 不再需要 `set A = true; set B = false;` 的手工同步
- **`GlobalHotkeyError`** — 全局热键保存失败原因（空值 / 格式错误 / 缺少修饰键 / 预检冲突 / 注册失败）
- **`DebugUiState`** — Debug Tab 状态（选中的调试 Provider、下拉展开态、调试刷新与日志级别恢复）
- **`SettingsTab`** — 设置窗口 Tab 枚举
- **`HeaderStatusKind`** — 头部状态徽章类型（Synced/Syncing/Stale/Offline）
- **`provider_panel_flags()`** — 面板可见性规则（单一真理来源）
- **`compute_popup_height()`** — Provider 面板的弹窗高度计算（配额卡片 + 账户信息 / dashboard 行）。Overview 面板不走这里：它的高度取决于 session 内的展开记忆，由 `AppSession::popup_height()` 分派到 `overview_card_rows()` + `models::compute_popup_height_for_overview()`，展开卡片时窗口会跟着长高。`overview_provider_renders_quotas()` 是 selector 与高度计算共用的状态契约：只有当前真正渲染配额内容的 Connected / 缓存 Error 卡片才按展开行数计高，Refreshing / Disconnected / informational 卡片始终按单行状态卡计高
- **`compute_header_status()`** — 头部状态文本计算

测试目录：`state_tests/`（按域拆分，共享 fixture 在 `common.rs`）

### `action.rs` — 动作定义

- **`AppAction`** — 所有用户交互和系统事件的枚举（导航、设置变更、Provider 操作、调试等）
  - `SubmitNewApi(NewApiConfig)` 以领域配置对象承载完整表单提交；新增 NewAPI 字段只需扩展 `NewApiConfig`，不再同步修改 action 字段列表
  - `SaveGlobalHotkey(String)` 将 General Tab 捕获到的候选热键提交给 runtime 做预检、重绑和持久化
  - `*Finished` action（如 `NewApiSaveFinished` / `ScriptProviderDeleteFinished`）承接 runtime I/O 结果，reducer 统一决定状态回滚、通知、render 和 reload
- **`SettingChange`** — 设置变更子枚举
- **`DebugNotificationKind`** — 调试通知类型

### `reducer.rs` / `reducer/` — 纯函数状态变换

- **`reduce(session, action) → Vec<AppEffect>`** — 核心 reducer；顶层使用单个穷尽 match 直接解构 action 并调用对应领域函数，不再通过家族 dispatcher 的 `_ => unreachable!` 二次分派。新增 `AppAction` 变体时，编译器会要求补齐唯一分派入口
- **顶层 `reducer.rs` 只做单层穷尽 action 分发**，具体状态变换按领域拆到子 reducer：
  - `reducer/settings.rs` — 导航 / 设置窗口通用 UI 状态 / `SettingChange` / 全局热键 / 弹窗可见性
  - `reducer/provider_sidebar.rs` — Provider 开关、设置页 Provider 选择、token 编辑、sidebar 增删和排序
  - `reducer/refresh.rs` — 手动刷新、刷新事件、Provider 热重载，以及热重载后的悬空引用清理
  - `reducer/newapi.rs` — NewAPI 新增 / 编辑 / 删除表单流与对应 effect 发射
  - `reducer/script_provider.rs` — 自定义脚本 Provider 新增 / 编辑 / 测试 / 删除表单流与对应 effect 发射
  - `reducer/debug.rs` — Debug Tab 操作、调试刷新、日志和调试通知
  - `reducer/shared.rs` — 跨子 reducer 共享的纯 helper，如 `build_config_sync_request()`、刷新能力判断、动态图标同步
- **全局热键保存流**：`SaveGlobalHotkey` 不直接修改 `settings.system.global_hotkey`；reducer 发出 `ContextEffect::ApplyGlobalHotkey`，runtime 完成平台冲突 probe、注册和同步持久化后返回 `GlobalHotkeyApplyFinished`。只有保存成功时 reducer 才提交新值；持久化失败会恢复旧平台热键并保留旧设置。其中 macOS 使用 `RegisterEventHotKey` 的系统级注册路径。
- **自定义 Provider 自动注册**：`SubmitNewApi` 保存时通过 `models::newapi_provider_id()` 计算 ID（含 user_id 维度，同站多账号为 `{slug}-{user}:newapi`）并预注册到 `enabled_providers` + `sidebar_providers`；新增模式下身份（站点 + 账号）已被占用时拒绝保存并通知用户改用编辑，不静默覆盖 YAML；编辑模式下 Provider 身份始终来自 `SettingsModalState::EditingNewApi` 的原始 `base_url` / `original_filename` / `original_id`（编辑保持身份不变，不随 user_id 修改迁移），不信任 action payload 修改身份；YAML 生成和文件写入委托给 `NewApiEffect::SaveProvider`；runtime 回传 `NewApiSaveFinished` 后，reducer 再统一通知、reload 或回滚
- **NewAPI 删除 / 加载流**：`DeleteNewApi` 会先把 `SettingsModalState::ConfirmingDeleteNewApi` 恢复为 `Idle`，然后委托 `NewApiEffect::DeleteProvider` 执行磁盘删除；`EditNewApi` 委托 `NewApiEffect::LoadConfig` 读取 YAML，runtime 通过 `NewApiLoadFinished` 回填编辑态或失败通知
- **脚本 Provider 流**：`SubmitScriptProvider` 预注册 `{slug}:script` custom provider 并委托 `ScriptProviderEffect::SaveProvider` 写入脚本 + YAML；`TestScriptProvider` 只发送后台测试请求，不持久化，完成或排队失败都由 `ScriptProviderTestFinished` 回填结果；`EditScriptProvider` / `DeleteScriptProvider` 的磁盘 I/O 都在 runtime effect 中执行，并通过 `ScriptProvider*Finished` action 回到 reducer

测试目录：`reducer_tests/`（按 settings / refresh / provider_sidebar / newapi / script_provider / debug 拆分）

### `effect.rs` — 副作用声明

- **`AppEffect`** — 两级副作用枚举（`Context(ContextEffect)` / `Common(CommonEffect)`）
  - `ContextEffect` — 需要 GPUI 上下文的 effect（Render / OpenSettingsWindow / OpenUrl / ApplyTrayIcon / ApplyGlobalHotkey / QuitApp）
  - `CommonEffect` — GPUI-free 的领域路由 effect（Settings / Notification / Refresh / Debug / NewApi / ScriptProvider）
  - 领域子枚举：`SettingsEffect`、`NotificationEffect`、`RefreshEffect`、`DebugEffect`、`NewApiEffect`、`ScriptProviderEffect`
  - `From<ContextEffect>` / `From<CommonEffect>` / `From<领域子枚举>` trait impl — reducer 使用 `SubEnum::Variant.into()` 风格构造
- **`TrayIconRequest`** — 托盘图标请求类型（Static/DynamicStatus）

### `quota_alert.rs` — 配额告警领域状态机

- **`QuotaAlertTracker`** — 追踪各 Provider 的 quota 状态转换，产出告警事件；通知阈值（剩余 ≤10% Low / =0% Exhausted）有意低于托盘图标状态阈值（50%/20%），早预警靠图标、晚警报靠通知，详见 `quota_alert.rs` 顶部注释
- **`QuotaAlert`** — 告警领域事件（LowQuota / Exhausted / Recovered）
- 该模块只表达“应该发什么告警”，不关心 OS 通知如何发送

### `newapi_ops.rs` — NewAPI 保存操作纯函数

NewAPI 保存完成后由 reducer 调用的纯状态操作逻辑：

- **`rollback_newapi_edit()`** — 编辑模式失败回滚：从 config 重建 `NewApiEditData` 回填表单
- **`rollback_newapi_create()`** — 新增模式失败回滚：从 `enabled_providers` + `sidebar_providers` 中移除预注册 ID（而非写回 disabled）+ 恢复空表单 + 回退 `selected_provider`
- **`newapi_save_notification_keys()`** — 根据保存成功结果选择通知 i18n key（partial / edit_success / save_success）
- **`newapi_save_failed_notification_keys()`** — YAML 写入失败并回滚表单后使用的失败通知 key。
- **`newapi_load_failed_notification_keys()`** — 编辑态 YAML 读取失败时使用的失败通知 key。

本模块为纯函数，不包含 I/O 或 GPUI 依赖。生产构建中它只在 `app` feature 开启时参与编译；无 `app` 的 `lib` 本地测试场景仍会编译该模块以保留单元测试覆盖。

### `script_provider_ops.rs` — 脚本 Provider 保存操作纯函数

脚本 Provider 保存完成后由 reducer 调用的状态回滚和通知 key 选择：

- **`rollback_script_provider_edit()`** — 编辑模式失败时保留原表单数据和原文件名。
- **`rollback_script_provider_create()`** — 新增模式失败时移除预注册 provider，并恢复添加表单。
- **`script_provider_save_notification_keys()`** — 根据保存成功结果选择通知 i18n key（partial / edit_success / save_success）。
- **`script_provider_save_failed_notification_keys()`** — 脚本 / YAML 写入失败并回滚表单后使用的失败通知 key。

脚本执行和文件读写在 `runtime/effects/script_provider.rs` 中完成；provider reload、用户通知、表单回滚由 runtime 回传完成 action 后在 reducer 中声明为 effects。

### `selectors/` — 视图状态选择器

从 `AppSession` 中派生 ViewModel，供 UI 渲染使用：

| 文件 | 职责 |
|------|------|
| `mod.rs` | ViewModel 类型定义（含 `OverviewQuotaItem`）+ 公共 re-export（含 D-Bus DTO） |
| `tray.rs` | 弹窗面板 ViewModel（header / provider detail / nav / global actions） |
| `settings.rs` | 设置窗口 ViewModel（provider list / detail / available providers / 右侧面板 enum）；表单类 right pane 显式携带 `FormIdentity`，供 UI 判断输入缓存是否可复用 |
| `dbus_dto.rs` | D-Bus JSON DTO（`DBusQuotaSnapshot` 等）+ 格式化函数，跨平台可测试 |
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
          → 可选返回后续 AppAction（I/O completion / enqueue failure）
          → reduce(...) 继续处理完成事件
```

## 约束

- **不可导入 `gpui`** — 这是最核心的测试边界。所有类型必须是纯 Rust。
- **不可导入 `providers/`** — 避免 application → providers 的反向依赖。NewAPI 纯数据类型位于 `models/newapi.rs`，脚本 Provider 纯数据类型位于 `models/script_provider.rs`。
- **不可导入 `platform/notification` 承载业务规则** — quota 告警状态机留在 application，platform 只负责通知发送适配。
- Reducer 必须是**纯函数**（给定 state + action → 确定的 effects），便于测试。
- CommonEffect handler 需要同步回到状态机时应返回后续 `AppAction`，不要直接修改 `AppSession`；少数 runtime-only 临时状态（如 `DebugEffect::StartRefresh` 保存日志级别）是明确例外。
- Effect handler 不得在执行期间再次调用 `dispatch_*()`（重入保护）。
