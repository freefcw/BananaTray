use super::super::SettingsView;
use crate::application::AppAction;
use crate::application::SettingsProviderListItemViewState;
use crate::models::ProviderId;
use crate::runtime;
use crate::theme::Theme;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, hsla, px, App, AppContext, Context, Div, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Pixels, Point, Render, Stateful, StatefulInteractiveElement,
    Styled, Window,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::AppState;

use crate::ui::widgets::{render_provider_icon, render_svg_icon};
use rust_i18n::t;

// ============================================================================
// 拖拽排序数据类型
// ============================================================================

/// 拖拽时携带的数据：被拖动的 Provider ID
#[derive(Clone)]
struct DraggedProvider {
    id: ProviderId,
    icon: String,
    display_name: String,
}

/// 拖拽预览视图（半透明的 provider 名称卡片）
struct DragPreview {
    icon: String,
    display_name: String,
}

impl Render for DragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .bg(hsla(250.0 / 360.0, 0.6, 0.4, 0.3))
            .border_1()
            .border_color(hsla(250.0 / 360.0, 0.6, 0.5, 0.5))
            .shadow_md()
            .opacity(0.85)
            .child(render_provider_icon(
                self.icon.clone(),
                px(18.0),
                hsla(0.0, 0.0, 1.0, 0.9),
            ))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(0.0, 0.0, 1.0, 0.9))
                    .child(self.display_name.clone()),
            )
    }
}

// ============================================================================
// Sidebar 渲染函数
// ============================================================================

/// Provider 行内容：拖拽手柄 + icon + 名称 + 启用标识点
fn render_sidebar_item_content(
    icon: String,
    display_name: String,
    is_selected: bool,
    is_enabled: bool,
    theme: &Theme,
) -> Div {
    // 设计稿：选中项图标和文字为亮色/紫色，未选中为灰色
    let icon_color = if is_selected {
        theme.text.primary
    } else {
        theme.text.muted
    };
    let name_color = if is_selected {
        theme.text.primary
    } else {
        theme.text.muted
    };

    // 拖拽手柄：六点网格图标，基于主题 muted 色降低透明度暗示可拖动
    let mut drag_handle_color = theme.text.muted;
    drag_handle_color.a = 0.35;
    let drag_handle = div()
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .w(px(10.0))
        .child(render_svg_icon(
            "src/icons/drag-handle.svg",
            px(10.0),
            drag_handle_color,
        ));

    // 名称行：名字（flex_1 撑满）
    let name_row = div()
        .flex_1()
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(name_color)
        .child(display_name);

    // 启用圆点：右对齐，固定在行末
    let enabled_dot = div()
        .flex_none()
        .w(px(7.0))
        .h(px(7.0))
        .rounded_full()
        .when(is_enabled, |d| d.bg(theme.status.success));

    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .pl(px(4.0))
        .pr(px(12.0))
        .h(px(40.0))
        .w_full()
        .child(drag_handle)
        .child(render_provider_icon(icon, px(20.0), icon_color))
        .child(name_row)
        .child(enabled_dot)
}

