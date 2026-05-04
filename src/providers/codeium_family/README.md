# src/providers/codeium_family/

Codeium 系 Provider 的共享底层实现。

这里故意只放 **Antigravity / Windsurf** 都会长期复用的本地 source primitive，不负责完整的 source orchestration。

## 架构

```text
codeium_family/
├── spec.rs           — Provider 规格定义（静态常量）
├── mod.rs            — 共享入口：descriptor() / classify_unavailable() / refresh_live() / refresh_cache()
├── cache_source.rs   — 本地 cache source 入口与 protobuf / cachedPlanInfo 回退编排
├── cache_source/     — cache DB 查询、auth status 解码、cachedPlanInfo 解析与 quota 构造
├── live_source.rs    — 本地 language_server 进程发现 + API 调用
└── parse_strategy.rs — 同一领域数据的多种载荷解析（protobuf / JSON）
```

Windsurf 专属的云端 seat management API 实现不在这里，而在 `src/providers/windsurf/seat_source.rs`。

## 共享层职责

`codeium_family` 当前只负责这些稳定共性：

- `CodeiumFamilySpec` 参数化的 provider 差异
- provider descriptor 构建
- 本地 live source 刷新
- 本地 cache source 刷新
- 本地进程 / cache DB / auth status 的共享 helper
- diagnostics/debug CLI 需要的本地探测能力
- provider refresh/source/parser 边界返回 `ProviderResult<T>`，把本地缺失、解析失败、
  无数据等情况收敛成 `ProviderError`

这里**不负责**：

- Antigravity / Windsurf 的 fallback 顺序
- Windsurf seat API 调用
- Windsurf seat + cache 的 quota 合并策略

## Provider-Owned Orchestration

当前的 source orchestration 明确收回到 provider facade：

```text
Antigravity
  refresh()
    ├─→ codeium_family::refresh_live()
    └─→ codeium_family::refresh_cache()

Windsurf
  refresh()
    ├─→ windsurf::seat_source::fetch_refresh_data()  # daily / weekly
    ├─→ codeium_family::refresh_live()
    └─→ codeium_family::refresh_cache()              # fallback / missing weekly补齐
```

这样拆的原因是：

- Antigravity 和 Windsurf 是两个独立 provider，不是同一个 provider 的两个品牌皮肤
- Windsurf 的 seat API 是产品特有实时数据源，不应反向污染共享层
- 共享层保留为“本地 source primitive”，未来更容易继续复用或替换

## Runtime Source Labels

运行时 `source_label` 按真实命中的 source 覆盖，而不是永远使用静态 metadata：

- `local api`
- `local cache`
- `seat api`
- `seat api + local cache`

`spec.source_label` 只是静态兜底文案；Windsurf 当前使用 `"local/cloud fallback"` 作为默认说明。

## `CodeiumFamilySpec`

每个 provider 的稳定差异继续通过 `spec.rs` 表达：

- `provider_id`
- `display_name`
- `dashboard_url`
- `ide_name`
- `cache_db_config_relative_path`
- `auth_status_key_candidates`
- `process_markers`
- `cached_plan_info_key_candidates`
- `cache_max_age_secs` — 缓存 SQLite 的 mtime 最大可信年龄（秒）

`cache_db_config_relative_path` 是相对系统配置目录的路径：macOS 会解析到
`~/Library/Application Support/<provider>/...`，Linux 会解析到
`$XDG_CONFIG_HOME/<provider>/...`（通常是 `~/.config/<provider>/...`）。
diagnostics 会列出实际尝试的候选路径。

language server 进程发现同时支持 macOS 的 `language_server_macos*`
和 Linux 的 `language_server_linux_*`。端口探测使用可用的 `lsof`
候选路径，避免不同发行版把 `lsof` 放在 `/usr/bin` 或 `/usr/sbin`
时漏检。

如果未来出现新的稳定产品差异，优先考虑继续加到 spec。
只有当差异本质上属于 provider 自己的 orchestration 或云端 source 时，才应放回 facade。

## 缓存陈旧检测

`cache_source::read_refresh_data` 在打开 SQLite 之前会从候选 DB 中选择第一份新鲜 cache：
如果较高优先级候选存在但已陈旧，会继续尝试后面的候选路径。所有存在的候选都超出
`spec.cache_max_age_secs` 时，才返回 `ProviderError::Unavailable`，避免上游把
language server 长期未运行后的旧快照当作真实配额上报。

mtime 取 `state.vscdb`、`state.vscdb-wal`、`state.vscdb-journal` 三者中**最新的**：
SQLite WAL 模式下新写入先到 `-wal`，主 DB 文件 mtime 在 checkpoint 之前可能远落后，
只看主文件会把"还在活跃写入"的 cache 误判为 stale。

availability 语义刻意拆成两层：

- `cache_source::is_available()` 表示本地 quota cache source 可用，要求 DB 存在且新鲜。
- `cache_source::has_cache_db()` 只表示存在可尝试读取 auth / apiKey 的 DB。Windsurf
  provider-level `check_availability()` 使用这一层，让 seat API 不会被陈旧 quota 快照阻断。

进入解析后还有第二道闸：

- `parse_strategy::CacheParseStrategy`（protobuf 路径，Antigravity / 旧版 Windsurf）
- `cache_source::cached_plan::build_quota_from_cached`（JSON 路径，新版 Windsurf）

两条路径都对单条 quota 的 `reset_at_unix` 做 `<= now` 判断：reset 时间已过 →
服务端已经重置配额，缓存的 `remaining_fraction` 是过期数据，统一视为 100% 剩余
并清除倒计时。两道闸的语义互补：mtime 闸防"整体快照过老"，reset 闸防"个别配额到期"。

## 测试

- `mod.rs`：共享 helper / diagnostics 工具测试
- `cache_source_tests.rs`：cache key / JSON fallback / quota 推断测试
- `live_source.rs`：进程识别、端口探测、endpoint 选择测试
- `parse_strategy.rs`：protobuf / JSON payload 解析测试

Windsurf seat API 相关测试位于 `src/providers/windsurf.rs` 与 `src/providers/windsurf/seat_source.rs`。

## 维护规则

如果你在这里新增代码，先问一句：

> 这是 Antigravity 和 Windsurf 都会共享的本地 primitive，还是只是某个 provider 的编排特例？

只有前者才应该进入 `codeium_family/`。
