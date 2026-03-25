use super::provider_logic;
use super::AppView;
use crate::models::{ConnectionStatus, ProviderKind, ProviderStatus, StatusLevel};
use crate::theme::Theme;
use gpui::*;

const USAGE_ICON: &str = "src/icons/usage.svg";

impl AppView {
    pub(crate) fn render_provider_detail(
        &self,
        kind: ProviderKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state.borrow();
        let provider = state.providers.iter().find(|p| p.kind == kind).cloned();
        drop(state);

        if let Some(provider) = provider {
            self.render_provider_panel(&provider, true, true, cx)
        } else {
            div().child("Provider not found").into_any_element()
        }
    }

    fn render_provider_panel(
        &self,
        provider: &ProviderStatus,
        highlighted: bool,
        show_actions: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.global::<Theme>();
        let has_quotas = !provider.quotas.is_empty();
        let card_bg = if highlighted {
            theme.bg_card_active
        } else {
            theme.bg_card
        };
        let card_border = theme.border_subtle;
        let title_color = if highlighted {
            theme.element_active
        } else {
            theme.text_primary
        };
        let sub_color = theme.text_secondary;
        let account_text = provider_logic::provider_account_label(provider, highlighted);
        let last_updated = provider.format_last_updated();

        let shell = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(12.0))
            .rounded(px(14.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            // Row 1: Provider name (left) + account email (right)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(title_color)
                            .child(provider.kind.display_name()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(sub_color)
                            .child(account_text),
                    ),
            )
            // Row 2: Updated time
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(last_updated),
            );

        let shell =
            if has_quotas {
                shell.children(provider.quotas.iter().enumerate().map(|(index, quota)| {
                    self.render_quota_bar(quota, highlighted, index > 0, theme)
                }))
            } else {
                shell.child(self.render_provider_empty_state(provider, highlighted, theme))
            };

        let shell = if show_actions {
            let dashboard_url = provider.kind.dashboard_url();
            shell.child(
                div()
                    .border_t_1()
                    .border_color(theme.border_subtle)
                    .pt(px(8.0))
                    .mt(px(2.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(4.0))
                            .py(px(5.0))
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .text_size(px(13.0))
                            .text_color(if highlighted {
                                theme.element_active
                            } else {
                                theme.text_primary
                            })
                            .child(self.render_svg_icon(
                                USAGE_ICON,
                                px(13.0),
                                if highlighted {
                                    theme.text_secondary
                                } else {
                                    theme.text_muted
                                },
                            ))
                            .child("Usage Dashboard")
                            .on_mouse_down(MouseButton::Left, move |_, _, _| {
                                let _ = std::process::Command::new("open")
                                    .arg(dashboard_url)
                                    .spawn();
                            }),
                    ),
            )
        } else {
            shell
        };

        shell.into_any_element()
    }

    fn render_provider_empty_state(
        &self,
        provider: &ProviderStatus,
        highlighted: bool,
        theme: &Theme,
    ) -> impl IntoElement {
        let title = match provider.connection {
            ConnectionStatus::Connected => "Waiting for usage data",
            ConnectionStatus::Disconnected => "Connection required",
            ConnectionStatus::Error => "Refresh failed",
        };
        let message = provider_logic::provider_empty_message(provider);

        div()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(14.0))
            .bg(theme.bg_subtle)
            .px(px(12.0))
            .py(px(10.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if highlighted {
                        theme.element_active
                    } else {
                        theme.text_primary
                    })
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(relative(1.4))
                    .text_color(if highlighted {
                        theme.element_active
                    } else {
                        theme.text_secondary
                    })
                    .child(message),
            )
    }

    fn render_quota_bar(
        &self,
        q: &crate::models::QuotaInfo,
        highlighted: bool,
        show_divider: bool,
        theme: &Theme,
    ) -> impl IntoElement {
        let pct = q.percentage();
        let remaining_pct = (100.0 - pct).max(0.0);
        let bar_fill = match q.status_level() {
            StatusLevel::Green => theme.status_success,
            StatusLevel::Yellow => theme.status_warning,
            StatusLevel::Red => theme.status_error,
        };
        let title_color = if highlighted {
            theme.element_active
        } else {
            theme.text_primary
        };
        let secondary_color = if highlighted {
            theme.text_secondary
        } else {
            theme.text_muted
        };

        let row = div()
            .flex_col()
            .gap(px(5.0))
            // Quota label
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(title_color)
                    .child(q.label.clone()),
            )
            // Progress bar
            .child(
                div()
                    .w_full()
                    .h(px(10.0))
                    .bg(theme.progress_track)
                    .rounded_full()
                    .border_1()
                    .border_color(theme.border_subtle)
                    .overflow_hidden()
                    .child(
                        div()
                            .w(relative(pct as f32 / 100.0))
                            .h_full()
                            .bg(if highlighted {
                                theme.element_active
                            } else {
                                bar_fill
                            })
                            .rounded_full(),
                    ),
            )
            // Bottom info row: "XX% left" on left, reset info on right
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(secondary_color)
                            .child(format!("{:.0}% left", remaining_pct)),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(secondary_color)
                            .child(provider_logic::format_quota_usage(q)),
                    ),
            );

        if show_divider {
            row.mt(px(6.0))
        } else {
            row
        }
    }
}
