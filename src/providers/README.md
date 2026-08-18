# src/providers/

Provider abstraction layer and all 16 AI provider implementations.

## Core Abstractions

### `mod.rs` — Registry + Helpers | `ai_provider.rs` — Trait | `error.rs` — Error Types

- **`AiProvider`** trait (`ai_provider.rs`, async_trait) — core refresh contract every provider exposes:
  - `descriptor() -> ProviderDescriptor` — provider ID + `ProviderMetadata`
  - `check_availability(ctx: &ProviderExecutionContext) -> ProviderResult<()>` — environment/config check with structured error
  - `refresh(ctx: &ProviderExecutionContext) -> ProviderResult<RefreshData>` — fetch latest quota data; defaults to `NoData`, so `Monitorable` providers must override it while `Informational` / `Placeholder` entries normally do not
- **`ProviderExecutionContext`** — explicit refresh-time inputs, currently carrying BananaTray-managed `ProviderSettings` credentials for this refresh attempt
- **`ProviderCapabilities`** — product/settings capability adapter kept separate from the refresh contract:
  - `settings_capability() -> SettingsCapability` — declare settings UI capability (default: `None`)
  - `provider_capability() -> ProviderCapability` — declare whether the provider is `Monitorable`, `Informational`, or `Placeholder`
  - `resolve_token_input_state(settings)` — optional provider-side runtime token display state (masked value / source / edit mode)
- **`ProviderEntry`** — registry trait object combining `AiProvider + ProviderCapabilities`
- **`SettingsCapability`** — provider-defined settings capability:
  - `None` — no extra settings UI
  - `TokenInput(TokenInputCapability)` — generic token panel driven by static i18n keys + `credential_key`
  - `NewApiEditable` — NewAPI custom-provider editor actions
  - `ScriptEditable` — script custom-provider editor actions, including script/interpreter/test/delete controls
- **`ProviderCapability`** — provider product capability tier:
  - `Monitorable` — participates in normal refresh flows
  - `Informational` — reference-only entry, no refresh/retry actions
  - `Placeholder` — discoverable but not directly monitorable, no refresh/retry actions
- **`TokenInputCapability`** — token settings contract:
  - static UI metadata (`title_i18n_key`, `description_i18n_key`, `placeholder_i18n_key`, `create_url`)
  - every `*_i18n_key` and token source key must exist in all files under `locales/`; `src/i18n.rs` tests enforce this for literal and provider-declared keys
  - `credential_key` for persisted storage in `ProviderConfig::credentials`
  - only for BananaTray-managed token overrides; providers may still resolve auth from external files, CLI sessions, or env vars
- **`resolve_token_input_state()`** — optional `ProviderCapabilities` hook for provider-side runtime token display state; override only when default credential-store behavior is insufficient
- **`ProviderDescriptor`** — static description for registration and UI metadata. For built-in providers, `descriptor().id` is a registration/dedup/source descriptor and may include suffixes such as `codex:api`; settings/state routing uses `ProviderId::BuiltIn(kind)` and `ProviderKind::id_key()` instead. For custom providers, the YAML `id` is persisted as `ProviderId::Custom`.
- **`ProviderError`** — structured error enum with variants: `CliNotFound`, `Unavailable`, `AuthRequired`, `SessionExpired`, `FolderTrustRequired`, `UpdateRequired`, `ParseFailed`, `Timeout`, `NoData`, `NetworkFailed`, `ConfigMissing`, `FetchFailed`
- **`ProviderResult<T>`** — provider boundary result type (`Result<T, ProviderError>`) used by `AiProvider` and `ProviderManager`
- **`ProviderError::to_failure()` / `error_kind()`** — maps provider errors to stable `ProviderFailure` and `ErrorKind`; final locale-specific message generation belongs to selector/UI
- **`common/`** — crate-internal cross-provider helpers shared by multiple implementations (for example JWT decoding, CLI execution helpers, config path candidates, Unicode-safe secret preview masking)
- **`codeium_family/`** — crate-internal shared local-source/spec/parser primitives for Antigravity and Devin Desktop; provider-specific orchestration stays in each facade
- **`docs/archive/provider/provider-refactor-retrospective.md`** — why the provider layer was refactored this way, including rejected abstractions
- **`src/builtin_provider_manifest.rs`** — single compile-time manifest for built-in providers; feeds both `ProviderKind` generation and built-in registration
- **`register_providers!`** macro — consumes the manifest to declare private built-in provider modules and generate crate-internal `register_all()` function
- **`define_unit_provider!`** macro — boilerplate for zero-field provider structs

