# src/providers/common/

Provider 共享基础设施，提供所有 Provider 实现的通用工具。

## 文件说明

### `cli.rs` — CLI 命令执行

解决 macOS GUI 应用 `PATH` 受限的问题，为 CLI 类 Provider（Claude, Codex, Amp, Kiro 等）提供统一的命令执行层：

- **`command_exists(binary)`** — 检查 CLI 是否可用
- **`run_command(binary, args)`** — 执行命令，`NotFound` 错误统一映射为 `ProviderError::CliNotFound`
- **`run_checked_command()`** — 严格模式，非零退出码即报错
- **`run_lenient_command()`** — 宽容模式，有输出就返回 Ok（适用于 amp/kiro-cli 偶发非零退出码）
- **`stdout_or_stderr_text()`** — 某些 CLI 把业务输出写到 stderr 的兜底方案
- 非交互式 CLI 统一带超时，超时映射为 `ProviderError::Timeout`

### `path_resolver.rs` — CLI 路径解析

`cli.rs` 和 `runner.rs` 共享同一套路径规则，避免 GUI 环境下不同执行路径对 CLI 是否存在给出不同结论：

- **`enriched_path()` / `enrich_path(path)`** — 补充 `~/.local/bin`、`~/.bun/bin`、`~/.cargo/bin`、`~/.npm-global/bin`、`~/.amp/bin`、Homebrew 和系统路径
- **`locate_executable(binary)`** — 先查绝对路径和当前 `PATH`，再按共享候选目录兜底定位可执行文件

### `http_client.rs` — HTTP 客户端（ureq）

为 API 类 Provider（Gemini, Custom YAML 等）提供统一的 HTTP 请求层：

- **`HttpError`** — 结构化 HTTP 错误枚举（`Timeout` / `Transport` / `HttpStatus { code, body }`），
  provider 可通过 `downcast_ref::<HttpError>()` 精确分类，`ProviderError::classify()` 自动将
  401/403 映射为 `AuthRequired`、超时映射为 `Timeout`、传输错误映射为 `NetworkFailed`
- **`get()` / `get_with_headers()`** — GET 请求变体（4xx/5xx 返回 `HttpError::HttpStatus`）
- **`post_json()` / `post_form()`** — POST 请求（JSON / form-urlencoded）
- 全局共享 `LazyLock<Agent>`，`http_status_as_error(false)` 确保非 2xx 也能读取 body
- `set_headers!` 宏统一处理 `"Key: Value"` 格式的 header 注入

### `jwt.rs` — JWT 解码

- **`decode_payload<T>(token)`** — 从 JWT 中提取 payload 段并反序列化
- 不做签名验证（仅用于读取 claim 信息，如 Copilot 的 token 到期时间）

### `runner.rs` — 交互式 PTY 运行器

为需要终端环境的 CLI（Claude, Codex）提供 PTY 模拟：

- **`InteractiveRunner`** — 通过 `portable-pty` 创建伪终端，模拟真实 shell 环境
- **`InteractiveOptions`** — 超时、idle timeout、自动应答、环境变量排除、周期性 Enter
- 支持 `auto_responses` 机制自动回应 CLI 的交互式提示（如 `Continue? [y/n]` → `y`）
- 使用独立读线程 + `mpsc::channel` 实现非阻塞 I/O

## 消费关系

```
providers/claude/         → cli.rs + runner.rs
providers/codex/          → runner.rs + http_client.rs
providers/amp.rs          → cli.rs
providers/kiro.rs         → cli.rs
providers/copilot/        → http_client.rs + jwt.rs
providers/gemini/         → http_client.rs
providers/kimi/           → http_client.rs
providers/cursor/         → http_client.rs
providers/minimax/        → http_client.rs
providers/codeium_family/ → http_client.rs
providers/custom/         → http_client.rs
```

`path_resolver.rs` 是 `cli.rs` / `runner.rs` 的内部依赖，不由 provider facade 直接拼路径规则。
