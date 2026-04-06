use super::AppView;
use crate::application::{
    provider_detail_view_state, AccountInfoViewState, AppAction, DisabledProviderViewState,
    ProviderBodyViewState, ProviderDetailViewState, ProviderEmptyAction, ProviderEmptyViewState,
};
use crate::models::ProviderId;
use crate::refresh::RefreshReason;
use crate::runtime;
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;

/// 通用操作按钮（Lumina风格：半透明背景+圆角）
fn render_action_button(
    label: &str,
    theme: &Theme,
    handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    div().w_full().flex().justify_center().mt(px(8.0)).child(
        div()
            .px(px(16.0))
            .py(px(8.0))
            .rounded(px(10.0))
            .bg(theme.text_accent)
            .text_size(px(12.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.element_active)
            .cursor_pointer()
            .hover(|style| style.opacity(0.85))
            .child(label.to_string())
            .on_mouse_down(MouseButton::Left, handler),
    )
}

impl AppView {
    pub(crate) fn render_provider_detail(
        &self,
        id: &ProviderId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view_state = {
            let state = self.state.borrow();
            provider_detail_view_state(&state.session, id)
        };

        match view_state {
            ProviderDetailViewState::Disabled(vm) => self.render_provider_not_enabled(&vm, cx),
            ProviderDetailViewState::Missing { message } => div().child(message).into_any_element(),
            ProviderDetailViewState::Panel(vm) => self.render_provider_panel(vm, cx),
        }
    }

    fn render_provider_not_enabled(
        &self,
        vm: &DisabledProviderViewState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.global::<Theme>();
        let state = self.state.clone();
        let id = vm.id.clone();

        div()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(14.0))
            .px(px(20.0))
            .py(px(40.0))
            .rounded(px(14.0))
            .bg(theme.bg_card_inner)
            .border_1()
            .border_color(theme.border_subtle)
            .child(super::widgets::render_provider_icon(
                vm.icon.clone(),
                px(32.0),
                theme.text_muted,
            ))
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(vm.title.clone()),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_secondary)
                    .text_align(TextAlign::Center)
                    .line_height(relative(1.4))
                    .child(vm.hint.clone()),
            )
            .child(
                div()
                    .px(px(16.0))
                    .py(px(8.0))
                    .rounded(px(10.0))
                    .bg(theme.text_accent)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.element_active)
                    .cursor_pointer()
                    .child(t!("provider.open_settings").to_string())
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        runtime::dispatch_in_window(
                            &state,
                            AppAction::OpenSettings {
                                provider: Some(id.clone()),
                            },
                            window,
                            cx,
                        );
                    }),
            )
            .into_any_element()
    }

    fn render_provider_panel(
        &self,
        vm: crate::application::ProviderPanelViewState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::clone(cx.global::<Theme>());
        let quotas_container = match &vm.body {
            ProviderBodyViewState::Refreshing { provider_name } => {
                self.render_refreshing_state(provider_name, &theme)
            }
            ProviderBodyViewState::Quotas { quotas, generation } => {
                let mut cards = div().flex_col();
                for (i, quota) in quotas.iter().enumerate() {
                    if i > 0 {
                        cards = cards.child(div().h(px(8.0)));
                    }
                    cards = cards.child(super::widgets::render_quota_bar(
                        quota,
                        &theme,
                        *generation,
                        vm.quota_display_mode,
                    ));
                }
                cards
            }
            ProviderBodyViewState::Empty(empty_vm) => {
                self.render_provider_empty_state(empty_vm, cx)
            }
        };

        // Dashboard 链接行（受 show_dashboard_button 设置控制，账户卡片存在时隐藏）
        let state_for_dashboard = self.state.clone();
        let id = vm.id.clone();
        let dashboard_row = if vm.show_dashboard {
            let dashboard_id = id.clone();
            Some(self.render_link_row(
                "src/icons/compass.svg",
                &t!("tooltip.dashboard"),
                &theme,
                move |_, window, cx| {
                    runtime::dispatch_in_window(
                        &state_for_dashboard,
                        AppAction::OpenDashboard(dashboard_id.clone()),
                        window,
                        cx,
                    );
                },
            ))
        } else {
            None
        };

        // 整体布局
        let mut container = div().flex_col();

        // 账户信息卡片（位于配额卡片上方）
        if let Some(ref account) = vm.account {
            container = container
                .child(self.render_account_info_card(account, &id, &theme))
                .child(div().h(px(8.0)));
        }

        container = container.child(quotas_container);

        if let Some(row) = dashboard_row {
            container = container.child(div().mt(px(8.0)).child(row));
        } else {
            // 无 Dashboard 行时补偿底部间距，保持与有 Dashboard 时视觉一致
            container = container.child(div().h(px(8.0)));
        }

        container.into_any_element()
    }

    /// 账户信息卡片：头像(首字符) + 邮箱 + 套餐徽章 + 更新时间 + > 箭头
    fn render_account_info_card(
        &self,
        account: &AccountInfoViewState,
        id: &ProviderId,
        theme: &Theme,
    ) -> Div {
        let avatar_char = account
            .email
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .next()
            .unwrap_or('?')
            .to_string();

        // 状态行：套餐徽章 + 更新时间
        let mut status_row = div().flex().items_center().gap(px(8.0));

        if let Some(ref tier) = account.tier {
            status_row = status_row.child(
                div()
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(4.0))
                    .bg(theme.text_accent_soft)
                    .text_size(px(10.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_accent)
                    .child(tier.to_uppercase()),
            );
        }

        status_row = status_row.child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(super::widgets::render_svg_icon(
                    "src/icons/status.svg",
                    px(12.0),
                    theme.text_muted,
                ))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.text_muted)
                        .child(account.updated_text.clone()),
                ),
        );

        let can_click = !account.dashboard_url.is_empty();
        let state_for_click = self.state.clone();

        let mut card = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(12.0))
            .py(px(12.0))
            .rounded(px(14.0))
            .bg(theme.bg_card_inner)
            .border_1()
            .border_color(theme.border_subtle)
            // 头像
            .child(
                div()
                    .w(px(44.0))
                    .h(px(44.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.0))
                    .bg(theme.bg_subtle)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .text_size(px(18.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_accent)
                    .child(avatar_char),
            )
            // 邮箱 + 状态行
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(account.email.clone()),
                    )
                    .child(status_row),
            )
            // 右侧 > 箭头
            .child(div().flex_shrink_0().child(super::widgets::render_svg_icon(
                "src/icons/chevron-right.svg",
                px(16.0),
                theme.text_muted,
            )));

        if can_click {
            card = card.cursor_pointer().hover(|style| style.opacity(0.85));
            card = card.on_mouse_down(MouseButton::Left, {
                let id = id.clone();
                move |_, window, cx| {
                    runtime::dispatch_in_window(
                        &state_for_click,
                        AppAction::OpenDashboard(id.clone()),
                        window,
                        cx,
                    );
                }
            });
        }

        card
    }

    /// 操作链接行（类似截图中的 "Usage Dashboard"）
    fn render_link_row(
        &self,
        icon: &'static str,
        label: &str,
        theme: &Theme,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(14.0))
            .py(px(10.0))
            .rounded(px(10.0))
            .cursor_pointer()
            .hover(|style| style.bg(theme.bg_subtle))
            .child(super::widgets::render_svg_icon(
                icon,
                px(16.0),
                theme.text_muted,
            ))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(theme.text_secondary)
                    .child(label.to_string()),
            )
            .on_mouse_down(MouseButton::Left, handler)
    }

    fn render_refreshing_state(&self, provider_name: &str, theme: &Theme) -> Div {
        div()
            .w_full()
            .flex_col()
            .gap(px(12.0))
            .py(px(40.0))
            .rounded(px(12.0))
            .bg(theme.bg_card_inner)
            .border_1()
            .border_color(theme.border_subtle)
            // spinner 占位
            .child(
                div().w_full().flex().justify_center().child(
                    div()
                        .w(px(36.0))
                        .h(px(36.0))
                        .rounded_full()
                        .border_3()
                        .border_color(theme.border_subtle)
                        .border_t_3()
                        .border_r_3()
                        .border_color(theme.text_accent),
                ),
            )
            // 文字
            .child(
                div()
                    .w_full()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .text_align(TextAlign::Center)
                    .child(t!("provider.fetching", name = provider_name).to_string()),
            )
    }

    fn render_provider_empty_state(
        &self,
        vm: &ProviderEmptyViewState,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.global::<Theme>();
        let entity = cx.entity().clone();
        let state_for_settings = self.state.clone();
        let id = vm.id.clone();

        let mut container = div()
            .w_full()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(12.0))
            .bg(theme.bg_card_inner)
            .border_1()
            .border_color(theme.border_subtle)
            .py(px(28.0))
            // 标题居中
            .child(
                div()
                    .w_full()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if vm.is_error {
                        theme.status_error
                    } else {
                        theme.text_primary
                    })
                    .text_align(TextAlign::Center)
                    .child(vm.title.clone()),
            );

        // 错误消息
        if !vm.message.is_empty() {
            container = container.child(
                div()
                    .w_full()
                    .px(px(16.0))
                    .text_size(px(12.0))
                    .line_height(relative(1.4))
                    .text_color(theme.text_secondary)
                    .text_align(TextAlign::Center)
                    .child(vm.message.clone()),
            );
        }

        if let Some(action) = vm.action {
            if action == ProviderEmptyAction::OpenSettings {
                container = container.child(render_action_button(
                    &t!("provider.open_config"),
                    theme,
                    move |_, window, cx| {
                        runtime::dispatch_in_window(
                            &state_for_settings,
                            AppAction::OpenSettings {
                                provider: Some(id.clone()),
                            },
                            window,
                            cx,
                        );
                    },
                ));
            } else {
                container = container.child(render_action_button(
                    &t!("provider.retry"),
                    theme,
                    move |_, _, cx| {
                        entity.update(cx, |view, cx| {
                            runtime::dispatch_in_context(
                                &view.state,
                                AppAction::RefreshProvider {
                                    id: id.clone(),
                                    reason: RefreshReason::Manual,
                                },
                                cx,
                            );
                        });
                    },
                ));
            }
        }

        container
    }
}
