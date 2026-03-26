use super::SettingsView;
use crate::app::widgets::{
    render_card, render_card_separator, render_checkbox, render_detail_section_title,
    render_info_row,
};
use crate::app::{persist_settings, provider_logic};
use crate::models::{AppSettings, ConnectionStatus, ProviderKind};
use crate::theme::Theme;
use gpui::*;

impl SettingsView {
    /// Read github_token from the actual config file on disk
    fn read_github_token_from_config() -> Option<String> {
        let path = crate::settings_store::config_path();
        let content = std::fs::read_to_string(path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;
        json.get("providers")
            .and_then(|p| p.get("github_token"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }

    /// Render Providers settings tab — CodeBar-style two-column layout
    pub(super) fn render_providers_tab(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let selected = self.state.borrow().settings_selected_provider;
        let providers = self.state.borrow().providers.clone();

        div()
            .flex()
            .flex_1()
            .min_h(px(540.0))
            .child(self.render_provider_sidebar(&providers, selected, settings, theme))
            .child(self.render_provider_detail_panel(&providers, selected, settings, theme))
    }

    // ══════ Left sidebar ══════

    fn render_provider_sidebar(
        &self,
        providers: &[crate::models::ProviderStatus],
        selected: ProviderKind,
        settings: &AppSettings,
        theme: &Theme,
    ) -> Div {
        let mut card = render_card().py(px(4.0));

        for (i, kind) in ProviderKind::all().iter().enumerate() {
            let provider = providers.iter().find(|p| p.kind == *kind);
            let is_selected = *kind == selected;
            let is_enabled = settings.is_provider_enabled(*kind);
            let subtitle = if let Some(p) = provider {
                provider_logic::provider_list_subtitle(p)
            } else {
                format!("Disabled — {}", kind.source_label())
            };

            let state = self.state.clone();
            let kind_copy = *kind;

            if i > 0 {
                card = card.child(render_card_separator());
            }

            let mut item = div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(8.0))
                .cursor_pointer();

            if is_selected {
                item = item.mx(px(4.0)).rounded(px(8.0)).bg(theme.element_selected);
            }

            item = item
                // Provider icon
                .child(
                    svg()
                        .path(kind.icon_asset())
                        .size(px(22.0))
                        .flex_shrink_0()
                        .text_color(if is_selected {
                            theme.element_active
                        } else {
                            theme.text_secondary
                        }),
                )
                // Name + subtitle column
                .child(
                    div()
                        .flex_col()
                        .flex_1()
                        .overflow_hidden()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(if is_selected {
                                            theme.element_active
                                        } else {
                                            theme.text_primary
                                        })
                                        .child(kind.display_name()),
                                )
                                // Green dot
                                .child(
                                    div()
                                        .w(px(6.0))
                                        .h(px(6.0))
                                        .rounded_full()
                                        .bg(theme.status_success),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .line_height(relative(1.3))
                                .text_color(if is_selected {
                                    theme.element_active
                                } else {
                                    theme.text_muted
                                })
                                .overflow_hidden()
                                .child(subtitle),
                        ),
                );

            // Enabled badge (blue checkbox)
            if is_enabled {
                item = item.child(render_checkbox(true, px(20.0), theme));
            }

            item = item.on_mouse_down(MouseButton::Left, move |_, window, _| {
                state.borrow_mut().settings_selected_provider = kind_copy;
                window.refresh();
            });

            card = card.child(item);
        }

        div()
            .flex_col()
            .w(px(190.0))
            .pl(px(8.0))
            .pr(px(4.0))
            .pt(px(8.0))
            .child(card)
    }

    // ══════ Right detail panel ══════

    fn render_provider_detail_panel(
        &self,
        providers: &[crate::models::ProviderStatus],
        selected: ProviderKind,
        settings: &AppSettings,
        theme: &Theme,
    ) -> Div {
        let provider = providers.iter().find(|p| p.kind == selected).cloned();
        let is_enabled = settings.is_provider_enabled(selected);
        let subtitle = if let Some(ref p) = provider {
            provider_logic::provider_detail_subtitle(p)
        } else {
            format!("{} · not available", selected.source_label())
        };

        let state_toggle = self.state.clone();
        let toggle_kind = selected;

        div()
            .flex_col()
            .flex_1()
            .pl(px(8.0))
            .pr(px(12.0))
            .pt(px(8.0))
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
                                    .path(selected.icon_asset())
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
                                            .child(selected.display_name()),
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
                                    .child("⟳"),
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
                                    move |_, window, _| {
                                        let settings = {
                                            let mut s = state_toggle.borrow_mut();
                                            let new_val =
                                                !s.settings.is_provider_enabled(toggle_kind);
                                            s.settings.set_provider_enabled(toggle_kind, new_val);
                                            if let Some(p) = s
                                                .providers
                                                .iter_mut()
                                                .find(|p| p.kind == toggle_kind)
                                            {
                                                p.enabled = new_val;
                                            }
                                            // Force a fresh refresh when the popup next opens
                                            if new_val {
                                                s.last_refresh_started = None;
                                            }
                                            s.settings.clone()
                                        };
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
            .child(self.render_settings_section(selected, settings, theme))
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
                let title = format!("Last {} fetch failed:", p.kind.display_name());
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
                section = section.child(self.render_copilot_settings(settings, theme));
            }
            _ => {
                section = section.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(relative(1.4))
                        .text_color(theme.text_secondary)
                        .child(format!(
                            "{} is configured automatically. No additional settings required.",
                            kind.display_name()
                        )),
                );
            }
        }

