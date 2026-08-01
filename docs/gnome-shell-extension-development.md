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
`DBusQuotaSnapshot`。`RefreshAll()` 返回的是调用前缓存，`QuotaClient` 只校验该响应；手动刷新状态
由后续信号快照中的 Provider `Refreshing` 状态驱动，没有 Provider 仍在刷新时立即恢复；若 5 秒内
没有收到刷新状态，也会自动恢复。
正式 Linux 安装包还会安装 Session D-Bus activation 文件和 systemd user
service；`QuotaClient` 启动时会异步请求 `StartServiceByName`，daemon 未运行时不再只能被动等待。
如果用户主动停止 daemon，扩展不会在 `NameOwnerChanged` 下线事件里立即拉起，只会在扩展启动或用户点击刷新/设置时请求 activation。

## 文件职责

| 文件 | 说明 |
|------|------|
| `gnome-shell-extension/extension.js` | GNOME Shell 扩展入口，只负责 `enable/disable` 生命周期和面板注册。 |
| `gnome-shell-extension/i18n.js` | Extension gettext 包装，所有 GNOME Shell UI 文案统一通过 `_()` 翻译。 |
| `gnome-shell-extension/panelButton.js` | `BananaTrayIndicator`，负责 PanelMenu.Button、弹窗装配、`QuotaClient` 回调和整体 UI 状态切换。 |
| `gnome-shell-extension/quotaClient.js` | D-Bus proxy、异步调用、`RefreshComplete` 监听和 JSON schema guard。 |
| `gnome-shell-extension/quotaPresentation.js` | 展示层纯函数：状态归一化、手动刷新进度判定、Provider/quota 排序、顶栏摘要聚合。 |
| `gnome-shell-extension/quotaWidgets.js` | 可复用 UI 组件：Provider 行、Quota 行、quota bar、状态点和文本 label helper。 |
| `gnome-shell-extension/po/zh_CN.po` | 简体中文翻译源文件。 |
| `gnome-shell-extension/locale/zh_CN/LC_MESSAGES/bananatray.mo` | GNOME Shell 运行时加载的简体中文 gettext 编译文件。 |
| `gnome-shell-extension/stylesheet.css` | 顶栏入口、overview popup、状态点、badge 和 quota bar 样式。 |
| `gnome-shell-extension/metadata.json` | UUID、名称和 GNOME Shell 版本兼容声明。 |
| `gnome-shell-extension/icons/bananatray-symbolic.svg` | 顶栏 symbolic 图标。安装和 nested 调试必须递归复制该目录。 |
| `scripts/dev-gnome-extension.sh` | nested GNOME Shell 调试入口。 |
| `scripts/dev-gnome-extension-watch.sh` | 真实桌面会话热重载：监控文件变化，自动 cp + disable/enable。 |
| `scripts/install-gnome-extension.sh` | 当前用户会话安装 / 诊断入口；递归复制扩展文件并检查 `State`。 |
| `scripts/gnome-extension-mock-daemon.js` | mock `com.bananatray.Daemon`，用于 UI 状态调试。 |
| `scripts/check-gnome-extension.sh` | 静态检查：必需文件、GJS/Node 语法、禁止同步 D-Bus 调用、schema guard 和 D-Bus contract parity。工具缺失（node/gettext/gjs 等）默认跳过对应检查并在结尾汇总列出；`GNOME_CHECK_STRICT=1` 时缺失即失败（CI 已开启，避免"跳过仍 passed"的假阳性）。 |
| `scripts/check-gnome-dbus-contract.mjs` | D-Bus 契约静态校验：比较 Extension client/mock 的 bus/path/XML/schema version，并确认 Rust iface/DTO 仍匹配。 |
| `scripts/test-gnome-packaging-contracts.sh` | 打包契约负例：逐个移除 schema version、activation placeholder 和 daemon-reload 标记，确认每个文件的漂移都会被门禁拦截。 |
| `scripts/test-gnome-extension-gjs.sh` | GJS 真实 D-Bus 集成测试：在 `dbus-run-session` 下用真实 `Gio.DBusProxy` 验证 `quotaClient.js` 全链路。需要 `gjs` + `dbus-run-session`，缺失时 skip。 |
| `gnome-shell-extension/tests/gjs-quota-client-integration.test.js` | GJS 集成测试驱动：启动 mock daemon → 真实 `QuotaClient` 端到端断言（proxy ready、`RefreshComplete` 信号、daemon 重连、schema 拒绝、`openSettings`、`destroy` 后无回调）。 |
| `gnome-shell-extension/tests/gjs-mock-daemon.js` | GJS 集成测试专用 mock D-Bus daemon（ESM）：复用 `quotaClient.js` 常量，支持注入自定义快照生成器。 |
| `gnome-shell-extension/tests/gjs-i18n-stub.js` | GJS 集成测试的 `i18n.js` 替身（passthrough），避免引入 `resource:///org/gnome/shell/...` 依赖。 |
| `scripts/bundle-gnome-extension.sh` | e.g.o 提交用 zip 打包：白名单运行时文件、metadata 校验、版本信息输出。 |
| `resources/linux/com.bananatray.Daemon.service` | Session D-Bus activation 文件，声明 `com.bananatray.Daemon` 如何启动。 |
| `resources/linux/bananatray.service` | systemd user service，供 D-Bus activation 或用户手动 `systemctl --user start` 启动。 |

