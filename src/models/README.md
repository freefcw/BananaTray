# src/models/

Core data types shared across the entire crate. **No GPUI dependency** — all types here are pure Rust, suitable for use in tests and non-UI modules.

## Files

### `provider.rs` — Provider Identity

- **`ProviderKind`** — enum of all supported providers (Claude, Gemini, Copilot, ClinePass, Codex, Kimi, Amp, Cursor, OpenCode, MiniMax, VertexAi, Kilo, Kiro, Antigravity, Windsurf, Grok, Custom). Generated from the crate-level `builtin_provider_manifest!` via the local `define_provider_kind!` macro. `Windsurf` is now the compatibility key for Devin: the UI display name and icon have moved to Devin, but the enum value, persisted state, and internal key remain `Windsurf` / `"windsurf"` to avoid breaking user config.
  - `all()` — static slice of all variants (defines canonical ordering)
  - `id_key()` — lowercase string identifier used in settings serialization (e.g. `"claude"`, `"vertexai"`)
  - `from_id_key()` — reverse lookup from string
- **`ProviderMetadata`** — display-oriented metadata: `display_name`, `brand_name`, `icon_asset`, `dashboard_url`, `account_hint`, `source_label`. Providers expose it via `ProviderDescriptor`.
- **`ProviderId`** — unified provider identifier: `BuiltIn(ProviderKind)` for built-in providers, `Custom(String)` for YAML-declared custom providers. Key methods: `id_key()`, `from_id_key()`, `kind()`, `is_custom()`.
- **`ProviderDescriptor`** — combines a registration/source descriptor ID with `ProviderMetadata`. Built-in descriptor IDs may include suffixes such as `codex:api`; persisted built-in identity always comes from `ProviderId::BuiltIn(kind).id_key()`.
- **`ProviderCapability`** — provider product capability tier: `Monitorable`, `Informational`, `Placeholder`. Refresh scheduling and empty-state UI semantics are keyed off this enum.
- **`SettingsCapability`** — provider settings UI capability declaration (pure data, GPUI-free). Variants: `None` (default, no extra settings UI), `TokenInput(TokenInputCapability)` (generic token input panel), `NewApiEditable { base_url }` (NewAPI config editor), `ScriptEditable { interpreter }` (script provider editor). The latter two carry the current config value so the settings card can show which config it manages; both are filled by `CustomProvider::settings_capability()` from the loaded YAML. `TokenInputCapability` now contains only static UI metadata and `credential_key`; provider-specific runtime display logic lives in `ProviderCapabilities::resolve_token_input_state()`.
- **`NavTab`** — navigation tab enum: `Provider(ProviderId)` or `Settings`

### `quota/` — Usage Data

Refactored into a sub-directory with its own [README](quota/README.md). External imports remain stable through `crate::models::{QuotaInfo, ProviderStatus, ...}` re-exports.

- **`QuotaType`** — discriminant for quota categories: `Session`, `Weekly`, `Monthly`, `ModelSpecific(String)`, `Credit` (currency, `$` prefix), `Points` (non-currency credits, e.g. Kiro), `General` (fallback)
- **`QuotaLabelSpec`** — quota title semantic payload. Providers store stable meaning (`Daily`, `Session`, `Weekly`, `Monthly`, `WeeklyModel { .. }`, `MonthlyCredits`, `Credits`, `Raw(String)` etc.); selector/UI turns it into locale-specific text.
- **`QuotaDetailSpec`** — quota detail semantic payload for the 4th line (`Unlimited`, `RequestCount`, `CreditRemaining`, `ResetAt`, `ResetDate`, `ExpiresInDays`, `Raw(String)`).
- **`StatusLevel`** — traffic-light severity: `Green`, `Yellow`, `Red` (implements `Ord`)
- **`QuotaInfo`** — single quota entry with numeric state plus display semantics:
  - numeric fields: `used`, `limit`, `quota_type`, `remaining_balance`
  - stable identity: `stable_key` (used for settings persistence / UI keys / hidden quota matching)
  - display payloads: `label_spec`, `detail_spec`
  - constructors: `with_details(...)`, `with_key(...)`, `balance_only(...)`, `balance_only_with_key(...)`
  - note: `QuotaInfo` no longer stores locale-dependent display strings
  - key methods:
  - `percentage()` / `percent_remaining()` — usage ratios (not clamped, allows >100% for over-quota)
  - `status_level()` — maps percentage to `StatusLevel` (thresholds: <80% Green, <95% Yellow, else Red)
  - `is_percentage_mode()` — true when `limit == 100.0` (data is already a percentage)
  - `is_balance_only()` — true when the quota is modeled as remaining balance instead of progress-bar usage
