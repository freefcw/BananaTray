# 分模块健康度

> **Rethink（2026-09-08 对照代码复核）**：这是模块地图，不是整改清单。要不要改代码，看 [README.md](./README.md)。当前没有产品项。

大文件不等于坏模块。下表按**生产代码职责是否单一**打分，不按 `wc -l`。

图例：健康 / 可维护但有债 / 痛点。本轮没有痛点模块。

## 总表

| 模块 | 判断 | 说明 |
|------|------|------|
| `application/reducer.rs` | 健康 | 浅分发器。224 行 match 是特性，不是胖函数。 |
| `application/reducer/*` | 可维护但有债 | 领域切开了；NewAPI/Script 镜像见架构 P1。`apply_refresh_event` 的热重载臂可抽。 |
| `application/state.rs` | 可维护但有债 | 会话根合理。`SettingsUiState` 偏杂；popup 高度策略可挪走。测试已外置。 |
| `application/selectors` | 健康偏债 | 文案集中正确。`source_label` 字符串表和 Vertex/Kilo kind 特判是 OCP 债。 |
| `application/quota_alert.rs` | 健康 | 只产领域事件。阈值应命名。 |
| `providers/ai_provider.rs` | 健康 | ISP 拆分干净。 |
| `providers/manager.rs` | 健康 | 深门面。848 行里测试从 313 起。不要按行数拆。 |
| `providers/custom/` | 健康 | schema/plan/extractor/loader 边界清楚。`loader.rs` 1159 行中测试从 413 起。 |
| `providers/*` 内置 | 健康偏债 | HTTP 骨架重复是允许的。Kiro raw log 是局部债。Cursor 登录态、Kimi PATH、Grok source_label 已收口。 |
| `providers/codeium_family` | 健康 | 共享 primitive，编排在 facade。不要合成 orchestrator。 |
| `refresh/` | 健康 | Scheduler / Coordinator / Worker 职责清楚。 |
| `runtime/` | 可维护但有债 | dispatch、重入、effect 路由好。Linux popup 字段和 gpu_cache 略越界。 |
| `bootstrap/` | 健康偏债 | `bootstrap.rs` 是薄 facade。真正接线在 `run_app`；D-Bus 泵还在 `dbus/`。 |
| `tray/` | 可维护但有债 | 文件已拆，`TrayController` 仍偏满。 |
| `ui/views` | 健康偏债 | 主路径吃 selector。托盘本地 `render_action_button` 同名。 |
| `ui/settings_window` | 可维护但有债 | NewAPI 校验失败已有表单提示；脚本预览不再默认 USD。启发式 / ID 仍在 View，Debug 仍自制下拉。按 README 不修。 |
| `ui/widgets` | 健康 | 控件在，调用方没用够。 |
| `models/` | 健康偏债 | 顶层 settings 已与 serde 分离；子结构仍带 serde。`QuotaLabelSpec` 封闭枚举是对的。 |
| `settings_store.rs` | 健康 | 迁移边界正确，测试长。 |
| `platform/` | 可维护但有债 | `auto_launch` 干净。`system.rs` 杂、反向依赖 providers CLI。`logging.rs` 把诊断 tail 焊在 logger 上。 |
| `dbus/` | 可维护但有债 | DTO 位置正确、iface 不持 AppState。action 桥越界到 bootstrap。 |
| `gnome-shell-extension/` | 可维护但有债 | 徽章已与 Rust 对齐（red → LOW）且有测试。次要排序仍用 `used/limit` 回退，按 README 不改。 |
| `theme/` / `utils/` / `i18n.rs` | 健康 | 无 SOLID 痛点。 |

## 不要被行数误导的文件

