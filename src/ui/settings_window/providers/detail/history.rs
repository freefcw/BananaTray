use super::DetailActionDispatcher;
use crate::application::{
    AppAction, HistoryChartView, HistoryLineView, SettingChange, SettingsProviderDetailViewState,
    SettingsProviderHistoryPhase,
};
use crate::history::HistoryRange;
use crate::models::RETENTION_PRESETS;
use crate::theme::Theme;
use crate::ui::settings_window::providers::shared;
use crate::ui::widgets::{
    history_line_color, render_detail_empty_card, render_detail_section_title,
    render_history_line_chart, render_history_retention_dropdown, render_segmented_control,
    HistoryRetentionMenu, SegmentedSize,
};
use gpui::{div, px, App, Div, InteractiveElement, ParentElement, Styled, Window};
use rust_i18n::t;

pub(super) fn render_history_section(
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let history = &detail.history;
    let mut section = div()
        .flex_col()
        .mt(px(20.0))
        .gap(px(10.0))
        .child(render_detail_section_title(
            &t!("provider.history.section"),
            theme,
        ))
        .child(render_toolbar(detail, dispatcher, theme));

    section = match &history.phase {
        SettingsProviderHistoryPhase::Message(message) => {
            section.child(render_detail_empty_card(message, theme))
        }
        SettingsProviderHistoryPhase::Charts(charts) => {
            let mut section = section;
            for chart in charts {
                let heading = if chart.lines.len() > 1 {
                    render_chart_legend(&chart.lines, theme)
                } else {
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.text.secondary)
                        .child(chart.title.clone())
                };
                section = section.child(div().flex_col().gap(px(6.0)).child(heading).child(
                    render_history_line_chart(
                        &chart.quota_key,
                        chart.lines.clone(),
                        chart.axis.clone(),
                        120.0,
                        theme,
                    ),
                ));
            }
            if charts_have_gap(charts) {
                section =
                    section.child(render_history_note(&t!("provider.history.gap_hint"), theme));
            }
            if charts_include_hidden_quota(detail, charts) {
                section = section.child(render_history_note(
                    &t!("provider.history.includes_hidden"),
                    theme,
                ));
            }
            section
        }
    };
    section.child(render_clear_control(
        history.confirming_clear,
        dispatcher,
        theme,
    ))
}

fn charts_have_gap(charts: &[HistoryChartView]) -> bool {
    charts.iter().any(|chart| {
        chart.lines.iter().any(|line| {
            line.segments
                .iter()
                .any(|segment| segment.points.iter().any(|point| point.gap_before))
        })
    })
}

fn charts_include_hidden_quota(
    detail: &SettingsProviderDetailViewState,
    charts: &[HistoryChartView],
) -> bool {
    charts.iter().any(|chart| {
        chart.lines.iter().any(|line| {
            detail
                .quota_visibility
                .iter()
                .any(|item| !item.visible && item.quota_key == line.quota_key)
        })
    })
}

fn render_chart_legend(lines: &[HistoryLineView], theme: &Theme) -> Div {
    let mut row = div().flex().flex_wrap().items_center().gap(px(10.0));
    for line in lines {
        row = row.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .size(px(8.0))
                        .rounded_full()
                        .bg(history_line_color(line.color_index, theme)),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.text.secondary)
                        .child(line.title.clone()),
                ),
        );
    }
    row
}

fn render_history_note(text: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(11.0))
        .text_color(theme.text.muted)
        .child(text.to_string())
}

fn render_toolbar(
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let range_dispatcher = dispatcher.clone();
    let options = vec![
        (
            t!("provider.history.range.24h").to_string(),
            HistoryRange::Last24Hours,
        ),
        (
            t!("provider.history.range.7d").to_string(),
            HistoryRange::Last7Days,
        ),
        (
            t!("provider.history.range.30d").to_string(),
            HistoryRange::Last30Days,
        ),
    ];
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(render_range_control(
            &options,
            &detail.history.range,
            theme,
            move |range, window, cx| {
                range_dispatcher.dispatch(AppAction::SetHistoryRange(range), window, cx);
            },
        ))
        .child(render_retention_controls(detail, dispatcher, theme))
}

fn render_range_control<F>(
    options: &[(String, HistoryRange)],
    current: &HistoryRange,
    theme: &Theme,
    on_select: F,
) -> Div
where
    F: Fn(HistoryRange, &mut Window, &mut App) + Clone + 'static,
{
    // 24h/7d/30d 三档 pill 组与外层容器均由分段控件提供：
    // SegmentedSize::HistoryRange 复现本页所需的紧凑尺寸与配色，避免重复手写一套控件。
    render_segmented_control(
        options,
        current,
        SegmentedSize::HistoryRange,
        theme,
        on_select,
    )
}

fn render_retention_controls(
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let history = &detail.history;
    let toggle = dispatcher.clone();
    let select = dispatcher.clone();
    let provider_id = detail.id.clone();
    let label = match history.retention_override {
        Some(days) => t!("settings.history_retention.days", n = days).to_string(),
        None => t!("provider.history.retention_follow_global").to_string(),
    };
    let mut options = vec![(
        t!("provider.history.retention_follow_global").to_string(),
        None,
    )];
    options.extend(RETENTION_PRESETS.into_iter().map(|days| {
        (
            t!("settings.history_retention.days", n = days).to_string(),
            Some(days),
        )
    }));

    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text.muted)
                .child(
                    t!(
                        "provider.history.retention_effective",
                        n = history.effective_days
                    )
                    .to_string(),
                ),
        )
        .child(render_history_retention_dropdown(
            label,
            HistoryRetentionMenu {
                open: history.dropdown_open,
                compact: true,
            },
            options,
            &history.retention_override,
            theme,
            move |_, window, cx| {
                toggle.dispatch(
                    AppAction::ToggleProviderHistoryRetentionDropdown,
                    window,
                    cx,
                );
            },
            move |days, window, cx| {
                select.dispatch(
                    AppAction::UpdateSetting(SettingChange::SetProviderHistoryRetentionDays {
                        provider_id: provider_id.clone(),
                        days,
                    }),
                    window,
                    cx,
                );
            },
        ))
}

fn render_clear_control(
    confirming: bool,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    if confirming {
        let confirm = dispatcher.clone();
        let cancel = dispatcher.clone();
        return shared::render_confirm_cancel_buttons(
            &t!("common.confirm"),
            &t!("common.cancel"),
            move |_, window, cx| {
                confirm.dispatch(AppAction::ConfirmClearProviderHistory, window, cx);
            },
            move |_, window, cx| {
                cancel.dispatch(AppAction::CancelClearProviderHistory, window, cx);
            },
            theme,
        );
    }
    let begin = dispatcher.clone();
    div()
        .cursor_pointer()
        .hover(|style| style.opacity(0.8))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme.text.muted)
                .child(t!("provider.history.clear").to_string()),
        )
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            begin.dispatch(AppAction::BeginClearProviderHistory, window, cx);
        })
}
