# Clean Code Review: BananaTray 全仓

> **Rethink（2026-09-07；2026-09-08 对照代码复核）**：本文件是取证底稿，不是待办清单。
> 决策以 [README.md](./README.md) 为准：当前没有产品项。
> 下文 P1（GNOME 徽章）、P2（Grok 来源）、P3 预览 USD、P8（命名时序策略）、P9（completion 分类）、P10（用户向错误边界）、P11（Kimi PATH）、P12（NewAPI 表单提示）：**已落地**。
> P4 双轨复制、P5/P6 按钮和下拉重复、P7 告警阈值、P13 计数器命名、P14 间距常量、P15 图标路径、P16 `FolderTrustRequired`：**不要求修**。
> `UpdateRequired` 已在 Claude CLI probe 使用，不要当死代码删。

## Summary

代码整体干净：注释讲不变量、魔法数多数已进 `PopupLayout` / settings 默认值、函数超过 100 行的生产路径很少。用户能看见的说错话已经收口。剩下最高杠杆的可维护性问题仍是**同一决策写了两遍**（自定义 Provider 管线、主按钮、下拉框）以及**字符串/数字没有名字却承担产品语义**（`source_label` 表、告警 10%）——按 README 都不修。架构层的双轨管线详见 [architecture-solid.md](./architecture-solid.md) P1，这里不重复展开。

## Findings

### P1: GNOME 与 App 对同一 `StatusLevel::Red` 文案不一致 — 已落地

- **原则**: DRY / 项目规范
- **位置**: `src/application/selectors/format.rs:256-262`；`gnome-shell-extension/quotaPresentation.js:79-90`；测试 `quotaPresentation.test.mjs`
- **级别**: 高（原）；**状态**: 已落地
- **问题（原状）**: GJS 曾把 `red` 显示为 `OUT`。Rust 把 Red 格式化成「偏低 / LOW」。
- **现状**: `statusBadgeLabel('red')` 现为 `LOW`，与 `format_quota_status_label` 对齐。`sortedQuotas` 次要键仍用 `used/limit`，按 README 不改。

### P2: `source_label` 靠裸字符串对齐，Grok 已回落到「自定义」 — 用户可见部分已落地

- **原则**: DRY / 命名
- **位置**: `src/providers/grok/mod.rs:34`；`src/application/selectors/format.rs:16-42`；测试 `format.rs:435-457`；守卫 `src/builtin_provider_manifest.rs:40-53`
- **级别**: 高（原）；**状态**: Grok 映射 + 全表扫描测试已落地
- **问题（原状）**: 翻译表没有 `"grok api"`，设置页副标题落到「自定义」。
- **现状**: `"grok api"` 已映射；漏下一家内置会让 `every_builtin_source_label_has_its_own_translation` 红。改成枚举按 README 不修。

### P3: 脚本表单在 View 里做产品判断，还默认货币为 USD — 预览已落地

- **原则**: 单一职责 / 魔法数字
- **位置**: `src/models/script_provider.rs:47-53`（`display_line`）；`script_provider_form.rs:34-68`（Cloudflare 启发式）、`:245-250`（每次 render 重算 ID）；模板 `script_provider_lifecycle.rs:52`
- **级别**: 高（原）；**状态**: 预览空单位已不猜货币
- **问题（仍在 View）**: 启发式、ID 生成、超时换算、校验仍在 GPUI View。生成脚本模板仍有 `or "USD"`，按 README 不管。
- **建议**: 迁出 UI 是结构债，按 README 不修。

### P4: NewAPI / Script 保存完成路径复制

- **原则**: DRY / 单一职责
- **位置**: `src/application/reducer/newapi.rs:100-141` vs `src/application/reducer/script_provider.rs:117-161`；`newapi_ops.rs` vs `script_provider_ops.rs`
- **级别**: 高（观察）；**状态**: 不要求修
- **问题**: 成功通知 + reload、失败时编辑回填 / 撤销预注册 + `PersistSettings`，两边结构相同、标识符不同。改 request_id 语义必须改两处。
- **建议**: 真要第三种自定义形态再抽 `save_finished` helper。不要合并表单类型。详见架构 P1。

### P5: 主操作按钮至少三套同名或平行实现

