use super::providers::shared::{
    render_input_field, render_settings_card, render_settings_card_title, FormFieldSpec,
};
use super::SettingsView;
use crate::application::{QuotaThresholdUnitViewState, SettingChange, SettingsTab};
use crate::models::{
    ProviderId, QuotaThresholdTarget, QuotaThresholdUnit, QuotaThresholds, QuotaThresholdsError,
};
use crate::theme::Theme;
use adabraka_ui::components::input_state::InputState;
use gpui::{
    div, px, AppContext, Context, Div, Entity, FontWeight, InteractiveElement, MouseButton,
    MouseDownEvent, ParentElement, Styled, Window,
};
use rust_i18n::t;
use std::rc::Rc;

pub(crate) struct QuotaThresholdDraft {
    pub target: QuotaThresholdTarget,
    pub unit: QuotaThresholdUnit,
    pub warning: Entity<InputState>,
    pub critical: Entity<InputState>,
    pub notify: Entity<InputState>,
    pub error: Option<QuotaThresholdsError>,
}

impl SettingsView {
    pub(in crate::ui::settings_window) fn begin_quota_threshold_draft(
        &mut self,
        target: QuotaThresholdTarget,
        unit: QuotaThresholdUnit,
        prefill: QuotaThresholds,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = &self.quota_threshold_draft {
            if draft.target == target && draft.unit == unit {
                return;
            }
        }
        let threshold_input = |value: f64, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut state = InputState::new(cx);
                state.content = format!("{value}").into();
                state.trim_on_blur = false;
                state
            })
        };
        self.quota_threshold_draft = Some(QuotaThresholdDraft {
            target,
            unit,
            warning: threshold_input(prefill.warning, cx),
            critical: threshold_input(prefill.critical, cx),
            notify: threshold_input(prefill.notify, cx),
            error: None,
        });
        cx.notify();
    }

    pub(in crate::ui::settings_window) fn clear_quota_threshold_draft(&mut self) {
        self.quota_threshold_draft = None;
    }

    pub(in crate::ui::settings_window) fn save_quota_threshold_draft(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<SettingChange> {
        let Some(draft) = &self.quota_threshold_draft else {
            return None;
        };
        let warning = draft.warning.read(cx).content().to_string();
        let critical = draft.critical.read(cx).content().to_string();
        let notify = draft.notify.read(cx).content().to_string();
        let target = draft.target.clone();
        let unit = draft.unit;

        match QuotaThresholds::parse(unit, &warning, &critical, &notify) {
            Ok(thresholds) => {
                let change = match &target {
                    QuotaThresholdTarget::Global => {
                        SettingChange::SetGlobalQuotaThresholds { unit, thresholds }
                    }
                    QuotaThresholdTarget::Provider(provider_id) => {
                        SettingChange::SetProviderQuotaThresholds {
                            provider_id: provider_id.clone(),
                            unit,
                            thresholds: Some(thresholds),
                        }
                    }
                };
                self.clear_quota_threshold_draft();
                Some(change)
            }
            Err(error) => {
                if let Some(draft) = &mut self.quota_threshold_draft {
                    draft.error = Some(error);
                }
                cx.notify();
                None
            }
        }
    }
}

pub(in crate::ui::settings_window) type QuotaThresholdChange =
    dyn Fn(SettingChange, &mut Window, &mut gpui::App);

pub(in crate::ui::settings_window) fn quota_threshold_target_matches_tab(
    target: &QuotaThresholdTarget,
    tab: SettingsTab,
    selected: &ProviderId,
) -> bool {
    match target {
        QuotaThresholdTarget::Global => tab == SettingsTab::General,
        QuotaThresholdTarget::Provider(_) => {
            tab == SettingsTab::Providers && target.is_provider(selected)
        }
    }
}

pub(in crate::ui::settings_window) fn render_quota_thresholds_section(
    view: &mut SettingsView,
    target: QuotaThresholdTarget,
    units: &[QuotaThresholdUnitViewState],
    theme: &Theme,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
    on_change: Rc<QuotaThresholdChange>,
) -> Div {
    let mut card = render_settings_card(theme)
        .child(render_settings_card_title(
            &t!("settings.quota_thresholds.title"),
            theme,
        ))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text.muted)
                .child(t!("settings.quota_thresholds.desc").to_string()),
        );

    for unit_state in units {
        card = card.child(render_quota_threshold_unit(
            view, &target, unit_state, theme, window, cx, &on_change,
        ));
    }

    card
}

