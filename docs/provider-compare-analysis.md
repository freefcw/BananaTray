# Provider 实现横向对比分析

本文按 provider 横向比较三个相邻仓库的实现：

- BananaTray：当前仓库，Rust + GPUI，路径 `./`
- ClaudeBar：Swift + SwiftUI，路径 `../ClaudeBar`
- CodexBar：Swift + SwiftUI，路径 `../CodexBar`

分析基于 2026-05-05 本地代码快照。重点不是“每个仓库的 provider 架构是什么”，而是“同一个 provider 在三个仓库里分别怎么实现、差异在哪里、BananaTray 可以借鉴什么”。

## 基础抽象对照

| 仓库 | Provider 抽象 | 状态归属 | 多数据源编排 | 动态扩展 |
|------|---------------|----------|--------------|----------|
| BananaTray | `AiProvider` trait + `ProviderManager` | reducer/runtime 持有状态，provider 只返回 `RefreshData` / `ProviderError` | 多数在 provider facade 内部手写 fallback | YAML custom provider |
| ClaudeBar | `AIProvider` rich domain object + `UsageProbe` | provider 自持 `snapshot/isSyncing/lastError` | provider 按 probe mode 选择 probe，少量内部 fallback | extension manifest + script/health check |
| CodexBar | `ProviderDescriptor` + `ProviderFetchPlan` + `ProviderFetchStrategy` | `UsageStore` 集中持有 snapshots/errors/attempts | strategy pipeline，记录 attempts，可按 runtime/source mode 规划 | 主要是编译期 provider |

## 覆盖矩阵

| Provider | BananaTray | ClaudeBar | CodexBar |
|----------|------------|-----------|----------|
| Codex | 已实现，OAuth API + 条件 CLI fallback | 已实现，RPC/API probe | 已实现，OAuth/CLI/Web dashboard pipeline |
| Claude | 已实现，API + CLI fallback | 已实现，CLI/API/pass/daily | 已实现，OAuth/CLI/Web planner |
| Gemini | 已实现，OAuth quota API | 已实现，API probe + CLI token refresh | 已实现，OAuth quota API + project discovery |
| Copilot | 已实现，internal API + 多 token source | 已实现，Billing/Internal API | 已实现，Device Flow + internal API |
| Cursor | 已实现，Cursor app DB token | 已实现，Cursor app DB token | 已实现，browser cookie / session |
| Kimi | 已实现，API token | 已实现，CLI/API mode | 已实现，cookie/API |
| MiniMax | 已实现，API key | 已实现，API key + region | 已实现，API/cookie/localStorage |
| Kiro | 已实现，CLI | 已实现，CLI | 已实现，CLI |
| Antigravity | 已实现，local API + cache | 已实现，local API | 已实现，local API |
| Windsurf | 已实现，seat/live/cache | 未实现 | 未实现 |
| Amp | 已实现，CLI | 已实现为 AmpCode，CLI | 已实现，Web cookie |
| OpenCode | Placeholder | 未实现 | 已实现，Web cookie |
| Kilo | Placeholder | 未实现 | 已实现，API/CLI |
| Vertex AI | Informational | 未实现 | 已实现，OAuth/Cloud Monitoring |
| Alibaba | 可用 YAML custom 接入，非内置 | 已实现 | 已实现 |
| z.ai | 可用 YAML custom 接入，非内置 | 已实现 | 已实现 |
| Bedrock | 未实现 | 已实现 | 未实现 |
| Mistral | 未实现 | 已实现，本地 Vibe 日志 | 未实现 |
| Warp | 未实现 | 未实现 | 已实现 |
| Ollama | 未实现 | 未实现 | 已实现 |
| OpenRouter | 可用 YAML custom 接入，非内置 | 未内置 | 已实现 |
| JetBrains AI | 未实现 | 未实现 | 已实现 |
| Factory / Droid | 未实现 | 未实现 | 已实现 |
| Augment | 未实现 | 未实现 | 已实现 |
| Synthetic | 未实现 | 未实现 | 已实现 |
| 用户自定义 | YAML provider | extension manifest + script | 无等价动态 provider |

## Codex

### BananaTray

