# Provider Blueprints

> 这是一份模式参考文档，不是强制文件树规范。
> 当前代码可以偏离这里的示例布局，只要仍然遵守 `docs/providers.md` 中的稳定边界即可。

这份文档沉淀 BananaTray 中已经验证过的 provider 设计蓝图，后续新增或重构 provider 时优先复用这里的结构，而不是重新发明抽象。

## 核心原则

- 先按稳定职责拆分，再考虑抽象复用。
- 只抽“跨 provider 长期稳定重复出现的动作”，不要抽一次性的业务流程。
- provider 层只返回结构化事实：
  - `descriptor()`
  - `check_availability()`
  - `refresh()`
- 用户提示文案统一交给 `ProviderErrorPresenter`，不要在 provider 内做 UI 拼接。

## 蓝图 A：标准 HTTP Provider

适用场景：
- 有稳定认证方式
- 有明确 HTTP 接口
- 返回 JSON 或固定响应结构

推荐目录（示意）：

```text
providers/my_provider/
  mod.rs        # provider facade / orchestration
  auth.rs       # token / credential loading and validation
  client.rs     # HTTP request construction and transport
  parser.rs     # response -> RefreshData / domain objects
```

零字段 provider 使用 `define_unit_provider!(MyProvider)` 宏来消除 struct + Default + new 的样板代码。
如果 provider 需要持有实例状态（如 Claude 的 probe 实例），则手动定义 struct。

当前参考：
- `codex/`
- `gemini/`
- `kimi/`
- `minimax/`
- `cursor/`
- `copilot/`

拆分边界：
- `auth.rs` 只负责“凭证从哪里来、是否过期、如何读取”
- `client.rs` 只负责“请求怎么发”
- `parser.rs` 只负责“响应怎么解释”
- `mod.rs` 只负责“执行顺序与错误回退”

## 蓝图 B：CLI Provider

适用场景：
- 数据只能通过 CLI 读取
- CLI 可用性检查、命令执行、退出码处理存在重复

推荐结构（示意）：

```text
providers/
  common/cli.rs
  my_cli_provider.rs
```

优先复用：
- `common::cli::command_exists()`
- `common::cli::run_command()`
- `common::cli::run_checked_command()`
- `common::cli::run_lenient_command()` — 容忍非零退出码（如 amp、kiro-cli）
- `common::cli::ensure_success()`
- `common::cli::stdout_text()` / `stdout_or_stderr_text()`

当前参考：
- `amp.rs`
- `kiro.rs`
- `opencode.rs`

边界说明：
- 通用层只处理“命令存在、执行、退出码”
- provider 自己处理“stdout/stderr 哪个是正文”“如何解析输出”

不要抽的内容：
- 某个 CLI 专属的 stderr 合并策略
- 某个 CLI 特有的正则解析

## 蓝图 C：本地服务 + 本地缓存回退 Provider

适用场景：
- 首选从本地进程或本地服务实时取数
- 次选从本地数据库 / 缓存文件兜底
- 同一个 provider 有多条 source path

推荐目录（示意）：

```text
providers/my_provider/
  mod.rs            # orchestration，只负责 source fallback
  live_source.rs    # 进程发现 / 端口发现 / 本地 API 请求
  cache_source.rs   # SQLite / 文件缓存读取
  parse_strategy.rs # 同一领域数据的多种载荷解析
```

当前参考：
- `src/providers/antigravity/mod.rs`
- `src/providers/codeium_family/`

### Codeium-family Provider 的稳定分层

- provider facade（如 `antigravity/mod.rs`、`windsurf.rs`）
  - 只做 source orchestration
  - `check_availability()` 判断共享本地 source 是否至少有一条可用
  - `refresh()` 决定 fallback 顺序
- `codeium_family/live_source.rs`
  - 查进程
  - 解析 `--csrf_token`
  - 发现端口
  - 轮询候选 endpoint
  - 调用 API 并转成 `RefreshData`
- `codeium_family/cache_source.rs`
  - 定位 `state.vscdb`
  - 读取 auth status
  - 解出 `userStatusProtoBinaryBase64` 或走 cachedPlanInfo fallback
  - 解析本地缓存中的 quota 数据
- `codeium_family/parse_strategy.rs`
  - 只处理“同一组领域数据的不同载荷格式”
  - 例如：API JSON / cache protobuf
- provider-specific source（如 `windsurf/seat_source.rs`）
  - 只处理某个 provider 独有的云端或旁路 source
  - 不反向塞回共享模块

### 为什么不要把 `live_source` 和 `parse_strategy` 合并成统一 trait

因为它们解决的不是同一层问题：

- `live_source/cache_source` 是“从哪里拿数据”
- `parse_strategy` 是“同一份领域数据如何解码”

前者是 source fallback，后者是 payload parsing。
两者都像 strategy，但语义不同，强行合并会让抽象失真。

### 回退策略建议

推荐顺序：

1. 实时 source
2. 本地 cache
3. 返回结构化技术错误

错误处理建议：

- `check_availability()` 反映“是否至少有一条可行 source”
- `refresh()` 记录 primary source 失败原因
- fallback 也失败时，把 primary + fallback 错误一起带回，便于诊断

## 蓝图 C-1：多 Source 编排 Provider

适用场景：
- provider 有两条以上真实取数路径
- 需要显式的优先级和回退顺序
- 每条 source 已经有自己的稳定实现

推荐目录（示意）：

```text
providers/my_provider/
  mod.rs         # source selection / fallback orchestration
  api_probe.rs   # source A
  cli_probe.rs   # source B
  probe.rs       # source trait / mode enum
```

当前参考：
- `claude/`

实现要点：
- `mod.rs` 只负责：
  - `check_availability()` 的“任一 source 可用即可”
  - `refresh()` 的 source 优先级
  - `ProbeMode` / source policy
- 每个 source 文件只负责自己的取数与解析
- 适合保留 trait，当且仅当 source 确实存在多个实现

不要做的事：
- 在 `mod.rs` 里塞进 source 的细节解析
- 把 `probe.rs` 扩展成跨 provider 的总控抽象

## 蓝图 D：占位 / 能识别但无法监控的 Provider

适用场景：
- 能检测安装或配置状态
- 但没有公开 API / CLI 输出，不支持真实配额读取

推荐结构：

```text
providers/my_placeholder.rs
```

实现要点：
- `check_availability()` 只判断安装或配置是否存在
- `refresh()` 明确返回 `ProviderError::unavailable(...)`
- 探测逻辑尽量抽成纯函数，便于补最小测试

当前参考：
- `kilo.rs`
- `vertex_ai.rs`
- `opencode.rs`

## 何时应该新增公共模块

满足以下条件再抽到 `providers/common/`：

1. 至少 2 个 provider 已经重复
2. 重复的是“动作边界”，不是“业务解释”
3. 抽出来不会迫使特例 provider 反向适配

已经验证可抽的公共模块：
- `common/jwt.rs`
- `common/cli.rs`
- `common/http_client.rs` — 共享 HTTP 客户端（ureq 封装、认证 header）
- `common/runner.rs` — PTY 交互式命令执行器

## 新增 Provider 时的推荐流程

1. 先判断它属于哪种蓝图
2. 先按蓝图建目录，不急着抽 trait
3. 先把 `refresh()` 跑通
4. 再把重复动作提到 `providers/common/`
5. 最后补文档和测试

## 反模式

- 为了“看起来统一”强行引入总控 trait
- 把 source fallback 和 payload parsing 混成一层抽象
- 在 provider 内直接拼 UI 错误文案
- 把一次性的分支逻辑抽成公共 helper
