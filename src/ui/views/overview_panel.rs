use crate::application::{
    format_quota_status_label, overview_view_state, AppAction, OverviewItemStatus,
    OverviewItemViewState, OverviewQuotaItem,
};
use crate::models::{NavTab, PopupLayout, ProviderId, StatusLevel};
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::{with_multiline_tooltip, with_tooltip};
use crate::ui::AppView;
use gpui::{
    div, px, relative, AnyElement, App, Context, Div, ElementId, Entity, FontWeight, Hsla,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Stateful, Styled, TextAlign,
    Window,
};
use rust_i18n::t;

/// 纯余额/无上限配额的分段虚线数量
const BALANCE_TRACK_DASH_COUNT: usize = 5;

/// Provider 名称单元格：[name] [更新失败徽标?] [折叠态配额维度?]。
///
/// 刷新失败但展示陈旧旧值时，名称旁挂「更新失败」警示徽标，
/// tooltip 说明失败原因——不能让旧数据冒充正常数据。
/// 多配额卡片处于折叠态时，名称右侧附带当前所展示最差配额的标签（如「周配额」）。
fn render_provider_name_cell(
    item: &OverviewItemViewState,
    name_color: Hsla,
    sub_label: Option<&str>,
    theme: &Theme,
) -> Div {
    let sub_label = sub_label.filter(|s| !s.is_empty());
    let mut name_div = div()
        .text_size(px(12.0))
        .line_height(relative(1.3))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(name_color)
        .overflow_hidden()
        .whitespace_nowrap()
        .child(item.display_name.clone());

    if sub_label.is_some() {
        name_div = name_div.flex_shrink_0();
    } else {
        name_div = name_div.min_w_0();
    }

    let mut cell = div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(5.0))
        .overflow_hidden()
        .child(name_div);

    if item.refresh_failed {
        let hint = item
            .failure_hint
            .clone()
            .unwrap_or_else(|| t!("provider.update_failed_badge").to_string());
        cell = cell.child(with_multiline_tooltip(
            format!("overview-stale-{}", item.id.id_key()),
            &hint,
            theme,
            div()
                .flex_shrink_0()
                .text_size(px(9.0))
                .font_weight(FontWeight::BOLD)
                .text_color(theme.status.warning)
                .child(t!("provider.update_failed_badge").to_string()),
        ));
    }

    if let Some(sub) = sub_label {
        cell = cell.child(
            div()
                .min_w_0()
                .text_size(px(11.0))
                .line_height(relative(1.3))
                .text_color(theme.text.muted)
                .overflow_hidden()
                .whitespace_nowrap()
                .child(sub.to_string()),
        );
    }

    cell
}

/// Overview 卡片公共外壳：id + 宽度 + 内边距 + 圆角 + 背景 + 边框 + 悬停态
fn render_card_shell(item: &OverviewItemViewState, theme: &Theme) -> Stateful<Div> {
    let hover_bg = theme.bg.card_inner_hovered;
    div()
        .id(ElementId::Name(
            format!("overview-{}", item.id.id_key()).into(),
        ))
        .w_full()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(10.0))
        .bg(theme.bg.card_inner)
        .border_1()
        .border_color(theme.border.subtle)
        .cursor_pointer()
        .hover(move |style| style.bg(hover_bg))
}

/// 向行容器追加 Provider 左侧标识三元组：[状态点] [图标] [名称单元格]
fn append_provider_identity<E: ParentElement>(
    row: E,
    item: &OverviewItemViewState,
    dot_bg: Hsla,
    icon_color: Hsla,
    name_color: Hsla,
    sub_label: Option<&str>,
    theme: &Theme,
) -> E {
    row.child(
        div()
            .w(px(PopupLayout::OVERVIEW_DOT_SIZE))
            .h(px(PopupLayout::OVERVIEW_DOT_SIZE))
            .rounded_full()
            .bg(dot_bg),
    )
    .child(crate::ui::widgets::render_provider_icon(
        item.icon.clone(),
        px(PopupLayout::OVERVIEW_ICON_SIZE),
        icon_color,
        theme.bg.card_inner,
    ))
    .child(render_provider_name_cell(
        item, name_color, sub_label, theme,
    ))
}

