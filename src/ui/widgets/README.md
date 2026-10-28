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
| `tooltip.rs` | `with_tooltip()` / `with_multiline_tooltip()` | 悬浮提示（支持多行） |

## controls/ — 复合交互控件

| 文件 | 组件 | 说明 |
|------|------|------|
| `action_button.rs` | `render_action_button()` | 主操作按钮，`ButtonVariant`（Primary/Danger/Outlined/Subtle）× `ButtonSize`（Compact/Panel/FullWidth/Pill） |
| `icon_button.rs` | `render_icon_tooltip_button()` | 图标按钮 + 悬浮 tooltip |
| `segmented_control.rs` | `render_segmented_control()` | 分段控件（类 iOS UISegmentedControl），`SegmentedSize`（Inline/HistoryRange）决定尺寸、边框与选中配色 |
| `stepper.rs` | `render_stepper()` + `StepperOptions` | 紧凑步进器 `[−] value [+]`，边界自动禁用按钮；数值格式与步长策略由调用方决定；按钮带 `stepper-decrement` / `stepper-increment` debug_selector 供 GPUI mock 测试定位（非 test-support 构建为 no-op） |
| `cadence_dropdown.rs` | `render_cadence_trigger()` | 刷新频率下拉菜单触发器 |
| `dropdown.rs` | `render_dropdown_trigger()` / `render_dropdown_panel()` / `render_dropdown_row()` | 下拉触发器 / 浮层面板 / 选项行共享构件 |
| `history_retention_dropdown.rs` | `render_history_retention_dropdown()` + `HistoryRetentionMenu` | 历史保留时长下拉（全局 / 单 Provider 两种粒度共用） |
| `hotkey_field.rs` | `render_hotkey_field_inline()` | 紧凑内联热键录入 chip，包裹 fc-ui HotkeyInputState |

## display/ — 数据展示

| 文件 | 组件 | 说明 |
|------|------|------|
| `quota_bar.rs` | `render_quota_card()` | 额度卡片。线条样式是大数字、进度条，可选在底部内嵌历史折线；`QuotaCardHistory.on_toggle` 有值时折线可点开放大 / 收起（每张卡独立，进程内记忆），放大后绘图区更高。圆圈样式把数字放进圆环，折线在右侧，忽略放大状态 |
| `quota_ring.rs` | （`render_quota_card` 的圆圈分支） | 顶栏沿用圆点、标题和状态。环内只放数字。有历史时右侧是折线，重置详情合并到顶栏标题旁（无详情时回退显示模式标签）；没有历史时详情在圆环右侧。纯余额无上限项使用虚线圆环与百分比实心弧区分 |
| `reset_card.rs` | `render_rate_limit_resets()` | 紧凑可兑换限流重置卡。标题栏左侧展示「可兑换重置 · 数量」、右侧标注「到期」列头；每条明细的到期时刻（当年省略年份为 `M/D HH:MM`，与折线图横轴斜杠格式一致）和剩余天数分列右对齐，时间色比额度状态色更沉；悬浮提示补充获取时间与完整到期时间 |
| `history_chart.rs` | `render_history_line_chart()` / `render_history_line_chart_with_options()` | 归一化折线，签名接收整张 `HistoryChartView`（图 id 用 `quota_key`）。`chart.period` 有值时是「本周期」图：多画 50% 中线刻度、现在位置的竖点线、从满到空的中性色理想配速虚线（不画伪采样圆点），线下不铺填充。配额卡内嵌时通过 `HistoryChartOptions` 注入注意/偏低状态的主折线色与圆环右侧持平序列的纵轴数值去重。两个采样之间保持上一个值，用量相同的采样并成一段平线，数值变化时再跳，台阶带圆角，线下有一层淡填充（持平序列压淡填充，断档桥接下方铺半透明过渡填充避免被切成孤立色块）。平时不画采样点；指针悬停在折线区域里显示虚线指示线并吸到最近真实采样 |
| `info_row.rs` | `render_kv_info_row()` / `render_info_cell()` / `render_path_info_cell()` | Key-Value 信息行，路径行支持点击打开文件管理器。`render_kv_info_row` 的右值配色由 `url` 有无决定（组件内部处理），调用方不传颜色 |
| `icon_row.rs` | `render_icon_row()` | 图标 + 文本行（三栏布局，用于设置项） |
| `card.rs` | `render_detail_section_title()` / `render_detail_empty_card()` / `render_detail_error_card()` | Provider 详情区段标题、空态卡片、失败卡片 |
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

- `ButtonSize` 四档的选择依据（Panel 和 FullWidth 视觉很接近，别凭感觉挑）：
  - `Compact` — 挂在信息行右侧的内联操作，固定 32 高
  - `Panel` — 卡片内成对出现的主/次操作，自带 `flex_1`，必须放在 `flex` 行里均分宽度
  - `FullWidth` — 独占一行的操作，宽度由调用方用 `div().flex_1()` / `w()` 包一层决定
  - `Pill` — 卡片内居中的单个主操作（如 Provider 面板的打开设置/重试），宽度随内容，
    调用方用 `w_full + justify_center` 包一层实现居中
- 不要在调用方另写一套按钮样式：Token 面板和自定义 provider 的编辑/删除曾各写一份，
  尺寸和圆角对不上，已收敛到 `ButtonSize::Panel`；Provider 面板的手写 pill 同理收敛到
  `ButtonSize::Pill`
- 所有组件接受 `&Theme` 参数获取颜色（不直接读 `cx.global::<Theme>()`），保持纯渲染逻辑
- 文本输入使用 `fc-ui` 的 `InputState`（单行）和 `TextareaState`（多行）；键盘编辑 wiring 统一走 fc-ui 官方
  `input_state::wire_actions` / `textarea_state::wire_actions`（自带 key_context / track_focus / 全部编辑 action），
  调用方只负责外壳样式，不要再手写 `on_action` 注册
