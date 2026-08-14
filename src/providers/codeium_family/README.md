# src/providers/codeium_family/

Cognition 系 Provider 的共享底层实现。

这里故意只放 **Antigravity / Devin Desktop** 都会长期复用的本地 source primitive，不负责完整的 source orchestration。

## 架构

```text
codeium_family/
├── spec.rs             — Provider 规格定义（静态常量）
├── mod.rs              — 共享入口：descriptor() / classify_unavailable() / refresh_live() / refresh_cache()
├── cache_source.rs     — 本地 cache source 入口与 protobuf / cachedPlanInfo 回退编排
├── cache_source/       — cache DB 查询、auth status 解码、cachedPlanInfo 解析与 quota 构造
├── live_source.rs      — 本地 language_server 进程发现 + API 调用
├── parse_strategy.rs   — 同一领域数据的多种载荷解析（protobuf / JSON）
└── quota_semantics.rs  — Devin seat/cache 共用的 active weekly 耗尽推断
```

Devin Desktop 专属的云端 seat management API 实现不在这里，而在 `src/providers/windsurf/seat_source.rs`。

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
- `quota_semantics` 只共享 Devin seat/cache 对同一 payload 语义的解释：weekly percentage
  缺失且 reset 仍在未来时，当前周期视为 0% remaining

这里**不负责**：

- Antigravity / Devin Desktop 的 fallback 顺序
- Devin Desktop seat API 调用
- Devin Desktop seat + cache 的 quota 合并策略

## Provider-Owned Orchestration

当前的 source orchestration 明确收回到 provider facade：

```text
Antigravity
  refresh()
    ├─→ codeium_family::refresh_live()
    └─→ codeium_family::refresh_cache()

Devin Desktop
  refresh()
    ├─→ windsurf::seat_source::fetch_refresh_data()  # daily / weekly
    ├─→ codeium_family::refresh_live()
    └─→ codeium_family::refresh_cache()              # fallback / missing weekly补齐
```

这样拆的原因是：

- Antigravity 和 Devin Desktop 是两个独立 provider，不是同一个 provider 的两个品牌皮肤
- Devin Desktop 的 seat API 是产品特有实时数据源，不应反向污染共享层
- 共享层保留为“本地 source primitive”，未来更容易继续复用或替换

这里的 weekly helper 共享的是两条 Devin source 对同一上游字段的纯解释，不负责选择
source、fallback 或合并结果，因此不改变 provider-owned orchestration 边界。

## Runtime Source Labels

运行时 `source_label` 按真实命中的 source 覆盖，而不是永远使用静态 metadata：

- `local api`
- `local cache`
- `seat api`
- `seat api + local cache`

`spec.source_label` 只是静态兜底文案；Devin Desktop 当前使用 `"local/cloud fallback"` 作为默认说明。Seat API 的运行时来源展示为 Devin Cloud（或 Devin Cloud + Local cache）。

## `CodeiumFamilySpec`

每个 provider 的稳定差异继续通过 `spec.rs` 表达：

- `provider_id`
- `display_name`
- `dashboard_url`
- `ide_name`
- `cache_db_config_relative_path`
- `auth_status_key_candidates`（Devin Desktop 当前仍优先使用 `windsurfAuthStatus`，并兼容未来 `devinAuthStatus`）
- `process_markers`
- `cached_plan_info_key_candidates`（Devin Desktop 当前仍优先使用 `windsurf.settings.cachedPlanInfo`，并兼容未来 `devin.settings.cachedPlanInfo`）
- `cache_max_age_secs` — 缓存 SQLite 的 mtime 最大可信年龄（秒）

`cache_db_config_relative_path` 是相对系统配置目录的路径：macOS 会解析到
`~/Library/Application Support/<provider>/...`，Linux 会解析到
`$XDG_CONFIG_HOME/<provider>/...`（通常是 `~/.config/<provider>/...`）。
diagnostics 会列出实际尝试的候选路径。

language server 进程发现同时支持 macOS 的 `language_server_macos*`
和 Linux 的 `language_server_linux_*`。端口探测使用可用的 `lsof`
候选路径，避免不同发行版把 `lsof` 放在 `/usr/bin` 或 `/usr/sbin`
时漏检。

