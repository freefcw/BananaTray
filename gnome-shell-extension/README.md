# BananaTray GNOME Shell Extension

在 GNOME Shell 顶部面板显示 AI coding assistant 配额使用情况。

通过 D-Bus 与 BananaTray Rust daemon 通信，获取配额数据并展示在面板弹窗中。

## 兼容性

- **GNOME Shell 45/46/47/48/49/50**（ESM imports only）
- 依赖 BananaTray daemon 正在运行并提供 `com.bananatray.Daemon` D-Bus 服务

## 使用说明

扩展加载成功后，GNOME 顶栏右侧会出现 BananaTray 图标、彩色状态点和一段简短摘要：

- 绿色：所有已启用 Provider 当前没有明显配额风险。
- 黄色：至少一个 Provider 正在刷新、离线，或有低配额提醒。
- 红色：至少一个 Provider 已耗尽、出错且没有可展示的缓存数据，或处于严重状态。

点击顶栏入口会打开总览弹窗：

- 顶部显示 daemon 同步状态和刷新按钮。
- Summary 区域显示 Provider 总数、Connected 数量和 Attention 数量。
- Provider 行显示连接状态、账号信息、套餐信息、所有可见 quota、状态徽标和进度条。
- `Open Settings` 会通过 D-Bus 调用 BananaTray daemon，在主应用中打开设置窗口。

刷新按钮调用 daemon 的 `RefreshAll`。按钮会立即返回当前缓存快照，真实刷新完成后 daemon 会通过
`RefreshComplete` 信号推送新快照，扩展收到后自动更新。

### 数据来源

扩展本身不抓取任何 provider 数据，只消费 BananaTray Rust daemon 暴露的
`com.bananatray.Daemon` Session D-Bus 服务。因此正常使用时需要同时满足：

1. BananaTray 主程序正在同一用户会话中运行。
2. D-Bus 上存在 `com.bananatray.Daemon`。
3. 扩展状态是 `Enabled: Yes` 且 `State: ACTIVE`。

可用下面的命令确认真实数据是否已经可用：

```bash
gdbus call --session \
  --dest com.bananatray.Daemon \
  --object-path /com/bananatray/Daemon \
  --method com.bananatray.Daemon.GetAllQuotas
```

如果该命令返回真实 JSON，扩展就会显示同一份数据；如果命令失败，先启动或排查 BananaTray 主程序。

## 安装

### 用户安装目录（推荐，无需 root）

```bash
# 递归复制扩展文件、启用扩展，并输出当前 Shell 状态
bash scripts/install-gnome-extension.sh

# 重新加载 GNOME Shell
# Wayland: 注销并重新登录
# X11: Alt+F2 → 输入 'r' → 回车
```

脚本会安装到
`~/.local/share/gnome-shell/extensions/bananatray@bananatray.github.io/`，并检查
`i18n.js`、`panelButton.js`、`quotaClient.js`、`quotaWidgets.js`、`locale/zh_CN/LC_MESSAGES/bananatray.mo`
与 `icons/bananatray-symbolic.svg`
等必需文件是否已经复制。手工安装时必须递归复制整个
`gnome-shell-extension/` 目录，不能只复制顶层 `extension.js`、`metadata.json` 和 `stylesheet.css`。

只查看当前安装和 Shell 状态：

```bash
bash scripts/install-gnome-extension.sh --status
```

### 系统安装目录

```bash
sudo mkdir -p /usr/share/gnome-shell/extensions/bananatray@bananatray.github.io
sudo cp -a gnome-shell-extension/. /usr/share/gnome-shell/extensions/bananatray@bananatray.github.io/
```

### 验证安装

```bash
gnome-extensions list | grep bananatray
gnome-extensions info bananatray@bananatray.github.io
```

`gnome-extensions info` 必须同时显示：

- `Enabled: Yes`
- `State: ACTIVE`

如果显示 `State: OUT OF DATE`，说明当前 GNOME Shell 版本不在
`metadata.json` 的 `shell-version` 列表里，Shell 不会加载扩展。更新
`metadata.json` 后需要重新复制扩展文件，并在 Wayland 会话注销重登。

## D-Bus 接口

扩展通过 Session Bus 与 `com.bananatray.Daemon` 通信。

### 调用流程

```
扩展启动 → bus_watch_name("com.bananatray.Daemon")
         → daemon 出现 → 异步创建 DBusProxy → GetAllQuotasAsync() 获取初始数据
         → daemon 消失 → 显示 "daemon not running" 提示

刷新按钮 → RefreshAllAsync()（返回当前缓存快照 + 通知 GPUI 主线程异步刷新）
设置按钮 → OpenSettingsAsync()
刷新完成 → RefreshComplete 信号（携带新快照）→ 自动更新界面
```

