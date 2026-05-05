# Providers

本文件描述 BananaTray 当前的 provider 模型和扩展边界。

它关注的是“有哪些 provider、provider 必须返回什么语义、如何新增一个 provider”，而不是每个 provider 的具体目录结构或内部实现文件。

## Built-in Providers

当前内置 14 个 provider，外加 YAML 自定义 provider。

| Provider | 设置 / 配置里的稳定 key | 主要数据来源 | Capability | 备注 |
|----------|--------------------------|--------------|------------|------|
| Claude | `claude` | HTTP API + CLI fallback | `Monitorable` | 多 source 编排 |
| Gemini | `gemini` | HTTP API | `Monitorable` | |
| Copilot | `copilot` | HTTP API | `Monitorable` | 支持 token 输入面板；保存的 `github_token` 会通过 refresh `UpdateConfig` 同步到后台运行时 |
| Codex | `codex` | HTTP API + CLI fallback | `Monitorable` | 读取 `~/.codex/auth.json`，解析 OAuth `id_token` 填充 email/plan；刷新时自动轮转 `id_token` 并注入 `ChatGPT-Account-Id` 以支持多账号；可通过 `~/.codex/config.toml` 的 `chatgpt_base_url` 切换自托管 ChatGPT 网关；OAuth 出现 timeout / 网络错误 / 5xx 时自动兑底到 CLI，顺序为 `codex app-server` JSON-RPC → PTY `/status`（429 限流不 fallback，因 CLI 共用同一 token 会撞同一限流） |
| Kimi | `kimi` | HTTP API | `Monitorable` | |
| Amp | `amp` | CLI | `Monitorable` | |
| Cursor | `cursor` | HTTP API + 本地数据 | `Monitorable` | |
| MiniMax | `minimax` | HTTP API | `Monitorable` | |
| Kiro | `kiro` | CLI | `Monitorable` | Credits / Bonus Credits 显示为积分（`X.XX / Y.YY`），不带 `$` 前缀；底层 `QuotaType::Points` |
| Antigravity | `antigravity` | 本地服务 + 本地缓存回退 | `Monitorable` | provider facade 自己编排 `live -> cache`，见 `antigravity-api.md` |
| Windsurf | `windsurf` | seat API + 本地服务 + 本地缓存回退 | `Monitorable` | provider facade 自己编排 `seat -> live -> cache`；seat API 的日 / 周配额优先，若 seat 缺周配额才用本地缓存补周配额。见 `antigravity-api.md` |
| Kilo | `kilo` | 占位 / 安装检测 | `Placeholder` | 只保留 provider 入口与环境检测，不参与正常刷新 |
| OpenCode | `opencode` | 占位 / 安装检测 | `Placeholder` | 只保留 provider 入口与环境检测，不参与正常刷新 |
| Vertex AI | `vertexai` | Gemini CLI 配置检测 | `Informational` | 说明 Gemini CLI 的 Vertex AI 认证路径，本身不直接抓取配额 |

## Custom Providers

自定义 provider 通过 YAML 声明，不需要新增 Rust 代码。

规范目录：

- macOS: `~/Library/Application Support/BananaTray/providers/`
- Linux: `$XDG_CONFIG_HOME/bananatray/providers/`

补充说明：

- 手工新增或编辑 YAML 后，当前通常需要重启应用才能重新加载。
- 应用内通过 NewAPI 表单保存 / 删除 provider 时，会显式触发 reload。
- `source: placeholder` 的自定义 provider 会被标记为 `Placeholder`，仅保留展示入口和可用性检查，不参与正常刷新。
- 详细 Schema 和示例见 `custom-provider.md` 与 `docs/examples/`。

## Stable Provider Contract

每个 provider 都遵守同一组稳定边界：

- 提供身份与展示元数据
- 提供能力层级（是否属于真正可监控 provider）
- 提供可用性检查
- 对 `Monitorable` provider 提供刷新能力
- 可选地声明设置页交互能力

### Provider Identity

当前有两层 provider 标识，不能混用：

- **设置 / 状态稳定 key**：内置 provider 使用 `ProviderKind::id_key()`，并通过 `ProviderId::BuiltIn(kind)` 进入 settings、refresh、sidebar、quota visibility 等状态；例如 `codex`、`windsurf`、`vertexai`。
- **Descriptor ID**：`AiProvider::descriptor().id` 用于 provider 注册去重和 source 描述，内置 provider 可能包含来源后缀，如 `codex:api`、`amp:cli`、`windsurf:api`。不要把它当成内置 provider 的 settings key。
- **自定义 provider ID**：YAML 的 `id` 会作为 `ProviderId::Custom(String)` 持久化，既是 descriptor ID，也是自定义 provider 的 settings/sidebar/order key。

