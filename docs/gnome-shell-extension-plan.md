# GNOME Shell Extension 方案与实现对照

> 状态：**已部分落地，仍有增强项**。本文最初是 GNOME / Mutter 不支持 layer-shell 时的技术预研；当前仓库已经落地了 Rust D-Bus 服务与 GNOME Shell Extension，因此本文改为记录"原计划 vs 当前实现"的差异、取舍和后续缺口。
>
> 触发背景：[gpui-linux-rendering-investigation.md](gpui-linux-rendering-investigation.md) 中调研 layer-shell 时发现 GNOME 这条路走不通，需要替代方案。

## 1. GNOME 对 layer-shell 的现状（2026 年）

**结论：GNOME / Mutter 原生不支持 `wlr-layer-shell-unstable-v1`，且 BananaTray 不能依赖 layer-shell 在 GNOME 上实现贴面板弹窗。**

依据：

- Mutter [issue #973](https://gitlab.gnome.org/GNOME/mutter/-/issues/973)：维护者 Jonas Ådahl 明确表态：
  - GNOME Shell 自身的面板/通知用的是私有协议，不会改用 layer-shell。
  - Mutter 不打算支持任意第三方 panel/dock，因为这与 GNOME 设计哲学冲突。
  - 唯一方向是让 libmutter 的下游使用者自行实现私有 Wayland 协议。
- 协议支持表 [absurdlysuspicious.github.io/wayland-protocols-table](https://absurdlysuspicious.github.io/wayland-protocols-table/)：Mutter 一栏 `wlr-layer-shell` 为 ✗。
- KDE / Hyprland / Sway / COSMIC / niri / Wayfire 等可作为非 GNOME 桌面的 layer-shell 目标。

## 2. 当前落地架构

核心思路已经落地为：**Rust 桌面应用提供数据与控制入口，GNOME Shell Extension 负责面板 UI，两者通过 Session D-Bus 通信。**

```text
BananaTray Rust app                       GNOME Shell Extension
  src/dbus/                                 gnome-shell-extension/
  - zbus ObjectServer                       - PanelMenu.Button 面板入口
  - com.bananatray.Daemon                   - PopupMenu 配额列表
  - /com/bananatray/Daemon                  - Gio.DBusProxy 客户端
  - JSON DBusQuotaSnapshot                  - RefreshComplete 信号刷新 UI
```

当前 D-Bus 契约以 [src/dbus/README.md](../src/dbus/README.md) 为准：

- Bus name：`com.bananatray.Daemon`
- Object path：`/com/bananatray/Daemon`
- Interface：`com.bananatray.Daemon`
- Methods：
  - `GetAllQuotas() -> s`
  - `RefreshAll() -> s`
  - `OpenSettings()`
- Signal：
  - `RefreshComplete(s)`
- Property：
  - `IsActive: b`

JSON 字符串是 `DBusQuotaSnapshot` 的序列化结果。DTO 定义在 `application::selectors::dbus_dto`，`src/dbus/serde_types.rs` 仅做 re-export。
快照顶层包含 `schema_version: 1`，Extension 会在渲染前做最小 schema 校验；同版本内允许新增字段，删除/改名/改类型必须提升版本。

## 3. 原计划与当前实现对照

| 计划项 | 当前实现 | 状态 |
|---|---|---|
| Rust 端提供 D-Bus 服务 | 已在 `src/dbus/` 实现，Linux + `app` 路径下使用 zbus 注册服务 | 已实现 |
| 使用 `org.bananatray.Quota1` / `org.bananatray.Quota` | 当前实际接口为 `com.bananatray.Daemon`，对象路径 `/com/bananatray/Daemon` | 已替代，需以当前接口为准 |
| 直接传 `ProviderSnapshot` 并派生 `zbus::Type` | 当前传输 JSON `DBusQuotaSnapshot`，GJS 端解析简单，`gdbus` 调试直观 | 已替代 |
| `GetSnapshot` / `Refresh` / `OpenSettings` / `Updated` | 当前为 `GetAllQuotas` / `RefreshAll` / `OpenSettings` / `RefreshComplete` | 已实现但命名不同 |
| 新增 `linux-dbus` feature | 未新增独立 feature；`zbus` 只在非 macOS target dependency 中声明，模块由 Linux + `app` 使用 | 已用现有 feature 策略覆盖 |
| GNOME 上跳过传统托盘绘制 | 当前 GNOME Extension 已启用且 `State: ACTIVE` 时完全跳过 GPUI/KSNI 托盘 bootstrap、点击回调和菜单安装，避免空 StatusNotifierItem 占位；`OUT OF DATE` / 加载失败时保留传统托盘 fallback | 已实现 |
| Extension 使用 `PanelMenu.Button` | `gnome-shell-extension/extension.js` 已实现 `BananaTrayIndicator` | 已实现 |
| Extension 监听 daemon 上线/下线 | 使用 `Gio.bus_watch_name()` 监听 `com.bananatray.Daemon` | 已实现 |
| 初始快照读取 | 使用异步 proxy 构造后调用 `GetAllQuotasAsync()` | 已实现 |
| 手动刷新 | 刷新按钮调用 `RefreshAllAsync()`，返回当前缓存并等待后续 `RefreshComplete` 推送 | 已实现 |
| 打开设置窗口 | Footer 按钮调用 `OpenSettingsAsync()`，Rust 侧转发到 GPUI 主线程 | 已实现 |
| 实时刷新 | Extension 连接 `RefreshComplete` 信号并重建 Provider 行 | 已实现 |
| Popup 内用量条形图 | 当前是状态点 + Provider 名称 + 主配额文本，尚未画条形图 | 待增强 |
| 拆分 `panelButton.js` / `quotaClient.js` | D-Bus/protocol 已拆到 `quotaClient.js`；UI row/panel 仍在 `extension.js` | 部分实现 |
| systemd user service + D-Bus activation | 当前应用运行时主动 request name，Extension 只 watch name；未提供 DBus activation 文件 | 待增强 |
| 打包发布到 e.g.o / zip | 当前有 `metadata.json` 和 README 安装说明，未提供打包脚本和 e.g.o 发布清单 | 待增强 |
| Extension 端 i18n | 当前 Extension 文案仍是英文硬编码 | 待增强 |
| 图标资源复用 | 当前面板入口使用状态点，不加载 `src/icons/` SVG | 待增强 |

## 4. 本次审计后的修正

- GJS 端 D-Bus proxy 构造改为异步，避免 GNOME Shell 主线程在 daemon 响应慢或异常时被同步调用阻塞。
- `GetAllQuotas` / `RefreshAll` / `OpenSettings` 改为 `Async` 方法调用，并在 daemon 消失或扩展销毁后丢弃过期回调结果。
- Extension 销毁或 daemon 下线时显式 `disconnectSignal()`，避免重复连接和泄漏。
- Linux GNOME Extension 模式下，Rust 侧不再调用 GPUI tray API，避免 GNOME 面板同时显示 Extension 图标和空 AppIndicator 占位。
- D-Bus JSON 快照补充 `schema_version`，Extension 在渲染前拒绝不支持版本或缺少必填字段的数据。
- Rust 端 Extension 模式检测改为要求 `gnome-extensions info` 同时满足 `Enabled: Yes` 和 `State: ACTIVE`；扩展 `OUT OF DATE` 或加载失败时继续保留 KSNI/AppIndicator fallback，避免面板入口完全消失。
- 扩展元数据声明兼容 GNOME Shell 45-50。

## 5. 仍需完善的问题

- **UI 表达仍偏 MVP**：当前只显示主配额文本，未实现计划中的条形图或多配额展开。
- **Extension UI 组件仍集中在 `extension.js`**：D-Bus client 已拆出，但随着图表、i18n、错误态继续增长，应继续拆出 row/panel 组件。
- **启动激活未完成**：还没有 systemd user service / D-Bus activation 文件，daemon 不运行时扩展只能显示等待状态。
- **GJS 缺少 GNOME Shell 集成测试**：Extension 已有运行时 schema guard、静态检查脚本和 CI 接入，但还没有真正启动 GNOME Shell 的自动化测试路径。
- **发布流程未闭环**：还没有 zip 打包、版本矩阵验证和 e.g.o 审核材料。
- **i18n 未覆盖 Extension**：GNOME Shell UI 文案仍未接入独立 gettext domain。

## 6. 推荐后续顺序

1. 拆出 row/panel 组件，让 `extension.js` 只保留生命周期装配。
2. 在 PopupMenu 中增加配额条形图，同时保留当前文本作为可读 fallback。
3. 增加 systemd user service / D-Bus activation 示例，解决 daemon 未运行时的启动体验。
4. 为 Extension 增加 i18n 和打包脚本，再评估 e.g.o 发布。
5. 评估 nested GNOME Shell 自动化测试，覆盖 Extension 加载和 mock daemon 数据刷新。

## 7. 关联文档

- [gnome-shell-extension/README.md](../gnome-shell-extension/README.md) — Extension 安装、D-Bus 通信流程和排障指南。
- [src/dbus/README.md](../src/dbus/README.md) — Rust D-Bus 服务线程模型、接口契约和 JSON 快照格式。
- [architecture.md](architecture.md) — 稳定架构边界。
- [gpui-linux-rendering-investigation.md](gpui-linux-rendering-investigation.md) — Linux 渲染问题原始调研。
