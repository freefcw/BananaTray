# src/platform/

平台适配层，集中管理所有与操作系统交互的代码。

## 模块分类

### GPUI 依赖模块（`cfg(feature = "app")`）

测试时不编译，仅在应用运行时可用。

#### `assets.rs` — GPUI 资源加载

- **`Assets`** — 实现 GPUI `AssetSource` trait，按优先级解析资源路径：
  1. `BANANATRAY_RESOURCES` 环境变量（AppImage）
  2. `.app/Contents/Resources/`（macOS bundle）
  3. `/usr/share/bananatray`（Linux deb/rpm）
  4. `CARGO_MANIFEST_DIR`（开发模式）

#### `single_instance.rs` — 单实例检测

- **`ensure_single_instance()`** — 通过 IPC local socket（interprocess crate）检测是否已有实例运行
- **`InstanceRole`** — Primary（首个实例，附带消息接收通道）或 Secondary（退出）

### 平台模块（始终编译）

被 bootstrap/runtime 和无 UI 场景复用。

#### `auto_launch.rs` — 开机自启动

- **`sync(enabled)`** — 同步自启动状态
- macOS: 使用 `SMAppService`（通过 smappservice-rs）
- Linux: 写入 XDG autostart `.desktop` 文件

#### `logging.rs` — 日志系统

- **`init()`** — 初始化 fern 日志系统 + panic hook
- 输出到文件（`~/Library/Logs/` 或 `~/.local/share/`）+ stderr
- 返回 `LogInit`（含日志路径，供 Debug Tab 使用）
- **`read_log_tail(path, max_lines)`** — 读取日志文件末尾 N 行（用于 Issue 上报）

#### `notification.rs` — 系统通知发送

- **`send_system_notification()`** — macOS 原生通知（UNUserNotificationCenter）
- **`send_plain_notification()`** — 简单文本通知
- 接收 `application::QuotaAlert` 领域事件，并适配到各 OS 通知实现

#### `paths.rs` — 配置路径解析

- **`app_config_dir()`** — 统一返回应用配置目录
- **`settings_path()`** — 返回 `settings.json` 的规范路径
- **`custom_providers_dir()`** — 返回自定义 Provider YAML 的规范目录
- **`custom_provider_path()`** — 返回单个自定义 Provider YAML 的规范路径
- **`migrate_legacy_custom_providers_dir()`** — 启动时将 macOS legacy lowercase 目录中的 YAML 迁移到规范目录

#### `system.rs` — 系统工具函数

- **`open_url()`** — 用默认浏览器打开 URL
- **`open_path_in_finder()`** — 在文件管理器中显示路径
- **`copy_to_clipboard()`** — 写入系统剪贴板
- **`is_dark_mode()`** — 查询系统暗色模式状态
- **`system_info_text()`** — 收集系统信息（调试用）

## 约束

- 平台模块（`auto_launch`、`notification`、`system`）**不可导入 `gpui`**
- `notification.rs` 只负责 OS 通知发送，不承载 application 业务状态机
- macOS 特定代码使用 `#[cfg(target_os = "macos")]` 守卫
- Linux 特定代码使用 `#[cfg(target_os = "linux")]` 守卫
