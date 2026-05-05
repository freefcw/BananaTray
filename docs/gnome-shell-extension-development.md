# GNOME Shell Extension Development

本文面向 BananaTray 维护者，说明如何开发和调试 `gnome-shell-extension/`。
用户安装、使用和排障入口见 `gnome-shell-extension/README.md`。

## 开发目标

GNOME Shell Extension 负责 GNOME 顶栏入口和弹窗 UI，Rust 主程序负责 provider 刷新、状态管理和
D-Bus 服务。两者通过当前用户的 Session D-Bus 通信：

```text
BananaTray Rust app
  └─ com.bananatray.Daemon
      ├─ GetAllQuotas() -> JSON snapshot
      ├─ RefreshAll() -> JSON snapshot
      ├─ OpenSettings()
      └─ RefreshComplete(JSON snapshot)

GNOME Shell Extension
  └─ BananaTrayExtension -> BananaTrayIndicator -> Provider / quota overview
                              └─ QuotaClient
```

扩展不直接读取配置文件、不执行 provider 刷新，也不保存业务状态。它只渲染 daemon 推送的
`DBusQuotaSnapshot`。

## 文件职责

| 文件 | 说明 |
|------|------|
| `gnome-shell-extension/extension.js` | GNOME Shell 扩展入口，只负责 `enable/disable` 生命周期和面板注册。 |
| `gnome-shell-extension/i18n.js` | Extension gettext 包装，所有 GNOME Shell UI 文案统一通过 `_()` 翻译。 |
| `gnome-shell-extension/panelButton.js` | `BananaTrayIndicator`，负责 PanelMenu.Button、弹窗装配、`QuotaClient` 回调和整体 UI 状态切换。 |
| `gnome-shell-extension/quotaClient.js` | D-Bus proxy、异步调用、`RefreshComplete` 监听和 JSON schema guard。 |
| `gnome-shell-extension/quotaPresentation.js` | 展示层纯函数：状态归一化、Provider/quota 排序、顶栏摘要聚合。 |
| `gnome-shell-extension/quotaWidgets.js` | 可复用 UI 组件：Provider 行、Quota 行、quota bar、状态点和文本 label helper。 |
| `gnome-shell-extension/po/zh_CN.po` | 简体中文翻译源文件。 |
| `gnome-shell-extension/locale/zh_CN/LC_MESSAGES/bananatray.mo` | GNOME Shell 运行时加载的简体中文 gettext 编译文件。 |
| `gnome-shell-extension/stylesheet.css` | 顶栏入口、overview popup、状态点、badge 和 quota bar 样式。 |
| `gnome-shell-extension/metadata.json` | UUID、名称和 GNOME Shell 版本兼容声明。 |
| `gnome-shell-extension/icons/bananatray-symbolic.svg` | 顶栏 symbolic 图标。安装和 nested 调试必须递归复制该目录。 |
| `scripts/dev-gnome-extension.sh` | nested GNOME Shell 调试入口。 |
| `scripts/install-gnome-extension.sh` | 当前用户会话安装 / 诊断入口；递归复制扩展文件并检查 `State`。 |
| `scripts/gnome-extension-mock-daemon.js` | mock `com.bananatray.Daemon`，用于 UI 状态调试。 |
| `scripts/check-gnome-extension.sh` | 静态检查：必需文件、GJS/Node 语法、禁止同步 D-Bus 调用、schema guard。 |

## Nested Shell 调试

Wayland 主会话不能热重启 GNOME Shell。扩展开发应使用 nested Shell：

```bash
bash scripts/dev-gnome-extension.sh
```

默认模式会：

1. 创建临时 GNOME profile。
2. 递归复制 `gnome-shell-extension/` 到临时扩展目录。
3. 在临时 dconf profile 中启用 `bananatray@bananatray.github.io`。
4. 启动 mock daemon。
5. 运行 `gnome-shell --devkit --wayland --no-x11`。

GNOME Shell 49+ 的 `--devkit` 模式需要 `mutter-devkit`。Ubuntu / Debian 上通常来自：

```bash
sudo apt install mutter-dev-bin
```

### Mock 数据模式

默认模式适合调 UI，不需要 Rust 主程序：

