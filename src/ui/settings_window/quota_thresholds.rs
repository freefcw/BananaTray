use super::providers::shared::{
    render_input_box, render_settings_card, render_settings_card_title,
};
use super::SettingsView;
use crate::application::{QuotaThresholdUnitViewState, SettingChange, SettingsTab};
use crate::models::{
    ProviderId, QuotaThresholdTarget, QuotaThresholdUnit, QuotaThresholds, QuotaThresholdsError,
};
use crate::theme::Theme;
use fc_ui::components::input_state::InputState;
use gpui::{
    div, px, AppContext, Context, Div, Entity, FontWeight, Hsla, InteractiveElement, MouseButton,
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
                InputState::new(cx)
                    .value(format!("{value}"))
                    .trim_on_blur(false)
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
    if matches!(target, QuotaThresholdTarget::Global) {
        // 通用设置页：说明已在分区头部作为副标题展示，卡片直接使用与“系统/自动化”一致的单层暗色卡片，避免多层嵌套
        let mut card = super::components::render_dark_card(theme);
        for (index, unit_state) in units.iter().enumerate() {
            card = card.child(render_quota_threshold_unit(
                view,
                &target,
                unit_state,
                index > 0,
                theme,
                window,
                cx,
                &on_change,
            ));
        }
        card
    } else {
        // Provider 详情页：上方无全局分区标题，保留卡片标题与说明，结构与 Token/Usage 面板统一
        let card = render_settings_card(theme)
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

        let mut list = div()
            .flex_col()
            .w_full()
            .rounded(px(10.0))
            .border_1()
            .border_color(theme.border.subtle)
            .overflow_hidden();
        for (index, unit_state) in units.iter().enumerate() {
            list = list.child(render_quota_threshold_unit(
                view,
                &target,
                unit_state,
                index > 0,
                theme,
                window,
                cx,
                &on_change,
            ));
        }

        card.child(list)
    }
}

#[allow(clippy::too_many_arguments)]
fn render_quota_threshold_unit(
    view: &mut SettingsView,
    target: &QuotaThresholdTarget,
    unit_state: &QuotaThresholdUnitViewState,
    with_divider: bool,
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

    let mut row = div().flex_col().gap(px(12.0)).px(px(14.0)).py(px(12.0));
    if with_divider {
        row = row.border_t_1().border_color(theme.border.subtle);
    }
    if editing {
        row = row.bg(theme.bg.subtle);
    }

    let header = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(render_unit_header(target, unit_state, theme))
        .child(if editing {
            render_draft_actions(theme, cx, on_change)
        } else {
            render_unit_actions(target, unit_state, theme, cx, on_change)
        });

    row = row.child(header);

    if editing {
        row = row.child(render_draft_editor(view, unit, theme, window, cx));
    } else {
        row = row.child(render_threshold_chips(unit, unit_state.effective, theme));
    }

    row
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
                .py(px(1.0))
                .rounded(px(5.0))
                .border_1()
                .border_color(theme.border.subtle)
                .text_size(px(10.5))
                .text_color(color)
                .child(label),
        );
    }

    row
}

/// 三个阈值徽章：颜色与托盘中额度状态色一致，让用户一眼对应「黄 / 红 / 通知」。
fn render_threshold_chips(unit: QuotaThresholdUnit, values: QuotaThresholds, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .child(render_threshold_chip(
            t!(
                "settings.quota_thresholds.chip.notify",
                value = format_threshold_value(unit, values.notify)
            )
            .to_string(),
            theme.text.accent,
            theme,
        ))
        .child(render_threshold_chip(
            t!(
                "settings.quota_thresholds.chip.critical",
                value = format_threshold_value(unit, values.critical)
            )
            .to_string(),
            theme.status.error,
            theme,
        ))
        .child(render_threshold_chip(
            t!(
                "settings.quota_thresholds.chip.warning",
                value = format_threshold_value(unit, values.warning)
            )
            .to_string(),
            theme.status.warning,
            theme,
        ))
}

fn render_threshold_chip(label: String, color: Hsla, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .px(px(8.0))
        .py(px(3.0))
        .rounded(px(999.0))
        .bg(color.opacity(0.14))
        .child(div().size(px(6.0)).rounded_full().bg(color))
        .child(
            div()
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text.secondary)
                .child(label),
        )
}

fn render_unit_actions(
    target: &QuotaThresholdTarget,
    unit_state: &QuotaThresholdUnitViewState,
    theme: &Theme,
    cx: &mut Context<SettingsView>,
    on_change: &Rc<QuotaThresholdChange>,
) -> Div {
    let mut actions = div().flex().flex_shrink_0().items_center().gap(px(4.0));
    match target {
        QuotaThresholdTarget::Global => {
            actions = actions.child(threshold_ghost_button(
                &t!("settings.quota_thresholds.edit"),
                theme.text.accent,
                theme,
                edit_click_handler(cx, target.clone(), unit_state),
            ));
        }
        QuotaThresholdTarget::Provider(provider_id) => {
            if unit_state.override_thresholds.is_some() {
                actions = actions
                    .child(threshold_ghost_button(
                        &t!("settings.quota_thresholds.follow_global"),
                        theme.text.secondary,
                        theme,
                        follow_global_click_handler(
                            cx,
                            provider_id.clone(),
                            unit_state.unit,
                            on_change.clone(),
                        ),
                    ))
                    .child(threshold_ghost_button(
                        &t!("settings.quota_thresholds.edit"),
                        theme.text.accent,
                        theme,
                        edit_click_handler(cx, target.clone(), unit_state),
                    ));
            } else {
                actions = actions.child(threshold_ghost_button(
                    &t!("settings.quota_thresholds.customize"),
                    theme.text.accent,
                    theme,
                    edit_click_handler(cx, target.clone(), unit_state),
                ));
            }
        }
    }
    actions
}

