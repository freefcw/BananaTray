use crate::application::{
    overview_view_state, AppAction, OverviewItemStatus, OverviewItemViewState, OverviewQuotaItem,
};
use crate::models::{NavTab, PopupLayout, StatusLevel};
use crate::runtime;
use crate::theme::Theme;
use crate::ui::AppView;
use gpui::{
    div, px, relative, AnyElement, Context, Div, ElementId, FontWeight, Hsla, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Stateful, Styled, TextAlign,
};
use rust_i18n::t;

/// 状态点颜色
fn dot_color(level: StatusLevel, theme: &Theme) -> Hsla {
    match level {
        StatusLevel::Green => theme.badge.healthy,
        StatusLevel::Yellow => theme.badge.degraded,
        StatusLevel::Red => theme.badge.offline,
    }
}

/// 状态徽章文本（紧凑版，缩写）
fn compact_badge_label(level: StatusLevel) -> &'static str {
    match level {
        StatusLevel::Green => "OK",
        StatusLevel::Yellow => "LOW",
        StatusLevel::Red => "OUT",
    }
}

/// 进度条颜色
fn bar_color(level: StatusLevel, theme: &Theme) -> Hsla {
    match level {
        StatusLevel::Green => theme.status.success,
        StatusLevel::Yellow => theme.status.warning,
        StatusLevel::Red => theme.status.error,
    }
}

impl AppView {
    pub(crate) fn render_overview_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.global::<Theme>().clone();
        let vm = {
            let state = self.state.borrow();
            overview_view_state(&state.session)
        };

        if vm.items.is_empty() {
            return self.render_overview_empty(&theme);
        }

        let mut container = div().flex_col();
        let count = vm.items.len();

        for (i, item) in vm.items.iter().enumerate() {
            let mut card = self.render_compact_provider_card(item, &theme, cx);
            if i < count - 1 {
                card = card.mb(px(8.0));
            }
            container = container.child(card);
        }

