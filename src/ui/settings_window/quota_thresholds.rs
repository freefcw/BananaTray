use super::providers::shared::{
    render_input_box, render_settings_card, render_settings_card_title,
};
use super::SettingsView;
use crate::application::{QuotaThresholdUnitViewState, SettingChange, SettingsTab};
use crate::models::{
    ProviderId, QuotaThresholdTarget, QuotaThresholdUnit, QuotaThresholds, QuotaThresholdsError,
};
use crate::theme::Theme;
use fc_ui::components::input_state::{InputEvent, InputState};
use gpui::{
    div, px, relative, AnyElement, AppContext, Context, Div, Entity, FontWeight, Hsla,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Styled, Window,
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
            let input = cx.new(|cx| {
                InputState::new(cx)
                    .value(format!("{value}"))
                    .trim_on_blur(false)
            });
            // 输入变化时重绘设置视图，让「正常」区间与百分比比例刻度跟随草稿实时联动
            cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify())
                .detach();
            input
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

    // 标题行固定高度：编辑态把操作按钮移到区间条下方，标题行只保留标题，进入 / 退出编辑时内容不跳动，
    // 窄宽度下也不会因按钮挤出卡片
    let mut header = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .min_h(px(26.0))
        .child(render_unit_header(target, unit_state, theme));
    if !editing {
        header = header.child(render_unit_actions(
            target, unit_state, theme, cx, on_change,
        ));
    }

    row = row.child(header);

    if editing {
        row = row.child(render_draft_editor(
            view,
            unit,
            unit_state.effective,
            theme,
            window,
            cx,
            on_change,
        ));
    } else {
        row = row.child(render_threshold_strip(
            threshold_segments(unit, unit_state.effective, theme),
            None,
            theme,
        ));
        if unit == QuotaThresholdUnit::Percentage {
            row = row.child(render_percentage_scale(unit_state.effective, theme));
        }
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

/// 区间条中的一段：标签行（色点 + 含义）+ 数值或输入框。
struct ThresholdSegment {
    selector: Option<&'static str>,
    label: String,
    color: Hsla,
    body: AnyElement,
}

/// 三个阈值档位，按剩余额度从少到多排列（弹窗提醒 ≤ 图标变红 < 图标变黄），
/// 颜色与托盘中额度状态色一致。
fn threshold_levels(theme: &Theme) -> [(&'static str, String, Hsla); 3] {
    [
        (
            "quota-threshold-notify",
            t!("settings.quota_thresholds.segment.notify").to_string(),
            theme.text.accent,
        ),
        (
            "quota-threshold-critical",
            t!("settings.quota_thresholds.segment.critical").to_string(),
            theme.status.error,
        ),
        (
            "quota-threshold-warning",
            t!("settings.quota_thresholds.segment.warning").to_string(),
            theme.status.warning,
        ),
    ]
}

/// 展示态区间条：三个阈值 + 「正常」区间，等宽排列，不随数值按比例缩放，
/// 因此货币 / 积分这类没有上限的单位也能使用同一套表现。
fn threshold_segments(
    unit: QuotaThresholdUnit,
    values: QuotaThresholds,
    theme: &Theme,
) -> Vec<ThresholdSegment> {
    let mut segments: Vec<ThresholdSegment> = threshold_levels(theme)
        .into_iter()
        .zip([values.notify, values.critical, values.warning])
        .map(|((_, label, color), value)| ThresholdSegment {
            selector: None,
            label,
            color,
            body: render_segment_value(format!("≤ {}", format_threshold_value(unit, value)), theme)
                .into_any_element(),
        })
        .collect();
    segments.push(normal_segment(unit, Some(values.warning), theme));
    segments
}

/// 「正常」区间：剩余额度高于图标变黄阈值；阈值暂不可解析时显示占位符。
fn normal_segment(
    unit: QuotaThresholdUnit,
    warning: Option<f64>,
    theme: &Theme,
) -> ThresholdSegment {
    let value = warning
        .map(|value| format!("> {}", format_threshold_value(unit, value)))
        .unwrap_or_else(|| "—".to_string());
    ThresholdSegment {
        selector: None,
        label: t!("settings.quota_thresholds.segment.normal").to_string(),
        color: theme.status.success,
        body: render_segment_value(value, theme)
            .text_color(theme.text.secondary)
            .into_any_element(),
    }
}

fn render_segment_value(value: String, theme: &Theme) -> Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(13.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text.primary)
        .child(value)
}

/// 单个区间块的最小宽度：容纳「色点 + 四字标签」与输入框，低于此宽度时改为两行两列。
const SEGMENT_MIN_WIDTH: f32 = 76.0;
const SEGMENT_GAP: f32 = 4.0;

fn render_threshold_strip(
    segments: Vec<ThresholdSegment>,
    selector: Option<&'static str>,
    theme: &Theme,
) -> Div {
    let mut strip = div().flex().flex_wrap().w_full().gap(px(SEGMENT_GAP));
    if let Some(selector) = selector {
        strip = strip.debug_selector(move || selector.to_string());
    }
    // 两两成组再整体换行：宽度足够时 4 列一行，窄窗口（如 Provider 详情页）退化为 2×2，
    // 不会出现 3 + 1 的割裂排布
    let mut segments = segments.into_iter().peekable();
    while segments.peek().is_some() {
        let mut pair = div()
            .flex()
            .flex_1()
            .min_w(px(SEGMENT_MIN_WIDTH * 2.0 + SEGMENT_GAP))
            .gap(px(SEGMENT_GAP));
        for segment in segments.by_ref().take(2) {
            pair = pair.child(render_threshold_segment(segment, theme));
        }
        strip = strip.child(pair);
    }
    strip
}

fn render_threshold_segment(segment: ThresholdSegment, theme: &Theme) -> Div {
    let mut label = div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(10.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text.muted)
        .child(segment.label);
    let mut cell = div()
        .flex_1()
        .min_w_0()
        .flex_col()
        .gap(px(4.0))
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .bg(segment.color.opacity(0.10));
    if let Some(selector) = segment.selector {
        label = label.debug_selector(move || format!("{selector}-label"));
        cell = cell.debug_selector(move || selector.to_string());
    }
    cell.child(
        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .min_w_0()
            .child(
                div()
                    .flex_shrink_0()
                    .size(px(6.0))
                    .rounded_full()
                    .bg(segment.color),
            )
            .child(label),
    )
    .child(segment.body)
}

/// 百分比单位有固定上限，额外画一条 0–100% 的比例刻度：红 / 黄 / 正常区间按真实宽度着色，
/// 弹窗提醒位置用强调色竖线标出。
fn render_percentage_scale(values: QuotaThresholds, theme: &Theme) -> Div {
    let critical = values.critical.clamp(0.0, 100.0);
    let warning = values.warning.clamp(critical, 100.0);
    let notify = values.notify.clamp(0.0, 100.0);
    let fraction = |value: f64| relative((value / 100.0) as f32);
    let end_label = |text: &'static str| {
        div()
            .flex_shrink_0()
            .text_size(px(10.0))
            .text_color(theme.text.muted)
            .child(text)
    };

    let bar = div()
        .relative()
        .flex_1()
        .min_w_0()
        .h(px(10.0))
        .child(
            div()
                .absolute()
                .top(px(2.0))
                .left_0()
                .right_0()
                .h(px(6.0))
                .flex()
                .rounded_full()
                .overflow_hidden()
                .bg(theme.status.progress_track)
                .child(
                    div()
                        .flex_shrink_0()
                        .h_full()
                        .w(fraction(critical))
                        .bg(theme.status.error),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .h_full()
                        .w(fraction(warning - critical))
                        .bg(theme.status.warning),
                )
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .bg(theme.status.success.opacity(0.35)),
                ),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left(fraction(notify))
                .ml(px(-1.0))
                .w(px(2.0))
                .h(px(10.0))
                .rounded(px(1.0))
                .bg(theme.text.accent),
        );

    div()
        .debug_selector(|| "quota-threshold-scale".to_string())
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(end_label("0%"))
        .child(bar)
        .child(end_label("100%"))
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