- 核心文件：`src/providers/codex/mod.rs`、`auth.rs`、`client.rs`、`parser.rs`、`status_probe.rs`、`config.rs`
- 抽象：`CodexProvider` 实现 `AiProvider`，由 `ProviderManager::refresh_by_id` 统一调度。
- 数据源：主路径为 ChatGPT/Codex usage API；部分网络/服务端错误时 fallback 到 `codex /status` CLI。
- 认证：读取 `~/.codex/auth.json` 的 `access_token`、`refresh_token`、`id_token`、`account_id`；支持 `~/.codex/config.toml` 的 `chatgpt_base_url`。
- fallback：401/403 会刷新 token 后重试；只有 timeout、transport、5xx 才 fallback CLI；429、401/403、普通 4xx 不 fallback。
- 返回：`RefreshData { quotas, account_email, account_tier, source_label }`，quota 为 `QuotaInfo`。
- 错误：统一收敛到 `ProviderError`，如 `ConfigMissing`、`SessionExpired`、`ParseFailed`。
- 特殊点：解析 JWT 提取 email/plan/account_id；token 刷新后重读 auth.json，避免 refresh_token 轮转导致内存状态过期。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/Codex/CodexProvider.swift`、`Sources/Infrastructure/Codex/CodexUsageProbe.swift`、`CodexAPIUsageProbe.swift`、`DefaultCodexRPCClient.swift`、`CodexCredentialLoader.swift`
- 抽象：`CodexProvider` 是 `@Observable` domain object，自持 `snapshot/isSyncing/lastError`；采集委托给 `UsageProbe`。
- 数据源：RPC 模式通过 `codex app-server` 的 `account/rateLimits/read`；API 模式直连 ChatGPT usage API；RPC 内部可 fallback 到 TTY `/status`。
- 认证：RPC 依赖 Codex CLI 登录态；API 读取 `~/.codex/auth.json`。
- fallback：provider 层按 `CodexProbeMode` 选择 RPC/API；API 401 后刷新 token 重试；RPC client 内部 RPC -> TTY fallback。
- 返回：`UsageSnapshot` + `[UsageQuota]`，含可选 `accountTier`。
- 设置：`CodexConfigCard` 提供 RPC/API 模式切换，并检查 OAuth credentials。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Codex/CodexProviderDescriptor.swift`、`CodexOAuth/*`、`CodexCLISession.swift`、`CodexWebDashboardStrategy.swift`、`Sources/CodexBar/Providers/Codex/*`
- 抽象：`ProviderDescriptor` + `ProviderFetchPipeline`；App 侧 `CodexProviderImplementation` 只贡献设置、登录和 runtime hook。
- 数据源：App auto 为 OAuth -> CLI；CLI runtime auto 为 Web -> CLI；支持显式 web/cli/oauth。
- 认证：OAuth 读 `~/.codex/auth.json`；Web 走浏览器 Cookie；CLI 走 Codex CLI session。
- fallback：pipeline 逐 strategy 执行，`shouldFallback` 控制后续策略；attempts 可记录每一步 availability/error。
- 返回：`UsageSnapshot`，另可返回 `CreditsSnapshot` 和 dashboard snapshot。
- 设置：Usage source picker、OpenAI cookies picker、manual Cookie、OpenAI web extras、historical tracking。

### 差异与借鉴

- BananaTray 的 fallback 判断最克制，避免 401/403/429 盲目 fallback 到共享 token 的 CLI，这点应保留。
- ClaudeBar 的 `codex app-server` RPC 路径比 TTY `/status` 更稳定，适合作为 BananaTray 未来增强。
- CodexBar 的 strategy attempts 最适合借鉴为 BananaTray 的 Debug/诊断能力。

## Claude

### BananaTray

- 核心文件：`src/providers/claude/mod.rs`、`api_probe.rs`、`cli_probe.rs`、`credentials.rs`、`probe.rs`
- 抽象：单个 `ClaudeProvider` 持有 `cli_probe/api_probe`，实现 `AiProvider`。
- 数据源：API 与 CLI 双源，默认 Auto 优先 API，失败 fallback CLI。
- 认证：API probe 来自 Claude OAuth credentials；CLI probe 依赖 `claude /usage`。
- fallback：API available 时先 API；API 失败且 CLI available 时 CLI；否则返回结构化 unavailable/auth 错误。
- 返回：`RefreshData::quotas_only(Vec<QuotaInfo>)`。
- 错误：`ProviderError`，含 `BothUnavailable`、`NoOauthCreds`、`CliNotFound` 等 advice。
- 设置：无用户可见的 Claude source mode 切换。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/Claude/ClaudeProvider.swift`、`ClaudeUsageProbe.swift`、`ClaudeAPIUsageProbe.swift`、`ClaudeCredentialLoader.swift`、`ClaudePassProbe.swift`
- 抽象：`@Observable ClaudeProvider` 管状态，Infrastructure 层负责 CLI/API/pass/daily analyzer。
- 数据源：CLI `/usage`、API OAuth usage、CLI `/cost` fallback；可附加 daily JSONL report。
- 认证：`~/.claude/.credentials.json`、Keychain `Claude Code-credentials`、`CLAUDE_CODE_OAUTH_TOKEN`。
- fallback：CLI/API 模式可切换；API 模式可启用 CLI fallback；CLI `/usage` 遇 subscriptionRequired fallback `/cost`。
- 返回：`UsageSnapshot`，支持 quotas、accountTier、costUsage、dailyUsageReport。
- 特殊点：Claude Pass、daily usage analyzer、自动 trust folder 写入后重试、过滤低 scope 的 `CLAUDE_CODE_OAUTH_TOKEN`。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Claude/ClaudeProviderDescriptor.swift`、`ClaudeSourcePlanner.swift`、`ClaudeUsageFetcher.swift`、`ClaudeOAuth/*`、`ClaudeWeb/*`
- 抽象：descriptor + source planner + strategy pipeline。
- 数据源：OAuth、Web、CLI，可选 web extras。
- 认证：Claude OAuth credentials store、环境 OAuth token、Claude CLI、浏览器/manual Cookie。
- fallback：`ClaudeSourcePlanner` 根据 runtime、sourceMode、webExtras、CLI/OAuth/Web 可用性生成 ordered steps。
- 返回：统一 `UsageSnapshot` 的 primary/secondary/tertiary windows + identity。
- 特殊点：Keychain prompt gate、OAuth delegated refresh、防止后台弹钥匙串。

### 差异与借鉴

- BananaTray 实现简洁，但缺用户可控 source mode 和 attempts 诊断。
- ClaudeBar 对 CLI 真实交互处理最细，包括 trust prompt、`/cost`、daily usage。
- CodexBar 的 `ClaudeSourcePlanner` 是三者最强的多源规划抽象，适合 BananaTray 替换 provider 内 if/else。

