# src/models/settings/

应用配置系统，按语义职责将设置分组为四个子结构体。

## 顶层结构

```
AppSettings
├── system: SystemSettings        — 系统行为
├── notification: NotificationSettings — 通知
├── display: DisplaySettings      — 显示/外观
└── provider: ProviderConfig      — Provider 管理（含 app-managed credentials）
```

## 文件说明

### `mod.rs` — 结构定义 + ProviderConfig 核心逻辑

- **`AppSettings`** — 顶层设置结构体
- **`SystemSettings`** — `auto_hide_window` / `start_at_login` / `refresh_interval_mins`
  - 关联常量 `DEFAULT_REFRESH_INTERVAL_MINS: u64 = 5`，供 `RefreshScheduler` 等模块引用，保持默认值单一来源
- **`NotificationSettings`** — `session_quota_notifications` / `notification_sound`
- **`DisplaySettings`** — `theme` / `language` / `tray_icon_style` / `quota_display_mode` / 各 UI 开关
- **`ProviderConfig`** — `credentials` / `enabled_providers` / `provider_order` / `hidden_quotas` / `sidebar_providers`
  - `is_enabled()` / `set_enabled()` / `remove_enabled_record()` / `prune_stale_custom_ids()` / `register_discovered_custom_providers()`
- **`ProviderSettings`** — 扁平 key-value 凭证存储（`github_token`、`custom_token` 等），位于 `ProviderConfig::credentials`
  - 这里只存 BananaTray 自己管理的 provider token；Provider 真实可用凭证也可能来自外部配置文件、CLI 登录态或环境变量
- 枚举：**`TrayIconStyle`**（Monochrome/Yellow/Colorful/Dynamic）、**`QuotaDisplayMode`**（Remaining/Used）、**`AppTheme`**（Light/Dark/System）

### 领域方法文件（ProviderConfig 的扩展 impl）

| 文件 | 职责 |
|------|------|
| `provider_config_ordering.rs` | Provider 排序逻辑：`ordered_provider_ids()` / `move_provider()` / `ensure_order_defaults()` |
| `provider_config_quota.rs` | 配额可见性：`is_quota_visible()` / `set_quota_visible()` |
| `provider_config_sidebar.rs` | Sidebar 管理：`sidebar_provider_ids()` / `register_discovered_custom_providers()` / `add_to_sidebar()` / `remove_from_sidebar()` |

### `tests.rs` — 单元测试

覆盖所有配置方法（排序、可见性、sidebar、序列化/反序列化兼容性）。

## 持久化

配置通过 `settings_store.rs` 以 JSON 格式序列化到平台配置目录：

- macOS: `~/Library/Application Support/BananaTray/settings.json`
- Linux: `~/.config/bananatray/settings.json`

serde 的 `#[serde(default)]` 保证新字段向前兼容。
