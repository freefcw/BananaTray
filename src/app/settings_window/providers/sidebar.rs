use super::super::SettingsView;
use crate::application::SettingsProviderListItemViewState;
use crate::application::{AppAction, ProviderOrderDirection};
use crate::models::ProviderId;
use crate::runtime;
use crate::theme::Theme;
use gpui::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::AppState;

use crate::app::widgets::{render_provider_icon, render_svg_icon};
use rust_i18n::t;

/// 单个排序箭头按钮（▲ 或 ▼）
fn render_sort_arrow_button(
    label: &str,
    theme: &Theme,
    state: Rc<RefCell<AppState>>,
    id: ProviderId,
    direction_up: bool,
) -> Div {
    div()
        .w(px(16.0))
        .h(px(12.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.0))
        .text_size(px(8.0))
        .text_color(theme.element.active)
        .cursor_pointer()
        .hover(|s| s.opacity(0.7))
        .child(label.to_string())
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            runtime::dispatch_in_window(
                &state,
                AppAction::ReorderProvider {
                    id: id.clone(),
                    direction: if direction_up {
                        ProviderOrderDirection::Up
                    } else {
                        ProviderOrderDirection::Down
                    },
                },
                window,
                cx,
            );
        })
}

/// 排序箭头列：hover 时显示，包含上移/下移按钮
fn render_sort_arrows(
    group_name: String,
    is_first: bool,
    is_last: bool,
    theme: &Theme,
    state: &Rc<RefCell<AppState>>,
    id: &ProviderId,
) -> Div {
    let mut arrow_col = div()
        .flex_col()
        .flex_shrink_0()
        .gap(px(2.0))
        .opacity(0.0)
        .group_hover(group_name, |s| s.opacity(1.0));

    if !is_first {
        arrow_col = arrow_col.child(render_sort_arrow_button(
            "▲",
            theme,
            state.clone(),
            id.clone(),
            true,
        ));
    }
    if !is_last {
        arrow_col = arrow_col.child(render_sort_arrow_button(
            "▼",
            theme,
            state.clone(),
            id.clone(),
            false,
        ));
    }
    arrow_col
}

/// Provider 行内容：icon + 名称 + 启用标识点（+ 可选箭头）
fn render_sidebar_item_content(
    icon: String,
    display_name: String,
    is_selected: bool,
    is_enabled: bool,
    theme: &Theme,
    arrows: Option<Div>,
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

    // 名称行：名字 + 启用圆点
    let name_row = div().flex().items_center().gap(px(8.0)).flex_1().child(
        div()
            .text_size(px(13.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(name_color)
            .child(display_name),
    );
    let name_row = if is_enabled {
        name_row.child(
            div()
                .w(px(7.0))
                .h(px(7.0))
                .rounded_full()
                .bg(theme.status.success),
        )
    } else {
        name_row
    };

    let mut content = div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .px(px(12.0))
        .h(px(40.0)) // 固定高度，防止箭头(▲/▼)出现时撑高整行
        .w_full()
        .child(render_provider_icon(icon, px(20.0), icon_color))
        .child(name_row);

    if let Some(arrow_el) = arrows {
        content = content.child(arrow_el);
    }
    content
}

/// 组装单个 sidebar 列表项（选中高亮 + group hover + 点击事件）
fn render_sidebar_item(
    item_content: Div,
    is_selected: bool,
    group_name: Option<String>,
    theme: &Theme,
    state: Rc<RefCell<AppState>>,
    id: ProviderId,
) -> Div {
    let mut item = div().flex().items_center().cursor_pointer();

    // 设置 group 标记以触发箭头的 group_hover
    if let Some(g) = group_name {
        item = item.group(g);
    }

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

    item.child(styled_wrapper)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            runtime::dispatch_in_window(
                &state,
                AppAction::SelectSettingsProvider(id.clone()),
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
        .child(render_svg_icon(
            "src/icons/provider-custom.svg",
            px(16.0),
            muted,
        ))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(muted)
                .child(format!("+ {}", t!("newapi.add_button"))),
        )
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            runtime::dispatch_in_window(&state, AppAction::EnterAddNewApi, window, cx);
        })
}

impl SettingsView {
    // ══════ Left sidebar ══════

    pub(in crate::app::settings_window) fn render_provider_sidebar(
        &mut self,
        items: &[SettingsProviderListItemViewState],
        theme: &Theme,
        _cx: &mut Context<Self>,
    ) -> Div {
        // 设计稿：sidebar 无背景色，直接在暗色底上列出 provider
        let mut list = div().flex_col().py(px(4.0));

        for (index, item_state) in items.iter().enumerate() {
            // 排序箭头：仅选中行渲染
            let (group_name, arrows) =
                if item_state.is_selected && (item_state.can_move_up || item_state.can_move_down) {
                    let gname = format!("sidebar-item-{index}");
                    let arrow_el = render_sort_arrows(
                        gname.clone(),
                        !item_state.can_move_up,
                        !item_state.can_move_down,
                        theme,
                        &self.state,
                        &item_state.id,
                    );
                    (Some(gname), Some(arrow_el))
                } else {
                    (None, None)
                };

            let content = render_sidebar_item_content(
                item_state.icon.clone(),
                item_state.display_name.clone(),
                item_state.is_selected,
                item_state.is_enabled,
                theme,
                arrows,
            );

            let item = render_sidebar_item(
                content,
                item_state.is_selected,
                group_name,
                theme,
                self.state.clone(),
                item_state.id.clone(),
            );

            list = list.child(item);
        }

        // 「+ 新增中转站」按钮
        list = list.child(render_add_relay_button(self.state.clone(), theme));

        // 使用 h_full() 自适应父容器高度（父容器已统一负责可用高度）
        div()
            .flex_col()
            .flex_none()
            .flex_basis(px(190.0))
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