## 真实桌面会话热重载

GNOME 45+ ESM 扩展在 `gnome-extensions disable/enable` 周期会完全卸载并重新导入模块，
不需要注销或重新登录。`dev-gnome-extension-watch.sh` 利用这一点实现自动热重载：

```bash
bash scripts/dev-gnome-extension-watch.sh
```

脚本会用 `inotifywait`（Linux）或 `fswatch`（macOS）监控 `gnome-shell-extension/` 目录，
文件变化时自动复制到用户扩展目录并执行 disable/enable。首次运行会自动安装。

单次同步（不持续监控）：

```bash
bash scripts/dev-gnome-extension-watch.sh --once
```

> **注意**：X11 上此方案非常可靠。Wayland 上大部分 GNOME 45+ 版本也支持，但个别早期版本
> 可能仍需注销重登。如果 disable/enable 后扩展未更新，改用 nested shell 方案。

## Nested Shell 调试

Wayland 主会话不能热重启 GNOME Shell。需要隔离调试环境时使用 nested Shell：

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

正式安装包中的 activation 文件安装后，可检查：

```bash
test -f /usr/share/dbus-1/services/com.bananatray.Daemon.service
test -f /usr/lib/systemd/user/bananatray.service || test -f /lib/systemd/user/bananatray.service
systemctl --user status bananatray.service
```

`bananatray.service` 使用 `Type=dbus` 和 `BusName=com.bananatray.Daemon`。主程序启动路径会在进入
GPUI run loop 后初始化 D-Bus 服务并 request name；如果未来把 D-Bus 注册延后到慢 I/O 之后，需要重新评估
systemd 的默认启动超时。

## 调试技巧

### GNOME Shell 环境变量

```bash
# 慢速动画，方便观察过渡效果和 popup 打开行为
GNOME_SHELL_SLOWDOWN_FACTOR=3 gnome-shell --devkit --wayland --no-x11

# 在 nested shell 中禁用所有其他扩展，只保留 BananaTray
GNOME_SHELL_DEBUG=backtrace-crashes gnome-shell --devkit --wayland --no-x11
```

### 实时查看扩展日志

```bash
# 主会话日志
journalctl -f -o cat /usr/bin/gnome-shell | grep -i bananatray

# nested shell（进程 PID）
journalctl -f -o cat _PID=$(pgrep -f 'gnome-shell --devkit')
```

### Looking Glass

在运行中的 GNOME Shell 里按 `Alt+F2 → lg` 打开 Looking Glass：

- **Extensions** tab 查看扩展状态、错误信息
- **Evaluator** tab 即时执行 GJS 代码（对 ESM 扩展模块缓存有限制）