### `manager.rs` — ProviderManager

Aggregation registry holding all provider implementations. Maintains exactly two indexes matching `ProviderId`'s two variants: `providers_by_kind` (built-in) and `custom_providers_by_id` (custom).

- `new()` — builds a pure built-in registry without filesystem I/O
- `load_default()` — explicitly scans the default custom-provider directory and merges valid YAML providers with built-ins
- `register()` — adds a provider (deduplicates by id and kind, and rejects custom IDs reserved by built-in stable keys)
- `provider_for_id(id)` — unified lookup by `ProviderId`
- `metadata_for(kind)` — returns metadata (derived from provider) with fallback
- `initial_statuses()` — generates `Vec<ProviderStatus>` for all `ProviderKind` variants
- `initial_statuses()` also copies each provider's `settings_capability()` and `provider_capability()` into runtime `ProviderStatus`
- `refresh_by_id(id, provider_credentials)` — routes built-in and custom providers through one refresh entrypoint; non-monitorable providers return `NoData`, monitorable providers receive a `ProviderExecutionContext`, run `check_availability(ctx)`, and then delegate to `refresh(ctx)`
- `ProviderManagerHandle` — shared snapshot handle used by foreground runtime and background refresh loop; hot-reload swaps the inner `Arc<ProviderManager>` atomically so both sides observe the same registry

ProviderManager / ProviderManagerHandle form the provider facade used by the rest of the app.
Concrete built-in provider modules, `common/`, `custom/`, and `codeium_family/` are crate-internal implementation details; do not treat their module paths as external API.

### `custom/` — YAML-backed Providers

- Custom provider YAML files are resolved through `crate::platform::paths`
- Canonical directory:
  - macOS: `~/Library/Application Support/BananaTray/providers/`
  - Linux: `$XDG_CONFIG_HOME/bananatray/providers/`

## Provider Implementations

