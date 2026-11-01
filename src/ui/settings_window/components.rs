use crate::application::{AppAction, SettingChange};
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::{render_segmented_control, render_toggle_switch, SegmentedSize};
use gpui::{
    div, prelude::FluentBuilder as _, px, relative, App, Div, FontWeight, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, ParentElement, Styled, Window,
};
use std::cell::RefCell;
use std::rc::Rc;

/// 设置页统一水平内边距（与顶部 Header 保持 20px 对齐）
pub(super) const TAB_PADDING_X: f32 = 20.0;
pub(super) const TAB_PADDING_BOTTOM: f32 = 20.0;

/// 标准设置行布局规格；需要自定义中间/右侧区域的行（如 Debug 日志路径行）复用同一组数值
pub(in crate::ui::settings_window) const ROW_PADDING_X: f32 = 14.0;
pub(in crate::ui::settings_window) const ROW_PADDING_Y: f32 = 11.0;
pub(in crate::ui::settings_window) const ROW_GAP: f32 = 16.0;
pub(in crate::ui::settings_window) const ROW_TITLE_SIZE: f32 = 13.5;
pub(in crate::ui::settings_window) const ROW_DESC_SIZE: f32 = 11.5;

// ============================================================================
// 统一分区标题与卡片外壳
// ============================================================================

/// 分区标题文字（大写、小号）；页面分区与带描述分区共用
fn section_header_title(title: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(11.0))
        .font_weight(FontWeight::BOLD)
        .text_color(theme.text.muted)
        .child(title.to_uppercase())
}

/// 分区标题（如 "系统与窗口"、"数据与历史"）— 大写、小号、紧凑间距
pub(super) fn render_section_header(title: &str, theme: &Theme) -> Div {
    div()
        .px(px(4.0))
        .pt(px(16.0))
        .pb(px(8.0))
        .child(section_header_title(title, theme))
}

/// 带描述的分区标题 — 大写小号标题 + 紧凑副标题说明
pub(super) fn render_section_header_with_desc(
    title: &str,
    description: &str,
    theme: &Theme,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .px(px(4.0))
        .pt(px(16.0))
        .pb(px(8.0))
        .child(section_header_title(title, theme))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text.secondary)
                .child(description.to_string()),
        )
}

/// 统一设置卡片容器
pub(super) fn render_dark_card(theme: &Theme) -> Div {
    div()
        .flex_col()
        .w_full()
        .rounded(px(12.0))
        .bg(theme.bg.card)
        .border_1()
        .border_color(theme.border.subtle)
        .overflow_hidden()
}

/// 卡片内分隔线
pub(super) fn render_divider(theme: &Theme) -> Div {
    div().h(px(0.5)).w_full().bg(theme.border.subtle)
}

// ============================================================================
// 统一设置行（无花哨彩色图标块，左侧标题+说明，右侧交互控件）
// ============================================================================

/// 标准设置行：左侧标题 + 描述，右侧任意控件
pub(super) fn render_setting_row(
    title: &str,
    description: &str,
    theme: &Theme,
    trailing: impl IntoElement,
) -> Div {
    render_setting_row_dimmable(title, description, false, theme, trailing)
}

/// 支持禁用弱化态的标准设置行
pub(super) fn render_setting_row_dimmable(
    title: &str,
    description: &str,
    dimmed: bool,
    theme: &Theme,
    trailing: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .gap(px(ROW_GAP))
        .px(px(ROW_PADDING_X))
        .py(px(ROW_PADDING_Y))
        .when(dimmed, |el| el.opacity(0.45))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(ROW_TITLE_SIZE))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text.primary)
                        .child(title.to_string()),
                )
                .when(!description.is_empty(), |col| {
                    col.child(
                        div()
                            .text_size(px(ROW_DESC_SIZE))
                            .line_height(relative(1.35))
                            .text_color(theme.text.muted)
                            .child(description.to_string()),
                    )
                }),
        )
        .child(div().flex_shrink_0().child(trailing))
}

/// 统一开关行（38×22 标准列表开关尺寸）
pub(super) fn render_switch_row<F>(
    title: &str,
    description: &str,
    enabled: bool,
    theme: &Theme,
    on_click: F,
) -> Div
where
    F: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    render_switch_row_dimmable(title, description, enabled, false, theme, on_click)
}

/// 支持父级总开关联动弱化的开关行
pub(super) fn render_switch_row_dimmable<F>(
    title: &str,
    description: &str,
    enabled: bool,
    dimmed: bool,
    theme: &Theme,
    on_click: F,
) -> Div
where
    F: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    let toggle = render_toggle_switch(enabled, px(38.0), px(22.0), px(16.0), theme)
        .flex_shrink_0()
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, on_click);
    render_setting_row_dimmable(title, description, dimmed, theme, toggle)
}

/// 直接派发 `SettingChange` 的开关行 —— 各 Tab 普通设置开关的标准写法，
/// 避免每个调用点重复 `state.clone()` + `dispatch_in_window` 模板。
pub(super) fn render_toggle_setting(
    state: &Rc<RefCell<runtime::AppState>>,
    label: &str,
    desc: &str,
    enabled: bool,
    theme: &Theme,
    change: SettingChange,
) -> Div {
    let state = state.clone();
    render_switch_row(label, desc, enabled, theme, move |_, window, cx| {
        crate::bootstrap::dispatch_in_window(
            &state,
            AppAction::UpdateSetting(change.clone()),
            window,
            cx,
        );
    })
}

/// 统一分段选择器行：左侧标题 + 描述，右侧 Inline SegmentedControl
pub(super) fn render_segmented_row<T, F>(
    title: &str,
    description: &str,
    options: Vec<(String, T)>,
    current: &T,
    theme: &Theme,
    on_select: F,
) -> Div
where
    T: PartialEq + Clone + 'static,
    F: Fn(T, &mut Window, &mut App) + Clone + 'static,
{
    render_setting_row(
        title,
        description,
        theme,
        render_segmented_control(&options, current, SegmentedSize::Inline, theme, on_select),
    )
}
