# OpenCode Provider

UI 展示名为 **OpenCode Go**；设置 / 状态稳定 key 仍是 `opencode`。

监控 OpenCode Go 订阅的滚动 5 小时 / 周 / 月用量窗口。

## Data Contract

- Endpoint: `GET https://opencode.ai/zen/go/v1/usage`
- Authentication: `Authorization: Bearer <api-key>`
- Response windows map in canonical display order:
  - `usage.rolling` → `QuotaType::Session` / `QuotaLabelSpec::Session`
  - `usage.weekly` → `QuotaType::Weekly` / `QuotaLabelSpec::Weekly`
  - `usage.monthly` → `QuotaType::Monthly` / `QuotaLabelSpec::Monthly`
- `percent` 是 **已用百分比**（官方由 `usagePercent` 格式化而来），经 `QuotaInfo::from_used_percent()` 入库。
- `resetsAt` 有则解析为 `QuotaDetailSpec::ResetAt`；BananaTray 不本地推算重置时间。
- `status`（`ok` / `rate-limited`）仅作上游状态，不单独做成 quota。

本实现跟踪官方 Go usage 端点（2026-08 起提供）。若上游字段改名，优先更新 parser fixtures。

## Credential Resolution

只读 OpenCode 本地凭据文件，不写回、不刷新：

1. `$XDG_DATA_HOME/opencode/auth.json`（若设置）
2. `~/.local/share/opencode/auth.json`（OpenCode xdg-basedir 默认）
3. `dirs::data_dir()/opencode/auth.json`（平台 data 目录兜底）

在文件内按 provider id 优先取：

1. `opencode-go`（`type: api` + 非空 `key`）
2. `opencode`（Zen API key；若同 workspace 已订阅 Go，也可访问本端点）

缺失 / 空 key / 非 `api` 类型 → `ConfigMissing`。HTTP 401 → `SessionExpired`；403（含「需要 Go 订阅」）→ `AuthRequired`。

## Module Boundaries

- `auth.rs`：auth.json 路径候选与 key 解析
- `client.rs`：Bearer GET
- `parser.rs`：响应 → 稳定 quota 语义
- `mod.rs`：descriptor / availability / refresh 与 HTTP 错误映射