## Gemini

### BananaTray

- 核心文件：`src/providers/gemini/mod.rs`、`auth.rs`、`client.rs`、`parser.rs`
- 数据源：Google Cloud Code private quota API。
- 认证：`~/.gemini/oauth_creds.json`；`~/.gemini/settings.json` 检查 auth type。
- fallback：access token 过期时运行 `gemini` CLI 触发刷新；API 401/403 时再 CLI refresh 后重试。
- 返回：`RefreshData::with_account(quotas, email, None)`。
- 错误：不支持 API key/Vertex AI 时返回 `ConfigMissing`；token invalid 返回 `SessionExpired`。
- 特殊点：显式拒绝 `api-key` 和 `vertex-ai`，只支持 OAuth personal。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/Gemini/GeminiProvider.swift`、`Sources/Infrastructure/Gemini/GeminiUsageProbe.swift`、`GeminiAPIProbe.swift`、`GeminiProjectRepository.swift`
- 数据源：实际走 API probe；CLI `/stats` 解析保留但标注为不可靠。
- 认证：`~/.gemini/oauth_creds.json`。
- fallback：API authenticationRequired 时运行 Gemini CLI `/quit` 刷新 token，再重试 API。
- 返回：`UsageSnapshot`，每个模型为 `.modelSpecific(modelId)` quota。
- 特殊点：`GeminiProjectRepository.fetchBestProject` 做 project discovery，提高 quota 准确性。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Gemini/GeminiProviderDescriptor.swift`、`GeminiStatusProbe.swift`
- 数据源：Cloud Code private API：`retrieveUserQuota`，并调用 `loadCodeAssist` 和 Cloud Resource Manager project discovery。
- 认证：`~/.gemini/oauth_creds.json`、`~/.gemini/settings.json`。
- fallback：无 CLI fallback；token 过期直接用 refresh_token 调 Google OAuth 刷新。
- 返回：`GeminiStatusSnapshot` 转 `UsageSnapshot`，Pro/Flash/Flash Lite 映射到 primary/secondary/tertiary。
- 错误：`GeminiStatusProbeError`，含 unsupported auth type、notLoggedIn、timedOut。

### 差异与借鉴

- BananaTray/ClaudeBar 依赖 CLI 触发 token refresh；CodexBar 直接 OAuth refresh，更少依赖 CLI。
- ClaudeBar/CodexBar 都做 project discovery，BananaTray 可借鉴增强准确性。
- CodexBar 的 tier 聚合更适合托盘 UI，可作为 BananaTray Gemini 展示优化方向。

## Copilot

### BananaTray

- 核心文件：`src/providers/copilot/mod.rs`、`token.rs`、`client.rs`、`parser.rs`
- 数据源：GitHub `api.github.com/copilot_internal/user`，另 best-effort `/user` 获取账户名。
- 认证：settings credential `github_token` -> `GITHUB_TOKEN` -> VSCode Copilot `hosts.json/apps.json` -> macOS Keychain `copilot-cli`。
- fallback：多 token source 解析，但数据源单一；token 解析有 5 秒缓存。
- 返回：`RefreshData`。
- 错误：401 -> `SessionExpired(LoginApp GitHub)`；403 -> token 缺 Copilot 权限；404 -> Copilot 未启用。
- 设置：声明 `SettingsCapability::TokenInput`，UI 自动渲染 GitHub token 面板。
- 特殊点：token source 展示区分 config/env/oauth/keychain，自动读取 Copilot 扩展 OAuth。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/Copilot/CopilotProvider.swift`、`CopilotUsageProbe.swift`、`CopilotInternalAPIProbe.swift`、`CopilotConfigCard.swift`
- 数据源：Billing API `/users/{username}/settings/billing/premium_request/usage`，或 Internal API `/copilot_internal/user`。
- 认证：settings stored GitHub token 或可配置 env var；billing 模式还需要 username。
- fallback：通过 `CopilotProbeMode` 手动选择 billing/internal；无自动 fallback。
- 返回：`UsageSnapshot`，billing 支持 manual override。
- 设置：probe mode、username、PAT、env var、monthly limit、manual override、测试按钮。
- 特殊点：manual override 专门处理 org/business API 无数据场景。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Copilot/CopilotProviderDescriptor.swift`、`CopilotUsageFetcher.swift`、`Sources/CodexBar/Providers/Copilot/*`
- 数据源：GitHub Copilot internal API `/copilot_internal/user`。
- 认证：`ProviderTokenResolver.copilotToken` 读取 `COPILOT_API_TOKEN` 或 Device Flow 保存的 token。
- fallback：单 strategy，无 fallback。
- 返回：`UsageSnapshot`，premium primary，chat secondary，identity.loginMethod 为 plan。
- 设置：GitHub Device Flow 登录按钮、secure token field、重新登录。
- 特殊点：Device Flow 登录体验完整，请求头模拟 VSCode/Copilot Chat 插件上下文。

### 差异与借鉴

- BananaTray token 来源覆盖最广。
- ClaudeBar 配置项最细，适合处理 billing API 与 org/business 空数据。
- CodexBar 登录流最好，BananaTray 可借鉴 Device Flow 降低手动 PAT 门槛。

## Cursor

### BananaTray

