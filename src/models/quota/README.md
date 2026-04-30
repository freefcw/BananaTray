# `models::quota`

配额与 Provider 运行时状态的数据模型。该目录只包含纯数据和纯方法，不依赖 GPUI。

## Public API

外部调用方应继续通过 `crate::models::{QuotaInfo, ProviderStatus, ...}` 使用这些类型；`src/models/mod.rs` 负责保持 re-export 路径稳定。

## Module Split

- `types.rs` — `QuotaType`、`StatusLevel`，以及语言无关 stable key / severity ordering。
- `label.rs` — `QuotaLabelSpec`、`QuotaDetailSpec` 和内部 `slugify_key`，负责保存展示语义而非 locale 文案。
- `info.rs` — `QuotaInfo` 构造函数、百分比计算、余额模式和状态阈值判断。
- `failure.rs` — `ProviderFailure`、`FailureReason`、`FailureAdvice` 的结构化失败语义。
- `refresh_data.rs` — provider 刷新成功后传回 runtime state 的 `RefreshData`。
- `provider_status.rs` — `ConnectionStatus`、`UpdateStatus`、`ErrorKind`、`ProviderStatus` 及状态转换方法。
- `tests.rs` — quota 模块单元测试，覆盖 stable key、状态阈值、构造器和 ProviderStatus 转换。

## Compatibility Notes

- 不要改变 serde 字段名、默认值或 enum variant；这些类型会进入设置持久化和运行时状态。
- `QuotaInfo::percentage()` / `percent_remaining()` 保持不 clamp，允许 over-quota 时超过 100% 或为负数。
- `ProviderStatus::new(provider_id, metadata)` 要求 `provider_id.kind()` 与 `metadata.kind` 对齐；debug 构建会断言。
- `QuotaLabelSpec::Credits` 在 `QuotaType::Points` 下保留旧 `stable_key = "general"` 以兼容 Kiro 早期版本（Regular Credits 早期被建模为 `General`）；其他 `Points` 类型的 quota 应使用专属 `QuotaLabelSpec` 变体或 `Raw(...)`，避免与 Kiro 共用 `"general"` key。
- `QuotaType::Credit` 与 `QuotaType::Points` 都不应通过 `is_percentage_mode()` 走百分比展示；它们有专属的显示分支（`$X.XX / $Y.YY` vs `X.XX / Y.YY`），但状态颜色阈值仍按 `percent_remaining()` 计算。