### 接口定义

| 方法 / 信号 | 方向 | 数据格式 |
|-------------|------|---------|
| `GetAllQuotas` → `s` | 扩展 → daemon | JSON `DBusQuotaSnapshot` |
| `RefreshAll` → `s` | 扩展 → daemon | JSON `DBusQuotaSnapshot` |
| `OpenSettings` | 扩展 → daemon | 无参数 |
| `RefreshComplete(s)` | daemon → 扩展 | JSON `DBusQuotaSnapshot` |
| `IsActive` (property) | 扩展 → daemon | `boolean` |

完整 XML 接口定义见 `quotaClient.js` 中的 `DBUS_INTERFACE_XML`。

### Overview 同步

弹窗展示的是 daemon 推送的 `DBusQuotaSnapshot` 总览视图：

- 顶栏入口使用扩展自带的 `icons/bananatray-symbolic.svg`，旁边状态点取所有 Provider 的最差状态（Red > Yellow > Green），文字显示整体 OK 或最需要关注的 Provider / quota。
- 弹窗头部显示 daemon 的 `header.status_text`，并汇总 Provider 总数、Connected 数量、Refreshing / Error / Offline 状态。
- 每个 Provider 行同步 `display_name`、`connection`、`account_email`、`account_tier`、`worst_status` 和所有可见 `quotas`；quota 按严重度排序，显示 label、预计算 `display_text` 和进度条。
- quota 进度条优先使用 v1 内新增的可选 `bar_ratio` 字段，使 Remaining / Used 模式与主应用 Overview 保持一致；旧 daemon 未提供时，Extension 会用 `used / limit` 作为降级值。

### JSON 快照兼容规则

`DBusQuotaSnapshot` 顶层必须包含 `schema_version`。当前 Extension 只接受
`schema_version: 1`，并在渲染前校验最小必填字段；字段缺失、类型不匹配或版本不支持时会显示错误态并写入 GNOME Shell 日志。

同一版本内允许 daemon 新增字段，Extension 会忽略未知字段。删除字段、改名、改类型或改变枚举字符串语义时必须提升 `schema_version`，并同步更新 Extension 校验逻辑。

## 文件说明

| 文件 | 职责 |
|------|------|
| `extension.js` | 扩展主入口：`BananaTrayExtension` 的 `enable/disable` 生命周期和 GNOME 面板注册 |
| `i18n.js` | Extension gettext 包装：所有 GNOME Shell UI 文案统一通过 `_()` 翻译 |
| `panelButton.js` | `BananaTrayIndicator`：PanelMenu.Button、弹窗装配、`QuotaClient` 回调和整体 UI 状态切换 |
| `quotaClient.js` | D-Bus client：接口 XML、proxy 生命周期、异步方法调用、`RefreshComplete` 监听、JSON schema guard |
| `quotaPresentation.js` | 展示层纯函数：状态归一化、Provider/quota 排序、顶栏摘要聚合 |
| `quotaWidgets.js` | 可复用 UI 组件：Provider 行、Quota 行、quota bar、状态点和文本 label helper |
| `metadata.json` | GNOME Shell 扩展元数据：UUID、名称、Shell 版本兼容性和 `gettext-domain` |
| `po/zh_CN.po` | 简体中文翻译源文件 |
| `locale/zh_CN/LC_MESSAGES/bananatray.mo` | GNOME Shell 运行时加载的简体中文 gettext 编译文件 |
| `stylesheet.css` | 弹窗样式：状态点颜色、Provider 行、头部/底部、滚动区域、加载/错误状态 |
| `icons/bananatray-symbolic.svg` | GNOME 顶栏使用的 symbolic 图标；安装/调试复制扩展时必须包含子目录 |

## 架构

### 组件层次

```
BananaTrayExtension (入口)
  └─ BananaTrayIndicator (PanelMenu.Button)
       ├─ Panel icon + 状态点 + 动态摘要
       ├─ Popup Menu:
       │    ├─ Header (图标 + 状态文本 + 刷新按钮)
       │    ├─ Summary (Provider / Connected / Attention)
       │    ├─ ScrollView → ProviderList → BananaTrayProviderRow × N
       │    │    └─ BananaTrayQuotaRow × N
       │    ├─ Loading placeholder (等待 daemon)
       │    └─ Footer (Open Settings 按钮)
       └─ QuotaClient (异步 D-Bus + schema guard)

支撑模块：
  ├─ quotaPresentation.js (纯展示数据整理)
  └─ quotaWidgets.js (Provider / quota 行组件)
```

