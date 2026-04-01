use super::SettingsView;
use crate::app::persist_settings;
use crate::app::widgets::render_card_separator;
use crate::models::{AppSettings, ProviderKind};
use crate::theme::Theme;
use gpui::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::AppState;

/// 单个排序箭头按钮（▲ 或 ▼）
fn render_sort_arrow_button(
    label: &str,
    theme: &Theme,
    state: Rc<RefCell<AppState>>,
    kind: ProviderKind,
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
        .text_color(theme.element_active)
        .cursor_pointer()
        .hover(|s| s.opacity(0.7))
        .child(label.to_string())
        .on_mouse_down(MouseButton::Left, move |_, window, _| {
            let mut s = state.borrow_mut();
            let moved = if direction_up {
                s.settings.move_provider_up(kind)
            } else {
                s.settings.move_provider_down(kind)
            };
            if moved {
                persist_settings(&s.settings);
            }
            drop(s);
            window.refresh();
        })
}

/// 排序箭头列：hover 时显示，包含上移/下移按钮
fn render_sort_arrows(
    group_name: String,
    is_first: bool,
    is_last: bool,
    theme: &Theme,
    state: &Rc<RefCell<AppState>>,
    kind: ProviderKind,
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
            kind,
            true,
        ));
    }
    if !is_last {
        arrow_col = arrow_col.child(render_sort_arrow_button(
            "▼",
            theme,
            state.clone(),
            kind,
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
    let icon_color = if is_selected {
        theme.element_active
    } else {
        theme.text_secondary
    };
    let name_color = if is_selected {
        theme.element_active
    } else {
        theme.text_primary
    };

    // 名称行：名字 + 启用圆点
    let name_row = div().flex().items_center().gap(px(4.0)).flex_1().child(
        div()
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(name_color)
            .child(display_name),
    );
    let name_row = if is_enabled {
        name_row.child(
            div()
                .w(px(6.0))
                .h(px(6.0))
                .rounded_full()
                .bg(theme.status_success),
        )
    } else {
        name_row
    };

    let mut content = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(10.0))
        .py(px(8.0))
        .w_full()
        .child(
            svg()
                .path(icon)
                .size(px(22.0))
                .flex_shrink_0()
                .text_color(icon_color),
        )
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
    kind: ProviderKind,
) -> Div {
    let mut item = div().flex().items_center().cursor_pointer();

    // 设置 group 标记以触发箭头的 group_hover
    if let Some(g) = group_name {
        item = item.group(g);
    }

    item.child(if is_selected {
        div()
            .rounded(px(8.0))
            .bg(theme.element_selected)
            .w_full()
            .child(item_content)
    } else {
        item_content
    })
    .on_mouse_down(MouseButton::Left, move |_, window, _| {
        state.borrow_mut().settings_ui.selected_provider = kind;
        window.refresh();
    })
}

impl SettingsView {
    // ══════ Left sidebar ══════

    pub(super) fn render_provider_sidebar(
        &self,
        providers: &[crate::models::ProviderStatus],
        selected: ProviderKind,
        settings: &AppSettings,
        theme: &Theme,
        viewport: Size<Pixels>,
    ) -> Div {
        // NOTE: 不使用 render_card()，因为它带有 overflow_hidden()，
        // 会让 Taffy 将 card 的 min-height 设为 0，导致 card 在 Scrollable
        // 内部被压缩到容器高度，永远不会溢出，滚动条无法触发。
        let mut card = div()
            .flex_col()
            .rounded(px(10.0))
            .bg(theme.bg_panel)
            .py(px(4.0));

        let ordered = settings.ordered_providers();

        for (i, kind) in ordered.iter().enumerate() {
            let is_selected = *kind == selected;
            let is_enabled = settings.is_provider_enabled(*kind);

            let status = providers.iter().find(|p| p.kind == *kind);
            let icon = status
                .map(|p| p.icon_asset().to_string())
                .unwrap_or_else(|| "src/icons/provider-unknown.svg".to_string());
            let display_name = status
                .map(|p| p.display_name().to_string())
                .unwrap_or_else(|| format!("{:?}", kind));

            if i > 0 {
                card = card.child(render_card_separator(theme));
            }

            let is_first = i == 0;
            let is_last = i == ordered.len() - 1;

            // 排序箭头：仅选中行渲染
            let (group_name, arrows) = if is_selected {
                let gname = format!("sidebar-item-{i}");
                let arrow_el =
                    render_sort_arrows(gname.clone(), is_first, is_last, theme, &self.state, *kind);
                (Some(gname), Some(arrow_el))
            } else {
                (None, None)
            };

            let content = render_sidebar_item_content(
                icon,
                display_name,
                is_selected,
                is_enabled,
                theme,
                arrows,
            );

            let item = render_sidebar_item(
                content,
                is_selected,
                group_name,
                theme,
                self.state.clone(),
                *kind,
            );

            card = card.child(item);
        }

        // Tab bar ≈ 50px, sidebar top-padding = 8px
        let sidebar_scroll_h = viewport.height - px(50.0) - px(8.0);

        div()
            .flex_col()
            .flex_none()
            .flex_basis(px(190.0))
            .pl(px(16.0))
            .pr(px(4.0))
            .pt(px(8.0))
            .overflow_hidden()
            .child(
                div()
                    .id("provider-sidebar-scroll")
                    .flex_col()
                    .h(sidebar_scroll_h)
                    .overflow_y_scroll()
                    .child(card),
            )
    }
}