## JSON 协议约束

当前 schema 版本是 `1`。同一版本内允许新增字段，但不能删除字段、改名、改类型或改变枚举语义。

Extension 当前依赖的最小字段：

- 顶层：`schema_version`、`header`、`providers`
- `header`：`status_text`、`status_kind`
- provider：`id`、`display_name`、`icon_asset`、`connection`、`account_email`、`account_tier`、`quotas`、`worst_status`
- quota：`label`、`used`、`limit`、`status_level`、`display_text`、`quota_type_key`

`bar_ratio` 是 v1 内的可选增强字段。存在时扩展优先使用它渲染进度条；不存在时用 `used / limit` 降级。

`elapsed_secs` 是 v1 内的可选增强字段，仅在 `status_kind == Stale` 时由 daemon 填充。扩展会优先用它本地化 `Synced` / `Syncing` / `Offline` / `x minutes ago` / `x hours ago`；若缺失则回退到 daemon 提供的 `status_text`。

Rust DTO 定义在 `src/application/selectors/dbus_dto.rs`，D-Bus 服务文档见 `src/dbus/README.md`。
`scripts/check-gnome-dbus-contract.mjs` 会在扩展检查中确认 Extension client、mock daemon、
Rust zbus iface 和 Rust DTO schema version 没有漂移。

## 开发约束

- 只使用 GNOME 45+ ESM imports。
- D-Bus 调用必须使用异步方法，禁止 `GetAllQuotasSync` / `RefreshAllSync` / `OpenSettingsSync`。
- 修改 D-Bus bus name、object path、XML、method/signal/property 或 JSON schema version 时，必须同步
  Extension client、mock daemon、Rust iface/DTO，并让 `scripts/check-gnome-dbus-contract.mjs` 通过。
- `extension.js` 只保留扩展生命周期入口；PanelMenu 逻辑放在 `panelButton.js`。
- `panelButton.js` 只通过 `QuotaClient` 访问 D-Bus，不直接定义 D-Bus XML，不直接创建 proxy；协议层放在 `quotaClient.js`。
- 纯展示数据整理放在 `quotaPresentation.js`，可复用 UI 组件放在 `quotaWidgets.js`，避免后续图表和错误态继续挤回入口文件。
- 用户可见的 Extension 自有 UI 文案必须通过 `i18n.js` 的 `_()` 包裹；带数量的文案使用 `ngettext()`，不要翻译以分隔符开头的片段。同步更新 `po/zh_CN.po` 与 `locale/zh_CN/LC_MESSAGES/bananatray.mo`。
- Extension 自有 UI 文案跟随 GNOME Shell / 系统 locale，不跟随 BananaTray 主应用语言设置；这是有意边界，不要在 Extension 端读取 app settings 或为此新增本地持久化配置。D-Bus 快照里的 provider / quota 文本由 daemon 按 app 当前语言生成，Extension 不做二次翻译。
- `OK` / `LOW` / `OUT` 等短 badge 文案需要保留 `# Translators:` 注释，说明它们属于 quota 状态语境。
- `St.ScrollView` 使用 `set_child()`，不要使用 GNOME 50 下会崩的 `add_actor()`。
- 修改 UI 后优先在 nested Shell 中验证实际加载状态，而不是只看主会话。
- 新增扩展资产时同步 `scripts/check-gnome-extension.sh`、`scripts/install-gnome-extension.sh` 和安装说明，避免用户安装时漏复制子目录。

## ZIP 打包与 e.g.o 发布

### 打包

```bash
bash scripts/bundle-gnome-extension.sh
```

默认输出到 `target/release/bundle/bananatray@bananatray.github.io-<version>.zip`。
可用 `--output DIR` 指定输出目录，`--check` 在打包前执行静态检查。

ZIP 只包含运行时文件（`metadata.json`、JS 模块、`stylesheet.css`、`locale/`、`icons/`），
不包含 `.po` 源文件、README、构建脚本或仓库元数据。

### metadata.json 版本管理