### 数据流

1. 扩展启动时 watch `com.bananatray.Daemon` bus name
2. daemon 出现 → 异步创建 `Gio.DBusProxy` → 调用 `GetAllQuotasAsync` 获取初始数据
3. 连接 `RefreshComplete` 信号 → daemon 每次刷新完成后自动推送数据
4. 刷新按钮 → 调用 `RefreshAllAsync`（触发刷新 + 返回当前快照）
5. 设置按钮 → 调用 `OpenSettingsAsync`（daemon 侧在 GPUI 主线程打开设置窗口）

### 状态点颜色规则

面板状态点和每行左侧的状态点颜色由 `worst_status` / 连接状态决定：

| worst_status | 颜色 |
|-------------|------|
| `Green` | `#4caf50` |
| `Yellow` | `#ff9800` |
| `Red` | `#f44336` |

面板状态点取所有 Provider 中最差状态（Red > Yellow > Green）。如果 Provider 正在
`Refreshing` 或 `Disconnected`，扩展以 Yellow 提醒；如果 `Error` 且没有缓存配额，以
Red 提醒；如果 `Error` 但仍有缓存配额，仍展示缓存 quota，并在账号信息行标注
`Cached data`。

## 开发

### 静态检查

```bash
./scripts/check-gnome-extension.sh
```

该检查会确认扩展必需文件存在，禁止同步 D-Bus 调用回归，确认
入口通过 `panelButton.js` 装配 `QuotaClient`，并在本机有 `node` 时对所有扩展
ES module 执行语法检查。若本机有 `msgfmt` / `xgettext` / `msgcmp`，还会校验
`po/zh_CN.po` 语法、`bananatray.mo` 是否由最新 `.po` 编译而来，以及 `_()` / `ngettext()`
文案是否都已进入翻译源。

### i18n

Extension 使用 `metadata.json` 中的 `gettext-domain: "bananatray"` 和本地
`locale/<lang>/LC_MESSAGES/bananatray.mo`。新增用户可见文案时：

1. 在 JS 中通过 `i18n.js` 导出的 `_()` 包裹普通文案，带数量的文案使用 `ngettext()`。
2. 同步更新 `po/zh_CN.po`。
3. 对 `OK` / `LOW` / `OUT` 这类短标签保留 `# Translators:` 语境注释，避免翻译者误解。
4. 重新编译运行时翻译文件：

```bash
msgfmt --check \
  --output-file=gnome-shell-extension/locale/zh_CN/LC_MESSAGES/bananatray.mo \
  gnome-shell-extension/po/zh_CN.po
```

D-Bus 快照里来自 daemon 的 `display_name`、quota `label` 和 `display_text` 不在 Extension
端二次翻译，由 daemon 侧保持一致语义。

### Nested Shell 调试（推荐）

Wayland 主会话不能在线重启 GNOME Shell。扩展 UI 开发建议使用 nested
GNOME Shell，它运行在独立窗口和独立 D-Bus session 里，不影响当前桌面：

```bash
bash scripts/dev-gnome-extension.sh
```

GNOME Shell 49+ 的 `--devkit` 模式需要 Mutter Development Kit 才会出现
可见窗口。Ubuntu / Debian 上如脚本提示缺少 `mutter-devkit`，先安装：

```bash
sudo apt install mutter-dev-bin
```

默认会：

1. 创建临时 GNOME profile
2. 把 `gnome-shell-extension/` 复制到临时扩展目录
3. 在临时 dconf profile 中启用 `bananatray@bananatray.github.io`
4. 启动 mock `com.bananatray.Daemon`
5. 运行 `gnome-shell --devkit --wayland --no-x11`

扩展加载成功后，nested Shell 顶栏右侧会出现 BananaTray 图标、彩色状态点和总览摘要。
默认 mock daemon 会轮转多个 Provider 状态，便于检查 Overview 同步、错误态、刷新态和
断开态。

要用真实 BananaTray 数据调试扩展，使用：

```bash
bash scripts/dev-gnome-extension.sh --app-daemon
```