```bash
bash scripts/dev-gnome-extension.sh
```

mock daemon 会轮转多 provider、多 quota、refreshing、error、disconnected 和 cached data 状态。
修改扩展 JS 模块、`stylesheet.css`、`metadata.json` 或 mock 数据后，关闭 nested Shell 并重新运行脚本。

### 真实数据模式

要让扩展显示真实 provider/quota 数据：

```bash
bash scripts/dev-gnome-extension.sh --app-daemon
```

该模式会在 nested D-Bus session 中启动真实 BananaTray（默认 `cargo run`）。脚本会保留调用者的真实
`XDG_CONFIG_HOME` / `XDG_DATA_HOME` / `XDG_CACHE_HOME` / `XDG_STATE_HOME`，因此真实 app 会读取当前用户的
`settings.json`、自定义 Provider 和 provider 凭据。

脚本还会设置两个只用于开发的环境变量：

| 变量 | 作用 |
|------|------|
| `BANANATRAY_SINGLE_INSTANCE_SUFFIX=gnome-dev` | 避免 nested app 与主会话 BananaTray 抢同一个单实例锁。 |
| `BANANATRAY_FORCE_GNOME_EXTENSION=1` | 强制真实 app 跳过 KSNI fallback，避免 nested Shell 尚未完成扩展注册时主会话出现第二个传统托盘图标。 |

可用 release 构建调试：

```bash
bash scripts/dev-gnome-extension.sh --app-command 'cargo run --release'
```

如果要自己手动启动 daemon：

```bash
bash scripts/dev-gnome-extension.sh --real-daemon
```

此模式不会启动 mock 或真实 app。需要从脚本输出或子进程环境中取 nested `DBUS_SESSION_BUS_ADDRESS`，
再在同一个 session bus 中启动 BananaTray。

## D-Bus 调试

扩展和 daemon 必须在同一个 Session D-Bus 上。主会话的 `gdbus` 命令只能检查主会话，不能检查 nested
Shell。nested 调试时可先找到脚本 child 进程，再读取其 bus 地址：

```bash
ps -ef | rg 'dev-gnome-extension|gnome-shell --devkit|gnome-extension-mock-daemon|target/debug/bananatray'

child=<bash-child-pid>
addr=$(tr '\0' '\n' < /proc/$child/environ | sed -n 's/^DBUS_SESSION_BUS_ADDRESS=//p')
```

检查扩展加载状态：

```bash
DBUS_SESSION_BUS_ADDRESS="$addr" gdbus call --session \
  --dest org.gnome.Shell \
  --object-path /org/gnome/Shell \
  --method org.gnome.Shell.Extensions.GetExtensionInfo \
  bananatray@bananatray.github.io
```

正常结果应包含：

- `enabled: true`
- `state: 1`
- `error: ''`

检查 daemon 数据：

```bash
DBUS_SESSION_BUS_ADDRESS="$addr" gdbus call --session \
  --dest com.bananatray.Daemon \
  --object-path /com/bananatray/Daemon \
  --method com.bananatray.Daemon.GetAllQuotas
```

## JSON 协议约束

当前 schema 版本是 `1`。同一版本内允许新增字段，但不能删除字段、改名、改类型或改变枚举语义。

Extension 当前依赖的最小字段：

- 顶层：`schema_version`、`header`、`providers`
- `header`：`status_text`、`status_kind`
- provider：`id`、`display_name`、`icon_asset`、`connection`、`account_email`、`account_tier`、`quotas`、`worst_status`
- quota：`label`、`used`、`limit`、`status_level`、`display_text`、`quota_type_key`

`bar_ratio` 是 v1 内的可选增强字段。存在时扩展优先使用它渲染进度条；不存在时用 `used / limit` 降级。

Rust DTO 定义在 `src/application/selectors/dbus_dto.rs`，D-Bus 服务文档见 `src/dbus/README.md`。

## 开发约束

