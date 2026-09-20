# BananaTray 代码审查（rethink 后）

上次把「结构上不够漂亮」和「用户会碰到」混在一起了。这份是纠偏后的结论。

详细底稿仍在 [architecture-solid.md](./architecture-solid.md)、[clean-code.md](./clean-code.md)、[modules.md](./modules.md)，**那些文件里标了高/中/低的条目，多数不值得单开。以本页为准。**

2026-09-08 对照代码复核：原先「值得处理」的 5 条用户可见问题已经落地。当前没有新的、有业务证据的必修项。

## 一句话

这个仓库的主结构是清楚的：界面发动作，纯逻辑改状态，后台去拉配额。现在不该做一场 SOLID 大重构。

用户曾经能看见的说错话、点了没反应，已经修完。

## 已处理（用户曾经能碰到）

### 1. Linux GNOME 面板曾把「配额偏低」写成「用完了」

配额还剩不到 20% 时，内部状态是红。托盘写成「偏低」，GNOME 面板曾写成 `OUT`（用完）。

已改：GJS `statusBadgeLabel('red')` 现为 `LOW`，与 `format_quota_status_label` 对齐，并有 `quotaPresentation.test.mjs` 钉住。不必升协议、不必动 Rust 核心。次要排序仍用 `used/limit` 回退，按当时决定不管。

### 2. Grok 的数据来源曾显示成「自定义」

Grok 标明来源是 `"grok api"`，设置页翻译表曾经没有这一条，副标题落到「自定义 / Custom」。

已改：补上 `provider.source_label.grok_api` 映射；`every_builtin_source_label_has_its_own_translation` 扫描全部内置，漏下一家会红。字符串表本身仍是结构债，按下面「不用动」不改成枚举。

### 3. 脚本 Provider 测试预览：没写单位时曾显示 USD

测试成功后的预览，单位为空曾写成 `"USD"`。

已改：`ScriptProviderQuotaPreview::display_line` 空单位不再猜货币。生成脚本模板里仍有 `or "USD"`（`script_provider_lifecycle.rs`），那是模板默认值，不是预览路径；没有单独的用户投诉，不管。

### 4. NewAPI 点保存，必填没填时界面曾没反应

名称 / 地址 / Cookie 为空，或除数填错，曾经只打日志。

已改：校验失败写入 `newapi_form_error` 并在表单上渲染。

### 5. Cursor 失败时曾弹出英文技术句

读 token / 解析 JWT 失败时，错误曾带着 `"Failed to read Cursor access token"` 这类英文进界面。

已改：缺 token / JWT 坏走 `AuthRequired` / `SessionExpired` + `LoginApp { "Cursor" }`。普通用户向错误统一走 `format_user_failure_message`，不再显示 `raw_detail`；Debug 只允许展示已经脱敏的技术细节。

顺手收掉、同样不再追踪：Kimi CLI 探测已走 `common::cli::command_exists`；Copilot / OpenCode 权限失败已收成封闭 `FailureAdvice`。

## 本轮确认后收口

- `FailureAdvice::ApiError` 不再保存 MiniMax 上游 `status_msg`，只保留稳定错误码；普通 UI 不会再透传上游英文。
- Kiro 解析失败不再把完整 CLI 输出放进诊断状态，只记录输出字节数。
- `AppAction::preserves_custom_provider_form_context()` 使用穷尽 match 保护后台完成动作分类，并有代表性测试。
- 退出、D-Bus、设置写入 debounce、SettingsWriter 独立销毁和设置窗口打开时序集中到顶层 `src/timing.rs`。

## 看过了，现在不用动

下面这些上次写进审查里了。从业务上看，**没有正在发生的用户问题**，不评级、不要求修。

| 上次说的 | 为什么先放着 |
|----------|----------------|
| NewAPI 和脚本保存流程很像，该抽公共函数 | 两边已经各自能保存、回滚、防幽灵 Provider。抽函数是给以后加第三种自定义类型准备的，现在没有第三种。 |
| 设置页状态袋太大、确认框用了三个 bool | 设置页能用。 |
| 主按钮写了三遍、Debug 下拉和刷新间隔下拉长得像 | 样子可能略不一致，不是功能坏了。 |
| 告警 10%、假进度条 0.8 | 产品故意：图标早预警、通知晚打断。改名字不改变用户看到的东西。 |
| D-Bus 接线不在 bootstrap、托盘状态放进了 AppState | 分层偏好。面板配额该怎么显示，不取决于文件在哪。 |
| Vertex / Kilo 的说明文案写死了类型名 | 现在只有这两家是「只展示不监控」，文案是对的。 |
| `FolderTrustRequired` 预留未用 | 用户碰不到。`UpdateRequired` 已在 Claude CLI probe 使用，不要当死代码删。 |
| 按行数拆 `loader.rs` / `ProviderManager` | 多出来的是测试。 |
| GNOME `sortedQuotas` 次要键仍用 `used/limit` | 徽章已对齐。当时明确不升协议。 |
| 生成脚本模板默认 `unit = "USD"` | 预览路径已不猜货币。模板是用户可改的样例。 |

加一家新的内置监控对象：改清单、写实现、放图标。这条路是通的，不要为审查去抽 HTTP 公共接口，也不要拆开现在的 `reduce` 分发。

## 当前没有产品项

不要顺着上次的 16 条清单做结构改造。也不要抽 NewAPI/Script 的 `save_finished` helper，除非真要第三种自定义形态。