该模式会在 nested D-Bus session 中启动真实 BananaTray（默认 `cargo run`），但保留你当前终端的真实
`XDG_CONFIG_HOME` / `XDG_DATA_HOME` / `XDG_CACHE_HOME` / `XDG_STATE_HOME`，因此会读取实际
`settings.json`、自定义 Provider 和 provider 凭据。脚本会设置
`BANANATRAY_SINGLE_INSTANCE_SUFFIX=gnome-dev`，所以不会和主会话里已经运行的 BananaTray
抢同一个单实例锁；同时设置 `BANANATRAY_FORCE_GNOME_EXTENSION=1`，避免真实 app 在 nested
Shell 尚未完全启动时注册传统 AppIndicator fallback。

常用参数：

```bash
# 按需显式增加 nested virtual monitor
bash scripts/dev-gnome-extension.sh --monitor 1600x1000

# 复用同一个临时 profile，保留 Shell 设置和扩展状态
bash scripts/dev-gnome-extension.sh --profile-dir /tmp/bananatray-gnome-profile

# 使用 release 构建启动真实 app daemon
bash scripts/dev-gnome-extension.sh --app-command 'cargo run --release'

# 不启用 mock daemon，改为你自己在 nested D-Bus session 中启动 BananaTray
bash scripts/dev-gnome-extension.sh --real-daemon
```

修改 `extension.js`、`panelButton.js`、`quotaPresentation.js`、`quotaWidgets.js`、
`stylesheet.css` 或 `metadata.json` 后，关闭这个 nested
Shell 窗口并重新运行脚本即可；不需要注销当前桌面。

如果看不到 BananaTray 图标或摘要，先看脚本终端里是否有 `Extension bananatray...` 错误。
也可以在脚本打印的 profile 路径对应的 nested D-Bus session 中检查
`GetExtensionInfo`，正常状态应包含 `enabled: true` 且 `error: ''`。

### 调试

```bash
# 查看 GNOME Shell 日志
journalctl -f -o cat | grep BananaTray

# 手动调用 D-Bus 方法
gdbus call --session --dest com.bananatray.Daemon \
  --object-path /com/bananatray/Daemon \
  --method com.bananatray.Daemon.GetAllQuotas

# 检查 daemon 是否在 bus 上
gdbus introspect --session --dest com.bananatray.Daemon \
  --object-path /com/bananatray/Daemon
```

### 修改后重载

1. 修改扩展 JS 模块或 `stylesheet.css`
2. 复制更新后的文件到扩展目录
3. 重新加载 GNOME Shell（X11: Alt+F2 → `r`；Wayland: 注销重登）
4. 或使用 `gnome-extensions disable/enable` 切换

## 排障

| 问题 | 原因 | 解决方案 |
|------|------|---------|
| 面板无 BananaTray 图标 | 扩展未启用、未重载，或 `State` 不是 `ACTIVE` | `bash scripts/install-gnome-extension.sh --status` 检查安装文件和 Shell 状态；若是 `OUT OF DATE`，更新 `metadata.json` 后重新安装并重载 Shell |
| 面板同时出现 BananaTray 图标和三个点 | daemon 版本仍在 Extension 模式下注册了传统 KSNI/AppIndicator 空入口 | 更新并重启 BananaTray daemon；确认日志包含 `skipping GPUI tray bootstrap`，且 `RegisteredStatusNotifierItems` 不再出现 BananaTray 进程对应项 |
| 弹窗背景透明、文字难以辨认 | 扩展覆盖了 GNOME Shell 默认 popup menu 样式类，导致主题背景未生效 | 菜单容器只能追加 `bananatray-menu-box`，不能替换默认样式类；重新安装扩展并重载 Shell |
| `State: ERROR` 且错误含 `add_actor is not a function` | GNOME 50 仍在加载旧版扩展，旧版 `St.ScrollView.add_actor()` API 已失效 | 运行 `bash scripts/install-gnome-extension.sh` 递归安装新版文件；若安装文件已无 `add_actor` 但 Shell 仍报旧错，Wayland 注销重登，X11 用 Alt+F2 → `r` 重启 Shell |
| `State: ERROR` 且提示找不到某个 `.js` 模块或图标 | 安装时漏复制子文件或 `icons/` 子目录 | 运行 `bash scripts/install-gnome-extension.sh`，或手工递归复制整个 `gnome-shell-extension/` 目录 |
| "Waiting for BananaTray daemon…" | daemon 未运行或 D-Bus 服务未注册 | 确认 `bananatray` 进程正在运行；`gdbus introspect` 检查 bus |
| "Failed to fetch quota data" | D-Bus 调用失败 | 检查 journalctl 日志；确认 daemon 版本匹配 |
| 刷新后数据不更新 | `RefreshComplete` 信号未收到 | 检查 daemon 是否正确发射信号；查看 journalctl |