- 核心文件：`src/providers/cursor/mod.rs`、`auth.rs`、`client.rs`、`parser.rs`
- 数据源：`https://cursor.com/api/usage-summary`。
- 认证：本地 Cursor SQLite `state.vscdb` 中的 `cursorAuth/accessToken`；JWT 提取 user id 后构造 WorkOS cookie。
- fallback：无多源 fallback。
- 返回：`RefreshData::quotas_only(...)`。
- 错误：读取 token/JWT/API 失败经 `ProviderError::classify`。
- 特殊点：不依赖浏览器 Cookie，直接复用 Cursor app 本地状态。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/Cursor/CursorProvider.swift`、`Sources/Infrastructure/Cursor/CursorUsageProbe.swift`
- 数据源：`https://cursor.com/api/usage-summary`。
- 认证：macOS Cursor `state.vscdb`，用 `/usr/bin/sqlite3` 读 `cursorAuth/accessToken`，JWT `sub` 拼 WorkOS cookie。
- fallback：无 fallback。
- 返回：`UsageSnapshot`，兼容 included plan、onDemand、billing cycle、enterprise 0 limit。
- 错误：DB 缺失、空 token、401 sessionExpired 等。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Cursor/CursorProviderDescriptor.swift`、`CursorStatusProbe.swift`、`Sources/CodexBar/Providers/Cursor/*`
- 数据源：Cursor web/API，通过浏览器 Cookie 或 app 内保存 session。
- 认证：浏览器 Cookie 自动导入、manual Cookie header、`CursorSessionStore` 持久化 session；Safari 优先。
- fallback：严格 session cookie 名称扫描失败后，fallback 到 Cursor domain cookies 让 API 验证。
- 返回：`CursorStatusSnapshot` 转 `UsageSnapshot`，Total primary、Auto secondary、API tertiary，on-demand 映射成本。
- 设置：Cookie source picker、manual Cookie、登录流、Cookie cache 状态。
- 特殊点：legacy/token-based 双形态兼容，Full Disk Access 指引更完善。

### 差异与借鉴

- BananaTray/ClaudeBar 的本地 DB 路径简单直接；CodexBar 的 Web Cookie 覆盖网站登录和多账号。
- CodexBar 数据模型最完整，Auto/API/On-demand/legacy plan 拆分值得参考。
- BananaTray 可借鉴 CodexBar 的错误提示和 Cookie fallback，覆盖 DB 缺失或权限问题。

## Kimi

### BananaTray

- 核心文件：`src/providers/kimi/mod.rs`、`auth.rs`、`client.rs`、`parser.rs`
- 数据源：Kimi billing HTTP API `GetUsages`。
- 认证：`KIMI_AUTH_TOKEN`；同时检查 `kimi` CLI 是否存在但不通过 CLI 拉取。
- fallback：无多源 fallback。
- 返回：`RefreshData::quotas_only(Vec<QuotaInfo>)`，包含 weekly tier 和 5h session。
- 错误：`ConfigMissing`、`CliNotFound`、`NoData`、`ParseFailed`。
- 特殊点：按 weekly limit 识别 Andante/Moderato/Allegretto。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/Kimi/KimiProvider.swift`、`KimiUsageProbe.swift`、`KimiCLIUsageProbe.swift`、`KimiTokenProvider.swift`
- 数据源：CLI `/usage` 与 API 两种 probe mode。
- 认证：API 模式为 `KIMI_AUTH_TOKEN` 或浏览器 `kimi-auth` cookie。
- fallback：API probe 不可用时 provider 内可回落到 CLI。
- 返回：`UsageSnapshot` + `UsageQuota`。
- 设置：设置 UI 分段控件切换 CLI/API。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Kimi/KimiProviderDescriptor.swift`、`KimiUsageFetcher.swift`、`KimiCookieImporter.swift`
- 数据源：web/API。
- 认证：手动 cookie/token -> 浏览器 cookie -> 环境变量。
- fallback：无 CLI；invalid/missing token 不 fallback。
- 返回：统一 `UsageSnapshot`，sourceLabel=`web`。
- 设置：cookie source 自动/手动/off，安全输入。
- 特殊点：JWT payload 解出 device/session/traffic headers。

### 差异与借鉴

- BananaTray 最简单，依赖环境 token。
- ClaudeBar 的 CLI/API 可切换适合补充无 token 场景。
- CodexBar 的 cookie source UI 和 JWT header 构造更完整。

## MiniMax

### BananaTray

- 核心文件：`src/providers/minimax/mod.rs`、`auth.rs`、`client.rs`、`parser.rs`
- 数据源：`/v1/api/openplatform/coding_plan/remains`。
- 认证：`MINIMAX_API_KEY`，`MINIMAX_REGION=international` 切 `.io`，默认 `.com`。
- fallback：无。
- 返回：每个 model 的 `QuotaInfo`。
- 错误：API base_resp、no data、config missing。
- 特殊点：按模型生成 `QuotaType::ModelSpecific`。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/MiniMax/MiniMaxProvider.swift`、`MiniMaxUsageProbe.swift`、`MiniMaxConfigCard.swift`
- 数据源：API-only。
- 认证：自定义 env var / `MINIMAX_API_KEY`，再 UserDefaults API key。
- fallback：无多源 fallback。
- 设置：region、API key、安全显示、自定义 env var、保存并测试。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/MiniMax/*`、`Sources/CodexBar/Providers/MiniMax/*`
- 数据源：Coding Plan web/cookie、remains API、API token。
- 认证：API key、cookie header、浏览器 cookie、local storage token、cookie cache。
- fallback：auto 根据 token 类型选择 API 或 web；API invalid 可 fallback web；HTML parse 失败转 remains API。
- 返回：`UsageSnapshot` 和 `MiniMaxUsageSnapshot`。
- 设置：API token、cookie source、cookie header、region、缓存来源展示。

### 差异与借鉴

- CodexBar 最完整，尤其 cookie/localStorage 组合和 host retry。
- BananaTray 可补设置 UI 和 cookie source，而不是只依赖 env API key。

## Kiro

### BananaTray

- 核心文件：`src/providers/kiro.rs`
- 数据源：`kiro-cli chat --no-interactive /usage`，额外 `kiro-cli whoami` 取邮箱。
- 认证：Kiro CLI 登录态。
- fallback：无。
- 返回：`RefreshData::with_account`，Regular/Bonus credits 使用 `QuotaType::Points`。
- 特殊点：stdout/stderr 合并、ANSI 清理、tier、bonus expiry、reset date 解析。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/Kiro/KiroProvider.swift`、`Sources/Infrastructure/Kiro/KiroUsageProbe.swift`
- 数据源：交互式 `kiro-cli` 输入 `/usage`、`/quit`。
- 认证：CLI 登录态。
- 返回：`UsageSnapshot`。
- 特殊点：实现较基础，Bonus 映射为 `.weekly`，Regular 为 `.timeLimit("Monthly")`。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Kiro/KiroProviderDescriptor.swift`、`KiroStatusProbe.swift`
- 数据源：`kiro-cli whoami` 预检 + `chat --no-interactive /usage`。
- 认证：CLI 登录态。
- 返回：`UsageSnapshot` primary=credits、secondary=bonus，identity 带 plan。
- 错误：cliNotFound/notLoggedIn/cliFailed/parseError/timeout。
- 特殊点：版本检测、idle timeout、支持新版 `Plan:` 输出。

### 差异与借鉴

- BananaTray 与 CodexBar 都能区分 regular/bonus credits，ClaudeBar 映射较粗。
- CodexBar 错误分类和 version/timeout 处理可借鉴。

## Antigravity

### BananaTray

- 核心文件：`src/providers/antigravity/mod.rs`、`src/providers/codeium_family/*`
- 数据源：本地 language server API，失败后 fallback 本地 cache。
- 认证：进程命令行 csrf token 和本地 cache。
- fallback：live -> cache。
- 返回：`RefreshData`。
- 特殊点：与 Windsurf 共享 Codeium-family live/cache primitives。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/Antigravity/AntigravityProvider.swift`、`Sources/Infrastructure/Antigravity/AntigravityUsageProbe.swift`
- 数据源：本地 language server API。
- 认证：`--csrf_token`。
- fallback：没有 cache fallback；会尝试 HTTPS ports 和 HTTP extension port。
- 返回：model-specific `UsageQuota`。
- 特殊点：`pgrep/lsof` 发现端口，支持 `GetUserStatus` 和 `GetCommandModelConfigs`。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Antigravity/AntigravityProviderDescriptor.swift`、`AntigravityStatusProbe.swift`
- 数据源：本地 language server API。
- 认证：csrf token。
- fallback：无 cache fallback，但 `GetUserStatus` 失败会请求 `GetCommandModelConfigs`。
- 返回：primary/secondary/tertiary 三窗口，按模型归类 Claude/Gemini Pro/Gemini Flash。
- 特殊点：Google Workspace status link、plan summary、代表模型选择。

### 差异与借鉴

- BananaTray 是三者里唯一有 local cache fallback 的实现。
- CodexBar 的模型归类和展示语义更适合 UI，可与 BananaTray 的 cache fallback 组合。

## Windsurf

### BananaTray

- 核心文件：`src/providers/windsurf.rs`、`src/providers/windsurf/seat_source.rs`、`src/providers/codeium_family/*`
- 数据源：seat management API -> local language server API -> local cache。
- 认证：Codeium/Windsurf cache DB 中的 `apiKey`。
- fallback：seat -> live -> cache；seat daily/weekly 可与 cache weekly 合并。
- 返回：`RefreshData`。
- 特殊点：读取 Windsurf app version；seat API 实时日配额 + cache 周配额合并。

### ClaudeBar

- 未实现。

### CodexBar

- 未实现。

### 差异与借鉴

- BananaTray 的 Windsurf 是目前三仓库唯一完整实现，可反向作为 ClaudeBar/CodexBar 蓝图。

## Amp

### BananaTray

- 核心文件：`src/providers/amp.rs`
- 数据源：`amp usage --no-color` CLI。
- 认证：Amp CLI 登录态。
- 返回：`RefreshData::with_account`，支持 `$remaining/$total` 和 balance-only。
- 特殊点：跳过 `$0 remaining`，balance-only 状态按绝对余额判断。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/AmpCode/AmpCodeProvider.swift`、`Sources/Infrastructure/AmpCode/AmpCodeUsageProbe.swift`
- 数据源：`amp usage --no-color` CLI。
- 认证：CLI 登录态。
- 返回：`UsageSnapshot`，providerId 为 `ampcode`。
- 特殊点：email 脱敏日志，balance line 使用 `dollarRemaining`。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Amp/AmpProviderDescriptor.swift`、`AmpUsageFetcher.swift`、`AmpUsageParser.swift`
- 数据源：`https://ampcode.com/settings` HTML。
- 认证：浏览器/手动 `session` cookie。
- fallback：无 CLI fallback。
- 返回：`AmpUsageSnapshot` 转 `UsageSnapshot`。
- 设置：cookie source 和 manual cookie。
- 特殊点：解析页面内 `freeTierUsage/getFreeTierUsage` 对象。

### 差异与借鉴

- BananaTray/ClaudeBar 走 CLI，依赖本机登录态但简单。
- CodexBar 走 Web cookie，可覆盖无 CLI 或 CLI 输出变化场景。

## OpenCode

### BananaTray

- 核心文件：`src/providers/opencode.rs`
- 状态：Placeholder。
- 数据源：只检测 `opencode` CLI；无真实 quota 数据。
- capability：`ProviderCapability::Placeholder`，不参与正常刷新。

### ClaudeBar

- 未实现。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/OpenCode/OpenCodeProviderDescriptor.swift`、`OpenCodeUsageFetcher.swift`、`OpenCodeCookieImporter.swift`
- 数据源：`opencode.ai/_server` server functions，先取 workspace，再取 subscription。
- 认证：`auth` / `__Host-auth` cookie，支持浏览器导入、手动 cookie、cookie cache。
- fallback：invalid credentials 时自动清缓存重试；GET 失败尝试 POST。
- 返回：rolling 5h + weekly `UsageSnapshot`。
- 设置：cookie source、workspace ID override。

### 差异与借鉴

- BananaTray 目前只是可发现入口；CodexBar 已有真实监控。
- 若 BananaTray 升级 OpenCode，可优先参考 CodexBar 的 workspace/server-function 解析。

## Kilo

### BananaTray

- 核心文件：`src/providers/kilo.rs`
- 状态：Placeholder。
- 数据源：只检测 VS Code 扩展 `~/.vscode/extensions/kilocode.kilo-code*`。
- capability：`ProviderCapability::Placeholder`。

### ClaudeBar

- 未实现。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Kilo/KiloProviderDescriptor.swift`、`KiloUsageFetcher.swift`、`KiloSettingsReader.swift`
- 数据源：Kilo tRPC batch API；auto 可 fallback 到 CLI session auth。
- 认证：API 为 `KILO_API_KEY`；CLI 为 `~/.local/share/kilo/auth.json` 的 `kilo.access`。
- fallback：API missing/unauthorized 时 fallback CLI。
- 返回：credits primary + Kilo Pass secondary + loginMethod。
- 设置：usage source 和 API key。
- 特殊点：Kilo Pass、bonus、auto top-up 信息。

### 差异与借鉴

- CodexBar 提供了 BananaTray 从 Placeholder 升级到 Monitorable 的完整参考。

## Vertex AI

### BananaTray

- 核心文件：`src/providers/vertex_ai.rs`
- 状态：Informational。
- 数据源：只读 `~/.gemini/settings.json`。
- 认证：检测 Gemini CLI 配置 `security.auth.selectedType == "vertex-ai"`。
- 返回：`refresh()` 返回 unavailable，不产出 quota。
- capability：`ProviderCapability::Informational`。

### ClaudeBar

- 未实现。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/VertexAI/VertexAIProviderDescriptor.swift`、`VertexAIOAuth/*`
- 数据源：Google Cloud Vertex AI OAuth usage fetcher，结合本地 Claude logs 做 token cost。
- 认证：`VertexAIOAuthCredentialsStore` / gcloud OAuth。
- 返回：`UsageSnapshot`。
- 设置：有登录流。

### 差异与借鉴

- BananaTray 目前是说明型入口；CodexBar 是真实监控。
- 可借鉴 CodexBar 的 OAuth credentials store + login flow，把 Vertex AI 升级为 Monitorable。

## Alibaba

### BananaTray

- 未实现内置 provider；可通过 YAML custom provider 接入。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/Alibaba/*`、`Sources/Infrastructure/Alibaba/AlibabaUsageProbe.swift`、`AlibabaBrowserCookieProvider.swift`
- 数据源：API key 请求和控制台 Cookie RPC。
- 认证：设置仓库 API key、手动 Cookie、浏览器 Cookie。
- 编排：API key 优先，缺失再 cookie。
- 返回：`UsageSnapshot` + `UsageQuota`。
- 设置：region、cookie source、manual cookie、API key、连接测试。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Alibaba/*`、`Sources/CodexBar/Providers/Alibaba/*`
- 数据源：API key 或 web console cookie。
- 认证：config/env API key、manual cookie、browser cookie、cookie cache。
- 编排：按 source mode；web 失败可清 cache 重取；支持 host retry。
- 返回：`AlibabaCodingPlanUsageSnapshot.toUsageSnapshot()`。
- 设置：region、cookie source/header、API key。

### 差异与借鉴

- CodexBar 的 cookie cache、host retry、Chromium locked DB fallback 更成熟。
- BananaTray 若做内置 Alibaba，应优先采用 CodexBar 式多源策略。

## z.ai

### BananaTray

- 未实现内置；可通过 YAML custom provider 或 NewAPI 风格配置接入。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/Zai/ZaiProvider.swift`、`Sources/Infrastructure/Zai/ZaiUsageProbe.swift`
- 数据源：z.ai quota API `/api/monitor/usage/quota/limit`。
- 认证：从 Claude `settings.json` 识别 `ANTHROPIC_BASE_URL` 和 `ANTHROPIC_AUTH_TOKEN`，可 fallback 到配置 env var。
- 编排：配置文件 token 优先，env var 兜底。
- 返回：`UsageSnapshot`。
- 特殊点：检测 z.ai/zhipu/dev endpoint。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Zai/ZaiProviderDescriptor.swift`、`ZaiUsageStats.swift`
- 数据源：z.ai quota API。
- 认证：provider config API key 或环境变量，经 `ProviderTokenResolver.zaiToken`。
- 返回：`ZaiUsageSnapshot.toUsageSnapshot()`，含 token/time limit、planName、MCP usage details。
- 设置：API token 和 region。

### 差异与借鉴

- ClaudeBar 偏“Claude Code 代理到 z.ai”配置自动发现。
- CodexBar 偏独立 API provider。
- BananaTray 可做双模式：Claude settings auto-detect + 手动 API token。

## Bedrock

### BananaTray

- 未实现。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/Bedrock/*`、`Sources/Infrastructure/Bedrock/BedrockUsageProbe.swift`、`BedrockCloudWatchClient.swift`、`BedrockPricingService.swift`
- 数据源：CloudWatch `AWS/Bedrock` metrics + AWS Pricing API。
- 认证：AWS SDK default chain / AWS profile / SSO resolver。
- 编排：跨 region 聚合；Pricing API 失败 fallback bundled defaults。
- 返回：`UsageSnapshot`，携带 `BedrockUsageSummary/modelUsages/cost/budget`。
- 设置：AWS profile、regions、daily budget。
- 特殊点：按预算计算 quota 百分比。

### CodexBar

- 未实现。

### 差异与借鉴

- ClaudeBar 是唯一完整实现；其 CloudWatch + pricing fallback + daily budget 模型可直接作为 BananaTray 参考。

## Mistral

### BananaTray

- 未实现。

### ClaudeBar

- 核心文件：`Sources/Domain/Provider/Mistral/MistralProvider.swift`、`Sources/Infrastructure/Mistral/MistralUsageProbe.swift`、`VibeSessionLogAnalyzer.swift`
- 数据源：本地 Vibe session logs `~/.vibe/logs/session/*/meta.json`。
- 认证：无网络/API key。
- 返回：`UsageSnapshot`，主要是 daily cost/token report。
- 特殊点：这是 Vibe/Mistral 本地日志分析，不是官方 Mistral API quota。

### CodexBar

- 未实现。

### 差异与借鉴

- 若 BananaTray 实现，需要命名和文案避免误导用户以为是 Mistral 官方 quota。

## Warp

### BananaTray

- 未实现。

### ClaudeBar

- 未实现。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Warp/WarpProviderDescriptor.swift`、`WarpUsageFetcher.swift`、`WarpSettingsReader.swift`
- 数据源：Warp GraphQL `https://app.warp.dev/graphql/v2?op=GetRequestLimitInfo`。
- 认证：API key/provider config/env，经 `ProviderTokenResolver.warpToken`。
- 返回：`WarpUsageSnapshot.toUsageSnapshot()`，含 request limit、used、bonus credits、next refresh。
- 设置：API key。

