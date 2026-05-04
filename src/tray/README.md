# src/tray/

系统托盘模块，管理托盘图标、弹窗窗口生命周期和多显示器定位。

## 模块结构

### `controller.rs` — TrayController

弹窗窗口的生命周期管理：

- **`TrayController`** — 持有 `AppState`（`Rc<RefCell<...>>`）、refresh channel、log path
- 管理弹窗的创建、显示/隐藏、定位
- 处理托盘点击事件和全局快捷键
- 弹窗句柄通过共享 `Cell<Option<WindowHandle<_>>>` 保存，失焦 auto-hide 依靠幂等守卫清理当前窗口，避免 stale handle 和误关新窗口
- Linux 下 auto-hide / toggle 复用同一窗口；当用户已经拖动过或已有保存位置时，隐藏会优先切到透明渲染并启用鼠标穿透，避免 Wayland `hide_window()`/`show_window()` 重新映射后回到屏幕中央

**托盘交互入口**：

- 使用 `on_tray_icon_click_event` 注册点击回调（替代 `on_tray_icon_event`），获取 `TrayIconClickEvent`（含 kind + 可选 position）
- macOS 启动时必须通过 GPUI `set_tray_panel_mode(true)` 切到 panel callback 模式，否则 status item 会走 NSMenu 模式，点击不会稳定进入回调，也就不会打开弹窗
- Linux 仍保留 tray menu fallback（Open / Settings / Quit），用于覆盖不同 tray host 对点击事件转发不一致的情况
- GNOME Shell Extension 已启用且处于 `State: ACTIVE` 时，Rust 侧不再安装 KSNI 菜单，托盘图标更新会清空 GPUI tray icon，由 Extension 独占面板入口，避免重复图标；扩展 `OUT OF DATE` 或加载失败时继续保留传统托盘 fallback
- Nested GNOME 调试的 `--app-daemon` 模式会设置 `BANANATRAY_FORCE_GNOME_EXTENSION=1`，即使 nested Shell 尚未完成 `gnome-extensions info` 注册，调试用 app 也会跳过 KSNI fallback，避免主会话里出现第二个传统托盘图标
- Linux popup 头部区域支持 `start_window_move()` 拖动；拖动开始后会短暂抑制 auto-hide，抑制期内收到的失焦事件会在保护期结束后复查，避免拖动中误关，也避免唯一一次失焦事件被吞掉后窗口常驻

**多显示器定位**（`preferred_window_bounds`）：

四级定位降级：

1. **Linux saved position** — 用户拖动后保存到 `settings.display.tray_popup.linux_last_position`，下次打开优先使用
2. **`tray_icon_anchor()`**（macOS 原生）— 获取 status item 的精确 bounds 和 `DisplayId`
3. **`tray_anchor_for_position()`**（Linux SNI 坐标）— 从 `ksni` 的 `activate(x, y)` 点击坐标构造近似锚点，匹配点击位置所在的显示器
4. **fallback** — Linux `TopRight`（margin 16px），macOS `Center`

锚点路径使用 `WindowPosition::TrayAnchored(anchor)` + `cx.compute_window_bounds()` 计算弹窗坐标；保存位置和 fallback 路径直接构造 `Bounds`。所有可确定目标显示器的路径都会把 `DisplayId` 透传给 `WindowOptions.display_id`，确保窗口创建在目标显示器。

关闭 popup 切换到设置窗口时，`close_popup` 会记录当前 `window.display(cx)`，`show_settings` 透传给 `schedule_open_settings_window`，保证设置窗口开在同一显示器

### `icon.rs` — 托盘图标管理

- **`apply_tray_icon(cx, request)`** — 根据 `TrayIconRequest` 更新系统托盘图标
- 支持 `TrayIconStyle`：Monochrome / Colorful / Dynamic
- Dynamic 模式根据当前 Provider 的 `StatusLevel` 切换图标颜色（Green/Yellow/Red）
- macOS 使用 GPUI 原生 `set_tray_icon_rendering_mode` API 控制图标渲染模式（Adaptive / Original），确保亮/暗模式下正确显示
- GNOME Shell Extension 模式下 `apply_tray_icon` 是 no-op，并会清空 GPUI tray icon；面板图标、状态点和总览摘要由 Extension 根据 D-Bus 快照独立渲染
- **Linux 平台差异**：默认图标样式为 Yellow（而非 Monochrome），因为 Linux 没有 template rendering，黑色单色图标在 GNOME Shell 深色面板上不可见。Monochrome 模式使用白色变体（`tray_icon_light.png`）确保可见性

## 图标资产

| 文件 | 用途 |
|------|------|
| `tray_icon.png` | Monochrome 模式图标（macOS，黑色） |
| `tray_icon_light.png` | Monochrome 模式图标（Linux，白色） |
| `tray_icon_colorful.png` | Colorful 模式图标 |
| `tray_icon_yellow.png` | Yellow / Dynamic Yellow 状态 |
| `tray_icon_red.png` | Dynamic Red 状态 |

## 约束

- 本模块在 `cfg(feature = "app")` 下编译，依赖 GPUI
- `TrayController` 包裹在 `Rc<RefCell<...>>` 中（GPUI 单线程模型）
- 弹窗尺寸由 `models::PopupLayout` 常量控制

## 已知限制

### Wayland 弹窗定位（上游 GPUI 限制）

BananaTray 当前可拖动 Linux popup 仍走普通 Wayland `xdg_toplevel` 路径，**Wayland 协议不允许客户端指定 `xdg_toplevel` 的位置**——compositor 完全掌控窗口放置。因此 `preferred_window_bounds` 计算出的坐标虽然正确，但会被 GNOME Mutter 等 compositor 忽略（通常居中放置）。

当前短期方案只改善交互可用性：Linux 用户可以拖动 popup；auto-hide 隐藏时会复用窗口，必要时改为透明渲染 + 鼠标穿透来保留 compositor 放置结果；并把拖动后的应用侧窗口坐标写入设置。X11 下该坐标通常可用于下次恢复；Wayland 下同一进程内重新显示更可能保留当前位置，但跨重启精确恢复仍不保证。

要实现 Wayland 上的精确 popup 定位，需要上游 GPUI 支持以下 Wayland 协议之一：

- **`wlr-layer-shell`** — 可指定屏幕位置和层级，适合面板/overlay 类应用（非标准协议，KDE/wlroots 支持，GNOME 需扩展）
- **`ext-layer-shell`** — 标准化中的 layer shell，GNOME 已有初步支持
- **`xdg-popup`** — 标准协议，可相对于父 surface 定位（但 tray icon 没有可用的父 surface）

### GNOME AppIndicator 点击事件

GNOME Shell 的 AppIndicator 扩展拦截左键点击并显示菜单（而非调用 SNI `activate()`），导致 `on_tray_icon_click_event` 不触发。Popup 通过菜单 "Open" 打开时走 `on_tray_menu_action` 路径，无点击坐标可用。此情况下 `tray_anchor_for_position` 自然跳过，走 fallback 路径。