/// 组装单个 sidebar 列表项（选中高亮 + 拖拽 + 放置目标 + 点击事件）
fn render_sidebar_item(
    item_content: Div,
    is_selected: bool,
    theme: &Theme,
    state: Rc<RefCell<AppState>>,
    id: ProviderId,
    index: usize,
    dragged: DraggedProvider,
) -> Stateful<Div> {
    // 设计稿：选中项有半透明紫色背景 + 紫色边框，未选中无背景。
    // 将两者统一为一个带 border_1() 的 div，避免边框切换造成的 1px 跳动。
    let styled_wrapper = if is_selected {
        div()
            .rounded(px(8.0))
            .w_full()
            .border_1()
            .border_color(hsla(250.0 / 360.0, 0.6, 0.5, 0.4))
            .bg(hsla(250.0 / 360.0, 0.6, 0.4, 0.2))
            .child(item_content)
    } else {
        div()
            .rounded(px(8.0))
            .w_full()
            .border_1()
            .border_color(gpui::transparent_black())
            .hover(|s| s.bg(theme.bg.subtle))
            .child(item_content)
    };

    let select_state = state.clone();
    let select_id = id.clone();
    let drop_state = state.clone();

    div()
        .id(("sidebar-provider", index))
        .flex()
        .items_center()
        .cursor_pointer()
        .child(styled_wrapper)
        // 拖拽源：开始拖动时创建半透明预览
        .on_drag(
            dragged.clone(),
            move |data, _offset: Point<Pixels>, _window, cx: &mut App| {
                cx.new(|_| DragPreview {
                    icon: data.icon.clone(),
                    display_name: data.display_name.clone(),
                })
            },
        )
        // 放置目标：拖入时显示视觉反馈（顶部紫色指示线）
        .drag_over::<DraggedProvider>(move |style, _, _, _| {
            style.border_color(hsla(250.0 / 360.0, 0.7, 0.6, 0.6))
        })
        // 放置处理：将拖动的 provider 移动到此位置
        .on_drop::<DraggedProvider>(move |data, window, cx| {
            runtime::dispatch_in_window(
                &drop_state,
                AppAction::MoveProviderToIndex {
                    id: data.id.clone(),
                    target_index: index,
                },
                window,
                cx,
            );
        })
        // 点击选中
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            runtime::dispatch_in_window(
                &select_state,
                AppAction::SelectSettingsProvider(select_id.clone()),
                window,
                cx,
            );
        })
}

/// 「+ 新增中转站」按钮
fn render_add_relay_button(state: Rc<RefCell<AppState>>, theme: &Theme) -> Div {
    let accent = theme.text.accent;
    let muted = theme.text.muted;

    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(8.0))
        .mt(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_dashed()
        .border_color(hsla(0.0, 0.0, 0.3, 0.3))
        .cursor_pointer()
        .hover(move |s| {
            s.border_color(accent)
                .bg(hsla(250.0 / 360.0, 0.6, 0.4, 0.1))
        })
        .child(render_svg_icon("src/icons/plus.svg", px(14.0), muted))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(muted)
                .child(t!("provider.add_button").to_string()),
        )
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            runtime::dispatch_in_window(&state, AppAction::EnterAddProvider, window, cx);
        })
}

impl SettingsView {
    // ══════ Left sidebar ══════

    pub(in crate::ui::settings_window) fn render_provider_sidebar(
        &mut self,
        items: &[SettingsProviderListItemViewState],
        theme: &Theme,
        _cx: &mut Context<Self>,
    ) -> Div {
        // 设计稿：sidebar 无背景色，直接在暗色底上列出 provider
        let mut list = div().flex_col().py(px(4.0));

        for (index, item_state) in items.iter().enumerate() {
            let content = render_sidebar_item_content(
                item_state.icon.clone(),
                item_state.display_name.clone(),
                item_state.is_selected,
                item_state.is_enabled,
                theme,
            );

            let dragged = DraggedProvider {
                id: item_state.id.clone(),
                icon: item_state.icon.clone(),
                display_name: item_state.display_name.clone(),
            };

            let item = render_sidebar_item(
                content,
                item_state.is_selected,
                theme,
                self.state.clone(),
                item_state.id.clone(),
                index,
                dragged,
            );

            list = list.child(item);
        }

        // 「+ 新增中转站」按钮
        list = list.child(render_add_relay_button(self.state.clone(), theme));

        // 使用 h_full() 自适应父容器高度（父容器已统一负责可用高度）
        div()
            .flex_col()
            .flex_none()
            .flex_basis(px(160.0))
            .pl(px(16.0))
            .pr(px(4.0))
            .pt(px(8.0))
            .h_full()
            .overflow_hidden()
            .child(
                div()
                    .id("provider-sidebar-scroll")
                    .flex_col()
                    .h_full()
                    .overflow_y_scroll()
                    .child(list),
            )
    }
}
