use crate::models::{
    AppSettings, AppTheme, ConnectionStatus, NavTab, ProviderKind, ProviderStatus, StatusLevel,
};
use crate::theme::Theme;
use crate::views::settings::SettingsPanel;
use gpui::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

const OVERVIEW_ICON: &str = "src/icons/overview.svg";
const SETTINGS_ICON: &str = "src/icons/settings.svg";
const ABOUT_ICON: &str = "src/icons/about.svg";
const QUIT_ICON: &str = "src/icons/quit.svg";
const SWITCH_ICON: &str = "src/icons/switch.svg";
const USAGE_ICON: &str = "src/icons/usage.svg";
const STATUS_ICON: &str = "src/icons/status.svg";
const AUTO_HIDE_ICON: &str = "src/icons/display.svg";

// ============================================================================
// 外部持久状态 (不随窗口销毁)
// ============================================================================

/// 应用持久状态，在窗口生命周期之外保持
pub struct AppState {
    pub providers: Vec<ProviderStatus>,
    pub settings: AppSettings,
    pub active_tab: NavTab,
    pub manager: Arc<crate::providers::ProviderManager>,
    pub refreshed: bool,
}

impl AppState {
    pub fn new() -> Self {
        let manager = Arc::new(crate::providers::ProviderManager::new());
        let providers = manager.initial_statuses();
        Self {
            providers,
            settings: AppSettings::default(),
            active_tab: NavTab::Overview,
            manager,
            refreshed: false,
        }
    }
}

// ============================================================================
// 窗口视图 (可多次创建/销毁)
// ============================================================================

pub struct AppView {
    state: Rc<RefCell<AppState>>,
    pub _activation_sub: Option<gpui::Subscription>,
}

impl AppView {
    pub fn new(state: Rc<RefCell<AppState>>, cx: &mut Context<Self>) -> Self {
        let theme = match state.borrow().settings.theme {
            AppTheme::Light => Theme::light(),
            AppTheme::Dark => Theme::dark(),
        };
        cx.set_global(theme);

        // 只在首次打开时刷新 provider 数据
        if !state.borrow().refreshed {
            state.borrow_mut().refreshed = true;
            Self::start_background_refresh(state.borrow().manager.clone(), cx);
        }

        Self {
            state,
            _activation_sub: None,
        }
    }

    fn start_background_refresh(
        manager: Arc<crate::providers::ProviderManager>,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().clone();
        cx.spawn(|_view, cx: &mut gpui::AsyncApp| {
            let async_cx = cx.clone();
            async move {
                let all_kinds = crate::models::ProviderKind::all().to_vec();
                for kind in all_kinds {
                    let mgr = manager.clone();
                    let result =
                        smol::unblock(move || smol::block_on(mgr.refresh_provider(kind))).await;

                    let entity = entity.clone();
                    match result {
                        Ok(quotas) => {
                            async_cx
                                .update(|cx| {
                                    entity.update(cx, |view, cx| {
                                        let mut s = view.state.borrow_mut();
                                        if let Some(p) =
                                            s.providers.iter_mut().find(|p| p.kind == kind)
                                        {
                                            p.quotas = quotas;
                                            p.connection = ConnectionStatus::Connected;
                                            p.last_updated_at =
                                                Some("Updated just now".to_string());
                                            p.error_message = None;
                                        }
                                        cx.notify();
                                    });
                                })
                                .ok();
                        }
                        Err(err) => {
                            async_cx
                                .update(|cx| {
                                    entity.update(cx, |view, cx| {
                                        let mut s = view.state.borrow_mut();
                                        if let Some(p) =
                                            s.providers.iter_mut().find(|p| p.kind == kind)
                                        {
                                            if p.quotas.is_empty() {
                                                p.connection = ConnectionStatus::Error;
                                            }
                                            p.last_updated_at = Some("Update failed".to_string());
                                            p.error_message = Some(err.to_string());
                                        }
                                        cx.notify();
                                    });
                                })
                                .ok();
                        }
                    }
                }
            }
        })
        .detach();
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let state = self.state.borrow();
        let active_tab = state.active_tab;
        drop(state);

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_base)
            .text_color(theme.text_primary)
            .p(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .bg(theme.bg_panel)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .rounded(px(16.0))
                    .child(self.render_top_nav(active_tab, cx))
                    .child(
                        div()
                            .id("content")
                            .flex_1()
                            .overflow_y_scroll()
                            .child(match active_tab {
                                NavTab::Overview => self.render_overview(cx),
                                NavTab::Provider(kind) => div()
                                    .px(px(12.0))
                                    .py(px(12.0))
                                    .child(self.render_provider_detail(kind, cx))
                                    .into_any_element(),
                                NavTab::Settings => self.render_settings_content(cx),
                            }),
                    )
                    .child(self.render_bottom_actions(cx)),
            )
    }
}

