use super::{icon_button, info, provider_id_for_action, DetailActionDispatcher};
use crate::application::{
    AppAction, ProviderDetailTab, SettingsProviderDetailViewState, SettingsProviderUsageViewState,
};
use crate::refresh::RefreshReason;
use crate::theme::Theme;
use crate::ui::settings_window::providers::shared;
use gpui::{
    div, prelude::FluentBuilder as _, px, transparent_black, Div, FontWeight, InteractiveElement,
    MouseButton, ParentElement, Styled,
};
use rust_i18n::t;

pub(super) fn render_header(
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let top_row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(render_header_identity(detail, theme))
        .child(render_header_actions(detail, dispatcher, theme));

    let mut header = div().flex().flex_col().child(top_row);
    if detail.can_refresh {
        header = header.child(render_detail_sub_tabs(detail, dispatcher, theme));
    }
    header
}

fn render_header_identity(detail: &SettingsProviderDetailViewState, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .flex_1()
        .min_w_0()
        .gap(px(14.0))
        .child(crate::ui::widgets::render_provider_icon_boxed(
            &detail.icon,
            px(52.0),
            px(30.0),
            theme.text.primary,
            theme.bg.subtle,
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text.primary)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(detail.display_name.clone()),
                )
                .child(info::render_status_summary(detail, theme)),
        )
}

fn render_detail_sub_tabs(
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    div()
        .w_full()
        .mt(px(16.0))
        .p(px(3.0))
        .rounded(px(9.0))
        .bg(theme.bg.subtle)
        .border_1()
        .border_color(theme.border.subtle)
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(render_sub_tab_pill(
            detail,
            ProviderDetailTab::Settings,
            dispatcher,
            theme,
        ))
        .child(render_sub_tab_pill(
            detail,
            ProviderDetailTab::UsageHistory,
            dispatcher,
            theme,
        ))
}

fn render_sub_tab_pill(
    detail: &SettingsProviderDetailViewState,
    tab: ProviderDetailTab,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let (selector, icon_path, label, show_error_dot) = match tab {
        ProviderDetailTab::Settings => (
            "provider-detail-tab-settings",
            "src/icons/settings.svg",
            t!("provider.tab.settings").to_string(),
            false,
        ),
        ProviderDetailTab::UsageHistory => (
            "provider-detail-tab-usage-history",
            "src/icons/usage.svg",
            t!("provider.tab.usage_history").to_string(),
            matches!(detail.usage, SettingsProviderUsageViewState::Error { .. }),
        ),
    };
    let is_active = detail.detail_tab == tab;
    let (bg, fg) = if is_active {
        (theme.nav.pill_active_bg, theme.nav.pill_active_text)
    } else {
        (transparent_black(), theme.text.secondary)
    };
    let hover_bg = theme.bg.card_inner_hovered;
    let hover_fg = theme.text.primary;

    div()
        .debug_selector(move || selector.to_string())
        .flex_1()
        .h(px(28.0))
        .px(px(10.0))
        .rounded(px(6.0))
        .bg(bg)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .cursor_pointer()
        .hover(move |style| {
            if is_active {
                style
            } else {
                style.bg(hover_bg).text_color(hover_fg)
            }
        })
        .child(crate::ui::widgets::render_svg_icon(icon_path, px(13.0), fg))
        .child(
            div()
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(fg)
                .whitespace_nowrap()
                .child(label),
        )
        .when(show_error_dot, |el| {
            el.child(
                div()
                    .w(px(6.0))
                    .h(px(6.0))
                    .rounded_full()
                    .bg(theme.status.error),
            )
        })
        .when(!is_active, |el| {
            // 当前子页签再点一次不应清草稿：reducer 对同 tab 会直接 return 且不 Render
            el.on_mouse_down(
                MouseButton::Left,
                dispatcher.interactive_cleanup_action(move || AppAction::SetProviderDetailTab(tab)),
            )
        })
}

fn render_header_actions(
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let mut row = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(render_remove_from_sidebar_button(detail, dispatcher, theme));

    if detail.can_refresh {
        row = row.child(icon_button(None, "src/icons/refresh.svg", 16.0, theme, {
            let provider_id = provider_id_for_action(&detail.id);
            dispatcher.interactive_action(move || AppAction::RefreshProvider {
                id: provider_id.clone(),
                reason: RefreshReason::Manual,
            })
        }));
    }

    row.child(
        crate::ui::widgets::render_toggle_switch(
            detail.is_enabled,
            px(44.0),
            px(24.0),
            px(18.0),
            theme,
        )
        .on_mouse_down(gpui::MouseButton::Left, {
            let provider_id = provider_id_for_action(&detail.id);
            dispatcher.interactive_action(move || AppAction::ToggleProvider(provider_id.clone()))
        }),
    )
}

fn render_remove_from_sidebar_button(
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    if detail.confirming_remove {
        return shared::render_confirm_cancel_buttons(
            &t!("common.confirm"),
            &t!("common.cancel"),
            {
                let provider_id = provider_id_for_action(&detail.id);
                dispatcher.interactive_action(move || {
                    AppAction::RemoveProviderFromSidebar(provider_id.clone())
                })
            },
            dispatcher.interactive_action(|| AppAction::CancelRemoveProvider),
            theme,
        );
    }

    icon_button(
        Some("remove-from-sidebar"),
        "src/icons/trash.svg",
        14.0,
        theme,
        dispatcher.interactive_cleanup_action(|| AppAction::ConfirmRemoveProvider),
    )
}