进程参数提取依赖 `pgrep` 的列表模式，但平台参数不同：macOS 用
`pgrep -lf`（仅输出进程名，受内核 15 字符截断限制），Linux 用
`pgrep -af`（输出完整命令行，可提取 `--windsurf_version` 等长参数）。
该差异通过 `live_source::PGREP_LIST_ARGS` 常量按 `cfg` 切换。

`ProcessInfo.windsurf_version` 从进程参数 `--windsurf_version` 提取，
仅 Linux `pgrep -af` 输出可获取；macOS 版本检测走 Info.plist 路径，
该字段恒为 `None`。

Windsurf app 版本检测（`seat_source::detect_windsurf_app_version`）
采用三级回退策略：
1. 从运行中进程参数提取（`--windsurf_version`，最可靠，直接来自运行实例）
2. 从 language server binary 路径推导 `product.json`（Electron 应用
   `resources/app/product.json`），读取 `windsurfVersion` 字段
3. 已知安装路径的 `product.json` → CLI `--version` 兜底

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

**未来 mtime 容忍窗口**：mtime 在未来时（时钟漂移 / NTP 微调 / 文件被恢复），
`FUTURE_MTIME_TOLERANCE_SECS`（当前 60s）以内按 fresh 处理避免断流；
超过则视为异常，返回 `Unavailable` 拒绝使用可能过期的缓存。

availability 语义刻意拆成两层：

- `cache_source::is_available()` 表示本地 quota cache source 可用，要求 DB 存在且新鲜。
- `cache_source::has_cache_db()` 只表示存在可尝试读取 auth / apiKey 的 DB。Devin Desktop
  provider-level `check_availability()` 使用这一层，让 seat API 不会被陈旧 quota 快照阻断。

进入解析后还有第二道闸：

- `parse_strategy::CacheParseStrategy`（protobuf 路径，Antigravity / 旧版 Devin Desktop）
- `cache_source::cached_plan::build_quota_from_cached`（JSON 路径，新版 Devin Desktop）

两条路径都对单条 quota 的 `reset_at_unix` 做 `<= now` 判断。reset 时间已过时，
缓存没有新周期的实际用量，必须丢弃该额度，不能推断为 100% 剩余；所有额度都被
丢弃时返回 `ProviderError::NoData`。两道闸的语义互补：mtime 闸防"整体快照过老"，
reset 闸防"数据库仍有其他状态写入、但个别额度快照已经失效"。

当额度因 reset 过期被丢弃时，`build_quota_from_cached` 会输出 debug 日志说明
丢弃原因（含 `stable_key` 和过期的 `reset_at_unix`），让刷新日志能自洽地解释
"解析出了 `remaining_percent` 却没有产生 quota"的情况。

两条解析路径都失败时，`read_refresh_data` 返回合并了 protobuf 和 cachedPlanInfo
各自真实失败原因的 `ParseFailed` 错误（如 `"protobuf: ...; cachedPlanInfo: ..."`），
避免最终错误只提及 protobuf 路径而掩盖 cachedPlanInfo 的实际失败原因（如 reset
过期导致的 `NoData`）。

## 测试

- `mod.rs`：共享 helper / diagnostics 工具测试
- `quota_semantics.rs`：active weekly 耗尽推断的纯函数测试
- `cache_source_tests.rs`：cache key / JSON fallback / quota 推断测试
- `live_source.rs`：进程识别、端口探测、endpoint 选择测试
- `parse_strategy.rs`：protobuf / JSON payload 解析测试

Devin Desktop seat API 相关测试位于 `src/providers/windsurf/mod.rs` 与 `src/providers/windsurf/seat_source.rs`。`debug-codeium-family devin` 是推荐诊断入口，`debug-codeium-family windsurf` 保留为兼容 alias。

## 维护规则

如果你在这里新增代码，先问一句：

> 这是 Antigravity 和 Devin Desktop 都会共享的本地 primitive，还是只是某个 provider 的编排特例？

只有前者才应该进入 `codeium_family/`。
