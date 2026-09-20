# Architecture Review: BananaTray 全仓 SOLID

> **Rethink（2026-09-07；2026-09-08 对照代码复核）**：本文件是取证底稿，不是待办清单。
> 决策以 [README.md](./README.md) 为准：当前没有产品项。
> 下文 P1（双轨管线）、P7（状态袋）、P9（分层越界）、P10（按类型特判文案）以及 Minor：**不评级、不要求修**。
> P2 / P3 的用户可见部分、P4 的空单位预览、P5（completion 分类）、P6（命名时序策略）和 P8（用户向错误边界）：已落地，见 README「已处理」和「本轮确认后收口」。不要把下面的 High 当成当前待办。

## Verdict

**混合偏健康**：核心边界（application 纯逻辑、providers 不碰 UI、runtime 是内核、bootstrap 是组合根）站得住，也有测试和文档护着。用户可见的展示分叉（GNOME 徽章、Grok 来源）已经收口。自定义 Provider 双轨管线和 `source_label` 字符串协议仍是结构债，**按 README 不修**——不是当前最高优先级。

## Architecture Map

```text
UI / GNOME Extension
        │  AppAction / D-Bus JSON
        ▼
 application/          ← 纯状态：reduce(session, action) → Vec<AppEffect>
   selectors/          ← ViewModel / D-Bus DTO / i18n 文案
        │
        ▼
 runtime/              ← dispatch + effect 执行 + AppState 组合容器
        │
   ┌────┼────────────┐
   ▼    ▼            ▼
bootstrap/        refresh/        providers/
组合根            调度+单飞        AiProvider 实现
 tray / dbus /    Scheduler       common / custom / 内置
 settings window  Coordinator
```

依赖方向（本轮 grep 核实）：

- `src/application/` **不** `use crate::providers`
- `src/providers/` **不** `use crate::application`
- UI / tray / bootstrap 发 `AppAction`，读 selector ViewModel
- 刷新与 runtime 持有 `ProviderManagerHandle`

复杂度本应藏在：`providers` 的认证/解析、`refresh` 的单飞与 generation、`application` 的会话不变量。结构上的泄漏点仍在图上两处：自定义 Provider 的「保存/删除/回滚」同时出现在 application + runtime + ui；展示字符串同时出现在 provider metadata、`format.rs`、GJS `quotaPresentation.js`。后者的用户可见分叉（GNOME OUT、Grok「自定义」）已经收口。

## Boundary

审查边界是**整仓产品路径**（默认 `app` feature 的托盘应用 + Linux GNOME 扩展），以 `docs/architecture.md` 声明的稳定边界为准。

假设：

- 近期会继续加内置 Provider，也可能再加第三种自定义 Provider 形态。
- 不假设要重写 Elm 管线；`AppAction` 单枚举 + 领域 reducer 是既定选择。

## Review Lenses

1. **边界与所有权** — runtime / bootstrap / ui / dbus 是否按文档各管各的。
2. **依赖方向** — application 是否保持 GPUI-free、providers-free。
3. **模块深度** — 大文件是深模块还是浅包装。
4. **变更放大** — 加一个 Provider / 一种自定义形态要改几处。
5. **接口稳定与扩展** — `AiProvider`、`SettingsCapability`、`ProviderError` 的 OCP 成本。
6. **错误边界** — 结构化错误是否在 facade 被冲成英文句子。

## Painful Center

**自定义 Provider 生命周期是两套平行宇宙。**

NewAPI 与 Script Provider 在这些层几乎 1:1 镜像：

| 层 | NewAPI | Script |
|----|--------|--------|
| Action | `EnterAddNewApi` … `CancelDeleteNewApi` | `EnterAddScriptProvider` … `CancelDeleteScriptProvider` |
| Effect | `NewApiEffect::{Save,Delete,Load}` | `ScriptProviderEffect::{Save,Delete,Load,Test}` |
| Reducer | `reducer/newapi.rs` | `reducer/script_provider.rs` |
| 纯函数 | `newapi_ops.rs` | `script_provider_ops.rs` |
| Runtime | `runtime/effects/newapi.rs` | `runtime/effects/script_provider.rs` |
| UI | `newapi_form.rs` | `script_provider_form.rs` |

保存失败回滚逻辑几乎逐行相同（编辑回填表单 vs 撤销预注册 + `PersistSettings`）：

