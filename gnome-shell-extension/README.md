# BananaTray GNOME Shell Extension

在 GNOME Shell 顶部面板显示 AI coding assistant 配额使用情况。

通过 D-Bus 与 BananaTray Rust daemon 通信，获取配额数据并展示在面板弹窗中。

## 兼容性

- **GNOME Shell 45/46/47/48/49/50**（ESM imports only）
- 依赖 BananaTray daemon 正在运行，或当前 Linux 安装已提供 `com.bananatray.Daemon` D-Bus activation

## 使用说明

扩展加载成功后，GNOME 顶栏右侧会出现 BananaTray 图标、彩色状态点和一段简短摘要。
状态点和摘要固定跟随 BananaTray 设置页中排序第一的已启用 Provider；要切换顶栏显示哪个
Provider，在主应用的 Provider 设置页拖拽调整排序即可。

- 绿色：排序第一的 Provider 当前没有明显配额风险。
- 黄色：排序第一的 Provider 正在刷新、离线，或有低配额提醒。
- 红色：排序第一的 Provider 已耗尽、出错且没有可展示的缓存数据，或处于严重状态。

点击顶栏入口会打开总览弹窗：

- 顶部右侧显示 daemon 同步状态，标题副文本显示紧凑 Provider 摘要。
- Provider 行显示连接状态、账号信息、套餐信息、所有可见 quota、状态徽标和进度条。
- `Sync Data` 调用 daemon 刷新缓存，`Settings` 会通过 D-Bus 调用 BananaTray daemon，
  在主应用中打开设置窗口。

刷新按钮调用 daemon 的 `RefreshAll`。按钮会立即返回当前缓存快照，真实刷新完成后 daemon 会通过
`RefreshComplete` 信号推送新快照，扩展收到后自动更新。

### 数据来源

扩展本身不抓取任何 provider 数据，只消费 BananaTray Rust daemon 暴露的
`com.bananatray.Daemon` Session D-Bus 服务。因此正常使用时需要同时满足：

1. BananaTray 主程序正在同一用户会话中运行，或安装包已安装 D-Bus activation 文件。
2. D-Bus 上存在 `com.bananatray.Daemon`，或 Session Bus 能激活该服务。
3. 扩展状态是 `Enabled: Yes` 且 `State: ACTIVE`。

可用下面的命令确认真实数据是否已经可用：

```bash
gdbus call --session \
  --dest com.bananatray.Daemon \
  --object-path /com/bananatray/Daemon \
  --method com.bananatray.Daemon.GetAllQuotas
```

如果该命令返回真实 JSON，扩展就会显示同一份数据。通过 deb/rpm 安装时，该调用也可以触发
D-Bus activation；如果命令失败，先确认 BananaTray 主程序或 activation 文件是否已经安装。

## 安装

### 从 extensions.gnome.org 安装（推荐）