### 差异与借鉴

- 这是清晰的 API-token provider，适合 BananaTray 先用 YAML custom 接入，必要时内置化。

## Ollama

### BananaTray

- 未实现。

### ClaudeBar

- 未实现。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Ollama/OllamaProviderDescriptor.swift`、`OllamaUsageFetcher.swift`、`OllamaUsageParser.swift`
- 数据源：`ollama.com/settings` HTML。
- 认证：浏览器 session cookie 或 manual cookie。
- 编排：尝试多个 browser/cookie candidate，auth 失败换下一个。
- 返回：`OllamaUsageSnapshot.toUsageSnapshot()`，含 plan/email/session/weekly。
- 设置：cookie source/manual cookie。
- 特殊点：NextAuth 分片 cookie 识别和 HTML retry parse。

### 差异与借鉴

- CodexBar 是典型“无公开 API，用浏览器 cookie + HTML parse”的实现，Cookie candidate 策略可复用。

## OpenRouter

### BananaTray

- 未实现内置；可用 YAML HTTP custom provider 接入。

### ClaudeBar

- 未内置；扩展机制测试里出现过 OpenRouter 示例。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/OpenRouter/OpenRouterProviderDescriptor.swift`、`OpenRouterUsageStats.swift`
- 数据源：OpenRouter `/credits`，辅助短超时请求 `/key` 获取 key quota/rate limit。
- 认证：API key/config/env `OPENROUTER_API_KEY`。
- 返回：`OpenRouterUsageSnapshot.toUsageSnapshot()`，含 balance、totalCredits、usage、keyLimit/keyUsage/rateLimit。
- 设置：API token。
- 特殊点：`/key` 不阻塞主 credits 更新。