`metadata.json` 中有两个版本字段：

| 字段 | 类型 | 用途 |
|------|------|------|
| `version` | 整数 | e.g.o 必需；每次提交新版本时必须递增 |
| `version-name` | 字符串 | 人类可读的 semver 版本号 |

发布新版本时：
1. 递增 `version`（如 1 → 2）
2. 更新 `version-name`（如 `"1.0.0"` → `"1.1.0"`）
3. 运行 `bash scripts/bundle-gnome-extension.sh --check`
4. 上传 zip 到 https://extensions.gnome.org/upload/

### e.g.o 审核要点

- `shell-version` 只列出已测试的版本，不要超前声明
- 代码必须可读，不能混淆或 minify
- 不能包含遥测/追踪代码
- 不能在 `init()` 中执行 UI 修改（当前架构已满足，入口在 `enable/disable`）
- `metadata.json` 的 `url` 字段应指向公开仓库

## 验证清单

提交扩展相关改动前至少运行：

```bash
bash scripts/check-gnome-extension.sh
bash scripts/test-gnome-packaging-contracts.sh
bash scripts/install-gnome-extension.sh --dry-run
cargo fmt --check
cargo test --lib
cargo clippy
```

`check-gnome-extension.sh` 会在 `gjs` 可用时自动跑 GJS 真实 D-Bus 集成测试。也可单独运行：

```bash
bash scripts/test-gnome-extension-gjs.sh
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
| 弹窗一直显示 daemon not running | 扩展和 daemon 不在同一个 Session D-Bus，或当前安装方式没有安装 D-Bus activation 文件 | 用 nested child 进程的 `DBUS_SESSION_BUS_ADDRESS` 检查 D-Bus；正式安装包还要确认 `/usr/share/dbus-1/services/com.bananatray.Daemon.service` 存在。 |
| `--app-daemon` 显示空 provider | 真实配置目录未被传入，或当前设置没有启用 provider | 检查脚本输出中的日志路径和 provider 配置路径；确认真实 `settings.json`。 |
| 修改样式后没有变化 | 主会话 Wayland 不能热重启 Shell，或 nested Shell 没重启 | 关闭 nested Shell 后重新运行脚本；主会话需要注销重登。 |
| 顶栏出现传统 AppIndicator | 调试 app 没有设置 `BANANATRAY_FORCE_GNOME_EXTENSION=1` | 使用 `--app-daemon` 启动真实 app，避免手动漏环境变量。 |

## 已知待增强项

以下增强项来自原始预研计划（已归档为 `archive/gnome-shell-extension-plan.md`），当前实现不阻塞使用但值得后续完善：

- ~~**UI 表达增强**~~：已实现多配额 Provider 展开/折叠交互、header 状态徽章颜色编码（Synced/Syncing/Stale/Offline）、账户 tier 彩色 badge、footer 双按钮（Sync Data + Settings）、全宽进度条。仍可增强：趋势图、更细的错误恢复提示。
- ~~**GNOME Shell 集成测试**~~：Extension 已有运行时 schema guard、静态检查脚本和 CI 接入。`scripts/test-gnome-extension-gjs.sh` 在 `dbus-run-session` + GJS 里用真实 `Gio.DBusProxy` 验证 `quotaClient.js` 的 D-Bus 方法调用、`RefreshComplete` 信号订阅和 schema 校验（覆盖 Node mock 单测无法触及的真实 GJS + D-Bus 路径）。`check-gnome-extension.sh` 在 `gjs` 可用时自动调起，`ci.yml` 显式安装 gjs + dbus 跑该 测试。UI 层（`panelButton.js`/`quotaWidgets.js`）的 nested GNOME Shell 端到端测试仍是后续增强项。
- ~~**发布流程闭环**~~：已实现 `scripts/bundle-gnome-extension.sh` zip 打包和 e.g.o 发布元数据；版本矩阵验证仍需手动。
- **i18n 语言覆盖**：当前只有简体中文翻译，后续发布前可按目标用户补充更多 locale。