        section
    }

    fn render_copilot_settings(&self, settings: &AppSettings, theme: &Theme) -> Div {
        // Check token from multiple sources: in-memory (loaded at startup), disk, env var
        let mem_token = settings
            .providers
            .github_token
            .clone()
            .filter(|s| !s.is_empty());
        let disk_token = Self::read_github_token_from_config();
        let env_token = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty());

        let (effective_token, source) = if let Some(t) = mem_token {
            (Some(t), "config file")
        } else if let Some(t) = disk_token {
            // Sync disk token into memory so future persist_settings won't overwrite it
            self.state.borrow_mut().settings.providers.github_token = Some(t.clone());
            (Some(t), "config file")
        } else if let Some(t) = env_token {
            (Some(t), "GITHUB_TOKEN env")
        } else {
            (None, "")
        };

        let has_token = effective_token.is_some();
        let masked = effective_token.as_ref().map(|t| {
            if t.len() <= 8 {
                "••••••••".to_string()
            } else {
                format!("{}••••{}", &t[..4], &t[t.len() - 4..])
            }
        });

        div()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child("GitHub Login"),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(relative(1.4))
                    .text_color(theme.text_secondary)
                    .child("Requires authentication via GitHub Token."),
            )
            .child(if has_token {
                div()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(6.0))
                            .bg(theme.status_success)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.element_active)
                            .child("Token configured"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child(format!("{} · via {}", masked.unwrap_or_default(), source)),
                    )
            } else {
                div()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(theme.text_muted)
                            .child("Set token via config file or GITHUB_TOKEN env var"),
                    )
                    .child(
                        div()
                            .w_full()
                            .py(px(8.0))
                            .rounded(px(8.0))
                            .bg(theme.text_primary)
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.element_active)
                            .cursor_pointer()
                            .flex()
                            .justify_center()
                            .child("Sign in with GitHub")
                            .on_mouse_down(MouseButton::Left, |_, _, _| {
                                let path = crate::settings_store::config_path();
                                if let Some(parent) = path.parent() {
                                    let _ = std::fs::create_dir_all(parent);
                                }
                                let _ = std::process::Command::new("open").arg(&path).spawn();
                            }),
                    )
            })
    }
}
