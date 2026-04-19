# src/providers/codeium_family/

Codeium 系 Provider 的共享底层实现。

这里故意只放 **Antigravity / Windsurf** 都会长期复用的本地 source primitive，不负责完整的 source orchestration。

## 架构

```text
codeium_family/
├── spec.rs           — Provider 规格定义（静态常量）
├── mod.rs            — 共享入口：descriptor() / classify_unavailable() / refresh_live() / refresh_cache()
├── cache_source.rs   — 本地 SQLite 缓存读取
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
    ├─→ codeium_family::refresh_live()
    ├─→ windsurf::seat_source::fetch_refresh_data()
    └─→ codeium_family::refresh_cache()
```

这样拆的原因是：

- Antigravity 和 Windsurf 是两个独立 provider，不是同一个 provider 的两个品牌皮肤
- Windsurf 的 seat API 是产品特有逻辑，不应反向污染共享层
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
- `cache_db_relative_path`
- `auth_status_key_candidates`
- `process_markers`
- `cached_plan_info_key_candidates`

如果未来出现新的稳定产品差异，优先考虑继续加到 spec。
只有当差异本质上属于 provider 自己的 orchestration 或云端 source 时，才应放回 facade。

## 测试

- `mod.rs`：共享 helper / diagnostics 工具测试
- `cache_source.rs`：cache key / JSON fallback / quota 推断测试
- `live_source.rs`：进程识别、端口探测、endpoint 选择测试
- `parse_strategy.rs`：protobuf / JSON payload 解析测试

Windsurf seat API 相关测试位于 `src/providers/windsurf.rs` 与 `src/providers/windsurf/seat_source.rs`。

## 维护规则

如果你在这里新增代码，先问一句：

> 这是 Antigravity 和 Windsurf 都会共享的本地 primitive，还是只是某个 provider 的编排特例？

只有前者才应该进入 `codeium_family/`。