- **原则**: DRY / 命名
- **位置**: `src/ui/widgets/controls/action_button.rs:49-56`；`src/ui/views/provider_panel.rs:47-66`（同名 `render_action_button`）；`src/ui/settings_window/providers/script_provider_form.rs` 与 `newapi_form.rs` 的手写按钮行
- **级别**: 中
- **问题**: 已有 `ButtonVariant` / `ButtonSize`。托盘详情和两个表单仍手写圆角、hover、字号。grep `render_action_button` 会打到两套 API。
- **建议**: 表单走共享控件；托盘局部函数改名或删除。NewAPI/Script 按钮行合成一个 `render_form_actions`。
- **Why now**: 改主按钮视觉要改 4 处，且同名函数会让后续重构打偏。

### P6: Debug Tab 复制了一套 cadence 下拉

- **原则**: DRY / 单一职责
- **位置**: `src/ui/settings_window/debug_tab.rs:463-651`（`render_debug_provider_dropdown` + 103 行的 options）；对照 `src/ui/widgets/controls/cadence_dropdown.rs:36-118`
- **级别**: 中
- **问题**: 同一套 `deferred` + 绝对定位 + 勾选行。文件 862 行把日志级别、环境卡、通知测试、控制台、自定义下拉捆在一起。选中名还在 View 里 `find`。
- **建议**: 抽共享 dropdown；`selected_label` 放进 `DebugConsoleViewState`。
- **Why now**: Debug UI 每加一项就在这个文件底部再堆一块。

### P7: 告警与占位进度条的阈值没有名字

- **原则**: 魔法数字
- **位置**: `src/application/quota_alert.rs:14-50`（`<= 0.0` Exhausted，`<= 10.0` Low）；`src/application/selectors/tray.rs:391-397`（balance_only 假 bar：Green `0.8` / Yellow `0.4` / Red `0.1`）；header stale `< 60` 秒（`src/application/state.rs` `compute_header_status`）
- **级别**: 中
- **问题**: 告警与图标阈值错开是有意产品决策（文件头注释写了），但 10% 仍是裸字面量。假 bar 比例看起来像真实用量。
- **建议**: `ALERT_LOW_REMAINING_PCT`、`STALE_AFTER_SECS`、`BALANCE_PLACEHOLDER_BAR_*`。注释保留「为什么和 50%/20% 错开」。
- **Why now**: 调阈值时搜索 `10.0` 会命中大量无关浮点。

### P8: 退出与 debounce 超时散落 — 已落地

- **原则**: 魔法数字 / 项目规范
- **状态**: app shutdown、D-Bus、settings debounce、SettingsWriter 独立 Drop 和 settings 开窗延迟已集中到顶层 `src/timing.rs`。
- **现状**: 文档记录各时长的协议用途；有界等待与超时 detach 行为未改变。

### P9: `reduce` 前的 completion 白名单是手写列表 — 已落地

- **原则**: 结构清晰度 / OCP
- **状态**: 分类迁入 `AppAction::preserves_custom_provider_form_context()` 的穷尽 match；新增 Action 时编译器会要求重新判断。
- **测试**: 后台完成动作保留表单上下文，前台动作使旧上下文失效。

### P10: Cursor 用 `anyhow::Context` 冲掉结构化错误 — 已落地

- **原则**: 项目规范（Provider 返回 `ProviderError`）
- **位置**: `src/providers/cursor/mod.rs:45-53`；`src/providers/cursor/auth.rs:39-53,89-93`；分类 `src/providers/error.rs:151-183`
- **级别**: 中（原）；**状态**: 已落地
- **问题（原状）**: Cursor 把英文 `"Failed to read Cursor access token"` 送进界面。
- **现状**: 缺 token / JWT 坏走 `AuthRequired` / `SessionExpired` + `LoginApp`；普通 UI 统一走 `format_user_failure_message`，不再展示 `raw_detail`。MiniMax API 业务错误只保留稳定错误码，Kiro 解析失败不再保存完整 CLI 输出。

### P11: Kimi CLI 探测绕开共享 PATH 逻辑 — 已落地

- **原则**: DRY / 项目规范
- **位置**: `src/providers/kimi/auth.rs:11-13`；对照 `src/providers/common/cli.rs`
- **级别**: 中（原）；**状态**: 已落地
- **现状**: `kimi_cli_exists()` 已走 `cli::command_exists("kimi")`。

### P12: NewAPI 表单校验失败只打日志 — 已落地

- **原则**: 结构清晰度
- **位置**: `src/ui/settings_window/providers/newapi_form.rs:153-157,170-190`
- **级别**: 中（原）；**状态**: 已落地
- **现状**: 校验失败写入 `newapi_form_error` 并渲染。`collect_submit_action` 仍兼校验 + 构造，但用户能看见错误。