- 只使用 GNOME 45+ ESM imports。
- D-Bus 调用必须使用异步方法，禁止 `GetAllQuotasSync` / `RefreshAllSync` / `OpenSettingsSync`。
- `extension.js` 只保留扩展生命周期入口；PanelMenu 逻辑放在 `panelButton.js`。
- `panelButton.js` 只通过 `QuotaClient` 访问 D-Bus，不直接定义 D-Bus XML，不直接创建 proxy；协议层放在 `quotaClient.js`。
- 纯展示数据整理放在 `quotaPresentation.js`，可复用 UI 组件放在 `quotaWidgets.js`，避免后续图表和错误态继续挤回入口文件。
- 用户可见的 Extension 自有 UI 文案必须通过 `i18n.js` 的 `_()` 包裹；带数量的文案使用 `ngettext()`，不要翻译以分隔符开头的片段。同步更新 `po/zh_CN.po` 与 `locale/zh_CN/LC_MESSAGES/bananatray.mo`。D-Bus 快照里的 provider / quota 文本由 daemon 负责，不在 Extension 端二次翻译。
- `OK` / `LOW` / `OUT` 等短 badge 文案需要保留 `# Translators:` 注释，说明它们属于 quota 状态语境。
- `St.ScrollView` 使用 `set_child()`，不要使用 GNOME 50 下会崩的 `add_actor()`。
- 修改 UI 后优先在 nested Shell 中验证实际加载状态，而不是只看主会话。
- 新增扩展资产时同步 `scripts/check-gnome-extension.sh`、`scripts/install-gnome-extension.sh` 和安装说明，避免用户安装时漏复制子目录。

## 验证清单

提交扩展相关改动前至少运行：

```bash
bash scripts/check-gnome-extension.sh
bash scripts/install-gnome-extension.sh --dry-run
cargo fmt --check
cargo test --lib
cargo clippy
```

更新翻译时额外运行：

```bash
msgfmt --check \
  --output-file=gnome-shell-extension/locale/zh_CN/LC_MESSAGES/bananatray.mo \
  gnome-shell-extension/po/zh_CN.po
```

改动 nested 调试脚本时额外运行：

```bash
bash -n scripts/dev-gnome-extension.sh
bash -n scripts/install-gnome-extension.sh
BANANATRAY_GNOME_DRY_RUN=true bash scripts/dev-gnome-extension.sh
BANANATRAY_GNOME_DRY_RUN=true bash scripts/dev-gnome-extension.sh --app-daemon
```

视觉或行为改动还应启动 nested Shell 做 smoke test：

```bash
bash scripts/dev-gnome-extension.sh
bash scripts/dev-gnome-extension.sh --app-daemon
```

确认脚本终端中出现 `BananaTray: daemon appeared on D-Bus`，并用 `GetExtensionInfo` 检查扩展没有 runtime error。

## 常见问题

| 现象 | 常见原因 | 处理 |
|------|----------|------|
| nested 窗口没有出现 | 缺少 `mutter-devkit` | 安装 `mutter-dev-bin`。 |
| 扩展未加载 | dconf profile 没在 `dbus-run-session` 前准备好，或 metadata 不兼容 | 使用脚本默认流程；检查 `GetExtensionInfo` 的 `state` 和 `error`。 |
| 主会话 `State: ERROR` 且仍报旧 `add_actor` 错误 | 用户扩展目录之前安装了旧版文件，GNOME Shell 进程仍缓存旧模块错误 | 运行 `bash scripts/install-gnome-extension.sh` 递归安装新版文件；Wayland 需要注销重登，X11 用 Alt+F2 → `r` 重启 Shell。 |
| 弹窗一直显示 daemon not running | 扩展和 daemon 不在同一个 Session D-Bus | 用 nested child 进程的 `DBUS_SESSION_BUS_ADDRESS` 检查 D-Bus。 |
| `--app-daemon` 显示空 provider | 真实配置目录未被传入，或当前设置没有启用 provider | 检查脚本输出中的日志路径和 provider 配置路径；确认真实 `settings.json`。 |
| 修改样式后没有变化 | 主会话 Wayland 不能热重启 Shell，或 nested Shell 没重启 | 关闭 nested Shell 后重新运行脚本；主会话需要注销重登。 |
| 顶栏出现传统 AppIndicator | 调试 app 没有设置 `BANANATRAY_FORCE_GNOME_EXTENSION=1` | 使用 `--app-daemon` 启动真实 app，避免手动漏环境变量。 |
