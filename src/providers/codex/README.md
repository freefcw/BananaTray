# Codex Provider

OpenAI Codex（ChatGPT 后端）配额抓取实现。对应 `~/.codex/auth.json` 的 OAuth 凭据。

## 文件分工

| 文件 | 职责 |
|------|------|
| `mod.rs` | `AiProvider` 实现入口；`refresh()` 流水线、OAuth → RPC → PTY fallback 决策、`should_fallback_to_cli` |
| `auth.rs` | `~/.codex/auth.json` 解析、JWT `id_token` 提取 email/plan/account_id、access_token 主动 + 被动刷新；刷新结果通过私有 sibling-temp 原子替换写回 |
| `config.rs` | `~/.codex/config.toml` 读取，`chatgpt_base_url` 解析与归一化（支持自托管 ChatGPT 网关） |
| `client.rs` | usage / reset-credits API HTTP 请求构造（含 `ChatGPT-Account-Id` 头）；重置明细请求独立限制为 1 秒，不改变主 usage 请求超时 |
| `parser.rs` | OAuth `/wham/usage` JSON 响应解析；把 snake_case HTTP 字段规范化后交给 `quota.rs`；抽取 `plan_type`；把 `rate_limit_reset_credits` 交给 `resets.rs`，不放进配额列表 |
| `quota.rs` | Codex quota 共享规则：窗口角色解析、窗口 `QuotaInfo` 构建、重复角色去重、credits balance 读取；credits 按积分点数展示并保持 stable key 兼容 |
| `resets.rs` | 可兑换限流重置：数量、类型、获取时间、到期时间。接受 usage / 详情 / app-server 的 snake_case 与 camelCase |
| `rpc_probe.rs` | OAuth 可恢复失败时优先启动 `codex app-server`，通过 JSON-RPC `account/rateLimits/read` 读取结构化配额；把 camelCase RPC 字段规范化后交给 `quota.rs` |
| `status_probe.rs` | RPC 也失败时通过 PTY 启动 `codex /status` 做最后兜底解析 |

## 数据流

```
load_credentials → ensure_access_token (proactive)
                 ↓
            resolve_usage_url (config.toml)
                 ↓
       fetch_usage (OAuth HTTP + reactive refresh on 401/403)
                 ↓
        ┌────────┴────────┐
        ↓ Ok              ↓ Timeout / NetworkFailed / 5xx
   parse_usage_response   rpc_probe::fetch_via_rpc
        │                 ↓ RPC failure
        │            status_probe::fetch_via_cli
        │
        │ OAuth 成功且只有 available_count 时
        ↓
   GET /wham/rate-limit-reset-credits（失败则保留数量摘要）
        └────────┬────────┘
                 ↓
        resolve_account (JWT)
                 ↓
        RefreshData::with_account + rate_limit_resets
```

## Fallback 决策表

CLI 兑底内部固定顺序为 `codex app-server` RPC → PTY `/status`。RPC 返回结构化 JSON，可提供 `usedPercent`、`resetsAt`、`windowDurationMins`、credits、`planType` 和 `rateLimitResetCredits`；PTY 仍保留为 experimental RPC 不可用时的最后兜底。OAuth 成功后的重置明细请求不走这条兑底。

| OAuth 错误 | 是否兑底到 CLI | 理由 |
|-----------|---------------|------|
| `HttpError::Timeout` | ✅ | 网络瞬断，CLI 路径可能能绕过 |
| `HttpError::Transport` | ✅ | 连接层失败，CLI 重试有意义 |
| `HttpError::HttpStatus` 5xx (500–599) | ✅ | 服务端临时故障，CLI 可能落到其他实例 |
| `HttpError::HttpStatus` 429 | ❌ | 限流随 token / 账号走，CLI 同 token 同样被限流 |
| `HttpError::HttpStatus` 401 / 403 | ❌ | 认证问题，CLI 共用 `~/.codex/auth.json` 同样失败 |
| `HttpError::HttpStatus` 其它 4xx | ❌ | 请求本身问题，CLI 不会修正 |
| `ProviderError::Timeout` / `NetworkFailed` | ✅ | 与 HTTP Timeout/Transport 等价处理 |
| `ProviderError::NoData` / `ParseFailed` | ❌ | 上游响应本身问题，CLI 不会救 |
| `ProviderError::SessionExpired` / `AuthRequired` | ❌ | 同 401/403 |
| `ProviderError::ConfigMissing` | ❌ | 环境未备 |

判定逻辑见 `mod.rs::should_fallback_to_cli`，决策矩阵由 `fallback_eligible_*` / `fallback_not_eligible_*` 系列测试（含 `fallback_not_eligible_for_429_rate_limited`）完整覆盖。

## 与 CodexBar 的对照

主要解析行为对齐 CodexBar `CodexOAuthUsageFetcher` / `CodexStatusProbe` / `CodexRateWindowNormalizer`：

- `quota.rs::resolve_role_from_minutes` ↔ `CodexRateWindowNormalizer.swift`
- `auth.rs::resolve_account` ↔ `CodexReconciledState.oauthIdentity`
- `config.rs::resolve_usage_url` ↔ `CodexOAuthUsageFetcher.resolveUsageURL`
- `rpc_probe.rs::parse_rate_limits` ↔ `CodexRPCClient.fetchRateLimits` / `account/rateLimits/read`
- `status_probe.rs::parse` ↔ `CodexStatusProbe.parse`（简化版，不解析 reset 时间戳）

差异：
- BananaTray 暂不实现 OpenAI Web Dashboard 抓取（CodexBar 的 code review / usage breakdown / credits history）
- BananaTray 暂不支持多 managed account 切换
- RPC fallback 可取百分比、窗口 reset 时间戳、窗口时长、credits balance、planType，以及 `rateLimitResetCredits` 里的重置类型 / 获取时间 / 到期时间；PTY fallback 只取百分比与 credits balance，不复现窗口 reset 时间戳，也没有可兑换重置明细
- 可兑换限流重置不是配额、积分或请求次数。托盘和设置页用单独的重置卡展示类型、获取时间、到期时间；缺明细时只显示一句数量说明。它不写入 quota history，也不触发额度告警。usage 响应通常只有 `available_count`，OAuth 成功后会再请求同主机的 `/wham/rate-limit-reset-credits`，独立超时为 1 秒；失败或超时保留 usage 数量摘要，不触发 token 刷新或 CLI fallback，不影响已经拿到的会话/周配额