fn render_quota_threshold_unit(
    view: &mut SettingsView,
    target: &QuotaThresholdTarget,
    unit_state: &QuotaThresholdUnitViewState,
    theme: &Theme,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
    on_change: &Rc<QuotaThresholdChange>,
) -> Div {
    let unit = unit_state.unit;
    let editing = view
        .quota_threshold_draft
        .as_ref()
        .is_some_and(|draft| draft.target == *target && draft.unit == unit);

    let mut block = div()
        .flex_col()
        .gap(px(8.0))
        .pt(px(8.0))
        .border_t_1()
        .border_color(theme.border.subtle)
        .child(render_unit_header(target, unit_state, theme));

    if editing {
        block = block.child(render_draft_editor(view, theme, window, cx, on_change));
    } else {
        block = block.child(render_unit_summary(
            target, unit_state, theme, cx, on_change,
        ));
    }

    block
}

fn render_unit_header(
    target: &QuotaThresholdTarget,
    unit_state: &QuotaThresholdUnitViewState,
    theme: &Theme,
) -> Div {
    let mut row = div().flex().items_center().gap(px(8.0)).child(
        div()
            .text_size(px(13.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text.primary)
            .child(unit_label(unit_state.unit)),
    );

    if matches!(target, QuotaThresholdTarget::Provider(_)) {
        let (label, color) = if unit_state.override_thresholds.is_some() {
            (
                t!("settings.quota_thresholds.custom").to_string(),
                theme.text.accent,
            )
        } else {
            (
                t!("settings.quota_thresholds.inherit").to_string(),
                theme.text.muted,
            )
        };
        row = row.child(
            div()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(6.0))
                .bg(theme.bg.subtle)
                .text_size(px(11.0))
                .text_color(color)
                .child(label),
        );
    }

    row
}

fn render_unit_summary(
    target: &QuotaThresholdTarget,
    unit_state: &QuotaThresholdUnitViewState,
    theme: &Theme,
    cx: &mut Context<SettingsView>,
    on_change: &Rc<QuotaThresholdChange>,
) -> Div {
    let effective = unit_state.effective;
    let summary = t!(
        "settings.quota_thresholds.summary",
        warning = format!("{}", effective.warning),
        critical = format!("{}", effective.critical),
        notify = format!("{}", effective.notify),
    )
    .to_string();

    let mut actions = div().flex().items_center().gap(px(8.0));
    match target {
        QuotaThresholdTarget::Global => {
            actions = actions.child(threshold_action_button(
                &t!("settings.quota_thresholds.edit"),
                theme,
                edit_click_handler(cx, target.clone(), unit_state),
            ));
        }
        QuotaThresholdTarget::Provider(provider_id) => {
            if unit_state.override_thresholds.is_some() {
                actions = actions
                    .child(threshold_action_button(
                        &t!("settings.quota_thresholds.edit"),
                        theme,
                        edit_click_handler(cx, target.clone(), unit_state),
                    ))
                    .child(threshold_action_button(
                        &t!("settings.quota_thresholds.follow_global"),
                        theme,
                        follow_global_click_handler(
                            cx,
                            provider_id.clone(),
                            unit_state.unit,
                            on_change.clone(),
                        ),
                    ));
            } else {
                actions = actions.child(threshold_action_button(
                    &t!("settings.quota_thresholds.customize"),
                    theme,
                    edit_click_handler(cx, target.clone(), unit_state),
                ));
            }
        }
    }

    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text.muted)
                .child(summary),
        )
        .child(actions)
}

fn render_draft_editor(
    view: &mut SettingsView,
    theme: &Theme,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
    on_change: &Rc<QuotaThresholdChange>,
) -> Div {
    let Some(draft) = &view.quota_threshold_draft else {
        return div();
    };
    let warning_focused = draft.warning.read(cx).focus_handle(cx).is_focused(window);
    let critical_focused = draft.critical.read(cx).focus_handle(cx).is_focused(window);
    let notify_focused = draft.notify.read(cx).focus_handle(cx).is_focused(window);
    let error = draft.error;

    let mut editor = div()
        .flex_col()
        .gap(px(4.0))
        .child(render_input_field(
            FormFieldSpec {
                id: "quota-threshold-warning",
                label: &t!("settings.quota_thresholds.field.warning"),
                hint: None,
                is_focused: warning_focused,
                margin_top: px(0.0),
            },
            &draft.warning,
            theme,
            window,
            cx,
        ))
        .child(render_input_field(
            FormFieldSpec {
                id: "quota-threshold-critical",
                label: &t!("settings.quota_thresholds.field.critical"),
                hint: None,
                is_focused: critical_focused,
                margin_top: px(0.0),
            },
            &draft.critical,
            theme,
            window,
            cx,
        ))
        .child(render_input_field(
            FormFieldSpec {
                id: "quota-threshold-notify",
                label: &t!("settings.quota_thresholds.field.notify"),
                hint: None,
                is_focused: notify_focused,
                margin_top: px(0.0),
            },
            &draft.notify,
            theme,
            window,
            cx,
        ));

    if let Some(error) = error {
        editor = editor.child(
            div()
                .text_size(px(11.0))
                .text_color(theme.status.error)
                .child(quota_threshold_error_text(error)),
        );
    }

    let view_entity = cx.entity().clone();
    editor.child(
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .child(threshold_action_button(
                &t!("settings.quota_thresholds.cancel"),
                theme,
                {
                    let view_entity = view_entity.clone();
                    move |_, _, cx| {
                        view_entity.update(cx, |view, cx| {
                            view.clear_quota_threshold_draft();
                            cx.notify();
                        });
                    }
                },
            ))
            .child(threshold_action_button(
                &t!("settings.quota_thresholds.save"),
                theme,
                {
                    let on_change = on_change.clone();
                    move |_, window, cx| {
                        let on_change = on_change.clone();
                        view_entity.update(cx, |view, cx| {
                            if let Some(change) = view.save_quota_threshold_draft(cx) {
                                on_change(change, window, cx);
                            }
                        });
                    }
                },
            )),
    )
}

