use super::provider_logic;
use super::settings_window::schedule_open_settings_window;
use super::AppView;
use crate::models::{ConnectionStatus, ProviderKind, ProviderStatus};
use crate::theme::Theme;
use gpui::*;
use log::{info, warn};

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
                    .child(format!("{} is not enabled", display_name)),
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
                        let display_id = window.display(cx).map(|d| d.id());
                        window.remove_window();
                        let settings_state = state.clone();
                        schedule_open_settings_window(settings_state, display_id, cx);
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
        let is_refreshing = provider.connection == ConnectionStatus::Refreshing;
        let has_quotas = !provider.quotas.is_empty();

        let card_bg = transparent_black(); // 彻底移除蓝底
        let card_border = transparent_black();
        let title_color = theme.text_primary;

        let last_updated = provider.format_last_updated();
        let tier_badge = provider.account_tier.clone();
        let account_email = provider.account_email.clone();

        let mut header_right = div().flex().items_center().gap(px(6.0));

        if let Some(ref email) = account_email {
            header_right = header_right.child(
                div()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(email.clone()),
            );
        }
        if let Some(ref tier) = tier_badge {
            header_right = header_right.child(
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

        let mut shell = div()
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
                            .child(provider.display_name().to_string()),
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

        if is_refreshing {
            // 刷新中 → 显示加载视图，替换 quota bars
            shell = shell.child(self.render_refreshing_state(provider, theme));
        } else if has_quotas {
            shell = shell.children(provider.quotas.iter().enumerate().map(|(index, quota)| {
                // 不再强调反色效果 (highlighted = false)
                super::widgets::render_quota_bar(quota, index > 0, theme)
            }));
        } else {
            shell = shell.child(self.render_provider_empty_state(provider, cx));
        }

        shell.into_any_element()
    }

    fn render_refreshing_state(
        &self,
        provider: &ProviderStatus,
        theme: &Theme,
    ) -> impl IntoElement {
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
                    .child(format!("Fetching {} usage data…", provider.display_name())),
            )
    }

    fn render_provider_empty_state(
        &self,
        provider: &ProviderStatus,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let title = match provider.connection {
            ConnectionStatus::Connected => "Waiting for usage data",
            ConnectionStatus::Refreshing => "Refreshing…",
            ConnectionStatus::Disconnected => "Connection required",
            ConnectionStatus::Error => "Refresh failed",
        };
        let message = provider_logic::provider_empty_message(provider);
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
                    .text_color(theme.text_primary)
                    .text_align(TextAlign::Center)
                    .child(title),
            )
            // 说明文字居中
            .child(
                div()
                    .w_full()
                    .px(px(16.0))
                    .text_size(px(12.0))
                    .line_height(relative(1.4))
                    .text_color(theme.text_secondary)
                    .text_align(TextAlign::Center)
                    .child(message),
            );

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
                        .child("Retry")
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

    /// Helper to concisely update the state for a single provider and trigger a view update.
    pub(crate) fn update_provider_state<F>(&self, kind: ProviderKind, cx: &mut Context<Self>, f: F)
    where
        F: FnOnce(&mut ProviderStatus),
    {
        let mut s = self.state.borrow_mut();
        if let Some(p) = s.provider_store.find_mut(kind) {
            f(p);
        }
        cx.notify();
    }

    /// Trigger a refresh for a single provider (used by the retry button).
    pub(crate) fn refresh_single_provider(&self, kind: ProviderKind, cx: &mut Context<Self>) {
        let manager = self.state.borrow().provider_store.manager.clone();

        self.update_provider_state(kind, cx, |p| {
            p.connection = ConnectionStatus::Refreshing;
            p.error_message = None;
        });

        cx.spawn(move |this: gpui::WeakEntity<AppView>, cx: &mut gpui::AsyncApp| {
            let mut async_cx = cx.clone();
            async move {
                let mgr = manager.clone();

                let mgr_check = mgr.clone();
                let available = smol::unblock(move || {
                    smol::block_on(mgr_check.is_provider_available(kind))
                })
                .await;

                if !available {
                    info!(target: "providers", "retry: provider {:?} unavailable", kind);
                    let _ = this.update(&mut async_cx, |view, cx| {
                        view.update_provider_state(kind, cx, |p| {
                            p.connection = ConnectionStatus::Disconnected;
                            p.error_message = Some("Provider is currently unavailable.".to_string());
                        });
                    });
                    return;
                }

                info!(target: "providers", "retry: refreshing provider {:?}", kind);
                let result =
                    smol::unblock(move || smol::block_on(mgr.refresh_provider(kind))).await;

                match result {
                    Ok(quotas) => {
                        info!(target: "providers", "retry: provider {:?} succeeded with {} quotas", kind, quotas.len());
                        let _ = this.update(&mut async_cx, |view, cx| {
                            view.update_provider_state(kind, cx, |p| {
                                p.quotas = quotas;
                                p.connection = ConnectionStatus::Connected;
                                p.last_refreshed_instant = Some(std::time::Instant::now());
                                p.last_updated_at = None;
                                p.error_message = None;
                            });
                        });
                    }
                    Err(err) => {
                        warn!(target: "providers", "retry: provider {:?} failed: {err}", kind);
                        let _ = this.update(&mut async_cx, |view, cx| {
                            view.update_provider_state(kind, cx, |p| {
                                p.connection = ConnectionStatus::Error;
                                p.last_updated_at = Some("Update failed".to_string());
                                p.error_message = Some(err.to_string());
                            });
                        });
                    }
                }
            }
        })
        .detach();
    }
}