- `src/application/reducer/newapi.rs:100-141`
- `src/application/reducer/script_provider.rs:117-161`

这不是「两个文件都挺长」。这是**同一个设计决策复制了两遍**。加第三种自定义形态（例如本地 OpenAPI 文件 Provider）会再复制一遍。局部命名、按钮样式、魔法数字都是次要的。

第二痛点是展示契约泄漏，见 P2 / P3。用户可见分叉已经收口；剩下的是字符串协议本身，按 README 不修。

## SOLID 对照

| 原则 | 现状 | 主要裂缝 |
|------|------|----------|
| **S 单一职责** | reducer 已按领域拆；`ProviderManager` 只做注册表；`QuotaAlertTracker` 只产领域事件 | NewAPI/Script 管线、`SettingsUiState` 杂物袋、脚本表单里的领域规则、退出协议散落 |
| **O 开闭** | 加内置 Provider：manifest + 实现 + 图标。`SettingsCapability::TokenInput` 驱动设置 UI；completion 分类由 `AppAction` 穷尽 match 保护 | `source_label` 字符串表、`format_non_monitoring_message` 按 `ProviderKind` 特判 |
| **L 里氏替换** | manager 对非 Monitorable 直接 `NoData`；Kilo/Vertex 不覆写 `refresh`；普通 UI 不展示 `raw_detail`，MiniMax API 错误只保留稳定错误码 | `FolderTrustRequired` 仍是预留变体 |
| **I 接口隔离** | `AiProvider` 与 `ProviderCapabilities` 分开；View-safe vs Full context 分开 | `FullContextCapabilities` 把开窗/图标/热键/退出绑在一个 trait 上 |
| **D 依赖倒置** | application → models；runtime 依赖能力抽象 | `dbus` 自己调 `bootstrap::dispatch_in_app`；`platform/system` 反向依赖 `providers::common::cli` |

## Options

| 方案 | 边界清晰 | 模块深度 | 迁移成本 | 风险 | 建议 |
|------|----------|----------|----------|------|------|
| **A. 保守：抽共享 lifecycle helper** | 中 | 加深 reducer 共享层 | 低 | 低 | 真要第三种形态再做 |
| **B. 强：`CustomProviderLifecycle` 动作族** | 高 | 一个生命周期模块藏掉镜像 | 中 | 中（要重钉测试） | 若确定要第三种自定义形态再做 |
| **C. 把 NewAPI/Script 合成一种表单类型** | 假清晰 | 把不同 payload 塞进一个神类型 | 高 | 高 | 拒绝 |
| **D. AppAction 再切家族枚举 + 二次 match** | 表面整齐 | 浅分发变回 `unreachable!` | 中 | 高 | 项目已经拒绝，继续拒绝 |

**若真要第三种自定义形态，做 A，不要做 C/D。** 共享的是「request_id 结算 + 成功通知/reload + 失败回滚预注册」，不是表单字段。现在只有两种形态，按 README 不要抽。B 在 A 之后才值得。

## Findings

### P1: 自定义 Provider 生命周期双轨复制 — Severity: High（观察，不要求修）

- **What I found**：`AppAction` 从 `EnterAddNewApi` 到 `CancelDeleteScriptProvider`（`src/application/action.rs:79-162`）是两套对称变体。`NewApiEffect` / `ScriptProviderEffect`（`src/application/effect.rs:97-143`）同构。`newapi_ops.rs` 与 `script_provider_ops.rs` 的 rollback / notification key 选择几乎 1:1。reducer 保存完成路径见上表。UI 设置区也按 capability 再分两支（`src/ui/settings_window/providers/detail/settings_section.rs:20-44`）。
- **APoSD / SOLID**：Information leakage + Repetition；OCP / SRP。
- **Why it adds complexity**：Change amplification。改「迟到失败不得覆盖新表单」必须在两套管线同时改对；漏一侧就是幽灵 Provider 或表单被旧结果打回。
- **Recommendation**：抽出 `finish_custom_provider_save` / `finish_custom_provider_delete`，由 NewAPI/Script reducer 注入：ID 回滚函数、i18n key、modal 变体。测试继续用现有 `reducer_tests/{newapi,script_provider}.rs` 钉行为。
- **Why-not / tradeoff**：
  - 为什么不维持现状：下一次改保存语义会再复制一次，历史已经证明这会发生（request_id、预注册、PersistSettings 回滚都是后来补上的）。
  - 为什么不直接上方案 B：现在只有两种形态，强行统一 action 会把 Script 独有的 Test 流和 NewAPI 的 identity 冲突守卫揉在一起，接口变浅。
  - **Red**：后人把「共享 helper」做成超长泛型，所有差异变成 callback 参数。**Blue**：helper 只收 3–4 个命名闭包/结构体字段，禁止 `dyn Fn` 森林。**残留**：UI 表单仍是两份，那是视图差异，可接受。