fn edit_click_handler(
    cx: &mut Context<SettingsView>,
    target: QuotaThresholdTarget,
    unit_state: &QuotaThresholdUnitViewState,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static {
    let view_entity = cx.entity().clone();
    let unit = unit_state.unit;
    let prefill = unit_state.effective;
    move |_, _, cx| {
        view_entity.update(cx, |view, cx| {
            view.begin_quota_threshold_draft(target.clone(), unit, prefill, cx);
        });
    }
}

fn follow_global_click_handler(
    cx: &mut Context<SettingsView>,
    provider_id: crate::models::ProviderId,
    unit: QuotaThresholdUnit,
    on_change: Rc<QuotaThresholdChange>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static {
    let view_entity = cx.entity().clone();
    move |_, window, cx| {
        view_entity.update(cx, |view, _| {
            view.clear_quota_threshold_draft();
        });
        on_change(
            SettingChange::SetProviderQuotaThresholds {
                provider_id: provider_id.clone(),
                unit,
                thresholds: None,
            },
            window,
            cx,
        );
    }
}

fn threshold_action_button(
    label: &str,
    theme: &Theme,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
) -> Div {
    div()
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(6.0))
        .bg(theme.bg.subtle)
        .border_1()
        .border_color(theme.border.strong)
        .cursor_pointer()
        .hover(|s| s.border_color(theme.text.accent))
        .text_size(px(12.0))
        .text_color(theme.text.primary)
        .child(label.to_string())
        .on_mouse_down(MouseButton::Left, on_click)
}

fn unit_label(unit: QuotaThresholdUnit) -> String {
    match unit {
        QuotaThresholdUnit::Percentage => {
            t!("settings.quota_thresholds.unit.percentage").to_string()
        }
        QuotaThresholdUnit::Currency => t!("settings.quota_thresholds.unit.currency").to_string(),
        QuotaThresholdUnit::Amount => t!("settings.quota_thresholds.unit.amount").to_string(),
    }
}

fn quota_threshold_error_text(error: QuotaThresholdsError) -> String {
    match error {
        QuotaThresholdsError::EmptyInput
        | QuotaThresholdsError::InvalidNumber
        | QuotaThresholdsError::NotFinite => {
            t!("settings.quota_thresholds.error.number").to_string()
        }
        QuotaThresholdsError::NotPositive => {
            t!("settings.quota_thresholds.error.positive").to_string()
        }
        QuotaThresholdsError::InvalidOrder => {
            t!("settings.quota_thresholds.error.order").to_string()
        }
        QuotaThresholdsError::PercentageExceeds100 => {
            t!("settings.quota_thresholds.error.over_100").to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProviderKind;

    #[test]
    fn global_draft_only_survives_on_general_tab() {
        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        let target = QuotaThresholdTarget::Global;

        assert!(quota_threshold_target_matches_tab(
            &target,
            SettingsTab::General,
            &claude
        ));
        for tab in [
            SettingsTab::Providers,
            SettingsTab::Display,
            SettingsTab::About,
            SettingsTab::Debug,
        ] {
            assert!(
                !quota_threshold_target_matches_tab(&target, tab, &claude),
                "Global 草稿不应在 {tab:?} 存活"
            );
        }
    }

    #[test]
    fn provider_draft_only_survives_on_owning_provider_detail() {
        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        let codex = ProviderId::BuiltIn(ProviderKind::Codex);
        let target = QuotaThresholdTarget::Provider(claude.clone());

        assert!(quota_threshold_target_matches_tab(
            &target,
            SettingsTab::Providers,
            &claude
        ));
        assert!(!quota_threshold_target_matches_tab(
            &target,
            SettingsTab::Providers,
            &codex
        ));
        for tab in [
            SettingsTab::General,
            SettingsTab::Display,
            SettingsTab::About,
            SettingsTab::Debug,
        ] {
            assert!(
                !quota_threshold_target_matches_tab(&target, tab, &claude),
                "Provider 草稿不应在 {tab:?} 存活"
            );
        }
        assert!(!quota_threshold_target_matches_tab(
            &QuotaThresholdTarget::Global,
            SettingsTab::Providers,
            &claude
        ));
    }
}