// ============================================================================
// 渲染方法
// ============================================================================

impl AppView {
    fn render_settings_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.state.borrow().settings.clone();
        let theme = cx.global::<Theme>();
        let state = self.state.clone();
        let entity = cx.entity().clone();

        div()
            .px(px(12.0))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .rounded(px(14.0))
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .px(px(14.0))
                    .py(px(12.0))
                    .cursor_pointer()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(self.render_footer_glyph(AUTO_HIDE_ICON, theme))
                            .child(
                                div()
                                    .flex_col()
                                    .gap(px(3.0))
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_primary)
                                            .child("Auto-hide window"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme.text_secondary)
                                            .child(
                                                "Close the tray popover when focus leaves the app.",
                                            ),
                                    ),
                            ),
                    )
                    .child(self.render_toggle_switch(settings.auto_hide_window, theme))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        {
                            let mut app_state = state.borrow_mut();
                            app_state.settings.auto_hide_window =
                                !app_state.settings.auto_hide_window;
                        }
                        entity.update(cx, |_, cx| {
                            cx.notify();
                        });
                    }),
            )
            .child(SettingsPanel::new(settings))
            .into_any_element()
    }

    fn render_top_nav(&self, active_tab: NavTab, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let nav_items = [
            (OVERVIEW_ICON, "Overview", NavTab::Overview),
            (
                ProviderKind::Claude.icon_asset(),
                "Claude",
                NavTab::Provider(ProviderKind::Claude),
            ),
            (
                ProviderKind::Gemini.icon_asset(),
                "Gemini",
                NavTab::Provider(ProviderKind::Gemini),
            ),
            (
                ProviderKind::Copilot.icon_asset(),
                "Copilot",
                NavTab::Provider(ProviderKind::Copilot),
            ),
            (
                ProviderKind::Amp.icon_asset(),
                "Amp",
                NavTab::Provider(ProviderKind::Amp),
            ),
            (
                ProviderKind::Kimi.icon_asset(),
                "Kimi",
                NavTab::Provider(ProviderKind::Kimi),
            ),
            (
                ProviderKind::Codex.icon_asset(),
                "Codex",
                NavTab::Provider(ProviderKind::Codex),
            ),
        ];

        div()
            .flex()
            .items_center()
            .w_full()
            .bg(theme.bg_panel)
            .border_b_1()
            .border_color(theme.border_subtle)
            .px(px(8.0))
            .py(px(8.0))
            .gap(px(6.0))
            .children(
                nav_items.into_iter().map(|(icon, label, tab)| {
                    self.render_nav_item(icon, label, tab, active_tab, cx)
                }),
            )
    }

    fn render_nav_item(
        &self,
        icon_path: &'static str,
        label: &'static str,
        tab: NavTab,
        active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = tab == active_tab;
        let theme = cx.global::<Theme>();
        let state = self.state.clone();
        let entity = cx.entity().clone();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .px(px(4.0))
            .py(px(6.0))
            .rounded(px(10.0))
            .cursor_pointer()
            .bg(if is_active {
                theme.element_selected
            } else {
                theme.bg_subtle
            })
            .child(
                div()
                    .w(px(20.0))
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(if is_active {
                        theme.text_accent_soft
                    } else {
                        theme.border_strong
                    })
                    .bg(if is_active {
                        theme.text_accent_soft
                    } else {
                        theme.bg_panel
                    })
                    .child(self.render_svg_icon(
                        icon_path,
                        px(13.0),
                        if is_active {
                            theme.element_active
                        } else {
                            theme.text_secondary
                        },
                    )),
            )
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(if is_active {
                        theme.element_active
                    } else {
                        theme.element_inactive
                    })
                    .child(label),
            )
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                state.borrow_mut().active_tab = tab;
                entity.update(cx, |_, cx| {
                    cx.notify();
                });
            })
    }

    fn render_overview(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.global::<Theme>();
        let state = self.state.borrow();
        let providers: Vec<_> = state
            .providers
            .iter()
            .filter(|p| {
                matches!(
                    p.kind,
                    ProviderKind::Claude
                        | ProviderKind::Gemini
                        | ProviderKind::Copilot
                        | ProviderKind::Amp
                )
            })
            .cloned()
            .collect();
        drop(state);

        let active_count = providers
            .iter()
            .filter(|provider| provider.connection == ConnectionStatus::Connected)
            .count();
        let alert_count = providers
            .iter()
            .filter(|provider| {
                provider.connection != ConnectionStatus::Connected
                    || provider.worst_status() == StatusLevel::Red
            })
            .count();
        let tracked_quota_count: usize =
            providers.iter().map(|provider| provider.quotas.len()).sum();

        div()
            .flex_col()
            .py(px(8.0))
            .child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child("Overview"),
                    )
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::BOLD)
                            .child("Quota snapshot"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_secondary)
                            .child("Live provider usage and connection health."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .pb(px(10.0))
                    .child(self.render_overview_stat(
                        "Connected",
                        format!("{} / {}", active_count, providers.len()),
                        theme.status_success,
                        theme,
                    ))
                    .child(self.render_overview_stat(
                        "Alerts",
                        alert_count.to_string(),
                        if alert_count == 0 {
                            theme.text_secondary
                        } else {
                            theme.status_warning
                        },
                        theme,
                    ))
                    .child(self.render_overview_stat(
                        "Tracked",
                        tracked_quota_count.to_string(),
                        theme.text_accent,
                        theme,
                    )),
            )
            .children(
                providers
                    .iter()
                    .map(|provider| self.render_provider_panel(provider, false, false, cx)),
            )
            .into_any_element()
    }

    fn render_provider_detail(&self, kind: ProviderKind, cx: &mut Context<Self>) -> AnyElement {
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
            theme.bg_panel
        };
        let card_border = if highlighted {
            theme.text_accent_soft
        } else {
            theme.border_subtle
        };
        let status_tint = if provider.connection != ConnectionStatus::Connected {
            theme.status_error
        } else {
            match provider.worst_status() {
                StatusLevel::Green => theme.status_success,
                StatusLevel::Yellow => theme.status_warning,
                StatusLevel::Red => theme.status_error,
            }
        };
        let title_color = if highlighted {
            theme.element_active
        } else {
            theme.text_primary
        };
        let sub_color = if highlighted {
            theme.element_active
        } else {
            theme.text_secondary
        };
        let status_text = self.provider_status_label(provider);
        let health_text = self.provider_health_label(provider);
        let account_text = self.provider_account_label(provider, highlighted);
        let usage_snapshot = self.provider_usage_snapshot(provider);
        let plan_text = if provider.is_paid {
            "Paid"
        } else if provider.connection == ConnectionStatus::Connected {
            "Ready"
        } else {
            "Setup"
        };
        let last_updated =
            provider
                .last_updated_at
                .clone()
                .unwrap_or_else(|| match provider.connection {
                    ConnectionStatus::Connected => "Updated recently".to_string(),
                    ConnectionStatus::Error => "Needs attention".to_string(),
                    ConnectionStatus::Disconnected => "Not connected".to_string(),
                });

        let shell = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(12.0))
            .rounded(px(14.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_start()
                    .child(
                        div()
                            .flex()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .w(px(28.0))
                                    .h(px(28.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(9.0))
                                    .border_1()
                                    .border_color(if highlighted {
                                        rgb(0xffffff).into()
                                    } else {
                                        status_tint
                                    })
                                    .bg(if highlighted {
                                        rgb(0xffffff).into()
                                    } else {
                                        theme.bg_subtle
                                    })
                                    .child(self.render_svg_icon(
                                        provider.kind.icon_asset(),
                                        px(16.0),
                                        if highlighted {
                                            theme.bg_card_active
                                        } else {
                                            status_tint
                                        },
                                    )),
                            )
                            .child(
                                div()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_size(px(18.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(title_color)
                                            .child(provider.kind.display_name()),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(sub_color)
                                                    .child(last_updated),
                                            )
                                            .child(self.render_provider_badge(
                                                status_text,
                                                highlighted,
                                                status_tint,
                                                theme,
                                            )),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_col()
                            .items_end()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(sub_color)
                                    .child(account_text),
                            )
                            .child(self.render_provider_badge(
                                plan_text,
                                highlighted,
                                status_tint,
                                theme,
                            )),
                    ),
            );

        let shell = if has_quotas {
            shell.child(
                div()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(self.render_summary_chip(
                                usage_snapshot,
                                highlighted,
                                status_tint,
                                theme,
                            ))
                            .child(self.render_summary_chip(
                                format!("{} quota sources", provider.quotas.len()),
                                highlighted,
                                if highlighted {
                                    rgb(0xffffff).into()
                                } else {
                                    theme.text_secondary
                                },
                                theme,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .rounded(px(10.0))
                            .bg(if highlighted {
                                theme.text_accent_soft
                            } else {
                                theme.bg_card
                            })
                            .border_1()
                            .border_color(if highlighted {
                                rgb(0xffffff).into()
                            } else {
                                theme.border_subtle
                            })
                            .px(px(10.0))
                            .py(px(7.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(if highlighted {
                                        theme.element_active
                                    } else {
                                        theme.text_secondary
                                    })
                                    .child(format!("{} quotas tracked", provider.quotas.len())),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(if highlighted {
                                        theme.element_active
                                    } else {
                                        status_tint
                                    })
                                    .child(health_text),
                            ),
                    )
                    .gap(px(10.0))
                    .children(
                        provider
                            .quotas
                            .iter()
                            .map(|quota| self.render_quota_bar(quota, highlighted, theme)),
                    ),
            )
        } else {
            shell.child(self.render_provider_empty_state(provider, highlighted, theme))
        };

        let shell = if show_actions {
            shell.child(
                div()
                    .flex_col()
                    .border_t_1()
                    .border_color(if highlighted {
                        rgb(0xffffff).into()
                    } else {
                        theme.border_subtle
                    })
                    .pt(px(6.0))
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .gap(px(5.0))
                            .child(self.render_action_link(SWITCH_ICON, "Switch", highlighted, cx))
                            .child(self.render_action_link(
                                USAGE_ICON,
                                "Dashboard",
                                highlighted,
                                cx,
                            ))
                            .child(self.render_action_link(STATUS_ICON, "Status", highlighted, cx)),
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
        let message = self.provider_empty_message(provider);

        div()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(if highlighted {
                rgb(0xffffff).into()
            } else {
                theme.border_strong
            })
            .bg(if highlighted {
                theme.bg_card_active
            } else {
                theme.bg_card
            })
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
        theme: &Theme,
    ) -> impl IntoElement {
        let pct = q.percentage();
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
        let sub_color = if highlighted {
            theme.element_active
        } else {
            theme.text_secondary
        };

        div()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(title_color)
                            .child(q.label.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(sub_color)
                            .child(self.format_quota_usage(q)),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .h(px(6.0))
                    .bg(theme.progress_track)
                    .rounded_full()
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
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(11.0))
                    .text_color(sub_color)
                    .child(format!("{:.0}% left", (100.0 - pct).max(0.0)))
                    .child(self.quota_health_copy(q)),
            )
    }

    fn render_provider_badge(
        &self,
        label: &str,
        highlighted: bool,
        tint: Hsla,
        theme: &Theme,
    ) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(3.0))
            .rounded_full()
            .border_1()
            .border_color(if highlighted {
                rgb(0xffffff).into()
            } else {
                tint
            })
            .bg(if highlighted {
                theme.text_accent_soft
            } else {
                theme.bg_subtle
            })
            .text_size(px(10.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(if highlighted {
                theme.element_active
            } else {
                tint
            })
            .child(label.to_string())
    }

    fn render_summary_chip(
        &self,
        label: String,
        highlighted: bool,
        tint: Hsla,
        theme: &Theme,
    ) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(9.0))
            .bg(if highlighted {
                theme.text_accent_soft
            } else {
                theme.bg_card
            })
            .border_1()
            .border_color(if highlighted {
                rgb(0xffffff).into()
            } else {
                theme.border_subtle
            })
            .text_size(px(10.0))
            .text_color(if highlighted {
                theme.element_active
            } else {
                tint
            })
            .child(label)
    }

    fn render_overview_stat(
        &self,
        label: &'static str,
        value: String,
        tint: Hsla,
        theme: &Theme,
    ) -> impl IntoElement {
        div()
            .flex_1()
            .flex_col()
            .gap(px(4.0))
            .px(px(10.0))
            .py(px(10.0))
            .rounded(px(12.0))
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(label),
            )
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(tint)
                    .child(value),
            )
    }

    fn provider_status_label(&self, provider: &ProviderStatus) -> &'static str {
        match provider.connection {
            ConnectionStatus::Connected => "Live",
            ConnectionStatus::Disconnected => "Setup needed",
            ConnectionStatus::Error => "Needs attention",
        }
    }

    fn provider_health_label(&self, provider: &ProviderStatus) -> &'static str {
        if provider.connection != ConnectionStatus::Connected {
            return "Unavailable";
        }

        match provider.worst_status() {
            StatusLevel::Green => "Healthy",
            StatusLevel::Yellow => "Watch",
            StatusLevel::Red => "Near limit",
        }
    }

    fn provider_empty_message(&self, provider: &ProviderStatus) -> String {
        if let Some(error) = &provider.error_message {
            if error.contains("Missing environment variable") {
                return format!(
                    "Connect {} credentials before quota tracking can start.",
                    provider.kind.display_name()
                );
            }

            if error.contains("session cookie expired") {
                return "Session expired. Sign in again to refresh usage.".to_string();
            }

            return error.clone();
        }

        match provider.connection {
            ConnectionStatus::Error => {
                format!(
                    "{} usage could not be refreshed right now.",
                    provider.kind.display_name()
                )
            }
            ConnectionStatus::Disconnected => {
                format!(
                    "Connect {} to start tracking quota.",
                    provider.kind.display_name()
                )
            }
            ConnectionStatus::Connected => "No usage details available yet.".to_string(),
        }
    }

    fn provider_account_label(&self, provider: &ProviderStatus, compact: bool) -> String {
        if let Some(email) = &provider.account_email {
            return email.clone();
        }

        if compact {
            match provider.kind {
                ProviderKind::Claude => "Anthropic".to_string(),
                ProviderKind::Gemini => "Google".to_string(),
                ProviderKind::Copilot => "GitHub".to_string(),
                ProviderKind::Codex => "OpenAI".to_string(),
                ProviderKind::Kimi => "Moonshot".to_string(),
                ProviderKind::Amp => "Amp CLI".to_string(),
            }
        } else {
            provider.kind.account_hint().to_string()
        }
    }

    fn provider_usage_snapshot(&self, provider: &ProviderStatus) -> String {
        let peak_percentage = provider
            .quotas
            .iter()
            .map(|quota| quota.percentage())
            .fold(0.0_f64, f64::max);

        if peak_percentage <= 0.0 {
            "No usage yet".to_string()
        } else {
            format!("Peak usage {:.0}%", peak_percentage)
        }
    }

    fn format_quota_usage(&self, quota: &crate::models::QuotaInfo) -> String {
        format!(
            "{} / {} used",
            self.format_amount(quota.used),
            self.format_amount(quota.limit)
        )
    }

    fn format_amount(&self, value: f64) -> String {
        if (value.fract() - 0.0).abs() < f64::EPSILON {
            format!("{:.0}", value)
        } else {
            format!("{:.1}", value)
        }
    }

    fn quota_health_copy(&self, quota: &crate::models::QuotaInfo) -> &'static str {
        match quota.status_level() {
            StatusLevel::Green => "Healthy",
            StatusLevel::Yellow => "Watch",
            StatusLevel::Red => "Near limit",
        }
    }

    fn render_action_link(
        &self,
        icon_path: &'static str,
        label: &'static str,
        highlighted: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();

        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .px(px(7.0))
            .py(px(5.0))
            .rounded_full()
            .bg(if highlighted {
                theme.text_accent_soft
            } else {
                theme.bg_card
            })
            .border_1()
            .border_color(if highlighted {
                rgb(0xffffff).into()
            } else {
                theme.border_subtle
            })
            .text_size(px(11.0))
            .text_color(if highlighted {
                theme.element_active
            } else {
                theme.text_primary
            })
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .w(px(14.0))
                            .h(px(14.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(if highlighted {
                                hsla(0.0, 0.0, 1.0, 0.18)
                            } else {
                                theme.bg_subtle
                            })
                            .child(self.render_svg_icon(
                                icon_path,
                                px(8.0),
                                if highlighted {
                                    theme.element_active
                                } else {
                                    theme.text_secondary
                                },
                            )),
                    )
                    .child(label),
            )
    }

    fn render_bottom_actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let state = self.state.clone();
        let entity = cx.entity().clone();

        div()
            .border_t_1()
            .border_color(theme.border_subtle)
            .flex_col()
            .py(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(16.0))
                    .py(px(5.0))
                    .text_size(px(13.0))
                    .cursor_pointer()
                    .child(self.render_footer_glyph(SETTINGS_ICON, theme))
                    .child("Settings...")
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        state.borrow_mut().active_tab = NavTab::Settings;
                        entity.update(cx, |_, cx| {
                            cx.notify();
                        });
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(16.0))
                    .py(px(5.0))
                    .text_size(px(13.0))
                    .cursor_pointer()
                    .child(self.render_footer_glyph(ABOUT_ICON, theme))
                    .child("About BananaTray"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(16.0))
                    .py(px(5.0))
                    .text_size(px(13.0))
                    .cursor_pointer()
                    .text_color(theme.text_primary)
                    .child(self.render_footer_glyph(QUIT_ICON, theme))
                    .child("Quit")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.quit();
                    }),
            )
    }

    fn render_footer_glyph(&self, icon_path: &'static str, theme: &Theme) -> impl IntoElement {
        div()
            .w(px(18.0))
            .h(px(18.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .border_1()
            .border_color(theme.border_strong)
            .child(self.render_svg_icon(icon_path, px(11.0), theme.text_secondary))
    }

    fn render_svg_icon(&self, path: &'static str, size: Pixels, color: Hsla) -> impl IntoElement {
        svg().path(path).size(size).text_color(color)
    }

    fn render_toggle_switch(&self, enabled: bool, theme: &Theme) -> impl IntoElement {
        div()
            .w(px(36.0))
            .h(px(20.0))
            .flex()
            .items_center()
            .rounded_full()
            .px(px(2.0))
            .bg(if enabled {
                theme.element_selected
            } else {
                theme.bg_subtle
            })
            .border_1()
            .border_color(if enabled {
                theme.text_accent_soft
            } else {
                theme.border_strong
            })
            .child(
                div()
                    .w(px(14.0))
                    .h(px(14.0))
                    .rounded_full()
                    .bg(theme.element_active)
                    .ml(if enabled { px(16.0) } else { px(0.0) }),
            )
    }
}