修改设置、排序、刷新请求或 D-Bus / selector 状态时，优先传递 `ProviderId`，不要在调用点手拼字符串。

实现层面的关键约束：

- provider 返回结构化事实，不直接拼 UI 文案。
- 错误统一返回 `ProviderError` 语义，而不是裸字符串。
- `AiProvider` 与 `ProviderManager::refresh_by_id` 对 refresh/runtime 层返回 `ProviderResult<RefreshData>`；底层 helper 仍可用 `anyhow` 保存技术上下文，但不能穿过 provider 边界。
- selector / UI 才负责把稳定语义格式化成当前语言。

## Provider Capability

`AiProvider::provider_capability()` 用来声明 provider 的产品语义层级，而不只是“能不能调用 `refresh()`”。

当前稳定层级：

- `Monitorable`
  - 真实可监控 provider。
  - 会进入启动 / 周期 / 手动 / Debug 刷新链路。
  - 设置页会显示刷新按钮和 quota visibility 配置。
- `Informational`
  - 说明型入口，用于解释认证路径、provider 关系或外部配置前提。
  - 不参与正常刷新。
  - UI 显示说明型空状态，不提供 retry / refresh 动作。
- `Placeholder`
  - 可被发现但当前无法直接监控的 provider。
  - 不参与正常刷新。
  - UI 保留 provider 入口，但不伪装成“可刷新只是刚好失败”。

## Settings Capability

Provider 可以声明自己的设置能力，UI 会按能力自动渲染对应交互。

当前稳定形态：

- `None`
  - 自动管理型 provider，没有额外交互设置。
- `TokenInput(...)`
  - 使用通用 token 输入面板。
- `NewApiEditable`
  - 面向通过 NewAPI 表单创建的自定义 provider，允许在设置页继续编辑。

重要边界：

- `provider.credentials` 只保存 BananaTray 自己托管的 token override。
- 某些 provider 的真实认证来源仍可能是外部配置文件、环境变量或 CLI 登录态。
- 设置页展示状态和后台刷新不是同一条调用栈：保存 token 后 reducer 会发送 `UpdateConfig`，后台 `RefreshCoordinator` 再调用 `ProviderManager::sync_provider_credentials()` 更新 provider 运行时快照；需要 app-managed override 的 provider 必须实现 `AiProvider::sync_provider_credentials()`。

## Error And Presentation Boundary

为避免 provider 越做越像 UI 层，当前约定保持为：

- provider 层负责：
  - 发现认证状态
  - 发起请求 / 调用 CLI
  - 解析响应
  - 在 facade 边界返回 `RefreshData` 或 `ProviderError`
- `ProviderError::to_failure()` / `error_kind()` 负责把 provider 错误映射为稳定失败语义和刷新状态分类
- selector 层负责把失败语义转换成最终展示文本
- quota 标题和详情也走稳定语义：
  - 标题使用 `QuotaLabelSpec`（如 `Daily`、`Weekly`、`MonthlyCredits`、`Credits`）
  - 第四行详情使用 `QuotaDetailSpec`

这样做的直接结果是：

- 切换语言不需要重新刷新 provider
- 离线 / 缓存状态也能被重新格式化展示
- provider 内部不会堆积越来越多的产品文案分支

## Adding A New Built-in Provider

新增内置 provider 时，优先遵循以下顺序：

1. 在 `ProviderKind` 中加入新的 built-in key。
2. 选择最接近的 provider blueprint，而不是先发明新抽象。
3. 实现 provider 本体并返回稳定的元数据 / 刷新语义 / 错误语义。
4. 明确声明 `provider_capability()`；如果不是可监控 provider，不要继续伪装成“普通 refresh 失败”。
5. 如需设置页交互，声明合适的 `SettingsCapability`。
6. 注册 provider，并补上 icon、测试与文档。

推荐同时查看：

- `provider-blueprints.md`
- `antigravity-api.md`（仅当你修改 Codeium-family provider）

## What This Doc Intentionally Avoids

以下内容不再作为这里的长期维护内容：

- 每个 provider 的实现文件路径
- 逐 provider 的内部模块树
- 某个 provider 当前恰好使用的临时 helper 细节

这些信息变化很快，更适合放在代码、模块 `README.md` 或专题文档里。
