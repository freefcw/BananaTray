use super::SettingsView;
use crate::app::widgets::{render_detail_section_title, render_info_row};
use crate::app::{persist_settings, provider_logic};
use crate::models::{AppSettings, ConnectionStatus, ProviderKind};
use crate::refresh::RefreshReason;
use crate::theme::Theme;
use gpui::*;

impl SettingsView {
    // ══════ Right detail panel ══════

    pub(super) fn render_provider_detail_panel(
        &self,
        providers: &[crate::models::ProviderStatus],
        selected: ProviderKind,
        settings: &AppSettings,
        theme: &Theme,
        viewport: Size<Pixels>,
    ) -> Div {
        let provider = providers.iter().find(|p| p.kind == selected).cloned();
        let is_enabled = settings.is_provider_enabled(selected);

        let (icon, display_name, subtitle) = if let Some(ref p) = provider {
            (
                p.icon_asset().to_string(),
                p.display_name().to_string(),
                provider_logic::provider_detail_subtitle(p),
            )
        } else {
            (
                "src/icons/provider-unknown.svg".to_string(),
                format!("{:?}", selected),
                format!("{:?} · not available", selected),
            )
        };

        let state_refresh = self.state.clone();
        let refresh_kind = selected;
        let state_toggle = self.state.clone();
        let toggle_kind = selected;

        let inner = div()
            .flex_col()
            .pl(px(8.0))
            .pr(px(12.0))
            .pt(px(8.0))
            .pb(px(12.0))
            .gap(px(16.0))
            // ── Header: icon + name + refresh + toggle ──
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                svg()
                                    .path(icon)
                                    .size(px(28.0))
                                    .text_color(theme.text_primary),
                            )
                            .child(
                                div()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_size(px(16.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.text_primary)
                                            .child(display_name),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(theme.text_muted)
                                            .child(subtitle),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            // Refresh button
                            .child(
                                div()
                                    .w(px(28.0))
                                    .h(px(28.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(theme.border_strong)
                                    .cursor_pointer()
                                    .text_size(px(14.0))
                                    .text_color(theme.text_muted)
                                    .child("⟳")
                                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                        let mut s = state_refresh.borrow_mut();
                                        s.request_provider_refresh(
                                            refresh_kind,
                                            RefreshReason::Manual,
                                        );
                                        drop(s);
                                        window.refresh();
                                    }),
                            )
                            // Toggle switch
                            .child(
                                crate::app::widgets::render_toggle_switch(
                                    is_enabled,
                                    px(44.0),
                                    px(24.0),
                                    px(18.0),
                                    theme,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    move |_, window, _cx| {
                                        let settings =
                                            state_toggle.borrow_mut().toggle_provider(toggle_kind);
                                        persist_settings(&settings);
                                        window.refresh();
                                    },
                                ),
                            ),
                    ),
            )
            // ── Info table ──
            .child(self.render_info_table(provider.as_ref(), is_enabled, theme))
            // ── Usage section ──
            .child(self.render_usage_section(provider.as_ref(), is_enabled, theme))
            // ── Settings section ──
            .child(self.render_settings_section(selected, settings, theme));

        let detail_scroll_h = viewport.height - px(65.0);

        div().flex_col().flex_1().overflow_hidden().child(
            div()
                .id("provider-detail-scroll")
                .flex_col()
                .h(detail_scroll_h)
                .overflow_y_scroll()
                .child(inner),
        )
    }

    // ══════ Info table ══════

    fn render_info_table(
        &self,
        provider: Option<&crate::models::ProviderStatus>,
        enabled: bool,
        theme: &Theme,
    ) -> Div {
        let state_text = if enabled { "Enabled" } else { "Disabled" };
        let source_text = "auto";
        let updated_text = provider
            .map(|p| p.format_last_updated())
            .unwrap_or_else(|| "Not fetched yet".to_string());
        let status_text = provider
            .map(|p| match p.connection {
                ConnectionStatus::Connected => "All Systems Operational".to_string(),
                ConnectionStatus::Disconnected => "Not detected".to_string(),
                ConnectionStatus::Refreshing => "Refreshing…".to_string(),
                ConnectionStatus::Error => "Error".to_string(),
            })
            .unwrap_or_else(|| "Unknown".to_string());

        div()
            .flex_col()
            .gap(px(6.0))
            .child(render_info_row("State", state_text, theme))
            .child(render_info_row("Source", source_text, theme))
            .child(render_info_row("Updated", &updated_text, theme))
            .child(render_info_row("Status", &status_text, theme))
    }

    // ══════ Usage section ══════

    fn render_usage_section(
        &self,
        provider: Option<&crate::models::ProviderStatus>,
        enabled: bool,
        theme: &Theme,
    ) -> Div {
        let mut section = div()
            .flex_col()
            .gap(px(8.0))
            .child(render_detail_section_title("Usage", theme));

        if !enabled {
            return section.child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_secondary)
                    .child("Enable this provider to start tracking usage."),
            );
        }

        if let Some(p) = provider {
            if !p.quotas.is_empty() {
                for quota in &p.quotas {
                    section =
                        section.child(crate::app::widgets::render_quota_bar(quota, false, theme));
                }
            } else if p.connection == ConnectionStatus::Error {
                let title = format!("Last {} fetch failed:", p.display_name());
                let msg = p
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "Unknown error".to_string());
                section = section
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(title),
                    )
                    .child(
                        div()
                            .px(px(10.0))
                            .py(px(8.0))
                            .rounded(px(6.0))
                            .bg(theme.bg_subtle)
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .line_height(relative(1.4))
                                    .text_color(theme.text_secondary)
                                    .child(msg),
                            ),
                    );
            } else {
                section = section.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.text_secondary)
                        .child("No usage yet"),
                );
            }
        } else {
            section = section.child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_secondary)
                    .child("Provider not available"),
            );
        }

        section
    }

    // ══════ Provider-specific settings ══════

    fn render_settings_section(
        &self,
        kind: ProviderKind,
        settings: &AppSettings,
        theme: &Theme,
    ) -> Div {
        let mut section = div()
            .flex_col()
            .gap(px(8.0))
            .child(render_detail_section_title("Settings", theme));

        match kind {
            ProviderKind::Copilot => {
                // 1. 解析 token（纯数据，由 provider 层处理）
                let mem_token = settings.providers.github_token.as_deref();
                let status = crate::providers::copilot::resolve_token(mem_token);

                // 2. 回写：若从磁盘/环境变量发现了 token，同步到内存状态
                if status.token.is_some() && settings.providers.github_token.is_none() {
                    self.state.borrow_mut().settings.providers.github_token = status.token.clone();
                }

                // 3. 委托 provider 渲染
                section = section.child(crate::providers::copilot::settings_ui::render_settings(
                    &status, theme,
                ));
            }
            _ => {
                let display_name = self
                    .state
                    .borrow()
                    .provider_store
                    .find(kind)
                    .map(|p| p.display_name().to_string())
                    .unwrap_or_else(|| format!("{:?}", kind));

                section = section.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(relative(1.4))
                        .text_color(theme.text_secondary)
                        .child(format!(
                            "{} is configured automatically. No additional settings required.",
                            display_name
                        )),
                );
            }
        }

        section
    }
}
