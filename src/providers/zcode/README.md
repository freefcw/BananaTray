# ZCode Provider

`ZCodeProvider` 监控 ZCode 使用的 GLM Coding Plan 配额。Z.ai 国际站和智谱 BigModel 国内站走同一套 monitor 响应，差别只在网关主机。

## 重置窗口

配额响应的 `data.limits[]` 里每一项是一条独立重置周期。分类看 `unit` / `number`，不看数组顺序，也不因为出现了新的 `type` 就丢掉已知窗口。

| `unit` | `number` | 窗口 | 展示 |
|--------|----------|------|------|
| `3` | `5` | 5 小时滚动 | `QuotaLabelSpec::Session`，stable key `session` |
| `3` | 其它正整数 | N 小时滚动 | `QuotaLabelSpec::RollingHours`，stable key `rolling-{N}h` |
| `6` | `1` 或 `7` | 7 天 | `QuotaLabelSpec::Weekly`。`number` 不是「几周」 |
| `5` | 任意 | 月 | `QuotaLabelSpec::Monthly`。旧套餐的 MCP / 工具调用额度 |

2026-07-30 之后的积分套餐用 `CREDIT_LIMIT`（5 小时 + 每周，MCP 从同一份积分里扣）。更早的套餐用 `TOKENS_LIMIT`（百分比）外加一条 `TIME_LIMIT` 月度 MCP。两种都要保留。

已用百分比优先用服务端 `percentage`（0–100，允许超过 100）。缺了这个字段、但有 `usage` 和 `remaining`（或 `currentValue`）时，才用剩余比例换算。不要用 token 原值当展示数字。

`nextResetTime` 是 epoch 毫秒，写到 `QuotaDetailSpec::ResetAt`；秒级时间戳视为无效，不做单位猜测。`0` 或缺失表示窗口还没开始，不在本地推算重置时间。

认不出的 `unit` 跳过。只要还有一条认得的窗口，就只返回认得的那些。一条都认不出是 `ParseFailed`，避免格式漂移被看成「还没有用量」。`limits` 为空是 `NoData`。

同一类窗口出现多次时，第二条起的 stable key 加 `-2`、`-3`，避免设置里的隐藏配额串掉。

套餐档位来自 `data.level`（如 `lite` / `pro` / `max`），放进 `account_tier`。

### 重置卡

ZCode 端内还有两种可手动使用的重置卡，和上面的周期重置不是一回事：

- `available_five_hour_resets`：5 小时额度重置卡
- `available_week_resets`：周额度重置卡

库存在 `GET https://zcode.z.ai/api/v1/coding-plan/reset/status`，不是配额接口。请求头是登录态：

- `Authorization: Bearer <zcodejwttoken>`
- `X-Bigmodel-Authorization: <oauth access token>`
- `Bigmodel-Target-Type: PERSONAL`（团队套餐为 `TEAM`，并带上组织 / 项目头）

`data` 里每张卡有 `expire_at`。ZCode 用它和 `Date.now()` 比较，所以按毫秒理解；已经过期的不计入可用张数。

这两份 token 在 `~/.zcode/v2/credentials.json` 里是 `enc:` 加密的。BananaTray 当前只用 API Key 监控周期配额，不解密这份登录态，也未实现重置卡库存请求或展示，不调用 `POST /api/v1/coding-plan/reset/use` 去核销。API Key 无法读取端内重置卡库存。

## 请求

- 国际站：`GET https://api.z.ai/api/monitor/usage/quota/limit`
- 国内站：`GET https://open.bigmodel.cn/api/monitor/usage/quota/limit`
- `Authorization: Bearer <api_key>`

网关：

- 本地配置里的 `baseURL` 主机是 `api.z.ai` / `z.ai` 时固定国际站，是 `open.bigmodel.cn` / `bigmodel.cn` 时固定国内站。
- 否则看 `ZAI_REGION`（`china` / `cn` / `bigmodel`，或 `global`）和 `Z_AI_API_HOST`（只接受上面的官方主机）。
- 都没有时先请求国际站。HTTP 401/403、响应体里的 401/403，或超时 / 传输失败，再试国内站一次。国内站也连不上时，保留国际站那次传输错误。5xx 不跨站重试。

## 凭据

每次刷新按这个顺序取 key，不把凭据留在 provider 内部：

1. BananaTray `zcode_api_key`
2. `ZAI_API_KEY`，其次 `GLM_API_KEY`
3. ZCode 明文配置里的 `apiKey` / `api_key`

本地文件顺序：

1. `ZCODE_PERSONAL_PROVIDER_CONFIG_FILE`
2. `$ZCODE_DATA_BASE_DIR/v2/provider_config.json`
3. `~/.zcode/v2/provider_config.json`
4. `~/.zcode/cli/provider_config.json`
5. `~/.zcode/cli/config.json`
6. `~/.zcode/v2/config.json`

只接受所在路径或同对象字段能对上 z.ai / BigModel / ZCode / GLM Coding Plan 的明文 key；显式配置了非官方 `baseURL` 的对象会跳过，不会把中转站 key 发到官方网关。`enc:` 开头的登录态不解密。短于 8 个字符的值忽略。

前两项已经有 key 时不再读本地文件。只有本地文件、且每一份都损坏时返回 `ParseFailed`；文件在但读不了返回 `Unavailable`；没有任何 key 返回 `ConfigMissing`。

## 模块

- `auth.rs`：凭据顺序、网关选择、本地 JSON
- `client.rs`：两个官方配额 URL 和 Bearer GET
- `parser.rs`：按重置窗口解析响应
- `mod.rs`：元数据、设置页 token 面板、跨网关重试
