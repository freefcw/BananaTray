use super::{provider_id_for_action, DetailActionDispatcher};
use crate::application::{AppAction, QuotaVisibilityItem, SettingChange};
use crate::models::ProviderId;
use crate::theme::Theme;
use crate::ui::widgets::{render_detail_section_title, render_svg_icon};
use gpui::{div, px, Div, InteractiveElement, MouseButton, ParentElement, Styled};
use rust_i18n::t;

const SHOW_QUOTA_ROW_ICON: bool = true;

pub(super) fn render_quota_visibility_section(
    provider_id: ProviderId,
    items: &[QuotaVisibilityItem],
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let section = div()
        .flex_col()
        .mt(px(20.0))
        .child(render_detail_section_title(
            &t!("provider.section.quota_visibility"),
            theme,
        ));

    if items.is_empty() {
        return section.child(render_empty_message(theme));
    }

    section.child(render_quota_visibility_list(
        provider_id,
        items,
        dispatcher,
        theme,
    ))
}

fn render_empty_message(theme: &Theme) -> Div {
    div()
        .mt(px(8.0))
        .text_size(px(12.0))
        .text_color(theme.text.secondary)
        .child(t!("provider.quota_visibility.empty").to_string())
}

fn render_quota_visibility_list(
    provider_id: ProviderId,
    items: &[QuotaVisibilityItem],
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let mut list = div()
        .flex_col()
        .mt(px(8.0))
        .rounded(px(10.0))
        .bg(theme.bg.card)
        .border_1()
        .border_color(theme.border.subtle)
        .overflow_hidden();

    for (index, item) in items.iter().enumerate() {
        list = list.child(render_quota_visibility_row(
            provider_id.clone(),
            item,
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
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let visible = item.visible;
    let quota_key = item.quota_key.clone();

    div()
        .flex()
        .items_center()
        .justify_between()
        .px(px(12.0))
        .py(px(8.0))
        .cursor_pointer()
        .hover(|s| s.bg(theme.bg.subtle))
        .child(render_quota_label(item, theme))
        .child(
            crate::ui::widgets::render_toggle_switch(visible, px(36.0), px(20.0), px(14.0), theme)
                .flex_shrink_0(),
        )
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

fn render_quota_label(item: &QuotaVisibilityItem, theme: &Theme) -> Div {
    let mut label = div().flex().items_center().gap(px(8.0));
    if SHOW_QUOTA_ROW_ICON {
        label = label.child(render_svg_icon(
            "src/icons/status.svg",
            px(14.0),
            quota_label_color(item.visible, theme),
        ));
    }

    label.child(
        div()
            .text_size(px(12.5))
            .text_color(quota_label_color(item.visible, theme))
            .child(item.label.clone()),
    )
}

fn quota_label_color(visible: bool, theme: &Theme) -> gpui::Hsla {
    if visible {
        theme.text.accent
    } else {
        theme.text.muted
    }
}
