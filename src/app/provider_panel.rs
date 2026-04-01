use super::provider_logic;
use super::settings_window::{
    schedule_open_settings_window, schedule_open_settings_window_with_provider,
};
use super::AppView;
use crate::models::{ConnectionStatus, ErrorKind, ProviderKind, ProviderStatus};
use crate::refresh::RefreshReason;
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
            .gap(px(14.0))
            .px(px(20.0))
            .py(px(40.0))
            .rounded(px(14.0))
            .bg(theme.bg_card_inner)
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
                        let display_id = window.display(cx).map(|d| d.id());
                        state.borrow_mut().view_entity = None;
                        window.remove_window();
                        schedule_open_settings_window(state.clone(), display_id, cx);
                    }),
            )
            .into_any_element()
    }

    fn render_provider_panel(
        &self,
        provider: &ProviderStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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

        // Quota 卡片列表
        let quotas_container = if is_refreshing {
            self.render_refreshing_state(&provider, &theme)
        } else if has_quotas {
            let gen = self.state.borrow().nav.generation;
            let theme_clone = theme.clone();
            div()
                .flex_col()
                .gap(px(16.0))
                .children(
                    provider
                        .quotas
                        .iter()
                        .enumerate()
                        .map(move |(index, quota)| {
                            super::widgets::render_quota_bar(quota, index > 0, &theme_clone, gen)
                        }),
                )
        } else {
            self.render_provider_empty_state(&provider, cx)
        };

        // Dashboard 链接行
        let dashboard_url = provider.dashboard_url().to_string();
        let dashboard_row = if !dashboard_url.is_empty() {
            Some(self.render_link_row(
                "src/icons/compass.svg",
                &t!("tooltip.dashboard"),
                &theme,
                move |_, _, _| {
                    let cmd = if cfg!(target_os = "linux") {
                        "xdg-open"
                    } else {
                        "open"
                    };
                    let _ = std::process::Command::new(cmd).arg(&dashboard_url).spawn();
                },
            ))
        } else {
            None
        };

        // 整体布局
        let mut container = div().flex_col().child(quotas_container);

        if let Some(row) = dashboard_row {
            container = container.child(div().mt(px(8.0)).child(row));
        }

        container.into_any_element()
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

    fn render_refreshing_state(&self, provider: &ProviderStatus, theme: &Theme) -> Div {
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

        // 检测是否为配置错误（需要显示"打开配置"而非"重试"）
        let is_config_error = matches!(
            provider.error_kind,
            ErrorKind::ConfigMissing | ErrorKind::AuthRequired
        );

        let (title, message) = if is_error {
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

        let show_action = matches!(
            provider.connection,
            ConnectionStatus::Error | ConnectionStatus::Disconnected
        );
        let kind = provider.kind;
        let entity = cx.entity().clone();
        let state = self.state.clone();

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
                    .text_color(if is_error {
                        theme.status_error
                    } else {
                        theme.text_primary
                    })
                    .text_align(TextAlign::Center)
                    .child(title),
            );

        // 错误消息
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

        if show_action {
            if is_config_error {
                container = container.child(render_action_button(
                    &t!("provider.open_config"),
                    theme,
                    move |_, window, cx| {
                        schedule_open_settings_window_with_provider(
                            state.clone(),
                            kind,
                            window.display(cx).map(|d| d.id()),
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
                            view.refresh_single_provider(kind, cx);
                        });
                    },
                ));
            }
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