fn render_draft_editor(
    view: &mut SettingsView,
    unit: QuotaThresholdUnit,
    theme: &Theme,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Div {
    let Some(draft) = &view.quota_threshold_draft else {
        return div();
    };
    let fields = [
        (
            "quota-threshold-notify",
            field_label_with_unit(&t!("settings.quota_thresholds.field.notify"), unit),
            theme.text.accent,
            draft.notify.clone(),
        ),
        (
            "quota-threshold-critical",
            field_label_with_unit(&t!("settings.quota_thresholds.field.critical"), unit),
            theme.status.error,
            draft.critical.clone(),
        ),
        (
            "quota-threshold-warning",
            field_label_with_unit(&t!("settings.quota_thresholds.field.warning"), unit),
            theme.status.warning,
            draft.warning.clone(),
        ),
    ];
    let error = draft.error;

    let mut inputs = div()
        .debug_selector(|| "quota-threshold-fields".to_string())
        .flex()
        .flex_col()
        .w_full()
        .gap(px(10.0));
    for (id, label, color, entity) in fields {
        let is_focused = entity.read(cx).focus_handle(cx).is_focused(window);
        inputs = inputs.child(
            div()
                .debug_selector(move || id.to_string())
                .flex_shrink_0()
                .w_full()
                .min_w_0()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().size(px(6.0)).rounded_full().bg(color))
                        .child(
                            div()
                                .debug_selector(move || format!("{id}-label"))
                                .text_size(px(12.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text.secondary)
                                .child(label),
                        ),
                )
                .child(render_input_box(
                    is_focused, false, &entity, theme, window, cx,
                )),
        );
    }

    let mut editor = div().flex_col().gap(px(10.0)).child(inputs);

    if let Some(error) = error {
        editor = editor.child(
            div()
                .text_size(px(11.5))
                .line_height(px(16.0))
                .text_color(theme.status.error)
                .child(quota_threshold_error_text(error)),
        );
    }

    editor
}

fn render_draft_actions(
    theme: &Theme,
    cx: &mut Context<SettingsView>,
    on_change: &Rc<QuotaThresholdChange>,
) -> Div {
    let view_entity = cx.entity().clone();
    let on_change = on_change.clone();

    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .child(
            threshold_ghost_button(
                &t!("settings.quota_thresholds.cancel"),
                theme.text.secondary,
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
            )
            .debug_selector(|| "quota-threshold-cancel".to_string()),
        )
        .child(
            threshold_primary_button(
                &t!("settings.quota_thresholds.save"),
                theme,
                move |_, window, cx| {
                    let on_change = on_change.clone();
                    view_entity.update(cx, |view, cx| {
                        if let Some(change) = view.save_quota_threshold_draft(cx) {
                            on_change(change, window, cx);
                        }
                    });
                },
            )
            .debug_selector(|| "quota-threshold-save".to_string()),
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

/// 轻量文字按钮：无边框，hover 时出现底色，用于行内次要操作。
fn threshold_ghost_button(
    label: &str,
    color: Hsla,
    theme: &Theme,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
) -> Div {
    let hover_bg = theme.bg.card_inner_hovered;
    div()
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .whitespace_nowrap()
        .child(label.to_string())
        .on_mouse_down(MouseButton::Left, on_click)
}

fn threshold_primary_button(
    label: &str,
    theme: &Theme,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
) -> Div {
    div()
        .px(px(12.0))
        .py(px(5.0))
        .rounded(px(6.0))
        .bg(theme.text.accent)
        .cursor_pointer()
        .hover(|s| s.opacity(0.9))
        .text_size(px(12.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.element.active)
        .whitespace_nowrap()
        .child(label.to_string())
        .on_mouse_down(MouseButton::Left, on_click)
}

/// 按单位格式化输入框标签：显示对应的单位符号以指导输入。
fn field_label_with_unit(base: &str, unit: QuotaThresholdUnit) -> String {
    match unit {
        QuotaThresholdUnit::Percentage => format!("{base} ≤ (%)"),
        QuotaThresholdUnit::Currency => format!("{base} ≤ ($)"),
        QuotaThresholdUnit::Amount => format!("{base} ≤"),
    }
}

/// 按单位格式化阈值：百分比带 `%`，货币带 `$` 前缀，积分 / 额度保持原值。
fn format_threshold_value(unit: QuotaThresholdUnit, value: f64) -> String {
    match unit {
        QuotaThresholdUnit::Percentage => format!("{value}%"),
        QuotaThresholdUnit::Currency => format!("${value}"),
        QuotaThresholdUnit::Amount => format!("{value}"),
    }
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
    fn field_labels_include_unit_context() {
        assert_eq!(
            field_label_with_unit("预警", QuotaThresholdUnit::Percentage),
            "预警 ≤ (%)"
        );
        assert_eq!(
            field_label_with_unit("预警", QuotaThresholdUnit::Currency),
            "预警 ≤ ($)"
        );
        assert_eq!(
            field_label_with_unit("预警", QuotaThresholdUnit::Amount),
            "预警 ≤"
        );
    }

    #[test]
    fn threshold_values_are_formatted_per_unit() {
        assert_eq!(
            format_threshold_value(QuotaThresholdUnit::Percentage, 50.0),
            "50%"
        );
        assert_eq!(
            format_threshold_value(QuotaThresholdUnit::Currency, 2.5),
            "$2.5"
        );
        assert_eq!(
            format_threshold_value(QuotaThresholdUnit::Amount, 100.0),
            "100"
        );
    }

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
