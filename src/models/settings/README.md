# src/models/settings/

应用配置系统，按语义职责将设置分组为四个子结构体。

## 顶层结构

```
AppSettings
├── system: SystemSettings        — 系统行为
├── notification: NotificationSettings — 通知
├── display: DisplaySettings      — 显示/外观
└── provider: ProviderConfig      — Provider 管理
```

## 文件说明

### `mod.rs` — 结构定义 + ProviderConfig 核心逻辑

- **`AppSettings`** — 顶层设置结构体
- **`SystemSettings`** — `auto_hide_window` / `start_at_login` / `refresh_interval_mins` / `global_hotkey`
- **`NotificationSettings`** — `session_quota_notifications` / `notification_sound`
- **`DisplaySettings`** — `theme` / `language` / `tray_icon_style` / `quota_display_mode` / 各 UI 开关
- **`ProviderConfig`** — `credentials` / `enabled_providers` / `provider_order` / `hidden_quotas` / `sidebar_providers`
  - `is_enabled()` / `set_enabled()` / `prune_stale_custom_ids()`
- 枚举：**`TrayIconStyle`**（Monochrome/Yellow/Colorful/Dynamic）、**`QuotaDisplayMode`**（Remaining/Used）、**`AppTheme`**（Light/Dark/System）

### 领域方法文件（ProviderConfig 的扩展 impl）

| 文件 | 职责 |
|------|------|
| `provider_config_ordering.rs` | Provider 排序逻辑：`ordered_provider_ids()` / `move_provider()` / `ensure_order_defaults()` |
| `provider_config_quota.rs` | 配额可见性：`is_quota_visible()` / `set_quota_visible()` |
| `provider_config_sidebar.rs` | Sidebar 管理：`sidebar_provider_ids()` / `add_to_sidebar()` / `remove_from_sidebar()` / `ensure_sidebar_defaults()` |

### `migration.rs` — 旧格式迁移

- `try_migrate()` — 将旧版扁平 JSON 设置迁移到当前四组子结构体格式
- 自动检测旧格式标志字段（如顶层 `auto_hide_window`），逐字段映射

### `tests.rs` — 单元测试

覆盖所有配置方法（排序、可见性、sidebar、迁移、序列化/反序列化兼容性）。

## 持久化

配置通过 `settings_store.rs` 以 JSON 格式序列化到平台配置目录：

- macOS: `~/Library/Application Support/BananaTray/settings.json`
- Linux: `~/.config/bananatray/settings.json`

serde 的 `#[serde(default)]` 保证新字段向前兼容。
