# Codeium-family Providers

本文件说明 BananaTray 当前对 Antigravity / Windsurf 的共享实现方式。

它是专题参考文档，不是 provider 注册表的完整契约。对外稳定边界请以 `docs/providers.md` 为准。

## 当前定位

BananaTray 把 **Antigravity** 和 **Windsurf** 视为两个独立的 built-in provider：

- UI 中独立展示
- 各自拥有独立的 metadata、图标和可用性判断
- 共享一套底层 Codeium-family 实现

共享的本地 source primitive 位于 `src/providers/codeium_family/`，具体 provider facade 分别位于 `src/providers/antigravity/` 和 `src/providers/windsurf.rs`。

## Stable Design

共享层只处理长期稳定的本地共性：

- 本地 language server 进程发现
- 本地接口调用
- 本地 cache fallback
- JSON / protobuf 解析

Windsurf 的 seat management API 不再放在共享层，而是收回到 provider 自己的模块 `src/providers/windsurf/seat_source.rs`。`codeium_family` 只暴露本地 source primitive，Windsurf facade 自己决定何时插入 seat API。

provider facade 负责两类东西：

- 产品身份与静态差异
- source orchestration

静态差异继续通过 spec 表达，包括：

- provider kind 与展示元数据
- `ide_name`
- cache DB 相对系统配置目录路径
- auth status key 候选
- 进程识别 marker
- provider-specific unavailable message

source orchestration 目前明确分开：

- Antigravity：`live -> cache`
- Windsurf：`seat -> live -> cache`

这意味着：

- 不要把 Windsurf 折叠成 Antigravity 的别名。
- 也不要把 Windsurf 的云端 fallback 反向塞进共享流程逻辑里。

## Refresh Path

当前 refresh 策略保持为：

1. Antigravity：优先尝试 live source，失败时回退本地 cache
2. Windsurf：优先尝试 seat API，失败时再尝试 live source，最后回退本地 cache
3. Windsurf 优先使用 seat API 返回的 daily / weekly quota；若 seat API 缺 weekly quota，则由 `windsurf.rs` 继续用本地 cache 补 weekly quota
4. 所有来源都失败时返回结构化错误

本地 cache 回退之前会做两道陈旧检查：

- **mtime 闸**：`cache_source::read_refresh_data` 会遍历 cache DB 候选路径，选择第一份
  新鲜 cache。单个 DB 的新鲜度取 `state.vscdb`、`state.vscdb-wal`、
  `state.vscdb-journal` 三者中**最新的 mtime**作为 cache 实际活跃时间，超过
  `spec.cache_max_age_secs`（Antigravity / Windsurf 当前都是 3 小时）即视为该候选
  整体快照不可信，并继续尝试后续候选；所有存在的候选都陈旧时才返回 `Unavailable`。
  之所以要看 sidecar：VS Code/Electron 系 SQLite 走 WAL 模式，新写入先到 `-wal`，
  主 DB 文件 mtime 在 checkpoint 之前可能远落后；只看主文件会把"还在活跃写入"的
  cache 误判为 stale。
- **reset 闸**：单条 quota 的 `reset_at_unix` 已过 → 服务端已经重置，缓存的
  `remaining_fraction` 是过期数据，统一视为 100% 剩余并清除倒计时。

`cache_source::is_available()` 与 `read_refresh_data()` 共用同一道 mtime 闸，避免本地
quota cache source 在 `check` 说"可用"但 `refresh` 立刻失败。Windsurf 的
provider-level `check_availability()` 还会单独接受"存在 cache DB"这个更弱条件，因为
seat API 只需要从 DB 中读取 apiKey，不应该被陈旧 quota 快照阻断。

因此用户可见行为是：缓存"还新但部分配额到期"→ 自动归零；缓存"整体太老"→
直接 unavailable，由上层显示为无数据，而不是误报。Stale 错误信息会带上具体路径、
age、阈值与"打开 IDE 一次以刷新本地缓存"的行动建议。

这里的关键不是“两个 provider 完全相同”，而是“它们共享同一套本地 source primitive，但各自保留自己的 orchestration 边界”。

## Stable Difference Dimensions

当前真正稳定、值得文档化的差异维度只有这些：

- provider 身份与展示名
- 本地 cache DB 配置目录相对路径
- auth status key 候选
- 进程参数 / 路径 marker
- dashboard URL
- 本地 cache 最大可信年龄（`cache_max_age_secs`）

本地 cache DB 路径会按平台解析：macOS 使用
`~/Library/Application Support/<provider>/...`，Linux 使用
`$XDG_CONFIG_HOME/<provider>/...`（通常是 `~/.config/<provider>/...`）。
language server 进程发现同样同时覆盖 macOS 的 `language_server_macos*`
和 Linux 的 `language_server_linux_*`。

如果未来还有差异，应优先继续加到 spec，而不是复制整套 provider 实现。

## Runtime Validation

当你修改 Codeium-family 实现后，建议在本机做一次运行时校验：

```bash
cargo run -- debug-codeium-family all
cargo run -- debug-codeium-family antigravity
cargo run -- debug-codeium-family windsurf
```

这个命令适合检查：

- cache DB 候选路径是否存在
- 关键 key 是否存在
- 进程 marker 是否仍能识别
- 端口 / csrf token 是否还能提取
- endpoint 提示是否合理

## Known Limits

- 本地服务的参数格式和 marker 可能随上游版本变化。
- 本地 cache key 名称可能因产品版本变化而漂移。
- 本地 HTTPS endpoint 可能使用自签证书。
- Windsurf seat API 依赖本地 auth status 中的 `apiKey`，请求体里的版本号使用本机安装版本的最佳努力探测；探测不到时不发送版本字段。
- cache fallback 只能反映本地已缓存的数据，不保证和实时服务完全一致；Windsurf 周配额应优先采用 seat API 的实时 `weeklyQuotaRemainingPercent`。

## Maintenance Rule

如果你只是新增一个普通 provider，不需要读这份文档。

只有在以下场景才需要同步更新这里：

- 修改 `codeium_family` 共享层边界
- 修改 Antigravity / Windsurf 的差异建模方式
- 修改 provider facade 的 source orchestration 顺序
- 修改运行时校验命令或关键诊断入口