| 文件 | `wc -l` | 实际情况 |
|------|---------|----------|
| `src/providers/custom/loader.rs` | 1159 | 生产 load+validate ~410 行，其余测试。 |
| `src/application/reducer_tests/newapi.rs` | 1056 | 测试，不是神类。 |
| `src/settings_store.rs` | 884 | 生产到 ~487，其后是损坏文件等测试。 |
| `src/providers/manager.rs` | 848 | 生产 ~311 行。 |
| `src/application/state.rs` | 840 | 多个小结构体共文件，测试在 `state_tests/`。 |
| `src/ui/settings_window/debug_tab.rs` | 862 | 这是真的偏满：渲染 + 自制下拉。 |
| `src/application/selectors/format.rs` | 806 | 穷尽 i18n match，职责单一；问题在字符串协议不是长度。行数会随文案增长。 |
| `src/lib.rs` `run_app` | ~115 | 启动编排可见，可把 D-Bus/debug CLI 再推进 bootstrap，不必拆函数。 |

## 函数长度（生产路径，手工核对）

扫描器会把带 `{` 的字符串算进函数体，`expand_env_vars` 报 143 行是假阳性，真实约 40 行（`src/providers/custom/url.rs:27-66`）。

值得看的真实长函数：

| 函数 | 位置 | 约行数 | 处理 |
|------|------|--------|------|
| `reduce` | `reducer.rs:18` | 223 | **保持**，浅分发 |
| `render_general_tab` | `general_tab.rs:27` | 122 | UI 编排，可再切 section |
| `render_newapi_form` | `newapi_form.rs:48` | 121 | 与 script form 共享按钮行 |
| `run_app` | `lib.rs:38` | 115 | 启动步骤列表，可读 |
| `render_debug_provider_options` | `debug_tab.rs:549` | 103 | 自制下拉，按 README 不抽 |
| `render_script_provider_form` | `script_provider_form.rs:230` | 100 | 领域逻辑仍在 View，按 README 不迁 |
| `render_header` | `app_view.rs:100` | 100 | UI，可接受 |
| `apply_refresh_event` | `reducer/refresh.rs:148` | 93 | 含热重载臂，按 README 不拆 |
| `process_refresh_outcome` | `reducer/refresh.rs:66` | 81 | 边界，可按 Success/Failed 再拆，不要求 |

`>= 4` 参数的函数很多是 GPUI `render_*(..., theme, window, cx)`，**不要**为参数个数再包一层。保存完成已经用 `NewApiSaveCompletion` 收拢，删除/加载可以同样做。

## 分层依赖（本轮核实）

```
application  ↛  providers, gpui, runtime, ui
models       ↛  application, ui          （例外：test_helpers → copilot_settings_capability）
providers    ↛  application, ui
refresh      →  providers, models
runtime      →  application, providers, refresh, platform
bootstrap    →  runtime, tray, ui, dbus
ui           →  application, runtime, theme
dbus         →  application DTO；**越界** → bootstrap::dispatch_in_app
platform     →  **局部越界** providers::common::cli
```

主路径符合 `docs/architecture.md`。两处越界记在架构 P9，不是「整个分层塌了」。

## 每个 SOLID 原则在模块上的落点

**S** — 先看 `ui/settings_window/providers/script_provider_form.rs` 和 NewAPI/Script reducer，再看 `SettingsUiState`。不要从拆 `format.rs` 开始。

**O** — 加 Monitorable HTTP Provider：manifest + 模块 + 图标，路径是开闭的。来源文案现有全表扫描测试护着；新增 `AppAction` 时，completion 分类与 reducer 分发都由穷尽 match 强制复核。非监控 hint 仍需按类型扩展。

**L** — trait 默认 `refresh = NoData` + 默认 `capability = Monitorable` 互不约束，靠 manager 守卫。生产路径没有 panic。Cursor 登录态 / Copilot·OpenCode 权限失败已收成封闭 advice；普通 UI 不再展示 `raw_detail`，MiniMax API 错误只保留稳定错误码。

**I** — Provider 侧已经隔离。Shell 侧 `FullContextCapabilities` 略胖，不是优先项。

**D** — application 的依赖倒置是这个仓库最值钱的设计，不要为了「方便」让 selector 去调 `ProviderManager`。
