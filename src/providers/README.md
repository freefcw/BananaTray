# src/providers/

Provider abstraction layer and all 14 AI provider implementations.

## Core Abstractions

### `mod.rs` — Trait + Registry

- **`AiProvider`** trait (async_trait) — core interface every provider must implement:
  - `descriptor() -> ProviderDescriptor` — provider ID + `ProviderMetadata`
  - `check_availability() -> Result<()>` — environment/config check with structured error
  - `refresh() -> Result<RefreshData>` — fetch latest quota data
- **`ProviderDescriptor`** — static description for registration and UI metadata
- **`ProviderError`** — structured error enum with variants: `CliNotFound`, `Unavailable`, `AuthRequired`, `SessionExpired`, `FolderTrustRequired`, `UpdateRequired`, `ParseFailed`, `Timeout`, `NoData`, `NetworkFailed`, `ConfigMissing`, `FetchFailed`
- **`ProviderErrorPresenter`** — maps `ProviderError` to UI message and `ErrorKind`
- **`common/`** — cross-provider helpers shared by multiple implementations (for example JWT decoding, CLI execution helpers)
- **`codeium_family/`** — shared live/cache/parser/spec logic for Antigravity and Windsurf
- **`docs/provider-refactor-retrospective.md`** — why the provider layer was refactored this way, including rejected abstractions
- **`register_providers!`** macro — declares provider modules and generates `register_all()` function
- **`define_unit_provider!`** macro — boilerplate for zero-field provider structs

### `manager.rs` — ProviderManager

Aggregation registry holding all provider implementations:
- `register()` — adds a provider (deduplicates by id and kind)
- `metadata_for(kind)` — returns metadata with fallback
- `initial_statuses()` — generates `Vec<ProviderStatus>` for all `ProviderKind` variants
- `refresh_provider(kind)` — checks availability then delegates to the provider's `refresh()`

## Provider Implementations

| File | Provider | ID | Data Source | Notes |
|------|----------|-----|-----------|-------|
| `claude/` | Claude | `claude` | HTTP API + CLI fallback | `mod.rs` orchestrates source selection; `api_probe.rs` / `cli_probe.rs` implement sources |
| `gemini/` | Gemini | `gemini:api` | HTTP API | Split into `auth.rs`, `client.rs`, `parser.rs`, `mod.rs` |
| `copilot/` | Copilot | `copilot:api` | GitHub API | Split into `token.rs`, `client.rs`, `parser.rs`; `settings_ui.rs` provides custom settings UI |
| `codex/` | Codex | `codex:api` | ChatGPT API | Split into `auth.rs`, `client.rs`, `parser.rs`, `mod.rs` |
| `kimi/` | Kimi | `kimi:api` | HTTP API | Split into `auth.rs`, `client.rs`, `parser.rs` |
| `amp.rs` | Amp | `amp:cli` | CLI output | Uses `common::cli` for availability and exit-code handling |
| `cursor/` | Cursor | `cursor:api` | HTTP API | Split into `auth.rs`, `client.rs`, `parser.rs`; reads token from local SQLite (`state.vscdb`) |
| `antigravity/` | Antigravity | `antigravity:api` | Local language server API | Thin facade over shared `codeium_family/` module |
| `windsurf.rs` | Windsurf | `windsurf:api` | Local language server API + local cache | Uses shared `codeium_family/` module |
| `minimax/` | MiniMax | `minimax:api` | HTTP API | Split into `auth.rs`, `client.rs`, `parser.rs` |
| `kiro.rs` | Kiro | `kiro:cli` | CLI | Uses `common::cli`; keeps stderr/stdout merge logic provider-local |
| `kilo.rs` | Kilo | `kilo:ext` | — | Placeholder (returns `Unavailable`) |
| `opencode.rs` | OpenCode | `opencode:cli` | — | Placeholder (returns `Unavailable`) |
| `vertex_ai.rs` | Vertex AI | `vertexai:gcloud` | — | Placeholder (redirects to Gemini) |

## Design Notes

- Provider layer returns structured facts; it does not format UI strings.
- Error presentation belongs to `src/providers/error_presenter.rs`.
- Multi-file providers should split along stable responsibilities first: `auth`, `client/source`, `parser`, `mod`.
- Only introduce extra traits when there are real multiple implementations (for example Claude probe strategies).
- `Claude::UsageProbe` and `Antigravity::ParseStrategy` are intentionally separate:
  - `UsageProbe` selects a data source (`CLI` vs `API`)
  - `ParseStrategy` decodes different payload formats from the same domain data
  - Share the fallback pattern conceptually, not via a forced common trait
- `Claude` uses explicit source orchestration in `mod.rs`:
  - `check_availability()` accepts either API or CLI source
  - `ProbeMode::Auto` prefers API and falls back to CLI
  - concrete source logic stays in `api_probe.rs` / `cli_probe.rs`
- `Antigravity` uses a dedicated source fallback blueprint:
  - `live_source.rs` handles process discovery + local API transport
  - `cache_source.rs` handles SQLite/local cache fallback
  - `mod.rs` only orchestrates fallback order

## Adding a New Provider

1. **Add `ProviderKind` variant** in `src/models/provider.rs` (`define_provider_kind!` macro + `id_key()` + `from_id_key()`)
2. **Create provider file or directory**:
   ```rust
   use super::{define_unit_provider, AiProvider};
   use crate::models::*;

   define_unit_provider!(MyProvider);

   #[async_trait::async_trait]
   impl AiProvider for MyProvider {
       fn descriptor(&self) -> ProviderDescriptor { /* ... */ }
       async fn check_availability(&self) -> anyhow::Result<()> { Ok(()) }
       async fn refresh(&self) -> anyhow::Result<RefreshData> { /* ... */ }
   }
   ```
3. **Add icon**: `src/icons/provider-myprovider.svg`
4. **Register**: add `my_provider => MyProvider` to `register_providers!` macro in `mod.rs`
5. **Test**: `cargo test --lib` — `test_all_provider_kinds_have_implementation` catches missing registrations

## Constraints

- Providers run on background threads (via `smol::unblock`). They must be `Send + Sync`.
- HTTP requests should use `crate::providers::common::http_client` (shared ureq agent).
- CLI-based providers should use `crate::providers::common::runner::InteractiveRunner` for PTY-based execution when interactive behavior is required.
- Return `ProviderError` variants (not raw strings) for structured classification.
- The `descriptor().metadata.kind` must match the `ProviderKind` variant — `ProviderManager::register()` asserts this.