| File | Provider | Settings key | Descriptor ID | Capability | Data Source | Notes |
|------|----------|--------------|---------------|------------|-------------|-------|
| `claude/` | Claude | `claude` | `claude` | `Monitorable` | HTTP API + CLI fallback | `mod.rs` orchestrates source selection; `api_probe.rs` / `cli_probe.rs` implement sources; `credentials.rs` handles OAuth credential loading/refresh/save; `probe.rs` defines `UsageProbe` trait + `ProbeMode` |
| `gemini/` | Gemini | `gemini` | `gemini:api` | `Monitorable` | HTTP API | Split into `auth.rs`, `client.rs`, `parser.rs`, `mod.rs` |
| `copilot/` | Copilot | `copilot` | `copilot:api` | `Monitorable` | GitHub API | Split into `token.rs`, `client.rs`, `parser.rs`; declares `SettingsCapability::TokenInput(TokenInputCapability)`, provides a custom multi-source token resolver, uses the shared Unicode-safe secret preview helper, and reads app-managed `github_token` from `ProviderExecutionContext` during refresh |
| `cline_pass/` | ClinePass | `cline-pass` | `cline-pass:api` | `Monitorable` | Cline usage API | Reads BananaTray `cline_api_key`, `CLINE_API_KEY`, or Cline's `providers.json` in that order; local OAuth is read-only and never refreshed or written by BananaTray. Maps `five_hour`, `weekly`, and `monthly` limits to stable quota semantics. See `cline_pass/README.md` for paths and response contracts |
| `codex/` | Codex | `codex` | `codex:api` | `Monitorable` | ChatGPT API + CLI fallback | Split into `auth.rs`, `client.rs`, `config.rs`, `parser.rs`, `rpc_probe.rs`, `status_probe.rs`, `mod.rs`. `refresh(ctx)` uses HTTP first; recoverable HTTP failures fall back to `codex app-server` JSON-RPC before PTY `/status`. `auth.rs` decodes the OAuth `id_token` JWT for email / plan / `chatgpt_account_id`; credentials are reloaded after token rotation so the `ChatGPT-Account-Id` header and `RefreshData.account_*` reflect the latest state. `config.rs` reads `~/.codex/config.toml` for `chatgpt_base_url` to support self-hosted ChatGPT gateways |
| `kimi/` | Kimi | `kimi` | `kimi:api` | `Monitorable` | HTTP API | Split into `auth.rs`, `client.rs`, `parser.rs` |
| `amp/` | Amp | `amp` | `amp:cli` | `Monitorable` | CLI output | Uses `common::cli`；订阅行拆成 current / legacy 两套策略，见 `amp/README.md` |
| `cursor/` | Cursor | `cursor` | `cursor:api` | `Monitorable` | HTTP API | Split into `auth.rs`, `client.rs`, `parser.rs`; reads token directly from local SQLite (`state.vscdb`) through bundled `rusqlite` without requiring an external `sqlite3` executable; parses Auto / API usage pools from `usage-summary`. Free tier (`membershipType = free`) hides the API pool while `apiPercentUsed` stays 0 and skips the `breakdown.total` limit fallback; a non-zero free API percentage is still shown — see [docs/providers.md](../../docs/providers.md) |
| `antigravity/` | Antigravity | `antigravity` | `antigravity:api` | `Monitorable` | Cloud quota API (macOS) + local language server API + local cache | Provider facade owns `cloud -> live -> cache` orchestration; `antigravity/cloud_source.rs` reads the agy CLI Keychain token (read-only) and calls the Google quota summary API with 429 cooldown, kept provider-local on top of shared `codeium_family/` primitives |
| `windsurf/` | Devin Desktop | `windsurf` | `windsurf:api` | `Monitorable` | Seat API + local language server API + local cache | Provider facade (`windsurf/mod.rs`) owns `seat -> live -> cache` orchestration; `windsurf/seat_source.rs` keeps the seat API provider-local |
| `minimax/` | MiniMax | `minimax` | `minimax:api` | `Monitorable` | HTTP API | Split into `auth.rs`, `client.rs`, `parser.rs` |
| `kiro.rs` | Kiro | `kiro` | `kiro:cli` | `Monitorable` | CLI | Uses `common::cli`; keeps stderr/stdout merge logic provider-local |
| `kilo.rs` | Kilo | `kilo` | `kilo:ext` | `Placeholder` | Extension detection | Discoverable entry only; no normal refresh |
| `opencode/` | OpenCode Go | `opencode` | `opencode:api` | `Monitorable` | OpenCode Go usage API | Display name is OpenCode Go; stable settings key remains `opencode`. Reads `auth.json` (`opencode-go` / `opencode`); maps rolling / weekly / monthly used percent |
| `vertex_ai.rs` | Vertex AI | `vertexai` | `vertexai:gcloud` | `Informational` | Gemini CLI config detection | Reference-only entry for Gemini Vertex AI auth mode |
| `grok/` | Grok | `grok` | `grok:api` | `Monitorable` | Grok Build billing API | Reads `~/.grok/auth.json`; SuperGrok / subscription weekly pool via `cli-chat-proxy` `?format=credits`. See `grok/README.md` |

## Design Notes

- Provider layer returns structured facts; it does not format UI strings.
- Provider 应返回稳定语义而不是最终展示文案：
  - quota 标题用 `QuotaLabelSpec`
  - quota 第四行详情用 `QuotaDetailSpec`
  - 错误用 `ProviderError` / `ProviderFailure`
- `ProviderError::to_failure()` 负责把 provider 错误降为可持久化的失败语义；`ProviderError::error_kind()` 给刷新状态分类；`application/selectors/format.rs` 负责最终 i18n 文案。
- 语言切换不应触发 provider refresh；selector 基于最新 locale 即时重算展示字符串。
- When a provider already knows the user-facing remediation, return a structured `ProviderError`
  directly and keep technical diagnostics in logs instead of `anyhow::Context`.
- `AiProvider` implementations return `ProviderResult<T>`. Provider-owned source/parser
  boundaries should also prefer `ProviderResult<T>` once they encode domain semantics
  (for example Claude `UsageProbe` and Codeium-family `ParseStrategy`).
