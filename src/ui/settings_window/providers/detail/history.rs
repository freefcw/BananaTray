use super::DetailActionDispatcher;
use crate::application::{
    AppAction, SettingChange, SettingsHistoryPointView, SettingsHistorySegmentView,
    SettingsProviderDetailViewState, SettingsProviderHistoryPhase,
};
use crate::history::HistoryRange;
use crate::models::RETENTION_PRESETS;
use crate::theme::Theme;
use crate::ui::settings_window::providers::shared;
use crate::ui::widgets::{
    render_detail_empty_card, render_detail_section_title, render_history_retention_dropdown,
    render_segmented_control, SegmentedSize,
};
use gpui::{
    canvas, div, fill, point, px, size, Bounds, Div, Hsla, InteractiveElement, ParentElement,
    PathBuilder, Pixels, Point, Styled, Window,
};
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
        .child(render_toolbar(detail, dispatcher, theme))
        .child(render_retention_row(detail, dispatcher, theme));

    section = match &history.phase {
        SettingsProviderHistoryPhase::Message(message) => {
            section.child(render_detail_empty_card(message, theme))
        }
        SettingsProviderHistoryPhase::Charts(charts) => {
            let mut section = section.child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text.muted)
                    .child(t!("provider.history.gap_hint").to_string()),
            );
            for chart in charts {
                let segments = chart.segments.clone();
                let color = theme.text.accent;
                section = section.child(
                    div()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.text.secondary)
                                .child(chart.title.clone()),
                        )
                        .child(
                            div().h(px(140.0)).w_full().child(
                                canvas(
                                    |_, _, _| {},
                                    move |bounds, _, window, _| {
                                        paint_chart(bounds, &segments, color, window);
                                    },
                                )
                                .size_full(),
                            ),
                        ),
                );
            }
            section.child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text.muted)
                    .child(t!("provider.history.includes_hidden").to_string()),
            )
        }
    };
    section
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
        .child(render_segmented_control(
            &options,
            &detail.history.range,
            SegmentedSize::Compact,
            theme,
            move |range, window, cx| {
                range_dispatcher.dispatch(AppAction::SetHistoryRange(range), window, cx);
            },
        ))
}

fn render_retention_row(
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

    let mut row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div().flex_col().gap(px(2.0)).child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text.secondary)
                    .child(
                        t!(
                            "provider.history.retention_effective",
                            n = history.effective_days
                        )
                        .to_string(),
                    ),
            ),
        )
        .child(render_history_retention_dropdown(
            label,
            history.dropdown_open,
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
        ));

    row = row.child(render_clear_control(
        history.confirming_clear,
        dispatcher,
        theme,
    ));
    row
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
        .h(px(24.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .rounded(px(6.0))
        .bg(theme.bg.subtle)
        .cursor_pointer()
        .hover(|style| style.opacity(0.8))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme.text.secondary)
                .child(t!("provider.history.clear").to_string()),
        )
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            begin.dispatch(AppAction::BeginClearProviderHistory, window, cx);
        })
}

fn paint_chart(
    bounds: Bounds<Pixels>,
    segments: &[SettingsHistorySegmentView],
    color: Hsla,
    window: &mut Window,
) {
    for segment in segments {
        paint_segment(bounds, &segment.points, color, window);
    }
}

fn paint_segment(
    bounds: Bounds<Pixels>,
    points: &[SettingsHistoryPointView],
    color: Hsla,
    window: &mut Window,
) {
    let mut run = Vec::new();
    for point in points {
        if point.gap_before && !run.is_empty() {
            paint_run(&run, color, window);
            run.clear();
        }
        run.push(chart_point(bounds, point.x_ratio, point.y_ratio));
    }
    if !run.is_empty() {
        paint_run(&run, color, window);
    }
}

fn paint_run(points: &[Point<Pixels>], color: Hsla, window: &mut Window) {
    if let [only] = points {
        window.paint_quad(fill(
            Bounds {
                origin: point(only.x - px(1.5), only.y - px(1.5)),
                size: size(px(3.0), px(3.0)),
            },
            color,
        ));
        return;
    }
    let mut builder = PathBuilder::stroke(px(1.5));
    builder.move_to(points[0]);
    for point in &points[1..] {
        builder.line_to(*point);
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

fn chart_point(bounds: Bounds<Pixels>, x_ratio: f64, y_ratio: f64) -> Point<Pixels> {
    let x = bounds.origin.x + bounds.size.width * (x_ratio as f32);
    let y = bounds.origin.y + bounds.size.height * (1.0 - y_ratio as f32);
    point(x, y)
}