        container.into_any_element()
    }

    /// 空状态：无已启用 Provider
    fn render_overview_empty(&self, theme: &Theme) -> AnyElement {
        div()
            .w_full()
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

    /// 根据配额数量自适应布局：
    /// - 1 个配额：单行卡片
    /// - 2+ 个配额：默认折叠（最差配额 + "▸N"）/ 点击展开为多行列表
    fn render_compact_provider_card(
        &self,
        item: &OverviewItemViewState,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let entity = cx.entity().clone();
        let id = item.id.clone();

        let row = match &item.status {
            OverviewItemStatus::Quota {
                status_level,
                quotas,
            } => match quotas.len() {
                0 => {
                    debug_assert!(false, "Quota variant should never have empty quotas vec");
                    self.render_card_disconnected(item, theme)
                }
                1 => self.render_card_single_row(item, *status_level, &quotas[0], false, theme, cx),
                _ => {
                    // 2+ 配额：折叠/展开
                    let is_expanded = self.overview_expanded.contains(&item.id);
                    if is_expanded {
                        self.render_card_expanded(item, *status_level, quotas, theme, cx)
                    } else {
                        self.render_card_single_row(
                            item,
                            *status_level,
                            &quotas[0],
                            true,
                            theme,
                            cx,
                        )
                    }
                }
            },
            OverviewItemStatus::Refreshing => self
                .build_card_base_row(
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
            OverviewItemStatus::Error { message } => self
                .build_card_base_row(
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
            OverviewItemStatus::Disconnected => self.render_card_disconnected(item, theme),
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
    // [dot] [icon] [name] [value] [bar] [badge] [▸?]
    // ========================================================================

    /// 单行卡片：`expandable = false` 为纯单行，`true` 为折叠态（追加 ▸ 按钮）
    fn render_card_single_row(
        &self,
        item: &OverviewItemViewState,
        overall_level: StatusLevel,
        quota: &OverviewQuotaItem,
        expandable: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let color = dot_color(overall_level, theme);
        let fill = bar_color(quota.status_level, theme);

        let mut row = self
            .build_card_base_row(item, color, theme.text.secondary, theme.text.primary, theme)
            .child(self.render_value_cell(&quota.display_text, theme))
            .child(self.render_bar(
                quota.bar_ratio,
                PopupLayout::OVERVIEW_BAR_W,
                PopupLayout::OVERVIEW_BAR_H,
                fill,
                theme,
            ))
            .child(self.render_badge(overall_level, color));

        // 展开列（固定宽度，保证所有单行卡片对齐）
        if expandable {
            let expand_id = item.id.clone();
            let entity = cx.entity().clone();
            row = row.child(
                div()
                    .id(ElementId::Name(
                        format!("expand-{}", item.id.id_key()).into(),
                    ))
                    .w(px(PopupLayout::OVERVIEW_EXPAND_W))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.0))
                    .text_color(theme.text.accent)
                    .cursor_pointer()
                    .child("▸")
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        entity.update(cx, |view, _cx| {
                            view.overview_expanded.insert(expand_id.clone());
                        });
                    }),
            );
        } else {
            // 空白占位，保证进度条对齐
            row = row.child(div().w(px(PopupLayout::OVERVIEW_EXPAND_W)).flex_shrink_0());
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
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let hover_bg = theme.bg.card_inner_hovered;
        let collapse_id = item.id.clone();
        let entity = cx.entity().clone();

        // header 行：[dot] [icon] [name] [▾]（展开态不显示整体 badge，各行已有独立 badge）
        let header_row = self
            .build_expanded_header(item, overall_level, theme)
            .child(
                div()
                    .id(ElementId::Name(
                        format!("collapse-{}", item.id.id_key()).into(),
                    ))
                    .w(px(PopupLayout::OVERVIEW_EXPAND_W))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.0))
                    .text_color(theme.text.accent)
                    .cursor_pointer()
                    .child("▾")
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        entity.update(cx, |view, _cx| {
                            view.overview_expanded.remove(&collapse_id);
                        });
                    }),
            );

        let mut quota_container = div()
            .w_full()
            .flex_col()
            .gap(px(PopupLayout::OVERVIEW_QUOTA_LINE_GAP))
            .pl(px(PopupLayout::OVERVIEW_QUOTA_ROW_PL));

        for q in quotas {
            quota_container = quota_container.child(self.render_quota_row(q, theme));
        }

        self.build_card_shell(item, theme)
            .hover(move |style| style.bg(hover_bg))
            .child(header_row)
            .child(quota_container)
    }

    // ========================================================================
    // 公共组件
    // ========================================================================

    /// 展开态 header 行：[dot] [icon] [name flex-1]（折叠按钮由调用方 .child() 追加）
    fn build_expanded_header(
        &self,
        item: &OverviewItemViewState,
        overall_level: StatusLevel,
        theme: &Theme,
    ) -> Div {
        let color = dot_color(overall_level, theme);
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(PopupLayout::OVERVIEW_GAP))
            .child(
                div()
                    .w(px(PopupLayout::OVERVIEW_DOT_SIZE))
                    .h(px(PopupLayout::OVERVIEW_DOT_SIZE))
                    .rounded_full()
                    .bg(color),
            )
            .child(crate::ui::widgets::render_provider_icon(
                item.icon.clone(),
                px(PopupLayout::OVERVIEW_ICON_SIZE),
                theme.text.secondary,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text.primary)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(item.display_name.clone()),
            )
    }

    /// 展开态卡片的公共外壳：id + flex_col + padding + 圆角 + 背景 + 边框
    fn build_card_shell(&self, item: &OverviewItemViewState, theme: &Theme) -> Stateful<Div> {
        div()
            .id(ElementId::Name(
                format!("overview-{}", item.id.id_key()).into(),
            ))
            .w_full()
            .flex_col()
            .gap(px(6.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(10.0))
            .bg(theme.bg.card_inner)
            .border_1()
            .border_color(theme.border.subtle)
            .cursor_pointer()
    }

    /// 展开态配额行：[label flex-1] [value 固定宽] [bar 固定宽] [badge]
    fn render_quota_row(&self, q: &OverviewQuotaItem, theme: &Theme) -> Div {
        let fill = bar_color(q.status_level, theme);
        let badge_color = dot_color(q.status_level, theme);
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
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
            .child(self.render_value_cell(&q.display_text, theme))
            .child(self.render_bar(
                q.bar_ratio,
                PopupLayout::OVERVIEW_BAR_W,
                PopupLayout::OVERVIEW_EXPANDED_BAR_H,
                fill,
                theme,
            ))
            .child(self.render_badge(q.status_level, badge_color))
    }

    /// Disconnected 状态卡片
    fn render_card_disconnected(
        &self,
        item: &OverviewItemViewState,
        theme: &Theme,
    ) -> Stateful<Div> {
        self.build_card_base_row(
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

    /// 构建公共 base row：[状态点] [图标] [名称 flex-1]
    fn build_card_base_row(
        &self,
        item: &OverviewItemViewState,
        dot_bg: Hsla,
        icon_color: Hsla,
        name_color: Hsla,
        theme: &Theme,
    ) -> Stateful<Div> {
        let hover_bg = theme.bg.card_inner_hovered;
        div()
            .id(ElementId::Name(
                format!("overview-{}", item.id.id_key()).into(),
            ))
            .w_full()
            .flex()
            .items_center()
            .gap(px(PopupLayout::OVERVIEW_GAP))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(10.0))
            .bg(theme.bg.card_inner)
            .border_1()
            .border_color(theme.border.subtle)
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            // 状态点
            .child(
                div()
                    .w(px(PopupLayout::OVERVIEW_DOT_SIZE))
                    .h(px(PopupLayout::OVERVIEW_DOT_SIZE))
                    .rounded_full()
                    .bg(dot_bg),
            )
            // Provider 图标
            .child(crate::ui::widgets::render_provider_icon(
                item.icon.clone(),
                px(PopupLayout::OVERVIEW_ICON_SIZE),
                icon_color,
            ))
            // Provider 名称
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(name_color)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(item.display_name.clone()),
            )
    }

    // ── 原子 UI 元素 ──

    /// 固定宽度数值单元格（右对齐）
    fn render_value_cell(&self, text: &str, theme: &Theme) -> Div {
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
    fn render_badge(&self, level: StatusLevel, color: Hsla) -> Div {
        div()
            .w(px(PopupLayout::OVERVIEW_BADGE_W))
            .flex_shrink_0()
            .text_size(px(9.0))
            .font_weight(FontWeight::BOLD)
            .text_color(color)
            .text_align(TextAlign::Right)
            .child(compact_badge_label(level))
    }

    /// 固定宽度进度条
    fn render_bar(&self, ratio: f32, width: f32, height: f32, fill: Hsla, theme: &Theme) -> Div {
        div()
            .w(px(width))
            .h(px(height))
            .flex_shrink_0()
            .bg(theme.status.progress_track)
            .rounded_full()
            .overflow_hidden()
            .child(div().h_full().rounded_full().bg(fill).w(relative(ratio)))
    }
}
