# Providers

## Supported Providers (14 built-in + custom)

| Provider | ID | Data Source | Implementation |
|----------|-----|-----------|----------------|
| Claude | `claude` | HTTP API + CLI fallback | `providers/claude/` (multi-file) |
| Gemini | `gemini:api` | HTTP API | `providers/gemini/` |
| Copilot | `copilot:api` | HTTP API (GitHub) | `providers/copilot/` |
| Codex | `codex:api` | HTTP API (ChatGPT) | `providers/codex/` |
| Kimi | `kimi:api` | HTTP API | `providers/kimi/` |
| Amp | `amp:cli` | CLI (`amp usage`) | `providers/amp.rs` |
| Cursor | `cursor:api` | HTTP API + local SQLite | `providers/cursor/` |
| Antigravity | `antigravity:api` | Local language server API + local cache | `providers/antigravity/` + `providers/codeium_family/` ([Codeium-family 架构文档](antigravity-api.md)) |
| Windsurf | `windsurf:api` | Local language server API + local cache | `providers/windsurf.rs` + `providers/codeium_family/` ([Codeium-family 架构文档](antigravity-api.md)) |
| MiniMax | `minimax:api` | HTTP API | `providers/minimax/` |
| Kiro | `kiro:cli` | CLI (`kiro-cli chat --no-interactive /usage`) | `providers/kiro.rs` |
| Kilo | `kilo:ext` | — (placeholder) | `providers/kilo.rs` |
| OpenCode | `opencode:cli` | — (placeholder) | `providers/opencode.rs` |
| Vertex AI | `vertexai:gcloud` | — (placeholder) | `providers/vertex_ai.rs` |

## Custom Provider（YAML 声明式）

除内置 Provider 外，用户可以通过 YAML 文件声明自定义 Provider，无需编写代码。

支持的数据获取方式：CLI 命令 / HTTP GET / HTTP POST
支持的认证方式：Bearer Token / 自定义 Header / Login（用户名密码）/ Cookie / Session Token
支持的解析方式：JSON 点分路径 / 正则表达式 capture group

| 场景示例 | 模板文件 |
|----------|----------|
| NewAPI / OneAPI 中转站 | `docs/examples/custom-provider-newapi.yaml` |
| HTTP API（POST） | `docs/examples/custom-provider-http.yaml` |
| CLI 命令行工具 | `docs/examples/custom-provider-cli.yaml` |

详细使用说明：[自定义 Provider 使用指南](custom-provider.md)

## AiProvider Trait

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;
    async fn check_availability(&self) -> Result<()> { Ok(()) }
    async fn refresh(&self) -> Result<RefreshData>;
}
```

### Supporting Types

- `ProviderDescriptor` — 收敛 provider 的注册 ID 与展示元数据
- `ProviderError` — provider 层返回的结构化错误
- `ProviderErrorPresenter` — 将 `ProviderError` 映射为 UI 文案和 `ErrorKind`
- [Provider Refactor Retrospective](provider-refactor-retrospective.md) — 本次 provider 重构的根因、决策过程与优化方向
- `providers/common/cli.rs` — CLI provider 共享的可用性检查、命令执行与退出码处理
- [Provider Blueprints](provider-blueprints.md) — 后续新增/重构 provider 的复用蓝图

Providers run on background threads via `smol::unblock`. They must be `Send + Sync`.

## Abstraction Boundary

- `Claude` 的 `UsageProbe` 解决“从 CLI 还是 API 取数”
- `Antigravity` 的 `ParseStrategy` 解决“API JSON 和本地缓存 protobuf 如何解析”
- 两者都体现了 fallback / strategy 思想，但抽象层级不同，不应硬统一成单一 trait
- `Claude` 现在采用显式 source 编排：`mod.rs` 只负责 mode + fallback，source 细节留在 `api_probe.rs` / `cli_probe.rs`

## Adding a New Provider

1. Add `ProviderKind` variant in `src/models/provider.rs` (`define_provider_kind!` macro)
2. Add `id_key()` and `from_id_key()` match arms in the same file
3. Create `src/providers/my_provider.rs` (or `src/providers/my_provider/` for multi-file providers)
4. Implement `AiProvider` for `MyProvider`
5. Add icon: `src/icons/provider-myprovider.svg`
6. Register in `src/providers/mod.rs`: add to `register_providers!` macro
7. `cargo test --lib` — `test_all_provider_kinds_have_implementation` catches missing registrations
