# src/tray/

系统托盘模块，管理托盘图标、弹窗窗口生命周期和多显示器定位。

## 模块结构

### `controller.rs` — TrayController

弹窗窗口的生命周期管理：

- **`TrayController`** — 持有 `AppState`（`Rc<RefCell<...>>`）、refresh channel、log path
- 管理弹窗的创建、显示/隐藏、定位
- 处理托盘点击事件和全局快捷键
- 弹窗句柄通过共享 `Cell<Option<WindowHandle<_>>>` 保存，失焦 auto-hide 依靠幂等守卫清理当前窗口，避免 stale handle 和误关新窗口

**多显示器定位**（`preferred_window_bounds`）：

- 通过 GPUI `cx.tray_icon_anchor()` 获取被点击托盘图标所在的 `DisplayId` 与菜单栏局部 bounds
- 用 `WindowPosition::TrayAnchored(anchor)` + `cx.compute_window_bounds()` 计算弹窗在该显示器上的局部坐标
- 返回的 `DisplayId` 连同 bounds 一起透传给 `WindowOptions.display_id`，确保 GPUI 在目标显示器创建窗口
- anchor 不可用时回退：Linux `TopRight`，其他平台 `Center`
- 关闭 popup 切换到设置窗口时，`close_popup` 会记录当前 `window.display(cx)`，`show_settings` 透传给 `schedule_open_settings_window`，保证设置窗口开在同一显示器

### `icon.rs` — 托盘图标管理

- **`apply_tray_icon(cx, request)`** — 根据 `TrayIconRequest` 更新系统托盘图标
- 支持 `TrayIconStyle`：Monochrome / Colorful / Dynamic
- Dynamic 模式根据当前 Provider 的 `StatusLevel` 切换图标颜色（Green/Yellow/Red）
- macOS 使用 GPUI 原生 `set_tray_icon_rendering_mode` API 控制图标渲染模式（Adaptive / Original），确保亮/暗模式下正确显示

## 图标资产

| 文件 | 用途 |
|------|------|
| `tray_icon.png` | Monochrome 模式图标 |
| `tray_icon_colorful.png` | Colorful 模式图标 |
| `tray_icon_yellow.png` | Dynamic 模式 - Yellow 状态 |
| `tray_icon_red.png` | Dynamic 模式 - Red 状态 |

## 约束

- 本模块在 `cfg(feature = "app")` 下编译，依赖 GPUI
- `TrayController` 包裹在 `Rc<RefCell<...>>` 中（GPUI 单线程模型）
- 弹窗尺寸由 `models::PopupLayout` 常量控制
