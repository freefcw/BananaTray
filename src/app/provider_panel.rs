use super::provider_logic;
use super::settings_window::schedule_open_settings_window;
use super::AppView;
use crate::models::{ConnectionStatus, ProviderKind, ProviderStatus};
use crate::refresh::RefreshReason;
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;

impl AppView {
    pub(crate) fn render_provider_detail(
        &self,
        kind: ProviderKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state.borrow();
        let is_enabled = state.settings.is_provider_enabled(kind);
        let provider = state.provider_store.find(kind).cloned();
        drop(state);

        if !is_enabled {
            return self.render_provider_not_enabled(kind, cx);
        }

        if let Some(provider) = provider {
            self.render_provider_panel(&provider, cx)
        } else {
            div()
                .child(t!("provider.not_found").to_string())
                .into_any_element()
        }
    }

    fn render_provider_not_enabled(
        &self,
        kind: ProviderKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.global::<Theme>();
        let state = self.state.clone();
        let provider = state.borrow().provider_store.find(kind).cloned();

        let (icon, display_name) = if let Some(p) = provider {
            (p.icon_asset().to_string(), p.display_name().to_string())
        } else {
            (
                "src/icons/provider-unknown.svg".to_string(),
                format!("{:?}", kind),
            )
        };

        div()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .px(px(20.0))
            .py(px(40.0))
            .rounded(px(14.0))
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border_subtle)
            .child(svg().path(icon).size(px(32.0)).text_color(theme.text_muted))
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(format!(
                        "{}",
                        t!("provider.not_enabled", name = display_name)
                    )),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_secondary)
                    .text_align(TextAlign::Center)
                    .line_height(relative(1.4))
                    .child(t!("provider.enable_hint").to_string()),
            )
            .child(
                div()
                    .px(px(14.0))
                    .py(px(8.0))
                    .rounded(px(10.0))
                    .bg(theme.text_accent)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.element_active)
                    .cursor_pointer()
                    .child(t!("provider.open_settings").to_string())
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        let display_id = window.display(cx).map(|d| d.id());
                        state.borrow_mut().view_entity = None;
                        window.remove_window();
                        schedule_open_settings_window(state.clone(), display_id, cx);
                    }),
            )
            .into_any_element()
    }

    fn render_account_info_card(
        &self,
        provider: &ProviderStatus,
        theme: &Theme,
        _entity: Entity<AppView>,
    ) -> Div {
        let last_updated = provider.format_last_updated();
        let tier_badge = provider.account_tier.clone();
        let account_email = provider.account_email.clone();

        // 如果既没有邮箱也没有套餐等级，只显示更新时间
        if account_email.is_none() && tier_badge.is_none() {
            return div()
                .flex()
                .justify_end()
                .items_center()
                .px(px(12.0))
                .py(px(6.0))
                .rounded(px(10.0))
                .bg(theme.bg_card)
                .border_1()
                .border_color(theme.border_subtle)
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.text_secondary)
                        .child(last_updated),
                );
        }

        // 有账户信息时，显示邮箱 + 套餐等级 + 更新时间
        div()
            .flex()
            .justify_between()
            .items_baseline()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(10.0))
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .children(account_email.clone().map(|email| {
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(email)
                    }))
                    .children(tier_badge.map(|tier| {
                        div()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .bg(theme.text_accent)
                            .text_color(theme.element_active)
                            .child(tier)
                    })),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(last_updated),
            )
    }

    fn render_provider_panel(
        &self,
        provider: &ProviderStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // 先获取所有需要的数据，避免借用冲突
        let entity = cx.entity().clone();
        let theme = Theme::clone(cx.global::<Theme>());
        let provider = provider.clone();
        let is_refreshing = provider.connection == ConnectionStatus::Refreshing;
        let is_error = provider.connection == ConnectionStatus::Error;
        let has_quotas = !provider.quotas.is_empty();

        // 错误状态：不显示账户卡片，直接显示错误空状态
        if is_error && !has_quotas {
            return self
                .render_provider_empty_state(&provider, cx)
                .into_any_element();
        }

        // 创建账户信息卡片（不需要 cx，因为 entity 已经 clone）
        let account_card = self.render_account_info_card(&provider, &theme, entity);

        // 创建配额信息容器
        let quotas_container = if is_refreshing {
            self.render_refreshing_state(&provider, &theme)
        } else if has_quotas {
            div().flex_col().gap(px(8.0)).children(
                provider.quotas.iter().enumerate().map(|(index, quota)| {
                    super::widgets::render_quota_bar(quota, index > 0, &theme)
                }),
            )
        } else {
            self.render_provider_empty_state(&provider, cx)
        };

        // 整体布局：账户卡片 + 配额容器
        div()
            .flex_col()
            .gap(px(12.0))
            .child(account_card)
            .child(quotas_container)
            .into_any_element()
    }

    fn render_refreshing_state(&self, provider: &ProviderStatus, theme: &Theme) -> Div {
        div()
            .w_full()
            .gap(px(12.0))
            .py(px(32.0))
            .rounded(px(8.0))
            .bg(theme.bg_subtle)
            // spinner 居中行
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
            // 文字居中行
            .child(
                div()
                    .w_full()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .text_align(TextAlign::Center)
                    .child(format!(
                        "{}",
                        t!("provider.fetching", name = provider.display_name())
                    )),
            )
    }

    fn render_provider_empty_state(
        &self,
        provider: &ProviderStatus,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.global::<Theme>();
        let is_error = provider.connection == ConnectionStatus::Error;

        let (title, message) = if is_error {
            // 错误状态：优先显示错误消息作为标题
            let error_msg = provider.error_message.as_deref().unwrap_or("");
            (
                t!("provider.refresh_failed").to_string(),
                error_msg.to_string(),
            )
        } else {
            let title = match provider.connection {
                ConnectionStatus::Connected => t!("provider.waiting").to_string(),
                ConnectionStatus::Refreshing => t!("provider.status.refreshing").to_string(),
                ConnectionStatus::Disconnected => t!("provider.connection_required").to_string(),
                ConnectionStatus::Error => unreachable!(),
            };
            let message = provider_logic::provider_empty_message(provider);
            (title, message)
        };

        let show_refresh = matches!(
            provider.connection,
            ConnectionStatus::Error | ConnectionStatus::Disconnected
        );
        let kind = provider.kind;
        let entity = cx.entity().clone();

        let mut container = div()
            .w_full()
            .gap(px(8.0))
            .rounded(px(8.0))
            .bg(theme.bg_subtle)
            .py(px(24.0))
            // 标题居中
            .child(
                div()
                    .w_full()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if is_error {
                        theme.text_accent
                    } else {
                        theme.text_primary
                    })
                    .text_align(TextAlign::Center)
                    .child(title),
            );

        // 错误状态：显示错误消息（如果有）
        if !message.is_empty() {
            container = container.child(
                div()
                    .w_full()
                    .px(px(16.0))
                    .text_size(px(12.0))
                    .line_height(relative(1.4))
                    .text_color(theme.text_secondary)
                    .text_align(TextAlign::Center)
                    .child(message),
            );
        }

        if show_refresh {
            container = container.child(
                div().w_full().flex().justify_center().mt(px(4.0)).child(
                    div()
                        .px(px(12.0))
                        .py(px(6.0))
                        .rounded(px(8.0))
                        .bg(theme.text_accent)
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.element_active)
                        .cursor_pointer()
                        .child(t!("provider.retry").to_string())
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            entity.update(cx, |view, cx| {
                                view.refresh_single_provider(kind, cx);
                            });
                        }),
                ),
            );
        }

        container
    }

    /// Trigger a refresh for a single provider (used by the retry button).
    pub(crate) fn refresh_single_provider(&self, kind: ProviderKind, cx: &mut Context<Self>) {
        self.state
            .borrow_mut()
            .request_provider_refresh(kind, RefreshReason::Manual);
        cx.notify();
    }
}
