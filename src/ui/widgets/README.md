# src/ui/widgets/

可复用 GPUI UI 组件库，托盘弹窗和设置窗口共享使用。

## 组件清单

### 按钮与交互

| 文件 | 组件 | 说明 |
|------|------|------|
| `action_button.rs` | `render_action_button()` | 主操作按钮，支持 `ButtonVariant`（Primary/Secondary/Danger） |
| `icon_button.rs` | `render_icon_tooltip_button()` | 图标按钮 + 悬浮 tooltip |
| `toggle.rs` | `render_toggle_switch()` | 开关切换控件（可定制尺寸） |
| `checkbox.rs` | `render_checkbox()` | 复选框 |
| `segmented_control.rs` | `render_segmented_control()` | 分段控件（类 iOS UISegmentedControl） |
| `cadence_dropdown.rs` | `render_cadence_trigger()` | 刷新频率下拉菜单触发器 |

### 数据展示

| 文件 | 组件 | 说明 |
|------|------|------|
| `quota_bar.rs` | `render_quota_bar()` | 额度进度条（带动画、渐变色、标签） |
| `info_row.rs` | `render_kv_info_row()` / `render_info_cell()` | Key-Value 信息行 |
| `card.rs` | `render_detail_section_title()` | Provider 详情区域标题 |

### 图标

| 文件 | 组件 | 说明 |
|------|------|------|
| `icon.rs` | `render_svg_icon()` / `render_footer_glyph()` | SVG 图标渲染（尺寸 + 颜色） |
| `colored_icon.rs` | `render_colored_icon()` | 带背景色圆角图标 |
| `provider_icon.rs` | `render_provider_icon()` | Provider 品牌图标（含方形 boxed 变体） |
| `icon_row.rs` | `render_icon_row()` | 图标 + 文本行（用于设置项） |

### 输入

| 文件 | 组件 | 说明 |
|------|------|------|
| `simple_input.rs` | `render_simple_input()` / `render_simple_textarea()` | 自定义文本输入框（避免 adabraka-ui InputState 的 IME 崩溃问题） |
| `input_actions.rs` | `register_input_actions()` | 注册 Ctrl+A/C/V/X 等输入快捷键的 GPUI action |

### 辅助

| 文件 | 组件 | 说明 |
|------|------|------|
| `tab.rs` | 底部导航 tab 渲染 | Provider 切换 tab |
| `global_actions.rs` | 底部工具栏 | Refresh / Dashboard / Settings / Quit 按钮组 |
| `tooltip.rs` | `with_tooltip()` / `with_multiline_tooltip()` | 悬浮提示（支持多行） |

## 使用方式

所有组件通过 `mod.rs` 的 `pub(crate) use` re-export：

```rust
use crate::ui::widgets::{render_action_button, ButtonVariant};
use crate::ui::widgets::{render_quota_bar, render_svg_icon};
```

## 约束

- 所有组件接受 `&Theme` 参数获取颜色（不直接读 `cx.global::<Theme>()`），保持纯渲染逻辑
- `SimpleInputState` 是自研输入状态管理，替代 `adabraka-ui` 的 `InputState`（后者在中文 IME 环境下会触发 GPUI panic）
