# Codex Provider

OpenAI Codex（ChatGPT 后端）配额抓取实现。对应 `~/.codex/auth.json` 的 OAuth 凭据。

## 文件分工

| 文件 | 职责 |
|------|------|
| `mod.rs` | `AiProvider` 实现入口；`refresh()` 流水线、OAuth → CLI fallback 决策、`should_fallback_to_cli` |
| `auth.rs` | `~/.codex/auth.json` 解析、JWT `id_token` 提取 email/plan/account_id、access_token 主动 + 被动刷新 |
| `config.rs` | `~/.codex/config.toml` 读取，`chatgpt_base_url` 解析与归一化（支持自托管 ChatGPT 网关） |
| `client.rs` | usage API HTTP 请求构造（含 `ChatGPT-Account-Id` 头） |
| `parser.rs` | OAuth `/wham/usage` JSON 响应解析；按 `limit_window_seconds` 区分 session/weekly；credits / plan_type 抽取 |
| `status_probe.rs` | OAuth 失败时通过 PTY 启动 `codex /status` 兑底解析 |

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
   parse_usage_response   status_probe::fetch_via_cli
        │                 │
        └────────┬────────┘
                 ↓
        resolve_account (JWT)
                 ↓
        RefreshData::with_account
```

## Fallback 决策表

| OAuth 错误 | 是否兑底到 CLI | 理由 |
|-----------|---------------|------|
| `HttpError::Timeout` | ✅ | 网络瞬断，CLI 走本地路径可能能绕过 |
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

- `parser.rs::resolve_role` ↔ `CodexRateWindowNormalizer.swift`
- `auth.rs::resolve_account` ↔ `CodexReconciledState.oauthIdentity`
- `config.rs::resolve_usage_url` ↔ `CodexOAuthUsageFetcher.resolveUsageURL`
- `status_probe.rs::parse` ↔ `CodexStatusProbe.parse`（简化版，不解析 reset 时间戳）

差异：
- BananaTray 暂不实现 OpenAI Web Dashboard 抓取（CodexBar 的 code review / usage breakdown / credits history）
- BananaTray 暂不支持多 managed account 切换
- CLI fallback 只取百分比与 credits balance，不复现 reset 时间戳（OAuth 已能精确给 epoch_secs，CLI 退化合理）