/// 单行卡片的公共基础行（外壳 + 水平 flex + 左侧标识）
fn build_single_line_card_row(
    item: &OverviewItemViewState,
    dot_bg: Hsla,
    icon_color: Hsla,
    name_color: Hsla,
    sub_label: Option<&str>,
    theme: &Theme,
) -> Stateful<Div> {
    let row = render_card_shell(item, theme)
        .flex()
        .items_center()
        .gap(px(PopupLayout::OVERVIEW_GAP));
    append_provider_identity(row, item, dot_bg, icon_color, name_color, sub_label, theme)
}

/// 状态型单行卡片基础行（Refreshing / Error / Disconnected，无折叠子标签）
fn build_status_card_row(
    item: &OverviewItemViewState,
    dot_bg: Hsla,
    icon_color: Hsla,
    name_color: Hsla,
    theme: &Theme,
) -> Stateful<Div> {
    build_single_line_card_row(item, dot_bg, icon_color, name_color, None, theme)
        .min_h(px(PopupLayout::OVERVIEW_ITEM_HEIGHT))
}

/// 空状态：无已启用 Provider
fn render_overview_empty(theme: &Theme) -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .py(px(40.0))
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text.secondary)
                .child(t!("provider.no_usage_details").to_string()),
        )
        .into_any_element()
}

/// Disconnected 状态卡片
fn render_card_disconnected(item: &OverviewItemViewState, theme: &Theme) -> Stateful<Div> {
    build_status_card_row(
        item,
        theme.text.muted,
        theme.text.muted,
        theme.text.muted,
        theme,
    )
    .child(
        div()
            .text_size(px(11.0))
            .text_color(theme.text.muted)
            .child(t!("provider.not_connected").to_string()),
    )
}

/// 展开态配额行：[label flex-1] [value 固定宽] [bar 固定宽] [badge] [展开列占位]
fn render_quota_row(q: &OverviewQuotaItem, theme: &Theme) -> Div {
    let fill = theme.status_bar_color(q.status_level);
    let badge_color = theme.status_badge_color(q.status_level);
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(PopupLayout::OVERVIEW_GAP))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(11.0))
                .text_color(theme.text.secondary)
                .whitespace_nowrap()
                .overflow_hidden()
                .child(q.label.clone()),
        )
        .child(render_value_cell(&q.display_text, theme))
        .child(render_quota_visual(
            q,
            PopupLayout::OVERVIEW_EXPANDED_BAR_H,
            fill,
            theme,
        ))
        .child(render_badge(q.status_level, badge_color))
        // 右侧与折叠态展开按钮等宽占位，保证展开态与折叠态数值/进度条/状态列垂直对齐
        .child(render_expand_spacer())
}

/// 展开按钮列的等宽空白占位
fn render_expand_spacer() -> Div {
    div().w(px(PopupLayout::OVERVIEW_EXPAND_W)).flex_shrink_0()
}

/// 固定宽度数值单元格（右对齐）
fn render_value_cell(text: &str, theme: &Theme) -> Div {
    div()
        .w(px(PopupLayout::OVERVIEW_VALUE_W))
        .flex_shrink_0()
        .text_size(px(12.0))
        .font_weight(FontWeight::BOLD)
        .text_color(theme.text.primary)
        .whitespace_nowrap()
        .text_align(TextAlign::Right)
        .child(text.to_string())
}

/// 固定宽度状态徽章（右对齐）
fn render_badge(level: StatusLevel, color: Hsla) -> Div {
    div()
        .w(px(PopupLayout::OVERVIEW_BADGE_W))
        .flex_shrink_0()
        .text_size(px(9.0))
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .text_align(TextAlign::Right)
        .child(format_quota_status_label(level))
}

/// 根据配额是否有总额上限选择渲染方式：
/// - 常规配额：渲染实心百分比进度条
/// - 纯余额/积分/超额用量：渲染分段细虚线轨道，避免假百分比造成误解
fn render_quota_visual(quota: &OverviewQuotaItem, height: f32, fill: Hsla, theme: &Theme) -> Div {
    if quota.is_balance_only {
        render_balance_track(PopupLayout::OVERVIEW_BAR_W, height, fill)
    } else {
        render_bar(
            quota.bar_ratio,
            PopupLayout::OVERVIEW_BAR_W,
            height,
            fill,
            theme,
        )
    }
}

/// 纯余额/无上限配额的分段引导线（不传达伪百分比比例）
fn render_balance_track(width: f32, height: f32, fill: Hsla) -> Div {
    let dash_color = fill.opacity(0.35);
    let mut track = div()
        .w(px(width))
        .h(px(height))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(4.0));

    for _ in 0..BALANCE_TRACK_DASH_COUNT {
        track = track.child(div().flex_1().h(px(2.0)).rounded_full().bg(dash_color));
    }

    track
}

