use super::actions::{render_editable_provider_actions, EditableProviderActions};
use super::quota_visibility;
use super::DetailActionDispatcher;
use crate::application::{
    AppAction, SettingChange, SettingsProviderDetailViewState, SettingsProviderUsageViewState,
};
use crate::models::{ProviderCapability, QuotaThresholdTarget, SettingsCapability};
use crate::theme::Theme;
use crate::ui::settings_window::providers::shared::{
    render_settings_card, render_settings_card_header,
};
use crate::ui::settings_window::SettingsView;
use crate::ui::widgets::render_svg_icon;
use gpui::{div, hsla, px, relative, Context, Div, ParentElement, Styled, TextAlign, Window};
use rust_i18n::t;

pub(super) fn render_settings_section(
    view: &mut SettingsView,
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Div {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .mt(px(16.0))
        .pb(px(20.0));

    section = match detail.settings_capability.clone() {
        SettingsCapability::TokenInput(capability) => {
            section.child(super::super::token_input_panel::render_token_input_panel(
                &detail.id,
                capability,
                &detail.token_input,
                view,
                theme,
                cx,
            ))
        }
        SettingsCapability::NewApiEditable { base_url } => {
            section.child(render_editable_provider_actions(
                EditableProviderActions::newapi(detail.id.clone(), base_url),
                detail.confirming_delete_newapi,
                dispatcher,
                theme,
            ))
        }
        SettingsCapability::ScriptEditable { interpreter } => {
            section.child(render_editable_provider_actions(
                EditableProviderActions::script_provider(detail.id.clone(), interpreter),
                detail.confirming_delete_script_provider,
                dispatcher,
                theme,
            ))
        }
        SettingsCapability::None => {
            if !detail.can_refresh && !detail.show_quota_thresholds {
                let placeholder =
                    placeholder_for_provider_capability(detail.provider_capability, &detail.usage);
                section.child(render_info_placeholder_card(
                    placeholder.title,
                    placeholder.description,
                    theme,
                ))
            } else {
                section
            }
        }
    };

    if detail.show_quota_visibility {
        section = section.child(quota_visibility::render_quota_visibility_section(
            detail.id.clone(),
            &detail.quota_visibility,
            dispatcher,
            theme,
        ));
    }

    if detail.can_refresh || detail.show_quota_thresholds {
        section = section.child(render_rules_card(
            view, detail, dispatcher, theme, window, cx,
        ));
    }

    section
}

/// 合并「用量提醒步长」与「额度状态与提醒阈值」为单张规则卡片，
/// 统一承载当前 Provider 对全局提醒与状态变色规则的按需覆盖。
fn render_rules_card(
    view: &mut SettingsView,
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Div {
    let header = render_settings_card_header(
        "src/icons/status.svg",
        t!("provider.rules.title").to_string(),
        None,
        t!("provider.rules.desc").to_string(),
        theme,
    );
    let mut card = render_settings_card(theme).child(header);

    if detail.can_refresh {
        let select_dispatcher = dispatcher.clone();
        let provider_id = detail.id.clone();
        card = card.child(
            crate::ui::settings_window::quota_usage::render_provider_quota_usage_row(
                detail.quota_usage_step_pct,
                detail.global_quota_usage_step_pct,
                theme,
                move |step, window, cx| {
                    select_dispatcher.dispatch(
                        AppAction::UpdateSetting(SettingChange::SetProviderQuotaUsageStep {
                            provider_id: provider_id.clone(),
                            step_pct: step,
                        }),
                        window,
                        cx,
                    );
                },
            ),
        );
    }

    if detail.show_quota_thresholds {
        let target = QuotaThresholdTarget::Provider(detail.id.clone());
        let on_change: std::rc::Rc<
            crate::ui::settings_window::quota_thresholds::QuotaThresholdChange,
        > = {
            let dispatcher = dispatcher.clone();
            std::rc::Rc::new(
                move |change: SettingChange, window: &mut Window, cx: &mut gpui::App| {
                    dispatcher.dispatch(AppAction::UpdateSetting(change), window, cx);
                },
            )
        };
        card = crate::ui::settings_window::quota_thresholds::append_provider_threshold_units(
            card,
            view,
            &target,
            &detail.quota_thresholds,
            theme,
            window,
            cx,
            &on_change,
        );
    }

    card
}

struct PlaceholderText {
    title: String,
    description: String,
}

fn placeholder_for_provider_capability(
    capability: ProviderCapability,
    usage: &SettingsProviderUsageViewState,
) -> PlaceholderText {
    let usage_note = match usage {
        SettingsProviderUsageViewState::Empty { message } => Some(message.as_str()),
        _ => None,
    };
    match capability {
        ProviderCapability::Informational => PlaceholderText {
            title: t!("provider.informational.title").to_string(),
            description: usage_note
                .map(str::to_string)
                .unwrap_or_else(|| t!("provider.informational.desc").to_string()),
        },
        ProviderCapability::Placeholder | ProviderCapability::Monitorable => PlaceholderText {
            title: t!("provider.placeholder.title").to_string(),
            description: usage_note
                .map(str::to_string)
                .unwrap_or_else(|| t!("provider.placeholder.desc").to_string()),
        },
    }
}

fn render_info_placeholder_card(title: String, description: String, theme: &Theme) -> Div {
    let muted_color = hsla(0.0, 0.0, 0.45, 0.5);

    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .py(px(32.0))
        .px(px(20.0))
        .rounded(px(12.0))
        .bg(theme.bg.card_inner)
        .border_1()
        .border_color(theme.border.subtle)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w_full()
                .child(render_svg_icon(
                    "src/icons/settings.svg",
                    px(28.0),
                    muted_color,
                )),
        )
        .child(
            div()
                .mt(px(12.0))
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(theme.text.primary)
                        .text_align(TextAlign::Center)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(relative(1.45))
                        .text_color(theme.text.muted)
                        .text_align(TextAlign::Center)
                        .child(description),
                ),
        )
}