- **`ConnectionStatus`** — provider connection state: `Connected`, `Disconnected`, `Refreshing`, `Error`
- **`FailureReason`** / **`FailureAdvice`** / **`ProviderFailure`** — stable provider failure payload stored in state and formatted later by selectors
- **`ProviderStatus`** — full runtime state for one provider: metadata + connection status + quotas + account info + `last_failure` + timestamps
  - `last_failure` holds structured failure semantics, replacing the old cached `error_message`
  - locale switching should only re-render selector/UI text; it should not require provider refresh to clear cached strings
  - runtime-only `provider_capability` mirrors the registered `ProviderCapabilities::provider_capability()` so selectors can hide refresh/retry affordances for non-monitorable entries
  - `ProviderStatus::new(provider_id, metadata)` — unified constructor for built-in and custom providers. Callers must keep `provider_id.kind()` and `metadata.kind` aligned; debug builds assert this invariant.
- **`RefreshData`** — refresh result payload: `quotas: Vec<QuotaInfo>` + optional `account_email`, `account_tier`, runtime `source_label`

### `settings/` — User Preferences (sub-module)

Refactored into a sub-directory with its own [README](settings/README.md). Key types:

- **`AppSettings`** — top-level runtime configuration composed of `SystemSettings`, `NotificationSettings`, `DisplaySettings`, `LoggingSettings`, and `ProviderConfig`; `settings_store::PersistedAppSettingsV1` owns the top-level JSON persistence boundary
- **`ProviderConfig`** — provider enable/disable, ordering, sidebar, quota visibility, and app-managed credentials
- **`ProviderSettings`** — flattened credential key-value store (`github_token`, future `custom_token`, etc.), stored under `ProviderConfig::credentials` for provider-scoped persisted tokens owned by BananaTray
- **`TrayPopupSettings`** / **`SavedWindowPosition`** — persisted tray popup UI state, currently used for Linux drag-position restore
- **`TrayIconStyle`** / **`QuotaDisplayMode`** / **`AppTheme`** — display enums
- `provider_config_ordering.rs` / `provider_config_quota.rs` / `provider_config_sidebar.rs` — domain method extensions

### `newapi.rs` — NewAPI Provider Data Types

纯数据类型和 ID 计算逻辑，从 `providers/custom/generator.rs` 迁入以消除 `application/` → `providers/` 的反向依赖。

- **`NewApiConfig`** — 用户通过表单提交的 NewAPI 配置（display_name, base_url, cookie, user_id, divisor）
- **`NewApiEditData`** — 从 YAML 解析出的编辑回填数据（含 `original_filename`）
- **`extract_domain_slug(base_url)`** — URL → slug 纯函数（如 `https://my-api.example.com` → `my-api-example-com`）
- **`newapi_provider_id(base_url, user_id)`** — 从 URL 与可选账号维度计算 Provider ID（`{slug}:newapi` 或 `{slug}-{user}:newapi`），reducer 用于预注册

### `custom_provider_lifecycle.rs` — Custom Provider Lifecycle Outcomes

跨 `providers` / `runtime` / `application` 使用的自定义 provider 生命周期结果类型，保持 application 层不依赖 `providers/`：