/// 固定宽度进度条
fn render_bar(ratio: f32, width: f32, height: f32, fill: Hsla, theme: &Theme) -> Div {
    div()
        .w(px(width))
        .h(px(height))
        .flex_shrink_0()
        .bg(theme.status.progress_track)
        .rounded_full()
        .overflow_hidden()
        .child(div().h_full().rounded_full().bg(fill).w(relative(ratio)))
}

/// 派发 Overview 卡片展开/收起切换动作
fn dispatch_toggle_expanded(entity: &Entity<AppView>, id: &ProviderId, cx: &mut App) {
    let id = id.clone();
    entity.update(cx, |view, cx| {
        runtime::dispatch_in_context(&view.state, AppAction::ToggleOverviewExpanded(id), cx);
    });
}

impl AppView {
    pub(crate) fn render_overview_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.global::<Theme>().clone();
        let vm = {
            let state = self.state.borrow();
            overview_view_state(&state.session)
        };

        if vm.items.is_empty() {
            return render_overview_empty(&theme);
        }

        let mut container = div().flex_col();
        let count = vm.items.len();

        for (i, item) in vm.items.iter().enumerate() {
            let mut card = self.render_compact_provider_card(item, &theme, window, cx);
            if i < count - 1 {
                card = card.mb(px(8.0));
            }
            container = container.child(card);
        }