### P2: GNOME 徽章把 Red 渲染成耗尽 — 已落地

- **状态**：徽章文案已对齐。`statusBadgeLabel('red')` 现为 `LOW`（`quotaPresentation.js:79-90`），测试钉在 `quotaPresentation.test.mjs`。Rust 侧仍是 `format_quota_status_label`（`format.rs:256-262`）。
- **What I found（原状）**：D-Bus 只传 `status_level: "Green"|"Yellow"|"Red"`（`dbus_dto.rs:81-94,154-167`）。GJS 曾把 `red → OUT`。
- **残留**：`sortedQuotas` 次要键仍用 `(limit-used)/limit`（`quotaPresentation.js:123-132`）。按 README 不升协议、不改排序。

### P3: `source_label` 是跨层字符串协议，Grok 已经掉出表 — 用户可见部分已落地

- **状态**：`"grok api"` 已映射（`format.rs:37`）；`every_builtin_source_label_has_its_own_translation`（`builtin_provider_manifest.rs:40-53`）扫描全部内置，漏映射会红。点测在 `format.rs:435-457`。
- **What I found（仍准的结构债）**：各 Provider 在 metadata 里写裸英文（如 `src/providers/grok/mod.rs:34` `"grok api"`）。selector 再用另一张表翻译（`format.rs:16-42`）。加 Provider 仍要手补 match 臂，但现在有测试护着，不会再静默回落到「自定义」。
- **残留**：改成封闭枚举 / 来源类别按 README 不修。运行时 `runtime_source_label`（live vs cache）仍需要少量动态类别。

### P4: 脚本表单把领域规则放在 View — 预览 USD 已落地；其余不要求修

- **状态**：空单位预览已不猜货币（`src/models/script_provider.rs:47-53` `display_line`）。
- **What I found（仍在 View）**：Cloudflare 挑战启发式（`script_provider_form.rs:34-68`）、新增模式每次 render 重算 `unique_script_provider_id`（`:245-250`）。超时换算、空字段校验也在同一 View。生成脚本模板仍有 `or "USD"`（`script_provider_lifecycle.rs:52`），按 README 不管。
- **残留**：启发式 / ID / 校验迁出 UI 是结构债，按 README 不修。

### P5: 异步完成 skip-list 没有编译器保护 — 已落地

- `AppAction::preserves_custom_provider_form_context()` 现在使用穷尽 match 分类后台完成动作；新增 Action 时编译器会要求同步判断该生命周期语义。
- `reduce()` 只调用这个语义方法，不再维护第二份手写白名单。
- 代表性测试覆盖后台完成动作保留上下文、前台动作使上下文失效。

### P6: 退出生命周期时序散落 — 已落地

- app shutdown 60ms、D-Bus 20ms、settings debounce 500ms、SettingsWriter 独立 Drop 80ms 和设置窗口打开 10ms 已集中到顶层 `src/timing.rs`。
- 顶层位置避免由 `runtime` 反向拥有 settings window / D-Bus 的 shell 语义；`docs/architecture.md` 同步记录各时长的协议用途。
- 有界等待与超时 detach 语义保持不变，没有改成可能卡住退出的无限 join。

### P7: `SettingsUiState` 和确认态 ViewModel 在泄漏职责 — Severity: Medium

