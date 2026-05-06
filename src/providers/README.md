# src/providers/

Provider abstraction layer and all 14 AI provider implementations.

## Core Abstractions

### `mod.rs` — Trait + Registry

- **`AiProvider`** trait (async_trait) — core interface every provider exposes:
  - `descriptor() -> ProviderDescriptor` — provider ID + `ProviderMetadata`
  - `check_availability() -> ProviderResult<()>` — environment/config check with structured error
  - `refresh() -> ProviderResult<RefreshData>` — fetch latest quota data; defaults to `NoData`, so `Monitorable` providers must override it while `Informational` / `Placeholder` entries normally do not
  - `settings_capability() -> SettingsCapability` — declare settings UI capability (default: `None`)
  - `provider_capability() -> ProviderCapability` — declare whether the provider is `Monitorable`, `Informational`, or `Placeholder`
  - `sync_provider_credentials(credentials)` — optional runtime sync hook for BananaTray-managed provider credentials
- **`SettingsCapability`** — provider-defined settings capability:
  - `None` — no extra settings UI
  - `TokenInput(TokenInputCapability)` — generic token panel driven by static i18n keys + `credential_key`
  - `NewApiEditable` — NewAPI custom-provider editor actions
- **`ProviderCapability`** — provider product capability tier:
  - `Monitorable` — participates in normal refresh flows
  - `Informational` — reference-only entry, no refresh/retry actions
  - `Placeholder` — discoverable but not directly monitorable, no refresh/retry actions
- **`TokenInputCapability`** — token settings contract:
  - static UI metadata (`title_i18n_key`, `description_i18n_key`, `placeholder_i18n_key`, `create_url`)
  - every `*_i18n_key` and token source key must exist in all files under `locales/`; `src/i18n.rs` tests enforce this for literal and provider-declared keys
  - `credential_key` for persisted storage in `ProviderConfig::credentials`
  - only for BananaTray-managed token overrides; providers may still resolve auth from external files, CLI sessions, or env vars
- **`resolve_token_input_state()`** — optional `AiProvider` hook for provider-side runtime token display state (masked value / source / edit mode); override only when default credential-store behavior is insufficient
- **`sync_provider_credentials()`** — optional `AiProvider` hook used by the background refresh runtime. `RefreshRequest::UpdateConfig` carries the latest `ProviderConfig::credentials`; `RefreshCoordinator` syncs them into `ProviderManager` before refresh and after provider reload. TokenInput providers whose refresh path uses app-managed overrides must store a thread-safe snapshot here.
- **`ProviderDescriptor`** — static description for registration and UI metadata. For built-in providers, `descriptor().id` is a registration/dedup/source descriptor and may include suffixes such as `codex:api`; settings/state routing uses `ProviderId::BuiltIn(kind)` and `ProviderKind::id_key()` instead. For custom providers, the YAML `id` is persisted as `ProviderId::Custom`.
- **`ProviderError`** — structured error enum with variants: `CliNotFound`, `Unavailable`, `AuthRequired`, `SessionExpired`, `FolderTrustRequired`, `UpdateRequired`, `ParseFailed`, `Timeout`, `NoData`, `NetworkFailed`, `ConfigMissing`, `FetchFailed`
- **`ProviderResult<T>`** — provider boundary result type (`Result<T, ProviderError>`) used by `AiProvider` and `ProviderManager`
- **`ProviderError::to_failure()` / `error_kind()`** — maps provider errors to stable `ProviderFailure` and `ErrorKind`; final locale-specific message generation belongs to selector/UI
- **`common/`** — crate-internal cross-provider helpers shared by multiple implementations (for example JWT decoding, CLI execution helpers)
- **`codeium_family/`** — crate-internal shared local-source/spec/parser primitives for Antigravity and Windsurf; provider-specific orchestration stays in each facade
- **`docs/archeive/provider/provider-refactor-retrospective.md`** — why the provider layer was refactored this way, including rejected abstractions
- **`src/builtin_provider_manifest.rs`** — single compile-time manifest for built-in providers; feeds both `ProviderKind` generation and built-in registration
- **`register_providers!`** macro — consumes the manifest to declare private built-in provider modules and generate crate-internal `register_all()` function
- **`define_unit_provider!`** macro — boilerplate for zero-field provider structs

### `manager.rs` — ProviderManager

Aggregation registry holding all provider implementations. Maintains exactly two indexes matching `ProviderId`'s two variants: `providers_by_kind` (built-in) and `custom_providers_by_id` (custom).

