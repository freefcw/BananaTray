# `models::quota`

配额与 Provider 运行时状态的数据模型。该目录只包含纯数据和纯方法，不依赖 GPUI。

## Public API

外部调用方应继续通过 `crate::models::{QuotaInfo, ProviderStatus, ...}` 使用这些类型；`src/models/mod.rs` 负责保持 re-export 路径稳定。

## Module Split

- `types.rs` — `QuotaType`、`StatusLevel`，以及语言无关 stable key / severity ordering。
- `label.rs` — `QuotaLabelSpec`、`QuotaDetailSpec` 和内部 `slugify_key`，负责保存展示语义而非 locale 文案。
- `info.rs` — `QuotaInfo` 构造函数、百分比计算、余额模式和基于 `QuotaRules` 的状态阈值判断。
- `policy.rs` — 额度状态 / 告警阈值 policy：`QuotaThresholdUnit`（Percentage / Currency / Amount）、`QuotaThresholds`（warning / critical / notify 三档剩余值阈值，含校验、字符串解析与 `notify_threshold_reached` 判定）、`QuotaRules`（三单位全局规则）、`QuotaRuleOverrides`（Provider 级按单位整组覆盖）、`QuotaMeasurement`（单位化剩余值度量，附带 `comparison_scale` 记录计算 remaining 的原始操作数量级）与 `QuotaThresholdTarget`（设置草稿归属）。默认阈值：百分比 50/20/10，货币 10/2/1，积分·额度 100/20/10。
- `units.rs` — 百分比 / fraction 归一化常量与纯函数，仅供 quota 模块内部构造器使用；provider 应通过 `QuotaInfo` 的百分比构造器表达语义。
- `failure.rs` — `ProviderFailure`、`FailureReason`、`FailureAdvice` 的结构化失败语义。
- `refresh_data.rs` — provider 刷新成功后传回 runtime state 的 `RefreshData`。
- `provider_status.rs` — `ConnectionStatus`、`UpdateStatus`、`ErrorKind`、`ProviderStatus` 及状态转换方法。
- `tests.rs` — quota 模块单元测试，覆盖 stable key、状态阈值、构造器和 ProviderStatus 转换。

## Compatibility Notes

- 不要改变 serde 字段名、默认值或 enum variant；这些类型会进入设置持久化和运行时状态。
- `QuotaInfo::percentage()` / `percent_remaining()` 保持不 clamp，允许 over-quota 时超过 100% 或为负数。
- provider 若收到 `used_percent` / `remaining_percent` / `remaining_fraction`，应通过 `QuotaInfo::from_used_percent()`、`from_remaining_percent()`、`from_remaining_fraction()` 或对应 `with_key_*` 构造器创建配额，不要直接写百分比制 `limit` 或重复换算公式。
- `ProviderStatus::new(provider_id, metadata)` 要求 `provider_id.kind()` 与 `metadata.kind` 对齐；debug 构建会断言。
- 普通月度百分比配额使用 `QuotaType::Monthly` + `QuotaLabelSpec::Monthly`，两者的 `stable_key` 均为 `"monthly"`；不要与信用额语义的 `MonthlyCredits` 或带套餐层级的 `MonthlyTier` 混用。
- `QuotaLabelSpec::Credits` 在 `QuotaType::Points` 下保留旧 `stable_key = "general"` 以兼容 Kiro 早期版本（Regular Credits 早期被建模为 `General`）；其他 `Points` 类型的 quota 应使用专属 `QuotaLabelSpec` 变体或 `Raw(...)`，避免与 Kiro 共用 `"general"` key。
- `QuotaType::Credit` 与 `QuotaType::Points` 都不应通过 `is_percentage_mode()` 走百分比展示；它们有专属的显示分支（`$X.XX / $Y.YY` vs `X.XX / Y.YY`）。状态颜色与告警阈值也不再换算成百分比：`QuotaInfo::threshold_measurement()` 按 `QuotaType` 取各单位的原生剩余值（Credit → 货币余额，Points 与非 Credit 纯余额 → 原生额度，其余 → 剩余百分比），即使 `limit` 恰好为 100 也不误判单位。
- 状态与告警判定统一使用剩余值 + inclusive `<=` 边界：`remaining <= critical` → Red，`remaining <= warning` → Yellow；`remaining <= 0` 固定为 Exhausted（0 不是可设置的耗尽阈值），`remaining <= notify` → Low。颜色的 warning / critical 与告警的 notify 共用同一个边界比较（`QuotaThresholds::status_level` / `notify_threshold_reached` 均接收完整 `QuotaMeasurement`）：在 `<=` 之外再吸收 `max(threshold.abs() * 1e-12, comparison_scale * f64::EPSILON * 8)` 的浮点舍入偏差——`comparison_scale` 是计算 remaining 的原始操作数最大量级，且与 remaining 同单位：Currency / Points 取 `max(|limit|, |used|)`，Percentage 换算成百分比尺度 `(used.abs()/limit).max(1.0) * 100`，纯余额只取余额绝对值。没有固定绝对容差底限；耗尽判定仍是严格 `remaining <= 0`，阈值校验的顺序比较也保持严格，均不含任何容差。`comparison_scale` 纯派生不持久化，设置 JSON 形状不变。`QuotaDisplayMode` 只改变 Used / Remaining 展示，不改变判定方向。无有效 measurement（无余额且 `limit <= 0`、非有限数据）的 quota 不参与判定，不能被当成 0 或 100。
- 百分比来源的配额不能用 `QuotaType::Credit`：Credit 会让 `threshold_measurement()` 按货币余额判定，把 `95% used` 误当成剩余 \$5。仅含百分比的信用类展示（如 Claude CLI 的 percent-only `Extra usage`）应归一为 `QuotaType::General`，让剩余百分比参与判定；金额形态（`$X / $Y`）才保留 `Credit`。