访问 [BananaTray on e.g.o](https://extensions.gnome.org/) 搜索 **BananaTray**，
点击开关启用即可。GNOME Shell 会自动下载、安装和管理更新。

### 从 zip 文件安装

如果你拿到的是发布的 `.zip` 文件（来自 GitHub Release 或手动打包）：

```bash
gnome-extensions install bananatray@bananatray.github.io-1.0.0.zip

# 重新加载 GNOME Shell
# Wayland: 注销并重新登录
# X11: Alt+F2 → 输入 'r' → 回车

# 启用扩展
gnome-extensions enable bananatray@bananatray.github.io
```

`gnome-extensions install` 会将 zip 解压到
`~/.local/share/gnome-shell/extensions/bananatray@bananatray.github.io/`。
如果之前已安装旧版本，加 `--force` 覆盖：

```bash
gnome-extensions install --force bananatray@bananatray.github.io-1.1.0.zip
```

> **注意**：Extension zip 只包含 GNOME Shell 面板 UI（JS、CSS、翻译和图标），
> 不包含 BananaTray Rust 可执行文件。扩展通过 D-Bus 从 daemon 获取配额数据，
> 因此还需要从 [GitHub Releases](https://github.com/freefcw/BananaTray/releases)
> 下载并安装 BananaTray 主程序（deb/rpm/AppImage）。
> 详见下方"[数据来源](#数据来源)"。

### 从源码安装（开发者，无需 root）

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

Linux deb/rpm 安装包会同时安装：

- `/usr/share/dbus-1/services/com.bananatray.Daemon.service`
- `/usr/lib/systemd/user/bananatray.service`

这两个文件让 Session Bus 在扩展启动、刷新或 `gdbus call` 访问
`com.bananatray.Daemon` 时自动启动 `/usr/bin/bananatray`。从源码只运行
`scripts/install-gnome-extension.sh` 时不会写入系统 activation 文件，仍需要手动启动 `cargo run`
或安装 deb/rpm 打包产物。AppImage 不安装到宿主 D-Bus 搜索路径，因此不提供 D-Bus activation。

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

## 打包发布

生成符合 [extensions.gnome.org](https://extensions.gnome.org/) 提交要求的 zip 文件：

```bash
bash scripts/bundle-gnome-extension.sh
```

输出到 `target/release/bundle/bananatray@bananatray.github.io-<version>.zip`。
可用 `--check` 在打包前执行静态检查，`--output DIR` 指定输出目录。

发布新版本时，先递增 `metadata.json` 中的整数 `version` 字段，再运行打包脚本，
最后上传 zip 到 https://extensions.gnome.org/upload/ 。

维护者指南详见 `docs/gnome-shell-extension-development.md`。

## D-Bus 接口

扩展通过 Session Bus 与 `com.bananatray.Daemon` 通信。

### 调用流程

```
扩展启动 → bus_watch_name("com.bananatray.Daemon") + StartServiceByName("com.bananatray.Daemon")
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

- 顶栏入口使用扩展自带的 `icons/bananatray-symbolic.svg`，旁边状态点和文字都跟随排序第一的已启用 Provider，显示它的首要 quota/连接状态。
- 弹窗头部显示 daemon 的 `header.status_text`，并汇总 Provider 总数、Connected 数量、Refreshing / Error / Offline 状态。
- 每个 Provider 行同步 `display_name`、`connection`、`account_email`、`account_tier`、`worst_status` 和所有可见 `quotas`；行内左侧固定为 Provider 身份区，右侧固定为 tier / 状态 / 展开按钮列，避免不同 Provider 状态导致视觉跳动。
- quota 按严重度排序，显示 label、预计算 `display_text` 和进度条；数值列固定右对齐，折叠态的 `+N` 额外配额数量也落在同一列内。
- quota 进度条优先使用 v1 内新增的可选 `bar_ratio` 字段，使 Remaining / Used 模式与主应用 Overview 保持一致；旧 daemon 未提供时，Extension 会用 `used / limit` 作为降级值。填充色使用与主应用相同的渐变语义：紫蓝起点过渡到当前状态色。

### JSON 快照兼容规则

`DBusQuotaSnapshot` 顶层必须包含 `schema_version`。当前 Extension 只接受
`schema_version: 1`，并在渲染前校验最小必填字段；字段缺失、类型不匹配或版本不支持时会显示错误态并写入 GNOME Shell 日志。
`connection`、`worst_status` 和 quota `status_level` 的未知枚举值会写入日志，状态等级未知时按 Yellow 渲染，
用于暴露同版本内的 daemon / Extension 协议漂移。

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
       │    ├─ Header (图标 + Provider 摘要 + daemon 同步状态)
       │    ├─ ScrollView → ProviderList → BananaTrayProviderRow × N
       │    │    └─ BananaTrayQuotaRow × N
       │    ├─ Loading placeholder (等待 daemon)
       │    └─ Footer (Sync Data + Settings 按钮)
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

### 状态与摘要规则

面板状态点颜色由排序第一的已启用 Provider 的 `worst_status` / 连接状态决定：

| worst_status | 颜色 |
|-------------|------|
| `Green` | `#4caf50` |
| `Yellow` | `#ff9800` |
| `Red` | `#f44336` |

如果 Provider 正在 `Refreshing` 或 `Disconnected`，扩展以 Yellow 提醒；如果 `Error` 且没有
缓存配额，以 Red 提醒；如果 `Error` 但仍有缓存配额，仍展示缓存 quota，并在账号信息行标注
`Cached data`。

弹窗 Header 右侧徽章展示 daemon 返回的全局同步状态（如 `Synced` / `Syncing` / `Offline`）；
标题副文本展示紧凑 Provider 摘要（总数、已连接数，以及仅在非零时追加 refreshing / error /
offline）。Provider 行自身不再显示左侧状态点，正常配额用进度条和 `OK` / `LOW` / `OUT`
徽章表达，非 connected 状态用连接状态徽章表达。

Footer 中 `Sync Data` 默认为次级按钮；仅当 Header 状态为 `Syncing` / `Stale` /
`Offline` 时提升为蓝色或红色强调态。`Settings` 保持轻量按钮样式。

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
`locale/<lang>/LC_MESSAGES/bananatray.mo`，语言跟随 GNOME Shell / 系统 locale，
不读取 BananaTray 主应用的 `settings.display.language`。这是有意的边界：Extension 属于
Shell UI，daemon 未运行时也必须能用 Shell 的语言显示基础状态。

D-Bus 快照里来自 daemon 的 `display_name`、quota `label` 和 `display_text` 不在 Extension
端二次翻译，由 daemon 按主应用当前语言生成。因此用户可能看到：

- Extension 自有按钮、错误、Summary 等文案跟随 GNOME Shell / 系统语言。
- Provider 名称、quota 标签和 quota 数值文本跟随 BananaTray 主应用语言。

新增用户可见文案时：

1. 在 JS 中通过 `i18n.js` 导出的 `_()` 包裹普通文案，带数量的文案使用 `ngettext()`。
2. 同步更新 `po/zh_CN.po`。
3. 对 `OK` / `LOW` / `OUT` 这类短标签保留 `# Translators:` 语境注释，避免翻译者误解。
4. 重新编译运行时翻译文件：

```bash
msgfmt --check \
  --output-file=gnome-shell-extension/locale/zh_CN/LC_MESSAGES/bananatray.mo \
  gnome-shell-extension/po/zh_CN.po
```

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

推荐使用 watch 脚本自动热重载（GNOME 45+ ESM 支持 disable/enable 重载模块）：

```bash
bash scripts/dev-gnome-extension-watch.sh
```

手动重载：

1. 修改扩展 JS 模块或 `stylesheet.css`
2. 复制更新后的文件到扩展目录
3. `gnome-extensions disable bananatray@bananatray.github.io && gnome-extensions enable bananatray@bananatray.github.io`
4. 如果 disable/enable 未生效：X11 用 Alt+F2 → `r`；Wayland 注销重登

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