### 差异与借鉴

- CodexBar 对“主数据 + 辅助端点 bounded timeout”的处理适合 BananaTray 内置 provider。

## JetBrains AI

### BananaTray

- 未实现。

### ClaudeBar

- 未实现。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/JetBrains/JetBrainsProviderDescriptor.swift`、`JetBrainsStatusProbe.swift`、`JetBrainsIDEDetector.swift`
- 数据源：JetBrains IDE 配置 XML 中 `AIAssistantQuotaManager2` 的 quota/refill 信息。
- 认证：无；依赖 IDE 登录后本地配置。
- 返回：`JetBrainsStatusSnapshot.toUsageSnapshot()`。
- 设置：自定义 IDE base path。
- 特殊点：macOS 用 `XMLDocument`，Linux 用 regex XML parser。

### 差异与借鉴

- 本地配置文件 probe 很适合 BananaTray 的 GPUI-free provider 边界。

## Factory / Droid

### BananaTray

- 未实现。

### ClaudeBar

- 未实现。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Factory/FactoryProviderDescriptor.swift`、`FactoryStatusProbe.swift`、`FactoryLocalStorageImporter.swift`
- 数据源：Factory/Droid web/API。
- 认证：browser cookies、manual cookie、WorkOS/local storage token flows。
- 返回：standard/premium token usage、period、plan/tier/org/email。
- 设置：cookie source/manual cookie，有 login flow。
- 特殊点：多域 cookie：`factory.ai`、`app.factory.ai`、`auth.factory.ai`。

