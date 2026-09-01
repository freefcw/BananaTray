# Amp Provider

通过 `amp usage --no-color` 读取 Amp 用量。设置 / 状态稳定 key 是 `amp`。Descriptor ID 是 `amp:cli`。

## Data Contract

- 数据源：本机 `amp` CLI，没有 JSON 开关。
- **Markdown 加粗**：2026-08-31 起（Amp CLI `0.0.1788192028`）`--no-color` 仍输出 `**Label:**` 形态的加粗，`mod.rs::strip_markdown_bold` 统一剥离后再走各正则。
- 邮箱：`Signed in as <email> (...)`。
- Free 档：`Amp Free: <N>% remaining today (resets daily)` → `QuotaType::General` 百分比；括号说明原文进详情行。
- 信用额度：`Monthly credits: $X / $Y remaining` → `QuotaType::Credit`；`Individual credits: $0 remaining` 跳过展示。
- 订阅制（现行格式）：一行两个月度池，绝对值 + 括号百分比 ——
  `agent usage $6.42 of $20 remaining (32%)`（agent 调用额度，美元）、
  `orb usage 750h of 750h a1.small orb hours remaining (100%)`（远程实例，小时）。
  拆成独立 quota，`QuotaLabelSpec::SubscriptionUsage { plan, pool }`，
  进度条用 CLI 括号百分比（缺失时由绝对值换算），绝对值原文（如 `$6.42 of $20`）透传到详情行。
  行尾 `- period ... , ends in N days` 暂不入模型。

## 订阅行策略

各代文案拆成独立策略，互不认领对方的行。调度在 `subscription/mod.rs`：现行先，按代际从新到旧。

| 策略 | 文件 | 行前缀 | 池形态 | 状态 |
|------|------|--------|--------|------|
| current | `subscription/current.rs` | `Amp <Plan> Subscription:` | 绝对值 + 括号百分比，池名 `agent` / `orb` | 现行。Amp CLI `0.0.1788192028`（2026-08-31）起 |
| interim | `subscription/interim.rs` | `Amp <Plan> Subscription:` | 纯百分比，池名 `other` / `orb` | 已失效。2026-08-31 起 CLI 不再输出；预计 2026-11-30 删除 |
| legacy | `subscription/legacy.rs` | `Subscription <Plan>:` | 纯百分比，池名 `other` / `orb` | 已失效。2026-08-17 起 CLI 不再输出；预计 2026-11-17 删除 |

前缀相同、只靠池形态区分的策略（current vs interim），认不出池片段时必须返回 `None` 放行，不能返回空 vec 截断调度。

删除过时策略时：删对应文件、`subscription/mod.rs` 里对应的 `if`，以及标了旧格式的测试。删除前先搜日志 `amp: subscription matched interim` / `amp: subscription matched legacy`。

百分比池片段 `X% other usage and Y% orb usage remaining` 由 interim / legacy 共用，留在 `subscription/mod.rs` 的 `quotas_from_pool_text`；值池片段正则 `VALUE_POOL_RE` 也在 `subscription/mod.rs`。

## Module Boundaries

- `mod.rs`：descriptor / availability / CLI 调用 / markdown 剥离 / Free 与信用额度解析；订阅行交给策略
- `subscription/`：订阅行策略与调度，不碰 CLI