- `register()` — adds a provider (deduplicates by id and kind)
- `provider_for_id(id)` — unified lookup by `ProviderId`
- `metadata_for(kind)` — returns metadata (derived from provider) with fallback
- `initial_statuses()` — generates `Vec<ProviderStatus>` for all `ProviderKind` variants
- `initial_statuses()` also copies each provider's `settings_capability()` and `provider_capability()` into runtime `ProviderStatus`
- `refresh_by_id(id)` — routes built-in and custom providers through one refresh entrypoint; non-monitorable providers return `NoData`, monitorable providers check availability and then delegate to `refresh()`
- `sync_provider_credentials(credentials)` — fans out app-managed credentials to registered providers that need runtime credential snapshots
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
| `claude/` | Claude | `claude` | `claude` | `Monitorable` | HTTP API + CLI fallback | `mod.rs` orchestrates source selection; `api_probe.rs` / `cli_probe.rs` implement sources |
| `gemini/` | Gemini | `gemini` | `gemini:api` | `Monitorable` | HTTP API | Split into `auth.rs`, `client.rs`, `parser.rs`, `mod.rs` |
| `copilot/` | Copilot | `copilot` | `copilot:api` | `Monitorable` | GitHub API | Split into `token.rs`, `client.rs`, `parser.rs`; declares `SettingsCapability::TokenInput(TokenInputCapability)`, provides a custom multi-source token resolver, and syncs `github_token` into a runtime snapshot for refresh |
| `codex/` | Codex | `codex` | `codex:api` | `Monitorable` | ChatGPT API + CLI fallback | Split into `auth.rs`, `client.rs`, `parser.rs`, `rpc_probe.rs`, `status_probe.rs`, `mod.rs`. `refresh()` uses HTTP first; recoverable HTTP failures fall back to `codex app-server` JSON-RPC before PTY `/status`. `auth.rs` decodes the OAuth `id_token` JWT for email / plan / `chatgpt_account_id`; credentials are reloaded after token rotation so the `ChatGPT-Account-Id` header and `RefreshData.account_*` reflect the latest state |
| `kimi/` | Kimi | `kimi` | `kimi:api` | `Monitorable` | HTTP API | Split into `auth.rs`, `client.rs`, `parser.rs` |
| `amp.rs` | Amp | `amp` | `amp:cli` | `Monitorable` | CLI output | Uses `common::cli` for availability and exit-code handling |
| `cursor/` | Cursor | `cursor` | `cursor:api` | `Monitorable` | HTTP API | Split into `auth.rs`, `client.rs`, `parser.rs`; reads token from local SQLite (`state.vscdb`) |
| `antigravity/` | Antigravity | `antigravity` | `antigravity:api` | `Monitorable` | Local language server API + local cache | Provider facade owns `live -> cache` orchestration on top of shared `codeium_family/` primitives |
| `windsurf.rs` | Windsurf | `windsurf` | `windsurf:api` | `Monitorable` | Seat API + local language server API + local cache | Provider facade owns `seat -> live -> cache` orchestration; `windsurf/seat_source.rs` keeps the seat API provider-local |
| `minimax/` | MiniMax | `minimax` | `minimax:api` | `Monitorable` | HTTP API | Split into `auth.rs`, `client.rs`, `parser.rs` |
| `kiro.rs` | Kiro | `kiro` | `kiro:cli` | `Monitorable` | CLI | Uses `common::cli`; keeps stderr/stdout merge logic provider-local |
| `kilo.rs` | Kilo | `kilo` | `kilo:ext` | `Placeholder` | Extension detection | Discoverable entry only; no normal refresh |
| `opencode.rs` | OpenCode | `opencode` | `opencode:cli` | `Placeholder` | CLI detection | Discoverable entry only; no normal refresh |
| `vertex_ai.rs` | Vertex AI | `vertexai` | `vertexai:gcloud` | `Informational` | Gemini CLI config detection | Reference-only entry for Gemini Vertex AI auth mode |

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
- Multi-file providers should split along stable responsibilities first: `auth`, `client/source`, `parser`, `mod`.
- Only introduce extra traits when there are real multiple implementations (for example Claude probe strategies).
- `Claude::UsageProbe` and Codeium-family `ParseStrategy` are intentionally separate:
  - `UsageProbe` selects a data source (`CLI` vs `API`)
  - `ParseStrategy` decodes different payload formats from the same domain data
  - Share the fallback pattern conceptually, not via a forced common trait
- `Claude` uses explicit source orchestration in `mod.rs`:
  - `check_availability()` accepts either API or CLI source
  - `ProbeMode::Auto` prefers API and falls back to CLI
  - concrete source logic stays in `api_probe.rs` / `cli_probe.rs`
- Codeium-family providers keep orchestration in the provider facade instead of in the shared module:
  - `codeium_family/live_source.rs` handles process discovery + local API transport
  - `codeium_family/cache_source.rs` handles SQLite/local cache fallback
  - `antigravity/mod.rs` owns `live -> cache`
  - `windsurf.rs` owns `seat -> live -> cache`
  - `windsurf/seat_source.rs` contains the Windsurf-only cloud source

## Adding a New Provider

1. **Add manifest entry** in `src/builtin_provider_manifest.rs`: `MyProviderKind => "myprovider" => my_provider::MyProvider`
2. **Create provider file or directory** matching the manifest module path:
   ```rust
   use super::{define_unit_provider, AiProvider, ProviderResult};
   use crate::models::*;

   define_unit_provider!(MyProvider);

   #[async_trait::async_trait]
   impl AiProvider for MyProvider {
       fn descriptor(&self) -> ProviderDescriptor { /* ... */ }
       async fn check_availability(&self) -> ProviderResult<()> { Ok(()) }
       async fn refresh(&self) -> ProviderResult<RefreshData> { /* ... */ }
       fn settings_capability(&self) -> SettingsCapability { SettingsCapability::None }
   }
   ```
3. **Capability first**: if the entry is not truly monitorable, override `provider_capability()` and omit `refresh()` instead of relying on repeated `Unavailable` refreshes as product semantics
4. **Optional interactive settings**: return `SettingsCapability::TokenInput(TokenInputCapability { .. })` and choose a stable `credential_key`
5. **Add icon**: `src/icons/provider-myprovider.svg`
6. **Test**: `cargo test --lib` — `test_all_provider_kinds_have_implementation` catches manifest/implementation mismatches

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
