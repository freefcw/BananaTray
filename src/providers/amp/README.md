# Amp Provider

通过 `amp usage --no-color` 读取 Amp 用量。设置 / 状态稳定 key 是 `amp`。Descriptor ID 是 `amp:cli`。

## Data Contract

- 数据源：本机 `amp` CLI，没有 JSON 开关。
- 邮箱：`Signed in as <email> (...)`。
- Free 档：`Amp Free: <N>% remaining today (resets daily)` → `QuotaType::General` 百分比；括号说明原文进详情行。
- 信用额度：`Monthly credits: $X / $Y remaining` → `QuotaType::Credit`；`Individual credits: $0 remaining` 跳过展示。
- 订阅制：一行两个月度池（`other` = agent 用量，`orb` = 远程实例），拆成独立 quota，`QuotaLabelSpec::SubscriptionUsage { plan, pool }`。

## 订阅行策略

新旧文案拆成两套策略，互不认领对方的行。调度在 `subscription/mod.rs`：现行先，旧版后。

| 策略 | 文件 | 行前缀 | 状态 |
|------|------|--------|------|
| current | `subscription/current.rs` | `Amp <Plan> Subscription:` | 现行。Amp CLI `0.0.1786939945`（2026-08-17）起 |
| legacy | `subscription/legacy.rs` | `Subscription <Plan>:` | 已失效。同日 CLI 起不再输出；预计 2026-11-17 删除 |

删除旧版时：去掉 `subscription/legacy.rs`、`subscription/mod.rs` 里对应的 `if`，以及 `mod.rs` 里标了旧格式的测试。删除前先搜日志 `amp: subscription matched legacy`。

池片段 `X% other usage and Y% orb usage remaining` 两种前缀共用，留在 `subscription/mod.rs` 的 `quotas_from_pool_text`。

## Module Boundaries

- `mod.rs`：descriptor / availability / CLI 调用 / Free 与信用额度解析；订阅行交给策略
- `subscription/`：订阅行策略与调度，不碰 CLI
