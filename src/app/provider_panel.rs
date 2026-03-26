use super::provider_logic;
use super::settings_window::schedule_open_settings_window;
use super::AppView;
use crate::models::{ConnectionStatus, ProviderKind, ProviderStatus};
use crate::theme::Theme;
use gpui::*;

// const USAGE_ICON: &str = "src/icons/usage.svg";  // This is now used in mod.rs
// const REFRESH_ICON: &str = "src/icons/settings.svg";

impl AppView {
    pub(crate) fn render_provider_detail(
        &self,
        kind: ProviderKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state.borrow();
        let is_enabled = state.settings.is_provider_enabled(kind);
        let provider = state.providers.iter().find(|p| p.kind == kind).cloned();
        drop(state);

        if !is_enabled {
            return self.render_provider_not_enabled(kind, cx);
        }

        if let Some(provider) = provider {
            self.render_provider_panel(&provider, cx)
        } else {
            div().child("Provider not found").into_any_element()
        }
    }

    fn render_provider_not_enabled(
        &self,
        kind: ProviderKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.global::<Theme>();
        let state = self.state.clone();

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
            .child(
                svg()
                    .path(kind.icon_asset())
                    .size(px(32.0))
                    .text_color(theme.text_muted),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(format!("{} is not enabled", kind.display_name())),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_secondary)
                    .text_align(TextAlign::Center)
                    .line_height(relative(1.4))
                    .child("Enable it in Settings → Providers to start tracking quota."),
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
                    .child("Open Settings")
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        window.remove_window();
                        let settings_state = state.clone();
                        schedule_open_settings_window(settings_state, cx);
                    }),
            )
            .into_any_element()
    }

    fn render_provider_panel(
        &self,
        provider: &ProviderStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.global::<Theme>();
        let has_quotas = !provider.quotas.is_empty();

        let card_bg = transparent_black(); // 彻底移除蓝底
        let card_border = transparent_black();
        let title_color = theme.text_primary;

        let last_updated = provider.format_last_updated();
        let tier_badge = provider.account_tier.clone();
        let account_email = provider.account_email.clone();

        let header_right = div().flex().items_center().gap(px(6.0));

        // 只有当有 email 时展示账号信息
        let has_account_info = account_email.is_some() || tier_badge.is_some();

        let header_right = if has_account_info {
            let mut right = header_right;
            if let Some(ref email) = account_email {
                right = right.child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_secondary)
                        .child(email.clone()),
                );
            }
            if let Some(ref tier) = tier_badge {
                right = right.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .bg(theme.bg_subtle)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .text_color(theme.text_primary)
                        .child(tier.clone()),
                );
            }
            right
        } else {
            header_right
        };

        let shell = div()
            .flex()
            .flex_col()
            .gap(px(2.0)) // 极大减小留白 (1-2 pixels)
            .px(px(4.0)) // 极大减小留白
            .py(px(4.0)) // 极大减小留白
            .rounded(px(8.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(title_color)
                            .child(provider.kind.display_name()),
                    )
                    .child(header_right),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .mt(px(-2.0))
                    .child(last_updated),
            );

        let shell = if has_quotas {
            shell.children(provider.quotas.iter().enumerate().map(|(index, quota)| {
                // 不再强调反色效果 (highlighted = false)
                super::widgets::render_quota_bar(quota, index > 0, theme)
            }))
        } else {
            shell.child(self.render_provider_empty_state(provider, theme))
        };

        shell.into_any_element()
    }

    fn render_provider_empty_state(
        &self,
        provider: &ProviderStatus,
        theme: &Theme,
    ) -> impl IntoElement {
        let title = match provider.connection {
            ConnectionStatus::Connected => "Waiting for usage data",
            ConnectionStatus::Refreshing => "Refreshing…",
            ConnectionStatus::Disconnected => "Connection required",
            ConnectionStatus::Error => "Refresh failed",
        };
        let message = provider_logic::provider_empty_message(provider);

        div()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(8.0))
            .bg(theme.bg_subtle)
            .px(px(12.0))
            .py(px(10.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(relative(1.4))
                    .text_color(theme.text_secondary)
                    .child(message),
            )
    }
}