- **`CustomProviderLifecycleFailure`** — 保存、删除、加载过程中的结构化失败语义（非法 provider id、YAML 不存在、脚本 provider 非法、文件操作失败）
- **`NewApiSaveSuccess`** / **`ScriptProviderSaveSuccess`** — 保存成功的文件路径和 settings 同步结果
- **`ScriptProviderDeleteSuccess`** — 区分脚本 provider 完全删除与 YAML 已删但 companion script 删除失败的 partial success

### `script_provider.rs` — Script Provider Data Types

设置页脚本向导使用的纯数据类型和 stdout 解析逻辑：

- **`ScriptProviderConfig`** — 表单提交数据（display_name、provider_id、interpreter、timeout_ms、script）
- **`ScriptProviderEditData`** — 从生成的 YAML + 脚本文件回读出的编辑数据，保留原始 YAML / 脚本文件名
- **`ScriptProviderTestResult`** / **`ScriptProviderQuotaPreview`** — Run Test 的结果和预览数据
- **`script_provider_id(name)`** — 从展示名称生成 `{slug}:script` provider id
- **`parse_script_stdout(stdout)`** — 校验脚本 stdout JSON，读取 `remaining` / `used` / `unit` / `label` 等字段

### `layout.rs` — Popup Window Sizing

- **`PopupLayout`** — constants for popup dimensions: `WIDTH`, `CARD_HEIGHT`, `CARD_SPACER`, `DASHBOARD_ROW_HEIGHT`, `ACCOUNT_INFO_HEIGHT`, `MIN_HEIGHT`, `MIN_OVERVIEW_HEIGHT`, `MAX_HEIGHT`, plus Overview card/element sizing constants (`OVERVIEW_ITEM_HEIGHT`, `OVERVIEW_DOT_SIZE`, `OVERVIEW_ICON_SIZE`, `OVERVIEW_BAR_W`, `OVERVIEW_BAR_H`, `OVERVIEW_EXPANDED_BAR_H`, `OVERVIEW_VALUE_W`, `OVERVIEW_BADGE_W`, `OVERVIEW_EXPAND_W`, `OVERVIEW_QUOTA_LINE_HEIGHT`, `OVERVIEW_QUOTA_LINE_GAP` 等)
  - `overview_multi_item_height(quota_rows)` — 计算展开态多行布局的卡片高度
  - `MIN_OVERVIEW_HEIGHT` — Overview 专属最小高度（`FIXED_HEIGHT + OVERVIEW_ITEM_HEIGHT`）；不复用 `MIN_HEIGHT`，避免单 Provider 时窗口多出 ~87px 死空白
- **`compute_popup_height_for_quotas()`** — pure function mapping quota count to pixel height (clamped to min/max)
- **`compute_popup_height_detailed()`** — extended height calculation with dashboard row and account info flags
- **`compute_popup_height_for_overview(card_rows)`** — height calculation from per-provider card row counts (`1` = 折叠/单配额单行卡片，`>1` = 展开态多行卡片，走 `overview_multi_item_height`)，clamped to `MIN_OVERVIEW_HEIGHT` / `MAX_HEIGHT`。行数由 `AppSession::overview_card_rows()` 按展开记忆算出，供打开弹窗 / 切入 Overview 时定高；停留在 Overview 时展开折叠不改窗口

### `test_helpers.rs` — Test Fixtures

- Test helper functions for constructing `ProviderStatus`, `QuotaInfo`, `AppSession` etc. in unit tests
- Used by `application/` test modules to avoid boilerplate

## Constraints

- **No GPUI imports**. This module must remain framework-agnostic.
- `ProviderKind` ordering follows `src/builtin_provider_manifest.rs` and determines the default navigation tab order.
- When adding a built-in provider, update the single manifest entry; `ProviderKind`, `id_key()`, `from_id_key()`, and built-in registration are generated from it.
- `QuotaInfo` intentionally does not clamp percentages — callers handle display of over-quota states.
