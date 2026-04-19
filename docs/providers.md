# Providers

本文件描述 BananaTray 当前的 provider 模型和扩展边界。

它关注的是“有哪些 provider、provider 必须返回什么语义、如何新增一个 provider”，而不是每个 provider 的具体目录结构或内部实现文件。

## Built-in Providers

当前内置 14 个 provider，外加 YAML 自定义 provider。

| Provider | 设置 / 配置里的稳定 key | 主要数据来源 | 备注 |
|----------|--------------------------|--------------|------|
| Claude | `claude` | HTTP API + CLI fallback | 多 source 编排 |
| Gemini | `gemini` | HTTP API | |
| Copilot | `copilot` | HTTP API | 支持 token 输入面板 |
| Codex | `codex` | HTTP API + CLI fallback | 读取 `~/.codex/auth.json`，解析 OAuth `id_token` 填充 email/plan；刷新时自动轮转 `id_token` 并注入 `ChatGPT-Account-Id` 以支持多账号；可通过 `~/.codex/config.toml` 的 `chatgpt_base_url` 切换自托管 ChatGPT 网关；OAuth 出现 timeout / 网络错误 / 5xx 时自动兑底到 `codex /status`（429 限流不 fallback，因 CLI 共用同一 token 会撞同一限流） |
| Kimi | `kimi` | HTTP API | |
| Amp | `amp` | CLI | |
| Cursor | `cursor` | HTTP API + 本地数据 | |
| MiniMax | `minimax` | HTTP API | |
| Kiro | `kiro` | CLI | |
| Antigravity | `antigravity` | 本地服务 + 本地缓存回退 | provider facade 自己编排 `live -> cache`，见 `antigravity-api.md` |
| Windsurf | `windsurf` | 本地服务 + seat API + 本地缓存回退 | provider facade 自己编排 `live -> seat -> cache`；若 seat 只返回日配额，则再用本地缓存补周配额。见 `antigravity-api.md` |
| Kilo | `kilo` | 占位 / 安装检测 | 不做真实配额拉取 |
| OpenCode | `opencode` | 占位 / 安装检测 | 不做真实配额拉取 |
| Vertex AI | `vertexai` | 占位 / 环境检测 | 不做真实配额拉取 |

## Custom Providers

自定义 provider 通过 YAML 声明，不需要新增 Rust 代码。

规范目录：

- macOS: `~/Library/Application Support/BananaTray/providers/`
- Linux: `$XDG_CONFIG_HOME/bananatray/providers/`

补充说明：

- 手工新增或编辑 YAML 后，当前通常需要重启应用才能重新加载。
- 应用内通过 NewAPI 表单保存 / 删除 provider 时，会显式触发 reload。
- 详细 Schema 和示例见 `custom-provider.md` 与 `docs/examples/`。

## Stable Provider Contract

每个 provider 都遵守同一组稳定边界：

- 提供身份与展示元数据
- 提供可用性检查
- 提供刷新能力
- 可选地声明设置页交互能力

实现层面的关键约束：

- provider 返回结构化事实，不直接拼 UI 文案。
- 错误统一返回 `ProviderError` 语义，而不是裸字符串。
- selector / UI 才负责把稳定语义格式化成当前语言。

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

## Error And Presentation Boundary

为避免 provider 越做越像 UI 层，当前约定保持为：

- provider 层负责：
  - 发现认证状态
  - 发起请求 / 调用 CLI
  - 解析响应
  - 返回 `RefreshData` 或 `ProviderError`
- `providers/error_presenter.rs` 负责把 provider 错误映射为稳定失败语义
- selector 层负责把失败语义转换成最终展示文本

这样做的直接结果是：

- 切换语言不需要重新刷新 provider
- 离线 / 缓存状态也能被重新格式化展示
- provider 内部不会堆积越来越多的产品文案分支

## Adding A New Built-in Provider

新增内置 provider 时，优先遵循以下顺序：

1. 在 `ProviderKind` 中加入新的 built-in key。
2. 选择最接近的 provider blueprint，而不是先发明新抽象。
3. 实现 provider 本体并返回稳定的元数据 / 刷新语义 / 错误语义。
4. 如需设置页交互，声明合适的 `SettingsCapability`。
5. 注册 provider，并补上 icon、测试与文档。

推荐同时查看：

- `provider-blueprints.md`
- `antigravity-api.md`（仅当你修改 Codeium-family provider）

## What This Doc Intentionally Avoids

以下内容不再作为这里的长期维护内容：

- 每个 provider 的实现文件路径
- 逐 provider 的内部模块树
- 某个 provider 当前恰好使用的临时 helper 细节

这些信息变化很快，更适合放在代码、模块 `README.md` 或专题文档里。