- Low-level transport clients may still use `anyhow::Result` when callers need to inspect
  raw technical errors such as `HttpError`; classify them before returning from provider
  facade/source boundaries.
- Shared HTTP transport failures should surface as `common::http_client::HttpError`; provider code
  upgrades them to `ProviderError` only when it knows a clearer remediation.
- Provider CLI/HTTP raw output may contain account identifiers or secrets. Do not log raw bodies
  unless they have an explicit sanitizer; log byte counts and stable parse diagnostics instead.
- Multi-file providers should split along stable responsibilities first: `auth`, `client/source`, `parser`, `mod`.
- Only introduce extra traits when there are real multiple implementations (for example Claude probe strategies).
- `Claude::UsageProbe` and Codeium-family `ParseStrategy` are intentionally separate:
  - `UsageProbe` selects a data source (`CLI` vs `API`)
  - `ParseStrategy` decodes different payload formats from the same domain data
  - Share the fallback pattern conceptually, not via a forced common trait
- `Claude` uses explicit source orchestration in `mod.rs`:
  - `check_availability(ctx)` accepts either API or CLI source
  - `ProbeMode::Auto` prefers API and falls back to CLI
  - concrete source logic stays in `api_probe.rs` / `cli_probe.rs`
- Codeium-family providers keep orchestration in the provider facade instead of in the shared module:
  - `codeium_family/live_source.rs` handles process discovery + local API transport
  - `codeium_family/cache_source.rs` handles SQLite/local cache fallback
  - `codeium_family/quota_semantics.rs` holds the pure Devin weekly exhaustion rule shared by seat/cache parsing; orchestration remains provider-owned
  - `antigravity/mod.rs` owns `cloud -> live -> cache`; `antigravity/cloud_source.rs` contains the Antigravity-only cloud source
  - `windsurf/mod.rs` owns `seat -> live -> cache`
  - `windsurf/seat_source.rs` contains the Devin Desktop-only cloud source

## Adding a New Provider

1. **Add manifest entry** in `src/builtin_provider_manifest.rs`: `MyProviderKind => "myprovider" => my_provider::MyProvider`
2. **Create provider file or directory** matching the manifest module path:
   ```rust
   use super::{define_unit_provider, AiProvider, ProviderCapabilities, ProviderExecutionContext, ProviderResult};
   use crate::models::*;

   define_unit_provider!(MyProvider);

   #[async_trait::async_trait]
   impl AiProvider for MyProvider {
       fn descriptor(&self) -> ProviderDescriptor { /* ... */ }
       async fn check_availability(&self, ctx: &ProviderExecutionContext<'_>) -> ProviderResult<()> { Ok(()) }
       async fn refresh(&self, ctx: &ProviderExecutionContext<'_>) -> ProviderResult<RefreshData> { /* ... */ }
   }

   impl ProviderCapabilities for MyProvider {
       fn settings_capability(&self) -> SettingsCapability { SettingsCapability::None }
   }
   ```
3. **Capability first**: if the entry is not truly monitorable, override `provider_capability()` in `ProviderCapabilities` and omit `refresh(ctx)` instead of relying on repeated `Unavailable` refreshes as product semantics
4. **Optional interactive settings**: return `SettingsCapability::TokenInput(TokenInputCapability { .. })` and choose a stable `credential_key`
5. **Add icon**: `src/icons/provider-myprovider.svg`
6. **Test**: `cargo test --lib` — `test_all_provider_kinds_have_implementation` catches manifest/implementation mismatches

### 单文件 vs 多文件 Provider — 升级阈值

新 provider **默认应该从单文件 `my_provider.rs` 开始**（如 `kiro.rs` / `kilo.rs` / `vertex_ai.rs`）。强行套用 `auth.rs / client.rs / parser.rs / mod.rs` 的多文件骨架在小 provider 上只会制造无意义的跳转成本。

当 provider 出现 **以下任意一条** 时，再升级到多文件结构：

