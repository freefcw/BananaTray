# src/tray/

系统托盘模块，管理托盘图标、弹窗窗口生命周期和多显示器定位。

## 模块结构

### `controller.rs` — TrayController

弹窗窗口的生命周期管理：

- **`TrayController`** — 持有 `AppState`（`Rc<RefCell<...>>`）、refresh channel、log path
- 管理弹窗的创建、显示/隐藏、定位
- 处理托盘点击事件和全局快捷键
- 弹窗句柄通过共享 `Cell<Option<WindowHandle<_>>>` 保存，失焦 auto-hide 依靠幂等守卫清理当前窗口，避免 stale handle 和误关新窗口
- Linux 下 auto-hide / toggle 复用同一窗口；当用户已经拖动过或已有保存位置时，隐藏会优先切到透明渲染并启用鼠标穿透，避免普通 Wayland `xdg_toplevel` 路径重新映射后回到屏幕中央

**托盘交互入口**：

- 使用 `on_tray_icon_click_event` 注册点击回调（替代 `on_tray_icon_event`），获取 `TrayIconClickEvent`（含 kind + 可选 position）
- macOS 启动时必须通过 GPUI `set_tray_panel_mode(true)` 切到 panel callback 模式，否则 status item 会走 NSMenu 模式，点击不会稳定进入回调，也就不会打开弹窗
- Linux 仍保留 tray menu fallback（Open / Settings / Quit），用于覆盖不同 tray host 对点击事件转发不一致的情况
- Linux popup 头部区域在普通 `xdg_toplevel` fallback 路径支持 `start_window_move()` 拖动；拖动开始后会短暂抑制 auto-hide，抑制期内收到的失焦事件会在保护期结束后复查，避免拖动中误关，也避免唯一一次失焦事件被吞掉后窗口常驻。layer-shell surface 没有 toplevel move 能力，此时定位由 compositor 根据 layer-shell anchor / margin 完成

**多显示器定位**（`preferred_window_bounds`）：

四级定位降级：

1. **`tray_icon_anchor()`** — 获取托盘图标锚点；macOS 使用原生 status item bounds，Linux 若上游 GPUI 能提供锚点也复用
2. **`tray_anchor_for_position()`**（Linux SNI 坐标）— 从 `ksni` 的 `activate(x, y)` 点击坐标构造近似锚点，匹配点击位置所在的显示器
3. **Linux saved position** — 用户在普通 fallback 窗口拖动后保存到 `settings.display.tray_popup.linux_last_position`，无托盘锚点时使用
4. **fallback** — Linux `TopRight`（margin 16px），macOS `Center`

锚点路径使用 `WindowPosition::TrayAnchored(anchor)` + `cx.compute_window_bounds()` 计算普通窗口 fallback 坐标；Linux 同时通过 `LayerShellOptions::tray_panel()` 写入 `WindowOptions.layer_shell`，让支持 `zwlr_layer_shell_v1` 的 Wayland compositor 直接按输出、角落 anchor 和 margin 放置 popup。保存位置和 fallback 路径直接构造 `Bounds`，Linux 会用 `LayerShellOptions::from_window_bounds()` 转成 layer-shell anchor / margin。所有可确定目标显示器的路径都会把 `DisplayId` 透传给 `WindowOptions.display_id`，确保窗口创建在目标显示器。

关闭 popup 切换到设置窗口时，`close_popup` 会记录当前 `window.display(cx)`，`show_settings` 透传给 `schedule_open_settings_window`，保证设置窗口开在同一显示器

### `icon.rs` — 托盘图标管理

- **`apply_tray_icon(cx, request)`** — 根据 `TrayIconRequest` 更新系统托盘图标
- 支持 `TrayIconStyle`：Monochrome / Colorful / Dynamic
- Dynamic 模式根据当前 Provider 的 `StatusLevel` 切换图标颜色（Green/Yellow/Red）
- macOS 使用 GPUI 原生 `set_tray_icon_rendering_mode` API 控制图标渲染模式（Adaptive / Original），确保亮/暗模式下正确显示
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

### Wayland 弹窗定位

BananaTray 会为 Linux popup 设置 GPUI `WindowOptions.layer_shell`。在支持 `zwlr_layer_shell_v1` 的 Wayland compositor 上，GPUI 创建 wlr-layer-shell surface，按目标输出、角落 anchor 和 margin 放置弹窗；如果 compositor 不暴露该协议，GPUI 会自动回退到普通 `xdg_toplevel`，并记录 warning。

普通 `xdg_toplevel` fallback 仍受 Wayland 协议限制：客户端不能指定窗口位置，GNOME Mutter 等 compositor 可能忽略 `preferred_window_bounds` 并居中放置。此路径保留拖动与 saved position 机制作为交互兜底；X11 下该坐标通常可用于下次恢复，Wayland 下只保证尽量复用同一窗口映射结果。

协议覆盖仍取决于桌面环境：wlroots/KDE 类环境更可能支持 wlr-layer-shell；GNOME 默认环境通常需要扩展或未来 ext-layer-shell 支持。

### GNOME AppIndicator 点击事件

GNOME Shell 的 AppIndicator 扩展拦截左键点击并显示菜单（而非调用 SNI `activate()`），导致 `on_tray_icon_click_event` 不触发。Popup 通过菜单 "Open" 打开时走 `on_tray_menu_action` 路径，无点击坐标可用。此情况下 `tray_anchor_for_position` 自然跳过，走 fallback 路径。
