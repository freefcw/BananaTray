use crate::application::{
    overview_view_state, AppAction, OverviewItemStatus, OverviewItemViewState,
};
use crate::models::{NavTab, StatusLevel};
use crate::runtime;
use crate::theme::Theme;
use crate::ui::AppView;
use gpui::*;
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
// TODO: i18n — 当前硬编码英文缩写，后续应走 t!() 宏支持多语言
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
            // 除最后一个外，每个卡片底部加间距
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

    /// 构建公共 base row：[状态点] [图标] [名称 flex-1]
    ///
    /// 所有状态变体共享左侧三个元素，减少重复。
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
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(10.0))
            .bg(theme.bg.card_inner)
            .border_1()
            .border_color(theme.border.subtle)
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            // 状态点
            .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(dot_bg))
            // Provider 图标
            .child(crate::ui::widgets::render_provider_icon(
                item.icon.clone(),
                px(16.0),
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

    /// 紧凑卡片：单行展示 Provider 配额状态
    ///
    /// 布局：[状态点] [图标] [名称 flex-1] [右侧内容（因状态而异）]
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
                display_text,
                bar_ratio,
            } => {
                let level = *status_level;
                let color = dot_color(level, theme);
                let fill = bar_color(level, theme);
                let ratio = *bar_ratio;

                self.build_card_base_row(
                    item,
                    color,
                    theme.text.secondary,
                    theme.text.primary,
                    theme,
                )
                // 数值
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text.primary)
                        .whitespace_nowrap()
                        .child(display_text.clone()),
                )
                // 紧凑进度条
                .child(
                    div()
                        .w(px(60.0))
                        .h(px(4.0))
                        .flex_shrink_0()
                        .bg(theme.status.progress_track)
                        .rounded_full()
                        .overflow_hidden()
                        .child(div().h_full().rounded_full().bg(fill).w(relative(ratio))),
                )
                // 状态徽章
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(9.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(color)
                        .child(compact_badge_label(level)),
                )
            }
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
            OverviewItemStatus::Disconnected => self
                .build_card_base_row(
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
                ),
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
}
