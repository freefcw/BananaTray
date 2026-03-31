# src/providers/

Provider abstraction layer and all 12 AI provider implementations.

## Core Abstractions

### `mod.rs` — Trait + Registry

- **`AiProvider`** trait (async_trait) — core interface every provider must implement:
  - `metadata() -> ProviderMetadata` — display name, icon, dashboard URL, etc.
  - `id() -> &'static str` — unique implementation identifier (e.g. `"claude"`, `"copilot:api"`)
  - `kind() -> ProviderKind` — derived from metadata (default impl)
  - `is_available() -> bool` — environment check (e.g. CLI installed?)
  - `refresh() -> Result<RefreshData>` — fetch latest quota data (primary method)
  - `refresh_quotas() -> Result<Vec<QuotaInfo>>` — simpler variant for backward compat
- **`ProviderError`** — structured error enum with variants: `CliNotFound`, `Unavailable`, `AuthRequired`, `SessionExpired`, `FolderTrustRequired`, `UpdateRequired`, `ParseFailed`, `Timeout`, `NoData`, `NetworkFailed`, `ConfigMissing`, `FetchFailed`. Includes `classify(anyhow::Error)` for error downcast.
- **`register_providers!`** macro — declares provider modules and generates `register_all()` function
- **`define_unit_provider!`** macro — boilerplate for zero-field provider structs

### `manager.rs` — ProviderManager

Aggregation registry holding all provider implementations:
- `register()` — adds a provider (deduplicates by id and kind)
- `metadata_for(kind)` — returns metadata with fallback
- `initial_statuses()` — generates `Vec<ProviderStatus>` for all `ProviderKind` variants
- `refresh_provider(kind)` — delegates to the appropriate provider's `refresh()`

## Provider Implementations

| File | Provider | ID | Data Source | Notes |
|------|----------|-----|-----------|-------|
| `claude/` | Claude | `claude` | HTTP API + CLI fallback | Multi-file: `api_probe.rs` (HTTP), `cli_probe.rs` (PTY), `credentials.rs` (token), `probe.rs` (trait) |
| `gemini.rs` | Gemini | `gemini:api` | HTTP API | Token refresh via `gemini` CLI |
| `copilot/` | Copilot | `copilot:api` | GitHub API | `settings_ui.rs` provides custom settings UI for token input |
| `codex.rs` | Codex | `codex:api` | ChatGPT API | Reads local auth token file |
| `kimi.rs` | Kimi | `kimi:api` | HTTP API | Auth token from browser cookies/config |
| `amp.rs` | Amp | `amp:cli` | CLI output | Parses `amp usage --no-color` |
| `cursor.rs` | Cursor | `cursor:api` | HTTP API | Reads access token from local SQLite (`state.vscdb`) |
| `minimax.rs` | MiniMax | `minimax:api` | HTTP API | Supports both CN and international endpoints |
| `kiro.rs` | Kiro | `kiro:cli` | CLI (interactive PTY) | Sends `/usage` + `/quit` via PTY |
| `kilo.rs` | Kilo | `kilo:ext` | — | Placeholder (returns `Unavailable`) |
| `opencode.rs` | OpenCode | `opencode:cli` | — | Placeholder (returns `Unavailable`) |
| `vertex_ai.rs` | Vertex AI | `vertexai:gcloud` | — | Placeholder (redirects to Gemini) |

## Adding a New Provider

1. **Add `ProviderKind` variant** in `src/models/provider.rs` (`define_provider_kind!` macro + `id_key()` + `from_id_key()`)
2. **Create provider file**: `src/providers/my_provider.rs`
   ```rust
   use super::{define_unit_provider, AiProvider};
   use crate::models::*;

   define_unit_provider!(MyProvider);

   #[async_trait::async_trait]
   impl AiProvider for MyProvider {
       fn metadata(&self) -> ProviderMetadata { /* ... */ }
       fn id(&self) -> &'static str { "myprovider:api" }
       async fn refresh(&self) -> anyhow::Result<RefreshData> { /* ... */ }
   }
   ```
3. **Add icon**: `src/icons/provider-myprovider.svg`
4. **Register**: add `my_provider => MyProvider` to `register_providers!` macro in `mod.rs`
5. **Test**: `cargo test --lib` — `test_all_provider_kinds_have_implementation` catches missing registrations

## Constraints

- Providers run on background threads (via `smol::unblock`). They must be `Send + Sync`.
- HTTP requests should use `crate::utils::http_client` (shared ureq agent).
- CLI-based providers should use `crate::utils::interactive_runner::InteractiveRunner` for PTY-based execution.
- Return `ProviderError` variants (not raw anyhow strings) for structured error display.
- The `metadata().kind` must match the `ProviderKind` variant — `ProviderManager::register()` asserts this.