### 差异与借鉴

- 认证处理比 quota 解析更复杂；BananaTray 若实现应先建立 cookie source 与 token account override 能力。

## Augment

### BananaTray

- 未实现。

### ClaudeBar

- 未实现。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Augment/AugmentProviderDescriptor.swift`、`AuggieCLIProbe.swift`、`AugmentStatusProbe.swift`、`AugmentSessionKeepalive.swift`
- 数据源：优先 `auggie` CLI，失败后 web/API cookie。
- 认证：CLI 登录态、browser cookie 或 manual cookie。
- fallback：CLI `notAuthenticated/noOutput` 可回退 web，parseError 不回退。
- 返回：`AugmentStatusSnapshot.toUsageSnapshot()`，含 credits、billing end、email/plan。
- 设置：cookie source/manual cookie。
- 特殊点：session keepalive，主动刷新快过期 cookie。

### 差异与借鉴

- Augment 是清晰的“CLI 优先，Web 兜底”模板，适合 BananaTray 未来多源 provider 参考。

## Synthetic

### BananaTray

- 未实现。

### ClaudeBar

- 未实现。

### CodexBar

- 核心文件：`Sources/CodexBarCore/Providers/Synthetic/SyntheticProviderDescriptor.swift`、`SyntheticUsageStats.swift`
- 数据源：Synthetic quota API。
- 认证：API key/config/env `SYNTHETIC_API_KEY`。
- 返回：`SyntheticUsageSnapshot.toUsageSnapshot()`。
- 设置：API key。
- 特殊点：更像测试/演示 provider，用来覆盖多 quota/window 解析。

### 差异与借鉴

- 适合作为 provider 框架测试样例，不一定适合作为 BananaTray 默认可见 provider。

## 自定义 / 扩展 Provider

### BananaTray

- 核心文件：`src/providers/custom/schema.rs`、`provider.rs`、`fetch.rs`、`extractor.rs`、`loader.rs`、`docs/custom-provider.md`
- 抽象：`CustomProvider: AiProvider` 解释 YAML。
- 数据源：CLI、HTTP GET、HTTP POST、placeholder。
- 认证：bearer、bearer_env、header_env、file_token、login、cookie、session_token。
- fallback：无内建多策略 fallback，一份 YAML 选择一种 source。
- 返回：统一 `RefreshData` / `QuotaInfo`。
- 设置：NewAPI provider 可编辑，自定义 provider 进入通用 provider 列表。

### ClaudeBar

- 核心文件：`Sources/Domain/Extension/*`、`Sources/Infrastructure/Extension/*`、`Sources/App/Views/Settings/ExtensionConfigCard.swift`
- 抽象：`ExtensionProvider: AIProvider`，每个 section 一个 `UsageProbe`。
- 数据源：外部脚本 JSON 输出或内建 health check。
- 认证/配置：manifest `configFields` 注入环境变量。
- 编排：多 section 并发刷新并 merge snapshots。
- 返回：`UsageSnapshot`，支持 quotas/cost/daily/metrics/status。
- 设置：按 manifest 自动渲染 string/number/path/secret/toggle/choice。

### CodexBar

- 无等价用户动态扩展主路径。
- 主要通过编译期 `UsageProvider`、`ProviderDescriptor`、`ProviderFetchPlan` 增加 provider。
- `~/.codexbar/config.json` 可提供 API key/cookie/source/region 等配置，但 provider 集合仍由代码枚举决定。

### 差异与借鉴

- BananaTray YAML 安全、可控、易分发，但表达复杂 fallback 较弱。
- ClaudeBar 脚本扩展表达力最强，但安全边界和跨平台诊断成本更高。
- CodexBar descriptor/pipeline 最适合高质量内置 provider。
- 最合理的 BananaTray 方向：保留 YAML custom，同时给内置多源 provider 引入轻量 strategy attempts。

## 对 BananaTray 的优先建议

1. 保留现有 `ProviderId` / `ProviderKind` / `ProviderError` / `RefreshCoordinator` 边界，不改成有状态 provider object。
2. 引入轻量版 source attempts：记录每个 source 是否 available、是否失败、错误摘要，用于 Debug 面板和日志。
3. Codex 已增加 `codex app-server` RPC 路径，后续可继续把这类 source attempts 暴露到 Debug 面板。
4. 对 Claude/Codex/Windsurf/Antigravity 这类多源 provider，把 source planning 抽成纯函数单测。
5. 对 Copilot 借鉴 CodexBar Device Flow，但保留 BananaTray 现有多 token source 自动发现能力。
6. 对 Placeholder provider 优先评估 Kilo/OpenCode：CodexBar 已有真实监控实现，可作为升级参考。
7. 对 Gemini/Cursor 借鉴 CodexBar 的 primary/secondary/tertiary 聚合展示，减少 provider-specific quota 在 UI 的散乱感。
8. 对 YAML custom provider 不建议直接开放任意脚本；若要增强表达力，优先增加受限的 schema 能力和可观测错误。