- 单文件超过 ~250 行，并且能按"认证 / 请求 / 解析"自然切分
- 有 **多个数据源 / 多个 fallback 路径**（如 Claude 的 API + CLI、Codex 的 HTTP + JSON-RPC + PTY、Codeium-family 的 live + cache）
- 有独立的 **认证流程**（如 OAuth token refresh、JWT 解析、本地 SQLite token 读取）需要独立测试边界
- 同一个 provider 对应 **多种 payload 解析策略**，并且策略本身需要按 trait 实现（参考 Claude `UsageProbe` / Codeium-family `ParseStrategy`）

升级时按以下顺序拆分，**不要一次拆光**：

1. 先拆 `auth.rs`（凭证读取 / token 解码 / 多源 fallback）
2. 再拆 `client.rs` 或 `*_source.rs`（HTTP / CLI / 本地缓存的具体源）
3. 最后拆 `parser.rs`（payload 反序列化 + `QuotaInfo` 组装）
4. `mod.rs` 只保留 `AiProvider` 刷新 impl、`ProviderCapabilities` impl 和源编排（"先 API 后 CLI"这类业务规则属于这里）

反模式：

- **不要** 为只有一处使用的 helper 单独建文件 — 留在 `mod.rs` 内
- **不要** 在 provider 模块内复制 `common/http_client` / `common/cli` / `common/runner` / `common/config_paths` 已有的能力
- **不要** 把 i18n 文案下沉到 provider 层；provider 只返回 `ProviderError` / `QuotaLabelSpec` / `QuotaDetailSpec`

## Adding a New `ProviderError` Variant

`ProviderError` 是 closed enum（不接受 `Custom(String)` 兜底），新增一个变体属于"语义级改动"，需要把语义在多个层级保持对齐：

1. **加变体**：`src/providers/error.rs` 中添加 variant，并实现 `Display` 分支
2. **降级到稳定语义**：
   - `ProviderError::to_failure()` — 映射到 `ProviderFailure { reason, advice }`，让 reducer / selector 在不感知具体错误的情况下展示
   - `ProviderError::error_kind()` — 映射到 `ErrorKind`，让 refresh 调度器分类（决定是否计入连续失败、是否触发 retry 提示等）
3. **HTTP 升级路径**：如果新错误可能来自 transport 层，更新 `ProviderError::classify()`，把对应的 `HttpError::HttpStatus { code, .. }` 升级到这个变体
4. **i18n 文案**：在 `locales/<lang>.yml` 中**每一种语言**都加上对应的 `provider.failure.<key>.*` 文案；`src/i18n.rs` 的测试会捕获缺失
5. **selector 格式化**：`src/application/selectors/format.rs::format_failure_message` 增加分支（如有专属 advice，也要在 `format_failure_advice` 处理）
6. **避免**：
   - 不要在 `anyhow::Context` 里写用户可见 remediation —— 那些只会留在日志里，不会进入 `ProviderFailure`
   - 不要在 provider 层硬编码语言相关文案 —— 用 `ProviderError` 表达语义，让 selector 决定文案
   - 不要把"特定 provider 才出现"的错误塞进通用变体 —— 如果只有一两个 provider 触发，保留为 `Unavailable { message }` 或 `FetchFailed { message }` 内的结构化信息更合适
7. **测试**：在 `src/providers/error_tests.rs` 中补 `error_kind()` / `to_failure()` 的映射用例，以及 `classify()` 对相关 `HttpError` 的分类用例

## Constraints

- Providers run on background threads (via `smol::unblock`). They must be `Send + Sync`.
- HTTP requests should use `crate::providers::common::http_client` (shared ureq agent).
- Non-interactive CLI providers should use `crate::providers::common::cli`; it shares command lookup and PATH enrichment with the PTY runner through `common::path_resolver`.
- CLI-based providers should use `crate::providers::common::runner::InteractiveRunner` for PTY-based execution when interactive behavior is required.
- Return `ProviderError` variants (not raw strings) for structured classification.
- Do not hide user remediation inside `anyhow::Context`; reserve context for technical details
  that stay in logs/debugging paths.
- The `descriptor().metadata.kind` must match the `ProviderKind` variant — `ProviderManager::register()` asserts this.
- Do not persist built-in settings under `descriptor().id`; use `ProviderId::BuiltIn(kind).id_key()` / `ProviderKind::id_key()` for settings, ordering, sidebar, refresh requests, and hidden quota state.
