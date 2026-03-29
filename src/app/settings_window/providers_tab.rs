use super::SettingsView;
use crate::app::widgets::{render_card_separator, render_detail_section_title, render_info_row};
use crate::app::{persist_settings, provider_logic};
use crate::models::{AppSettings, ConnectionStatus, ProviderKind};
use crate::refresh::{RefreshReason, RefreshRequest};
use crate::theme::Theme;
use gpui::*;

impl SettingsView {
    /// Render Providers settings tab — two-column layout
    pub(super) fn render_providers_tab(
        &self,
        settings: &AppSettings,
        theme: &Theme,
        viewport: Size<Pixels>,
    ) -> Div {
        let selected = self.state.borrow().settings_ui.selected_provider;
        let providers = self.state.borrow().provider_store.providers.clone();

        div()
            .flex()
            .h_full()
            .overflow_hidden()
            .child(self.render_provider_sidebar(&providers, selected, settings, theme, viewport))
            .child(
                self.render_provider_detail_panel(&providers, selected, settings, theme, viewport),
            )
    }

    // ══════ Left sidebar ══════

    fn render_provider_sidebar(
        &self,
        _providers: &[crate::models::ProviderStatus],
        selected: ProviderKind,
        settings: &AppSettings,
        theme: &Theme,
        viewport: Size<Pixels>,
    ) -> Div {
        // NOTE: 不使用 render_card()，因为它带有 overflow_hidden()，
        // 会让 Taffy 将 card 的 min-height 设为 0，导致 card 在 Scrollable
        // 内部被压缩到容器高度，永远不会溢出，滚动条无法触发。
        let mut card = div()
            .flex_col()
            .rounded(px(10.0))
            .bg(rgb(0xffffff))
            .py(px(4.0));
        let ordered = settings.ordered_providers();

        for (i, kind) in ordered.iter().enumerate() {
            let is_selected = *kind == selected;
            let is_enabled = settings.is_provider_enabled(*kind);

            let status = _providers.iter().find(|p| p.kind == *kind);
            let icon = status
                .map(|p| p.icon_asset().to_string())
                .unwrap_or_else(|| "src/icons/provider-unknown.svg".to_string());
            let display_name = status
                .map(|p| p.display_name().to_string())
                .unwrap_or_else(|| format!("{:?}", kind));

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

            item =
                item
                    // Provider icon
                    .child(svg().path(icon).size(px(22.0)).flex_shrink_0().text_color(
                        if is_selected {
                            theme.element_active
                        } else {
                            theme.text_secondary
                        },
                    ))
                    // Name + green dot (enabled indicator)
                    .child({
                        let name_row = div().flex().items_center().gap(px(4.0)).flex_1().child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if is_selected {
                                    theme.element_active
                                } else {
                                    theme.text_primary
                                })
                                .child(display_name),
                        );
                        if is_enabled {
                            name_row.child(
                                div()
                                    .w(px(6.0))
                                    .h(px(6.0))
                                    .rounded_full()
                                    .bg(theme.status_success),
                            )
                        } else {
                            name_row
                        }
                    });

            // Reorder arrows — always reserve space, only interactive when selected
            {
                let is_first = i == 0;
                let is_last = i == ordered.len() - 1;
                let state_up = self.state.clone();
                let state_down = self.state.clone();
                let kind_up = *kind;
                let kind_down = *kind;

                let mut arrows = div().flex_col().flex_shrink_0();

                let mut up_btn = div()
                    .w(px(16.0))
                    .h(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.0))
                    .text_size(px(8.0));

                let mut down_btn = div()
                    .w(px(16.0))
                    .h(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.0))
                    .text_size(px(8.0));

                if !is_selected {
                    // Invisible placeholder to keep height stable
                    up_btn = up_btn.text_color(transparent_black()).child("▲");
                    down_btn = down_btn.text_color(transparent_black()).child("▼");
                } else {
                    if is_first {
                        up_btn = up_btn.text_color(theme.border_subtle).child("▲");
                    } else {
                        up_btn = up_btn
                            .cursor_pointer()
                            .text_color(theme.element_active)
                            .hover(|s| s.bg(theme.border_subtle))
                            .child("▲")
                            .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                let mut s = state_up.borrow_mut();
                                if s.settings.move_provider_up(kind_up) {
                                    persist_settings(&s.settings);
                                }
                                drop(s);
                                window.refresh();
                            });
                    }

                    if is_last {
                        down_btn = down_btn.text_color(theme.border_subtle).child("▼");
                    } else {
                        down_btn = down_btn
                            .cursor_pointer()
                            .text_color(theme.element_active)
                            .hover(|s| s.bg(theme.border_subtle))
                            .child("▼")
                            .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                let mut s = state_down.borrow_mut();
                                if s.settings.move_provider_down(kind_down) {
                                    persist_settings(&s.settings);
                                }
                                drop(s);
                                window.refresh();
                            });
                    }
                }

                arrows = arrows.child(up_btn).child(down_btn);
                item = item.child(arrows);
            }

            item = item.on_mouse_down(MouseButton::Left, move |_, window, _| {
                state.borrow_mut().settings_ui.selected_provider = kind_copy;
                window.refresh();
            });

            card = card.child(item);
        }

        // Tab bar ≈ 65px, sidebar top-padding = 8px
        let sidebar_scroll_h = viewport.height - px(65.0) - px(8.0);

        div()
            .flex_col()
            .flex_none()
            .flex_basis(px(190.0))
            .pl(px(8.0))
            .pr(px(4.0))
            .pt(px(8.0))
            .overflow_hidden()
            .child(
                div()
                    .id("provider-sidebar-scroll")
                    .flex_col()
                    .h(sidebar_scroll_h)
                    .overflow_y_scroll()
                    .child(card),
            )
    }

    // ══════ Right detail panel ══════

    fn render_provider_detail_panel(
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
            .id("provider-detail-scroll")
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
                                        s.provider_store.set_connection(
                                            refresh_kind,
                                            ConnectionStatus::Refreshing,
                                        );
                                        s.send_refresh(RefreshRequest::RefreshOne {
                                            kind: refresh_kind,
                                            reason: RefreshReason::Manual,
                                        });
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
