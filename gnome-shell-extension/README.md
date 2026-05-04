# BananaTray GNOME Shell Extension

在 GNOME Shell 顶部面板显示 AI coding assistant 配额使用情况。

通过 D-Bus 与 BananaTray Rust daemon 通信，获取配额数据并展示在面板弹窗中。

## 兼容性

- **GNOME Shell 45/46/47/48**（ESM imports only）
- 依赖 BananaTray daemon 正在运行并提供 `com.bananatray.Daemon` D-Bus 服务

## 安装

### 用户安装目录（推荐，无需 root）

```bash
# 复制扩展文件到用户扩展目录
mkdir -p ~/.local/share/gnome-shell/extensions/bananatray@bananatray.github.io
cp gnome-shell-extension/* ~/.local/share/gnome-shell/extensions/bananatray@bananatray.github.io/

# 重新加载 GNOME Shell
# Wayland: 注销并重新登录
# X11: Alt+F2 → 输入 'r' → 回车

# 启用扩展
gnome-extensions enable bananatray@bananatray.github.io
```

### 系统安装目录

```bash
sudo mkdir -p /usr/share/gnome-shell/extensions/bananatray@bananatray.github.io
sudo cp gnome-shell-extension/* /usr/share/gnome-shell/extensions/bananatray@bananatray.github.io/
```

### 验证安装

```bash
gnome-extensions list | grep bananatray
gnome-extensions info bananatray@bananatray.github.io
```

## D-Bus 接口

扩展通过 Session Bus 与 `com.bananatray.Daemon` 通信。

### 调用流程

```
扩展启动 → bus_watch_name("com.bananatray.Daemon")
         → daemon 出现 → 创建 DBusProxy → GetAllQuotasSync() 获取初始数据
         → daemon 消失 → 显示 "daemon not running" 提示

刷新按钮 → RefreshAllSync()（返回当前缓存快照 + 通知 GPUI 主线程异步刷新）
设置按钮 → OpenSettingsSync()
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

完整 XML 接口定义见 `extension.js` 中的 `DBUS_INTERFACE_XML`。

## 文件说明

| 文件 | 职责 |
|------|------|
| `extension.js` | 扩展主入口：`BananaTrayIndicator`（PanelMenu.Button）+ `BananaTrayProviderRow`（行组件）+ `BananaTrayExtension`（enable/disable 生命周期） |
| `metadata.json` | GNOME Shell 扩展元数据：UUID、名称、Shell 版本兼容性 |
| `stylesheet.css` | 弹窗样式：状态点颜色、Provider 行、头部/底部、滚动区域、加载/错误状态 |

## 架构

### 组件层次

```
BananaTrayExtension (入口)
  └─ BananaTrayIndicator (PanelMenu.Button)
       ├─ _iconBin (面板状态点 — 显示 worst_status 颜色)
       ├─ Popup Menu:
       │    ├─ Header (标题 + 状态文本 + 刷新按钮)
       │    ├─ ScrollView → ProviderList → BananaTrayProviderRow × N
       │    ├─ Loading placeholder (等待 daemon)
       │    └─ Footer (Open Full View 按钮)
       └─ D-Bus Proxy (异步通信)
```

### 数据流

1. 扩展启动时 watch `com.bananatray.Daemon` bus name
2. daemon 出现 → 创建 `Gio.DBusProxy` → 同步调用 `GetAllQuotas` 获取初始数据
3. 连接 `RefreshComplete` 信号 → daemon 每次刷新完成后自动推送数据
4. 刷新按钮 → 同步调用 `RefreshAll`（触发刷新 + 返回当前快照）
5. 设置按钮 → 同步调用 `OpenSettings`（daemon 侧在 GPUI 主线程打开设置窗口）

### 状态点颜色规则

面板图标和每行左侧的状态点颜色由 `worst_status` 决定：

| worst_status | 颜色 |
|-------------|------|
| `Green` | 🟢 `#4caf50` |
| `Yellow` | 🟡 `#ff9800` |
| `Red` | 🔴 `#f44336` |

面板图标取所有 Provider 中最差状态（Red > Yellow > Green）。

## 开发

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

1. 修改 `extension.js` 或 `stylesheet.css`
2. 复制更新后的文件到扩展目录
3. 重新加载 GNOME Shell（X11: Alt+F2 → `r`；Wayland: 注销重登）
4. 或使用 `gnome-extensions disable/enable` 切换

## 排障

| 问题 | 原因 | 解决方案 |
|------|------|---------|
| 面板无 BananaTray 图标 | 扩展未启用或 GNOME Shell 未重载 | `gnome-extensions enable bananatray@bananatray.github.io` 然后重载 |
| "Waiting for BananaTray daemon…" | daemon 未运行或 D-Bus 服务未注册 | 确认 `bananatray` 进程正在运行；`gdbus introspect` 检查 bus |
| "Failed to fetch quota data" | D-Bus 调用失败 | 检查 journalctl 日志；确认 daemon 版本匹配 |
| 刷新后数据不更新 | `RefreshComplete` 信号未收到 | 检查 daemon 是否正确发射信号；查看 journalctl |
