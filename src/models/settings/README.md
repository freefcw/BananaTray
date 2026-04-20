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
- **`SystemSettings`** — `auto_hide_window` / `start_at_login` / `refresh_interval_mins` / `global_hotkey`
  - 关联常量 `DEFAULT_REFRESH_INTERVAL_MINS: u64 = 5`，供 `RefreshScheduler` 等模块引用，保持默认值单一来源
  - 关联常量 `DEFAULT_GLOBAL_HOTKEY`，作为首次启动和无效配置回退时的默认全局热键；值使用 GPUI 可回读的持久化格式
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

`system.global_hotkey` 持久化为 GPUI 可直接回读的字符串格式（如 macOS 上的 `cmd-shift-s`），
设置页中则通过键捕获控件展示为用户友好的快捷键标签。runtime 仍兼容读取旧版展示格式
（如 `Cmd+Shift+S` / `Cmd+S`），并会在成功注册后规范化回写。

若磁盘配置缺失该字段，会自动回落到 `DEFAULT_GLOBAL_HOTKEY`；若配置值本身无效，
启动阶段会回退默认值并修正磁盘；若配置合法但当前注册失败（例如冲突），则保留用户原值。
在 macOS 上，成功保存后的注册现在走系统级 `RegisterEventHotKey` 路径，而不是纯事件监听。
