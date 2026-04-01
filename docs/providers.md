# Providers

## Supported Providers (12)

| Provider | ID | Data Source | Implementation |
|----------|-----|-----------|----------------|
| Claude | `claude` | HTTP API + CLI fallback | `providers/claude/` (multi-file) |
| Gemini | `gemini:api` | HTTP API | `providers/gemini.rs` |
| Copilot | `copilot:api` | HTTP API (GitHub) | `providers/copilot/` |
| Codex | `codex:api` | HTTP API (ChatGPT) | `providers/codex.rs` |
| Kimi | `kimi:api` | HTTP API | `providers/kimi.rs` |
| Amp | `amp:cli` | CLI (`amp usage`) | `providers/amp.rs` |
| Cursor | `cursor:api` | HTTP API + local SQLite | `providers/cursor.rs` |
| Antigravity | `antigravity:api` | Local language server API | `providers/antigravity.rs` ([API 文档](antigravity-api.md)) |
| MiniMax | `minimax:api` | HTTP API | `providers/minimax.rs` |
| Kiro | `kiro:cli` | CLI (interactive PTY) | `providers/kiro.rs` |
| Kilo | `kilo:ext` | — (placeholder) | `providers/kilo.rs` |
| OpenCode | `opencode:cli` | — (placeholder) | `providers/opencode.rs` |
| Vertex AI | `vertexai:gcloud` | — (placeholder) | `providers/vertex_ai.rs` |

## AiProvider Trait

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn metadata(&self) -> ProviderMetadata;
    fn id(&self) -> &'static str;
    fn kind(&self) -> ProviderKind { self.metadata().kind }
    async fn is_available(&self) -> bool { true }
    async fn refresh(&self) -> Result<RefreshData>;
    async fn refresh_quotas(&self) -> Result<Vec<QuotaInfo>>;
}
```

Providers run on background threads via `smol::unblock`. They must be `Send + Sync`.

## Adding a New Provider

1. Add `ProviderKind` variant in `src/models/provider.rs` (`define_provider_kind!` macro)
2. Add `id_key()` and `from_id_key()` match arms in the same file
3. Create `src/providers/my_provider.rs` (use `define_unit_provider!(MyProvider)` for zero-field providers)
4. Implement `AiProvider` for `MyProvider`
5. Add icon: `src/icons/provider-myprovider.svg`
6. Register in `src/providers/mod.rs`: add to `register_providers!` macro
7. `cargo test --lib` — `test_all_provider_kinds_have_implementation` catches missing registrations