### P13: `SettingsUiState` 删除请求复用保存计数器名

- **原则**: 命名
- **位置**: `src/application/state.rs:543-548`
- **级别**: 低
- **问题**: `begin_custom_provider_delete` 递增 `custom_provider_save_request_id`。读代码的人会以为删除和保存共享业务语义，而不只是共享计数器。
- **建议**: 改名为 `custom_provider_request_id`，或删除用独立计数器。
- **Why now**: 下一次改 request_id 隔离时容易改错字段。

### P14: Overview 渲染仍有未走 `PopupLayout` 的间距

- **原则**: 魔法数字 / DRY
- **位置**: `src/models/layout.rs:16-80` 已集中常量；`src/ui/views/overview_panel.rs` 仍有 `mb(px(8.0))`、`gap(px(6.0))` 等字面量；设置窗 `mod.rs` 有 `viewport.height - px(100.0)`
- **级别**: 低
- **问题**: 高度计算用常量，绘制用字面量，改间距会漂。
- **建议**: Overview padding/gap 全部引用 `PopupLayout`；设置窗 chrome 同样抽常量。Debug/General 的 `ICON_BG_*` 设计稿色进 `Theme`。
- **Why now**: 不修也能工作；修高度拟合时才会踩。

### P15: `provider-unknown.svg` 路径复制

- **原则**: DRY
- **位置**: `src/application/selectors/settings.rs:32,52,145`；`src/application/selectors/tray.rs` 同类占位
- **级别**: 低
- **问题**: 禁用/缺失 Provider 的图标路径写了多遍。
- **建议**: 一个 `unknown_provider_icon()` 常量。
- **Why now**: 改资源路径时 selector 测试会漏。

### P16: 预留却未使用的 `ProviderError` 变体

- **原则**: YAGNI
- **位置**: `src/providers/error.rs:19-23`；Claude 使用点 `src/providers/claude/cli_probe.rs:199-200`
- **级别**: 低；**状态**: 不要求修。拆开看：`FolderTrustRequired` 仍预留未用；`UpdateRequired` 已在 Claude CLI probe 使用，不要当死代码删。
- **问题**: `FolderTrustRequired` 仍 `#[allow(dead_code)]`，关闭枚举的每一臂都要 Display / `to_failure` / i18n。
- **建议**: 真有 Claude trust-flow 再动 `FolderTrustRequired`。不要为对称再预留，也不要删 `UpdateRequired`。

## Good Patterns To Keep

- 领域注释写「为什么」，例如告警阈值与图标错开（`quota_alert.rs:7-12`）、layout 迁移必须带着 enabled 进 sidebar（`settings_store.rs:87-93`）。
- `PopupLayout` 把托盘高度从渲染里抽走，并记录 GPUI 空字符串仍占行高。
- `SettingsCapability::TokenInput` 让 Copilot 一类 Provider 加设置面板不必改 selector 分支。
- `define_unit_provider!` + 单文件起步、250 行再拆的阈值写在 README，避免过早 `auth/client/parser`。
- 密钥预览走 Unicode-safe helper，CI 禁 providers 内字面切片。
- selector 集中 i18n，Provider 返回 `QuotaLabelSpec` / `ProviderFailure`。
- 测试外置（`state_tests/`、`reducer_tests/`），所以 `state.rs` 840 行、`loader.rs` 1159 行看起来吓人，生产逻辑其实短。
- `reduce` 224 行是浅分发，不要按「函数 >100 行」拆它。

## Test Gaps

1. **内置 `source_label` 全映射** — 已落地：`builtin_provider_manifest.rs` `every_builtin_source_label_has_its_own_translation`。
2. **GJS 徽章** — 已落地：`quotaPresentation.test.mjs` 断言 `statusBadgeLabel('red') == 'LOW'`。
3. **脚本表单逻辑**：Cloudflare 启发式、ID 生成——只有真迁出 UI 时才需要 lib 测试。按 README 不迁。
4. **自定义 Provider save_finished**：只有真抽 helper 时才需要两边现有 `reducer_tests` 继续钉迟到失败 / `PersistSettings`。按 README 不抽。
5. **Cursor 错误分类** — 已落地：token / JWT 走 `AuthRequired` / `SessionExpired`。
6. **Kimi CLI 探测** — 已落地：`cli::command_exists`。

不需要为拆 `reduce()` 或抽 HTTP trait 写测试——那两件事不应该发生。
