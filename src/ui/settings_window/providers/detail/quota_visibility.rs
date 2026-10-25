use super::{provider_id_for_action, DetailActionDispatcher};
use crate::application::{AppAction, QuotaVisibilityItem, SettingChange};
use crate::models::ProviderId;
use crate::theme::Theme;
use crate::ui::widgets::{
    render_detail_empty_card, render_detail_section_title, render_svg_icon, with_tooltip,
};
use gpui::{
    div, px, relative, Div, InteractiveElement, MouseButton, ParentElement, Stateful, Styled,
};
use rust_i18n::t;

const SHOW_QUOTA_ROW_ICON: bool = true;

pub(super) fn render_quota_visibility_section(
    provider_id: ProviderId,
    items: &[QuotaVisibilityItem],
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let section = div()
        .flex()
        .flex_col()
        .mt(px(20.0))
        .child(render_detail_section_title(
            &t!("provider.section.quota_visibility"),
            theme,
        ));

    if items.is_empty() {
        return section.child(render_detail_empty_card(
            &t!("provider.quota_visibility.empty"),
            theme,
        ));
    }

    section.child(render_quota_visibility_list(
        provider_id,
        items,
        dispatcher,
        theme,
    ))
}

fn render_quota_visibility_list(
    provider_id: ProviderId,
    items: &[QuotaVisibilityItem],
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let mut list = div()
        .flex()
        .flex_col()
        .mt(px(10.0))
        .rounded(px(12.0))
        .bg(theme.bg.card_inner)
        .border_1()
        .border_color(theme.border.strong)
        .overflow_hidden();

    // 只有 2 个及以上可见配额时主额度选择才有意义（单配额折叠态就是它自己）
    let has_multiple_visible = items.iter().filter(|item| item.visible).count() > 1;
    for (index, item) in items.iter().enumerate() {
        list = list.child(render_quota_visibility_row(
            provider_id.clone(),
            item,
            has_multiple_visible,
            dispatcher,
            theme,
        ));
        if index + 1 < items.len() {
            list = list.child(div().h(px(0.5)).w_full().bg(theme.border.subtle));
        }
    }

    list
}

fn render_quota_visibility_row(
    provider_id: ProviderId,
    item: &QuotaVisibilityItem,
    has_multiple_visible: bool,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let visible = item.visible;
    let quota_key = item.quota_key.clone();
    let hover_bg = theme.bg.card_inner_hovered;

    let mut trailing = div().flex().items_center().gap(px(8.0));
    // 隐藏的配额不出现在总览，不给主额度入口
    if has_multiple_visible && item.visible {
        trailing = trailing.child(render_primary_toggle(
            provider_id.clone(),
            item,
            dispatcher,
            theme,
        ));
    }
    trailing = trailing.child(
        crate::ui::widgets::render_toggle_switch(visible, px(36.0), px(20.0), px(14.0), theme)
            .flex_shrink_0(),
    );

    div()
        .flex()
        .items_center()
        .justify_between()
        .px(px(16.0))
        .py(px(10.0))
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .child(render_quota_label(item, theme))
        .child(trailing)
        .on_mouse_down(MouseButton::Left, {
            let provider_id = provider_id_for_action(&provider_id);
            dispatcher.interactive_action(move || {
                AppAction::UpdateSetting(SettingChange::ToggleQuotaVisibility {
                    provider_id: provider_id.clone(),
                    quota_key: quota_key.clone(),
                })
            })
        })
}

/// 主额度星标：设置后总览折叠态展示该配额；再点一次恢复最差配额优先。
fn render_primary_toggle(
    provider_id: ProviderId,
    item: &QuotaVisibilityItem,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Stateful<Div> {
    let quota_key = item.quota_key.clone();
    let tooltip = if item.is_primary {
        t!("provider.quota_visibility.unset_primary").to_string()
    } else {
        t!("provider.quota_visibility.set_primary").to_string()
    };
    let hover_bg = theme.bg.subtle;
    let hover_text = theme.text.primary;
    let text_color = if item.is_primary {
        theme.text.accent
    } else {
        theme.text.muted
    };

    with_tooltip(
        format!("quota-primary-{}-{}", provider_id.id_key(), item.quota_key),
        &tooltip,
        theme,
        div()
            .w(px(20.0))
            .h(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .text_size(px(12.0))
            .text_color(text_color)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg).text_color(hover_text))
            .on_mouse_down(MouseButton::Left, {
                let dispatcher = dispatcher.clone();
                move |_, window, cx| {
                    cx.stop_propagation();
                    dispatcher.dispatch(
                        AppAction::UpdateSetting(SettingChange::TogglePrimaryQuota {
                            provider_id: provider_id_for_action(&provider_id),
                            quota_key: quota_key.clone(),
                        }),
                        window,
                        cx,
                    );
                }
            })
            .child(if item.is_primary { "★" } else { "☆" }),
    )
}

fn render_quota_label(item: &QuotaVisibilityItem, theme: &Theme) -> Div {
    let mut label = div().flex().items_center().gap(px(8.0));
    if SHOW_QUOTA_ROW_ICON {
        label = label.child(render_svg_icon(
            "src/icons/status.svg",
            px(14.0),
            quota_icon_color(item.visible, theme),
        ));
    }

    label.child(
        div()
            .text_size(px(13.0))
            .line_height(relative(1.3))
            .text_color(quota_label_color(item.visible, theme))
            .child(item.label.clone()),
    )
}

fn quota_icon_color(visible: bool, theme: &Theme) -> gpui::Hsla {
    if visible {
        theme.text.accent
    } else {
        theme.text.muted
    }
}

fn quota_label_color(visible: bool, theme: &Theme) -> gpui::Hsla {
    if visible {
        theme.text.primary
    } else {
        theme.text.muted
    }
}