- **What I found**：`SettingsUiState`（`src/application/state.rs:491-521`）同时装 tab/选中/cadence、token 编辑、modal、脚本测试四字段、自定义保存/删除 request_id、热键错误对。`begin_custom_provider_delete` 复用 `custom_provider_save_request_id` 计数器（`543-548`）。类型层互斥的 `SettingsModalState`（`583-620`）到 ViewModel 又打成多个 bool（selector `confirming_delete_newapi` 等）。
- **APoSD / SOLID**：SRP / ISP。
- **Recommendation**：拆 `CustomProviderAsyncState`；删除用自己的计数器或共用改名为 `custom_provider_request_id`。详情 ViewModel 用 `confirm: Option<ConfirmKind>`。
- **Why-not**：不要拆 `AppSession`。它作为 Elm 会话根是深模块，字段组合是对的。

### P8: Provider facade 的错误契约被字符串旁路 — 已落地

- Cursor 缺 token / JWT 坏走 `AuthRequired` / `SessionExpired` + `LoginApp`；HTTP 继续走结构化 `classify`。
- 普通 UI 统一调用 `format_user_failure_message`，`Unavailable` / `ParseFailed` / `FetchFailed` / `NetworkFailed` 的 `raw_detail` 不再透传，并有表驱动测试。
- `FailureAdvice::ApiError` 改为只携带稳定 `code: i32`；MiniMax 不再保存或展示上游 `status_msg`。
- Kiro 解析失败不再把完整 CLI 输出写入诊断状态，只保留字节数。
- Debug 可展示的 `raw_detail` 契约明确为已脱敏技术细节；禁止响应正文、凭据、配置值或敏感用户数据。
- `FolderTrustRequired` 仍是预留变体；`UpdateRequired` 已在 Claude CLI probe 使用，不要误删。

### P9: 声明的 shell 边界有两处被打穿 — Severity: Medium

- **What I found**：
  - `dbus::spawn_action_bridge` 直接 `bootstrap::dispatch_in_app`（`src/dbus/mod.rs:141-176`）；`run_app` 直接 `dbus::start_dbus_service`（`src/lib.rs:127-132`）。文档要求 GPUI wiring 留在 bootstrap。
  - `AppState` 持有 Linux popup 抑制时钟和「是否该存位置」（`src/runtime/app_state.rs:34-37`），这是 tray 状态机，不是会话领域。
  - `platform/system.rs` 调用 `crate::providers::common::cli::run_command_with_timeout` 做深色模式探测。
- **APoSD / SOLID**：DIP / 所有权。
- **Recommendation**：D-Bus action 泵收到 `bootstrap/workers/`；Linux popup 字段挪到 `TrayController`；CLI 超时下沉到 `platform`，providers 再调用 platform。
- **Why-not**：不要为此引入「Shell Manager」——文档明确禁止 runtime-owned shell 服务。组合根可以变厚，但只能在 bootstrap。

### P10: 非监控文案按 `ProviderKind` 特判 — Severity: Medium

- **What I found**：`format_non_monitoring_message` 对 `Informational + VertexAi`、`Placeholder + Kilo` 各写死文案，其余走通用 hint（`src/application/selectors/format.rs:151-168`）。能力已经在 `ProviderCapabilities` 上，文案却绕回具体 kind。
- **APoSD / SOLID**：OCP。新的 Informational Provider 只能拿到泛化句子。
- **Recommendation**：在 capability / metadata 上带稳定 hint key，或接受通用文案、删掉 kind 特判。Vertex/Kilo 的特殊说明更适合设置页静态 copy，而不是 selector 分支。

### Minor

- `FullContextCapabilities` 把开窗/tray/热键/退出绑在一起，Window/App adapter 大量透传（`src/runtime/mod.rs:146-160`，`src/bootstrap/capabilities.rs`）。可抽平台副作用默认实现，不是重构重点。
- `QuotaInfo` 构造器过多（`with_key` / `from_remaining_*` / `balance_only*`，`src/models/quota/info.rs:42-253`）。这是为了藏百分比换算，属于深模块，不要为「函数太多」再包 Builder。
- `QuotaInfo::format_remaining_signed`（`info.rs:265`）是展示格式化留在 models。可迁到 selector；优先级低于 P2/P3。
- `models/test_helpers.rs` 为对齐 Copilot 设置能力调用 `providers::copilot_settings_capability()`，测试辅助反向依赖。挪到 application/providers 测试即可。
- `kimi_cli_exists` 已走 `cli::command_exists`（`src/providers/kimi/auth.rs:11-13`）。PATH 补全不再是债。
- Gemini 与 Vertex 各自解析 `~/.gemini/settings.json`（`gemini/auth.rs`，`vertex_ai.rs:21-39`）。抽一个 `gemini_cli_auth_mode()` enum，不要合并成一个 Provider。

