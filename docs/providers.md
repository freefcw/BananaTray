# Providers

## Supported Providers (14 built-in + custom)

| Provider | `id_key` | Data Source | Implementation |
|----------|----------|-------------|----------------|
| Claude | `claude` | HTTP API + CLI fallback | `providers/claude/` (multi-file) |
| Gemini | `gemini` | HTTP API | `providers/gemini/` |
| Copilot | `copilot` | HTTP API (GitHub) | `providers/copilot/` |
| Codex | `codex` | HTTP API (ChatGPT) | `providers/codex/` |
| Kimi | `kimi` | HTTP API | `providers/kimi/` |
| Amp | `amp` | CLI (`amp usage`) | `providers/amp.rs` |
| Cursor | `cursor` | HTTP API + local SQLite | `providers/cursor/` |
| Antigravity | `antigravity` | Local language server API + local cache | `providers/antigravity/` + `providers/codeium_family/` ([Codeium-family 架构文档](antigravity-api.md)) |
| Windsurf | `windsurf` | Local language server API + local cache | `providers/windsurf.rs` + `providers/codeium_family/` ([Codeium-family 架构文档](antigravity-api.md)) |
| MiniMax | `minimax` | HTTP API | `providers/minimax/` |
| Kiro | `kiro` | CLI (`kiro-cli chat --no-interactive /usage`) | `providers/kiro.rs` |
| Kilo | `kilo` | — (placeholder) | `providers/kilo.rs` |
| OpenCode | `opencode` | — (placeholder) | `providers/opencode.rs` |
| Vertex AI | `vertexai` | — (placeholder) | `providers/vertex_ai.rs` |

## Custom Provider（YAML 声明式）

除内置 Provider 外，用户可以通过 YAML 文件声明自定义 Provider，无需编写代码。

配置目录：
- macOS: `~/Library/Application Support/BananaTray/providers/`
- Linux: `$XDG_CONFIG_HOME/bananatray/providers/`
- macOS 如存在旧目录 `~/Library/Application Support/bananatray/providers/`，应用会在启动时自动迁移到规范目录

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
    /// 声明设置 UI 能力（默认 None，即自动管理型）
    fn settings_capability(&self) -> SettingsCapability { SettingsCapability::None }
}
```

### Supporting Types

- `ProviderDescriptor` — 收敛 provider 的注册 ID 与展示元数据
- `SettingsCapability` — provider 声明的设置 UI 能力。`TokenInput(TokenInputCapability)` 会自动启用通用 token 面板，`TokenInputCapability` 只描述静态 UI 元数据和 `credential_key`
- `AiProvider::resolve_token_input_state()` — provider 侧运行时钩子，返回纯数据 `TokenInputState`；当 provider 需要多来源 token 探测（如 Copilot）时在这里实现，而不是把行为塞进 `SettingsCapability`
- `provider.credentials` — 仅保存 BananaTray 自己管理的 token override；provider 实际认证信息也可能来自外部配置文件、CLI 登录态或环境变量
- `ProviderError` — provider 层返回的结构化错误
- `providers/error_presenter.rs` — 将 `ProviderError` 映射为 UI 文案和 `ErrorKind`
- [Provider Refactor Retrospective](provider/provider-refactor-retrospective.md) — 本次 provider 重构的根因、决策过程与优化方向
- `providers/common/cli.rs` — CLI provider 共享的可用性检查、命令执行与退出码处理
- [Provider Blueprints](provider-blueprints.md) — 后续新增/重构 provider 的复用蓝图

Providers run on background threads via `smol::unblock`. They must be `Send + Sync`.

## Abstraction Boundary

- `Claude` 的 `UsageProbe` 解决"从 CLI 还是 API 取数"
- `Antigravity` 的 `ParseStrategy` 解决"API JSON 和本地缓存 protobuf 如何解析"
- 两者都体现了 fallback / strategy 思想，但抽象层级不同，不应硬统一成单一 trait
- `Claude` 现在采用显式 source 编排：`mod.rs` 只负责 mode + fallback，source 细节留在 `api_probe.rs` / `cli_probe.rs`

## Adding a New Provider

1. Add variant to `define_provider_kind!` macro in `src/models/provider.rs` — `id_key()` and `from_id_key()` are auto-generated
2. Create `src/providers/my_provider.rs` (or `src/providers/my_provider/` for multi-file providers)
3. Implement `AiProvider` for `MyProvider`
4. (Optional) Override `settings_capability()` to return `SettingsCapability::TokenInput(TokenInputCapability { .. })` for interactive settings
5. If you use `TokenInputCapability`, choose a stable `credential_key` and ensure the provider reads it through `settings.provider.credentials.get_credential(key)`
6. Add icon: `src/icons/provider-myprovider.svg`
7. Register in `src/providers/mod.rs`: add to `register_providers!` macro
8. `cargo test --lib` — `test_all_provider_kinds_have_implementation` catches missing registrations
