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
    history_line_color, render_detail_section_title, render_history_line_chart,
    render_history_retention_dropdown, render_segmented_control, render_svg_icon,
    HistoryRetentionMenu, SegmentedSize,
};
use gpui::{div, px, relative, App, Div, InteractiveElement, ParentElement, Styled, Window};
use rust_i18n::t;

pub(super) fn render_history_section(
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let history = &detail.history;
    let mut card = div()
        .flex()
        .flex_col()
        .mt(px(10.0))
        .px(px(16.0))
        .py(px(14.0))
        .gap(px(12.0))
        .rounded(px(12.0))
        .bg(theme.bg.card_inner)
        .border_1()
        .border_color(theme.border.strong)
        .child(render_toolbar(detail, dispatcher, theme));

    let mut notes = Vec::new();
    card = match &history.phase {
        SettingsProviderHistoryPhase::Message(message) => card.child(
            div()
                .py(px(18.0))
                .px(px(12.0))
                .rounded(px(8.0))
                .bg(theme.bg.card)
                .border_1()
                .border_color(theme.border.subtle)
                .child(
                    div()
                        .text_size(px(12.5))
                        .line_height(relative(1.45))
                        .text_color(theme.text.muted)
                        .child(message.clone()),
                ),
        ),
        SettingsProviderHistoryPhase::Charts(charts) => {
            let mut card = card;
            for (index, chart) in charts.iter().enumerate() {
                let mut chart_block = div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(render_chart_legend(&chart.lines, &chart.title, theme))
                    .child(render_history_line_chart(chart, 116.0, theme));
                if index > 0 {
                    chart_block = chart_block
                        .pt(px(12.0))
                        .border_t_1()
                        .border_color(theme.border.subtle);
                }
                card = card.child(chart_block);
            }
            if charts_have_gap(charts) {
                notes.push(t!("provider.history.gap_hint").to_string());
            }
            if charts_include_hidden_quota(detail, charts) {
                notes.push(t!("provider.history.includes_hidden").to_string());
            }
            card
        }
    };

    card = card.child(render_history_footer(
        notes,
        history.confirming_clear,
        dispatcher,
        theme,
    ));

    div()
        .flex()
        .flex_col()
        .mt(px(20.0))
        .child(render_detail_section_title(
            &t!("provider.history.section"),
            theme,
        ))
        .child(card)
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

fn render_chart_legend(lines: &[HistoryLineView], fallback_title: &str, theme: &Theme) -> Div {
    let mut row = div().flex().flex_wrap().items_center().gap(px(12.0));
    if lines.is_empty() {
        return row.child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text.secondary)
                .child(fallback_title.to_string()),
        );
    }
    for line in lines {
        let title = if line.title.is_empty() {
            fallback_title.to_string()
        } else {
            line.title.clone()
        };
        row = row.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .size(px(7.0))
                        .rounded_full()
                        .bg(history_line_color(line.color_index, theme)),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.text.secondary)
                        .child(title),
                ),
        );
    }
    row
}

fn render_history_footer(
    notes: Vec<String>,
    confirming_clear: bool,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let mut notes_col = div().flex_1().min_w_0().flex().flex_col().gap(px(3.0));
    for note in notes {
        notes_col = notes_col.child(
            div()
                .text_size(px(11.0))
                .text_color(theme.text.muted)
                .child(note),
        );
    }

    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .pt(px(10.0))
        .border_t_1()
        .border_color(theme.border.subtle)
        .child(notes_col)
        .child(render_clear_control(confirming_clear, dispatcher, theme))
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
        .pb(px(10.0))
        .border_b_1()
        .border_color(theme.border.subtle)
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
        Some(days) => t!("provider.history.retention_effective", n = days).to_string(),
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

    let mut row = div().flex().items_center().gap(px(6.0));
    if history.retention_override.is_none() {
        row = row.child(
            div()
                .text_size(px(11.5))
                .text_color(theme.text.muted)
                .whitespace_nowrap()
                .child(
                    t!(
                        "provider.history.retention_effective",
                        n = history.effective_days
                    )
                    .to_string(),
                ),
        );
    }

    row.child(render_history_retention_dropdown(
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
    let hover_bg = theme.bg.card_inner_hovered;
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(5.0))
        .px(px(8.0))
        .py(px(4.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(move |style| style.bg(hover_bg))
        .child(render_svg_icon(
            "src/icons/trash.svg",
            px(11.0),
            theme.text.muted,
        ))
        .child(
            div()
                .text_size(px(11.5))
                .text_color(theme.text.secondary)
                .child(t!("provider.history.clear").to_string()),
        )
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            begin.dispatch(AppAction::BeginClearProviderHistory, window, cx);
        })
}