## What Is Already Good

- **单层穷尽 `reduce`**（`src/application/reducer.rs:14-17`），新增变体编译失败，而不是运行时 `unreachable!`。
- **`SettingsModalState` 把互斥模态做成类型不变量**（`state.rs:583-620`）。
- **`AiProvider` / `ProviderCapabilities` 拆分**（`src/providers/ai_provider.rs:16-70`）；Token 设置面板由 capability 驱动（`token_input_panel.rs` 文件头就写了 OCP）。
- **编译期 Provider 清单**（`builtin_provider_manifest.rs`）+ kind 错配 panic。
- **application 不依赖 providers / gpui**；错误主路径 `ProviderError → to_failure → selector i18n`。
- **`codeium_family` 只共享 primitive，编排留在 facade**——这是正确的深模块，不是该抽的 orchestrator。
- **Effect 两级路由 + 错误 Full-context 立刻 panic**，禁止 warn 吞副作用。
- **dispatch 重入 RAII**（`src/runtime/mod.rs:85-113`）。
- **`AppState` 不持有 `SettingsView` / `DBusServiceHandle` / popup `WindowHandle`**。
- **持久化顶层 DTO 与 `AppSettings` 分离**（`settings_store.rs:98-120`），legacy layout 迁移有注释保护的产品不变量。
- **`PopupLayout` 把托盘高度魔法数收口**（`src/models/layout.rs`）。
- **Kilo Placeholder / Vertex Informational** 不伪装成刷新失败。
- **GJS 徽章与 Rust `format_quota_status_label` 对齐**（red → LOW），`quotaPresentation.test.mjs` 钉住。
- **内置 `source_label` 全映射测试**（`builtin_provider_manifest.rs`），漏一家会红。
- **Cursor 登录态失败走封闭 `FailureAdvice::LoginApp`**，不再把 `"Failed to read Cursor access token"` 送进 UI。

## Evidence Reviewed

打开并引用过的主要文件：

- `docs/architecture.md`，`src/application/README.md`，`src/providers/README.md`，`src/runtime/README.md`
- `src/lib.rs`，`src/application/{action,effect,reducer,state,mod}.rs`
- `src/application/reducer/{newapi,script_provider,refresh}.rs`
- `src/application/{newapi_ops,script_provider_ops,quota_alert}.rs`
- `src/application/selectors/{format,settings,dbus_dto,mod}.rs`
- `src/providers/{ai_provider,manager,error}.rs`
- `src/providers/{kimi,minimax,gemini,grok,cursor,kilo,vertex_ai}/**`
- `src/providers/custom/{loader,url,extractor}.rs`
- `src/runtime/{mod,app_state,background_job,settings_writer}.rs`
- `src/bootstrap.rs`，`src/bootstrap/event_sources/shutdown.rs`
- `src/dbus/mod.rs`，`src/tray/controller.rs`
- `src/ui/settings_window/{mod.rs,debug_tab.rs,providers/**}`
- `src/ui/views/{overview_panel,provider_panel}.rs`
- `src/models/{provider,layout,settings/mod,quota/{info,label,failure}}.rs`
- `src/settings_store.rs`，`src/refresh/coordinator.rs`
- `gnome-shell-extension/quotaPresentation.js`
- `locales/{en,zh-CN}.yml` 的 `provider.source_label.*`

实际跑过的搜索：`use crate::providers` in `application/`（无）；`use crate::application` in `providers/`（无）；`SettingsCapability::`；`display_source_label`；`ProviderKind::` in application；`format_remaining_signed`；函数行数扫描（`reduce` 224 行是浅分发；`loader.rs` 1159 行中测试从 413 起）。

2026-09-08 复核：GNOME 徽章、Grok 映射、脚本预览空单位、NewAPI 表单错误、Cursor 登录态、Kimi PATH、source_label 全映射测试，以及本轮错误展示边界 / completion 分类 / 命名时序策略均已在代码中。

## Next Step

当前没有产品项。GJS 徽章和 `source_label` 全映射测试已经落地。

不要抽 NewAPI/Script 的 `save_finished` helper，除非真要第三种自定义形态。不要从拆 `state.rs` 或抽 HTTP trait 开始。
