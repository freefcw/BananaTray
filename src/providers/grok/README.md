# Grok Provider

监控 Grok Build / grok.com 订阅周额度，对应 Grok TUI 里 `/usage` 看到的那套数。

设置 / 状态稳定 key 是 `grok`。Descriptor ID 是 `grok:api`。

这不是 xAI 开发者 API 的预付积分。`XAI_API_KEY` / Management API 余额与 SuperGrok 周池无关。

## Data Contract

- Endpoint: `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits`
- Authentication: `Authorization: Bearer <~/.grok/auth.json session key>`，并带 `x-xai-token-auth: xai-grok-cli`（与 Grok CLI 一致）
- 只走 `?format=credits`。不带该 query 的 `/v1/billing` 返回的是另一套月度 API spend，不能当 Grok Build 用量。
- `config.creditUsagePercent` → `QuotaType::Weekly` / `QuotaLabelSpec::Weekly`（`currentPeriod.type` 为 `USAGE_PERIOD_TYPE_MONTHLY` / `DAILY` 时改用对应标签）
- `currentPeriod.end` 或 `billingPeriodEnd` → `QuotaDetailSpec::ResetAt`
- `productUsage` 仅在多产品，或唯一产品百分比与总池不同时展示；与总池相同的单独 `GrokBuild` 行会丢掉，避免重复。
- 百分比经 `QuotaInfo::from_used_percent()` 入库。

## Credential Resolution

读取 Grok Build 的本地 OAuth session，必要时刷新并写回：

1. `$GROK_HOME/auth.json`（若设置）
2. `~/.grok/auth.json`

文件是 `issuer::id → session` 的 map。选用带非空 `key` 且 `expires_at` 最晚的 session。

Access token 大约 30 分钟过期。到期前 5 分钟会用 `refresh_token` 调 `https://auth.x.ai/oauth2/token`，再原子写回 `key` / `refresh_token` / `expires_at`。Grok CLI 自己也会读盘接管 sibling refresh。刷新失败时先继续用旧 token；billing 仍 401/403 则 `SessionExpired`，提示 `grok login`。

不写回除 token 字段以外的内容，也不读取浏览器 cookie。

## Module Boundaries

- `auth.rs`：auth.json 路径、session 选择、OIDC refresh、原子写回
- `client.rs`：Bearer GET billing
- `parser.rs`：credits 响应 → 稳定 quota 语义
- `mod.rs`：descriptor / availability / refresh；401 后的一次 refresh 重试集中在 `fetch_billing_authed`
