# src/ui/widgets/

可复用 GPUI UI 组件库，托盘弹窗和设置窗口共享使用。

## 目录结构

```
widgets/
├── mod.rs              — 统一 re-export，调用方仍用 crate::ui::widgets::X
├── primitives/         — 基础原子组件（无业务语义）
├── controls/           — 复合交互控件
└── display/            — 数据展示组件
```

## primitives/ — 基础原子组件

| 文件 | 组件 | 说明 |
|------|------|------|
| `icon.rs` | `render_svg_icon()` / `render_footer_glyph()` | SVG 图标渲染（尺寸 + 颜色） |
| `colored_icon.rs` | `render_colored_icon()` | 带背景色圆角图标（基于 icon.rs） |
| `toggle.rs` | `render_toggle_switch()` | 开关切换控件（可定制尺寸），纯视觉无事件 |
| `checkbox.rs` | `render_checkbox()` | 复选框 |
| `tooltip.rs` | `with_tooltip()` / `with_multiline_tooltip()` | 悬浮提示（支持多行） |

## controls/ — 复合交互控件

| 文件 | 组件 | 说明 |
|------|------|------|
| `action_button.rs` | `render_action_button()` | 主操作按钮，支持 `ButtonVariant`（Danger/Outlined/Subtle） |
| `icon_button.rs` | `render_icon_tooltip_button()` | 图标按钮 + 悬浮 tooltip |
| `segmented_control.rs` | `render_segmented_control()` | 分段控件（类 iOS UISegmentedControl） |
| `cadence_dropdown.rs` | `render_cadence_trigger()` | 刷新频率下拉菜单触发器 |
| `hotkey_field.rs` | `render_hotkey_field_inline()` | 紧凑内联热键录入 chip，包裹 adabraka-ui HotkeyInputState |
| `input_actions.rs` | `register_input_actions()` | 注册 Ctrl+A/C/V/X 等输入快捷键 |
| `tab.rs` | `render_icon_tab()` | 底部导航 tab 渲染（预留） |

## display/ — 数据展示

| 文件 | 组件 | 说明 |
|------|------|------|
| `quota_bar.rs` | `render_quota_bar()` | 额度进度条（带动画、渐变色、标签） |
| `info_row.rs` | `render_kv_info_row()` / `render_info_cell()` / `render_path_info_cell()` | Key-Value 信息行，路径行支持点击打开文件管理器 |
| `icon_row.rs` | `render_icon_row()` | 图标 + 文本行（三栏布局，用于设置项） |
| `card.rs` | `render_detail_section_title()` | 区段标题 + 预留卡片布局组件 |
| `provider_icon.rs` | `render_provider_icon()` | Provider 品牌图标（SVG / 首字母文本双模式，含方形 boxed 变体） |

## 已迁出的组件

| 原文件 | 新位置 | 原因 |
|--------|--------|------|
| `global_actions.rs` | `src/ui/views/global_actions.rs` | 包含 `impl AppView` 方法和业务逻辑，不是通用 widget |

## 使用方式

所有组件通过 `widgets/mod.rs` 的 glob re-export，调用方 **无需感知子目录**：

```rust
use crate::ui::widgets::{render_action_button, ButtonVariant};
use crate::ui::widgets::{render_quota_bar, render_svg_icon};
```

## 约束

- 所有组件接受 `&Theme` 参数获取颜色（不直接读 `cx.global::<Theme>()`），保持纯渲染逻辑
- 文本输入使用 `adabraka-ui` 的 `InputState`（单行）和 `TextareaState`（多行），配合 `input_actions.rs` 注册快捷键
