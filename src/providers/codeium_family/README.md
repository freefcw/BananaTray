# src/providers/codeium_family/

Codeium 系 Provider 的共享实现（Windsurf + Antigravity），通过 `CodeiumFamilySpec` 参数化实现一套代码支持多个 Provider。

## 架构

```
codeium_family/
├── spec.rs           — Provider 规格定义（静态常量）
├── mod.rs            — 入口：descriptor() / refresh_with_fallback() / classify_unavailable()
├── cache_source.rs   — 数据源 A：本地 SQLite 缓存
├── live_source.rs    — 数据源 B：本地 gRPC language_server
└── parse_strategy.rs — 响应解析策略（Protobuf / JSON 自动探测）
```

## 工作原理

### 双数据源回退

```
refresh_with_fallback(spec)
  │
  ├─→ live_source::fetch_refresh_data(spec)  ← 首选：直连本地 gRPC 服务
  │     ├── pgrep 发现 language_server 进程
  │     ├── lsof 获取监听端口
  │     ├── 发送 GetUserStatus gRPC 请求
  │     └── parse_strategy 解析响应
  │
  └─→ cache_source::read_refresh_data(spec)  ← 降级：读取 IDE 本地 SQLite 缓存
        ├── 打开 ~/Library/Application Support/{ide}/User/globalStorage/state.vscdb
        ├── 查询 ItemTable 中的 AuthStatus key
        └── JSON 解析用户配额数据
```

### `CodeiumFamilySpec` — 参数化规格

每个 Provider 的差异通过 `spec.rs` 中的静态常量表达：

| 字段 | Antigravity | Windsurf |
|------|-------------|----------|
| `provider_id` | `"antigravity"` | `"windsurf"` |
| `ide_name` | `"Antigravity"` | `"Windsurf"` |
| `cache_db_relative_path` | `Library/.../antigravity/...` | `Library/.../Windsurf/...` |
| `process_markers` | `["antigravity"]` | `["windsurf"]` |
| `auth_status_key_candidates` | IDE-specific keys | IDE-specific keys |

### `parse_strategy.rs` — 响应格式探测

gRPC 响应可能是 Protobuf 或 JSON 格式（取决于服务端版本），解析器自动探测：

1. 尝试 Protobuf 解码（prost）
2. 失败则尝试 JSON 解析
3. 从解析结果中提取 `firewall_status` / `individual_counts` → `RefreshData`

## 测试

- `mod.rs` 中包含 `mask_secret` 工具函数的单元测试
- `parse_strategy.rs` 中包含 Protobuf/JSON 解析的测试
- `cache_source.rs` 中包含 JSON 解析和 key 查询的测试

## 添加新的 Codeium 系 Provider

1. 在 `spec.rs` 中添加新的 `pub const NEW_SPEC: CodeiumFamilySpec = ...`
2. 创建瘦包装模块。两种模式可选：
   - **单文件**：`providers/new_provider.rs`（参考 `windsurf.rs`，47 行，最简方案）
   - **子目录**：`providers/new_provider/`（参考 `antigravity/`，需要额外的可用性检测或进程过滤逻辑时使用）
3. 在 `providers/mod.rs` 的 `register_providers!` 宏中添加一行
4. 在 `codeium_family/mod.rs` 的 `debug_report()` 中添加新 spec