/// 编辑态：输入框原位替换区间条中的数值，布局与展示态一致，避免编辑时高度暴涨。
fn render_draft_editor(
    view: &mut SettingsView,
    unit: QuotaThresholdUnit,
    effective: QuotaThresholds,
    theme: &Theme,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
    on_change: &Rc<QuotaThresholdChange>,
) -> Div {
    let Some(draft) = &view.quota_threshold_draft else {
        return div();
    };
    let inputs = [
        draft.notify.clone(),
        draft.critical.clone(),
        draft.warning.clone(),
    ];
    let error = draft.error;

    let mut segments = Vec::with_capacity(4);
    for ((selector, label, color), entity) in threshold_levels(theme).into_iter().zip(&inputs) {
        let is_focused = entity.read(cx).focus_handle(cx).is_focused(window);
        segments.push(ThresholdSegment {
            selector: Some(selector),
            label,
            color,
            body: render_input_box(is_focused, false, entity, theme, window, cx)
                .debug_selector(move || format!("{selector}-input"))
                .h(px(28.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(6.0))
                .text_size(px(12.5))
                .into_any_element(),
        });
    }

    let [notify, critical, warning] = inputs.map(|input| input.read(cx).content().to_string());
    let draft_values = QuotaThresholds::parse(unit, &warning, &critical, &notify).ok();
    let warning_preview = draft_values.map(|values| values.warning).or_else(|| {
        warning
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
    });
    segments.push(normal_segment(unit, warning_preview, theme));

    let mut editor = div().flex_col().gap(px(10.0)).child(render_threshold_strip(
        segments,
        Some("quota-threshold-fields"),
        theme,
    ));
    if unit == QuotaThresholdUnit::Percentage {
        // 草稿合法时刻度实时跟随输入；不合法时保留当前生效值，避免刻度跳成无意义状态
        editor = editor.child(render_percentage_scale(
            draft_values.unwrap_or(effective),
            theme,
        ));
    }

    let (message, color) = match error {
        Some(error) => (quota_threshold_error_text(error), theme.status.error),
        None => (
            t!("settings.quota_thresholds.hint").to_string(),
            theme.text.muted,
        ),
    };
    // 提示 / 错误与操作按钮同一行；宽度不足时按钮换到下一行并保持右对齐
    editor.child(
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(160.0))
                    .text_size(px(11.5))
                    .line_height(px(16.0))
                    .text_color(color)
                    .child(message),
            )
            .child(render_draft_actions(theme, cx, on_change).ml_auto()),
    )
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
