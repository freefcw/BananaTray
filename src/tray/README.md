# src/tray/

系统托盘模块，管理托盘图标、弹窗窗口生命周期和多显示器定位。

## 模块结构

### `controller.rs` — TrayController

弹窗窗口的生命周期管理：

- **`TrayController`** — 持有 `AppState`（`Rc<RefCell<...>>`）、refresh channel、log path
- 管理弹窗的创建、显示/隐藏、定位
- 处理托盘点击事件和全局快捷键
- 弹窗句柄通过共享 `Cell<Option<WindowHandle<_>>>` 保存，失焦 auto-hide 依靠幂等守卫清理当前窗口，避免 stale handle 和误关新窗口

### `display.rs` — 多显示器定位（macOS only）

- 通过 CoreGraphics FFI 获取当前鼠标所在显示器
- 计算弹窗在托盘图标下方的精确位置
- `#[cfg(target_os = "macos")]` 守卫

### `icon.rs` — 托盘图标管理

- **`apply_tray_icon(cx, request)`** — 根据 `TrayIconRequest` 更新系统托盘图标
- 支持 `TrayIconStyle`：Monochrome / Colorful / Dynamic
- Dynamic 模式根据当前 Provider 的 `StatusLevel` 切换图标颜色（Green/Yellow/Red）
- macOS 使用 `setTemplate` hack 确保图标在亮/暗模式下正确显示

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