        container.into_any_element()
    }

    /// 根据配额数量自适应布局：
    /// - 1 个配额：单行卡片
    /// - 2+ 个配额：默认折叠（最差配额 + 维度名）/ 点击展开为多行列表
    fn render_compact_provider_card(
        &self,
        item: &OverviewItemViewState,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let entity = cx.entity().clone();
        let id = item.id.clone();

        let row = match &item.status {
            OverviewItemStatus::Quota {
                status_level,
                quotas,
                expanded,
            } => match quotas.len() {
                0 => {
                    debug_assert!(false, "Quota variant should never have empty quotas vec");
                    render_card_disconnected(item, theme)
                }
                1 => {
                    self.render_card_single_row(item, *status_level, &quotas[0], false, window, cx)
                }
                // 2+ 配额：默认折叠，展开选择在本次运行期间被记住
                _ if *expanded => {
                    self.render_card_expanded(item, *status_level, quotas, window, cx)
                }
                _ => self.render_card_single_row(item, *status_level, &quotas[0], true, window, cx),
            },
            OverviewItemStatus::Refreshing => build_status_card_row(
                item,
                theme.text.accent,
                theme.text.muted,
                theme.text.primary,
                theme,
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text.muted)
                    .child(t!("provider.status.refreshing").to_string()),
            ),
            OverviewItemStatus::Error { message } => build_status_card_row(
                item,
                theme.status.error,
                theme.text.muted,
                theme.text.primary,
                theme,
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.0))
                    .text_color(theme.status.error)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(message.clone()),
            ),
            OverviewItemStatus::Disconnected => render_card_disconnected(item, theme),
        };

        // 点击跳转到 Provider 详情
        row.on_mouse_down(MouseButton::Left, move |_, _, cx| {
            entity.update(cx, |view, cx| {
                runtime::dispatch_in_context(
                    &view.state,
                    AppAction::SelectNavTab(NavTab::Provider(id.clone())),
                    cx,
                );
            });
        })
    }

    // ========================================================================
    // 单行卡片（1 配额 / 2+ 折叠态共用）
    // [dot] [icon] [name (+ 折叠态配额名)] [value] [bar] [badge] [▸?]
    // ========================================================================

    /// 单行卡片：`expandable = false` 为纯单行，`true` 为折叠态（追加 ▸ 按钮）
    fn render_card_single_row(
        &self,
        item: &OverviewItemViewState,
        overall_level: StatusLevel,
        quota: &OverviewQuotaItem,
        expandable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = cx.global::<Theme>().clone();
        let dot_color = theme.status_badge_color(overall_level);
        let fill = theme.status_bar_color(quota.status_level);
        let sub_label = expandable.then_some(quota.label.as_str());

        let mut row = build_single_line_card_row(
            item,
            dot_color,
            theme.text.secondary,
            theme.text.primary,
            sub_label,
            &theme,
        )
        .child(render_value_cell(&quota.display_text, &theme))
        .child(render_quota_visual(
            quota,
            PopupLayout::OVERVIEW_BAR_H,
            fill,
            &theme,
        ))
        // 徽章跟随所展示配额：设了主额度时行内数值是它，badge 也应是它而非最差等级。
        .child(render_badge(
            quota.status_level,
            theme.status_badge_color(quota.status_level),
        ));

        // 展开列（固定 24×24 尺寸，保证所有单行卡片宽度与 42px 总高度完全一致）
        if expandable {
            row = row.child(self.render_expand_toggle(item, false, &theme, window, cx));
        } else {
            row = row.child(render_expand_spacer().h(px(PopupLayout::OVERVIEW_EXPAND_W)));
        }

        row
    }

    // ========================================================================
    // 展开态（2+ 配额）：header + 每个配额独占一行
    // ========================================================================

    fn render_card_expanded(
        &self,
        item: &OverviewItemViewState,
        overall_level: StatusLevel,
        quotas: &[OverviewQuotaItem],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = cx.global::<Theme>().clone();
        let color = theme.status_badge_color(overall_level);

        // header 行：[dot] [icon] [name] [▾]（展开态不显示整体 badge，各行已有独立 badge）
        let header_base = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(PopupLayout::OVERVIEW_GAP));
        let header_row = append_provider_identity(
            header_base,
            item,
            color,
            theme.text.secondary,
            theme.text.primary,
            None,
            &theme,
        )
        .child(self.render_expand_toggle(item, true, &theme, window, cx));

        let mut quota_container = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(PopupLayout::OVERVIEW_QUOTA_LINE_GAP))
            .pl(px(PopupLayout::OVERVIEW_QUOTA_ROW_PL));

        for q in quotas {
            quota_container = quota_container.child(render_quota_row(q, &theme));
        }

        render_card_shell(item, &theme)
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(header_row)
            .child(quota_container)
    }

    /// 展开/折叠按钮：固定方形点击区域，支持 tooltip、Tab 聚焦和 Enter/Space 操作。
    fn render_expand_toggle(
        &self,
        item: &OverviewItemViewState,
        expanded: bool,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let (state_key, glyph, tooltip) = if expanded {
            ("expanded", "▾", t!("tooltip.collapse_quotas").to_string())
        } else {
            ("collapsed", "▸", t!("tooltip.expand_quotas").to_string())
        };
        // 外层焦点容器 ID 固定，保证 Tab 聚焦后用 Space/Enter 切换展开态时焦点不丢失。
        let control_id = ElementId::Name(format!("overview-expand-{}", item.id.id_key()).into());
        // 内层 tooltip 宿主 ID 随展开态切换，确保点击切换瞬间关闭旧浮层并刷新文案。
        let tooltip_id =
            ElementId::Name(format!("overview-expand-tip-{}-{state_key}", item.id.id_key()).into());
        let focus_handle = window
            .use_keyed_state(control_id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let tracked_focus = focus_handle.clone().tab_index(0).tab_stop(true);
        let is_focused = focus_handle.is_focused(window);

        let provider_id_for_mouse = item.id.clone();
        let provider_id_for_key = item.id.clone();
        let entity_for_mouse = cx.entity().clone();
        let entity_for_key = cx.entity().clone();
        let hover_bg = theme.bg.subtle;
        let hover_text = theme.text.primary;
        let idle_text = if expanded {
            theme.text.secondary
        } else {
            theme.text.muted
        };

        div()
            .id(control_id)
            .w(px(PopupLayout::OVERVIEW_EXPAND_W))
            .h(px(PopupLayout::OVERVIEW_EXPAND_W))
            .flex_shrink_0()
            .rounded(px(6.0))
            // 预留 1px 边框避免聚焦时跳动；默认透明，避免空闲态画出按钮外框
            .border_1()
            .border_color(if is_focused {
                theme.text.accent
            } else {
                gpui::transparent_black()
            })
            .text_size(px(13.0))
            .text_color(idle_text)
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg).text_color(hover_text))
            .track_focus(&tracked_focus)
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                dispatch_toggle_expanded(&entity_for_mouse, &provider_id_for_mouse, cx);
            })
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key == "space" || event.keystroke.key == "enter" {
                    cx.stop_propagation();
                    dispatch_toggle_expanded(&entity_for_key, &provider_id_for_key, cx);
                }
            })
            .child(with_tooltip(
                tooltip_id,
                &tooltip,
                theme,
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph),
            ))
    }
}
