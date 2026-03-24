use gpui::*;
use crate::models::{AppSettings, AppTheme, ConnectionStatus, NavTab, ProviderKind, ProviderStatus};
use crate::theme::Theme;
use crate::views::settings::SettingsPanel;
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::Arc;

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
}

impl AppView {
    pub fn new(state: Rc<RefCell<AppState>>, cx: &mut Context<Self>) -> Self {
        cx.set_global(Theme::dark());

        // 只在首次打开时刷新 provider 数据
        if !state.borrow().refreshed {
            state.borrow_mut().refreshed = true;
            Self::start_background_refresh(state.borrow().manager.clone(), cx);
        }

        Self { state }
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
                    let result = smol::unblock(move || {
                        smol::block_on(mgr.refresh_provider(kind))
                    })
                    .await;

                    let entity = entity.clone();
                    match result {
                        Ok(quotas) => {
                            async_cx
                                .update(|cx| {
                                    let _ = entity.update(cx, |view, cx| {
                                        let mut s = view.state.borrow_mut();
                                        if let Some(p) =
                                            s.providers.iter_mut().find(|p| p.kind == kind)
                                        {
                                            p.quotas = quotas;
                                            p.connection = ConnectionStatus::Connected;
                                            p.last_updated_at =
                                                Some("Updated just now".to_string());
                                        }
                                        cx.notify();
                                    });
                                })
                                .ok();
                        }
                        Err(_) => {
                            async_cx
                                .update(|cx| {
                                    let _ = entity.update(cx, |view, cx| {
                                        let mut s = view.state.borrow_mut();
                                        if let Some(p) =
                                            s.providers.iter_mut().find(|p| p.kind == kind)
                                        {
                                            if p.quotas.is_empty() {
                                                p.connection = ConnectionStatus::Error;
                                            }
                                            p.last_updated_at =
                                                Some("Update failed".to_string());
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
            .child(self.render_top_nav(active_tab, cx))
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(match active_tab {
                        NavTab::Overview => self.render_overview(cx),
                        NavTab::Provider(kind) => div()
                            .px(px(20.0))
                            .py(px(16.0))
                            .child(self.render_provider_detail(kind, cx))
                            .into_any_element(),
                        NavTab::Settings => {
                            let settings = self.state.borrow().settings.clone();
                            div()
                                .px(px(20.0))
                                .py(px(16.0))
                                .child(SettingsPanel::new(settings))
                                .into_any_element()
                        }
                    }),
            )
            .child(self.render_bottom_actions(cx))
    }
}

// ============================================================================
// 渲染方法
// ============================================================================

impl AppView {
    fn render_top_nav(&self, active_tab: NavTab, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();

        div()
            .flex()
            .items_center()
            .justify_start()
            .w_full()
            .h(px(56.0))
            .bg(theme.bg_base)
            .border_b_1()
            .border_color(theme.border_subtle)
            .px(px(12.0))
            .gap(px(8.0))
            .child(self.render_nav_item("🍱", "Overview", NavTab::Overview, active_tab, cx))
            .child(self.render_nav_item("🔱", "Claude", NavTab::Provider(ProviderKind::Claude), active_tab, cx))
            .child(self.render_nav_item("✨", "Gemini", NavTab::Provider(ProviderKind::Gemini), active_tab, cx))
            .child(self.render_nav_item("🐙", "Copilot", NavTab::Provider(ProviderKind::Copilot), active_tab, cx))
            .child(self.render_nav_item("⚡", "Amp", NavTab::Provider(ProviderKind::Amp), active_tab, cx))
            .child(self.render_nav_item("🏮", "Kimi", NavTab::Provider(ProviderKind::Kimi), active_tab, cx))
            .child(self.render_nav_item("📜", "Codex", NavTab::Provider(ProviderKind::Codex), active_tab, cx))
    }

    fn render_nav_item(
        &self,
        icon: &'static str,
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
            .items_center()
            .justify_center()
            .px(px(8.0))
            .py(px(4.0))
            .rounded_md()
            .cursor_pointer()
            .bg(if is_active { theme.element_selected } else { Hsla::transparent_black() })
            .child(div().text_size(px(18.0)).child(icon))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(if is_active { theme.text_primary } else { theme.text_secondary })
                    .child(label),
            )
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                state.borrow_mut().active_tab = tab;
                entity.update(cx, |_, cx| { cx.notify(); });
            })
    }

    fn render_overview(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.state.borrow();
        let rows: Vec<_> = state
            .providers
            .iter()
            .map(|p| self.render_compact_provider_row(p, cx))
            .collect();
        drop(state);

        div().flex_col().py(px(8.0)).children(rows).into_any_element()
    }

    fn render_compact_provider_row(
        &self,
        p: &ProviderStatus,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let worst_quota = p.quotas.first();
        let connected = p.connection == ConnectionStatus::Connected;
        let name = p.kind.display_name();
        let pct_text = worst_quota
            .map(|q| format!("{:.0}%", q.percentage()))
            .unwrap_or_else(|| "-".to_string());

        div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(20.0))
            .py(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded_full()
                            .bg(if connected { theme.status_success } else { theme.status_error }),
                    )
                    .child(div().text_size(px(14.0)).text_color(theme.text_primary).child(name)),
            )
            .child(div().text_size(px(12.0)).text_color(theme.text_secondary).child(pct_text))
    }

    fn render_provider_detail(&self, kind: ProviderKind, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.global::<Theme>();
        let state = self.state.borrow();
        let provider = state.providers.iter().find(|p| p.kind == kind);

        if let Some(p) = provider {
            let display_name = p.kind.display_name();
            let last_updated = p.last_updated_at.clone().unwrap_or_default();
            let email = p.account_email.clone().unwrap_or_default();
            let is_paid = p.is_paid;
            let quotas: Vec<_> = p.quotas.clone();
            drop(state);

            div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_start()
                        .child(
                            div()
                                .flex_col()
                                .child(div().text_size(px(18.0)).font_weight(FontWeight::BOLD).child(display_name))
                                .child(div().text_size(px(12.0)).text_color(theme.text_secondary).child(last_updated)),
                        )
                        .child(
                            div()
                                .flex_col()
                                .items_end()
                                .child(div().text_size(px(13.0)).child(email))
                                .child(div().text_size(px(12.0)).text_color(theme.text_accent).child(if is_paid { "Paid" } else { "Free" })),
                        ),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(px(12.0))
                        .children(quotas.iter().map(|q| self.render_quota_bar(q, cx))),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .flex_col()
                        .gap(px(4.0))
                        .child(self.render_action_link("🔑 Switch Account...", cx))
                        .child(self.render_action_link("📊 Usage Dashboard", cx))
                        .child(self.render_action_link("📈 Status Page", cx)),
                )
                .into_any_element()
        } else {
            drop(state);
            div().child("Provider not found").into_any_element()
        }
    }

    fn render_quota_bar(
        &self,
        q: &crate::models::QuotaInfo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let pct = q.percentage();

        div()
            .flex_col()
            .gap(px(4.0))
            .child(div().text_size(px(13.0)).font_weight(FontWeight::SEMIBOLD).child(q.label.clone()))
            .child(
                div()
                    .w_full()
                    .h(px(8.0))
                    .bg(theme.bg_subtle)
                    .rounded_full()
                    .child(
                        div()
                            .w(relative(pct as f32 / 100.0))
                            .h_full()
                            .bg(theme.text_accent)
                            .rounded_full(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(format!("{:.0}% left", 100.0 - pct))
                    .child("Resets in 1d"),
            )
    }

    fn render_action_link(
        &self,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        div()
            .py(px(4.0))
            .text_size(px(14.0))
            .text_color(theme.text_primary)
            .cursor_pointer()
            .hover(|s| s.text_color(theme.text_accent))
            .child(label)
    }

    fn render_bottom_actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let state = self.state.clone();
        let entity = cx.entity().clone();

        div()
            .border_t_1()
            .border_color(theme.border_subtle)
            .flex_col()
            .py(px(8.0))
            .child(
                div()
                    .px(px(20.0))
                    .py(px(4.0))
                    .text_size(px(14.0))
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme.text_accent))
                    .child("Settings...")
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        state.borrow_mut().active_tab = NavTab::Settings;
                        entity.update(cx, |_, cx| { cx.notify(); });
                    }),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(4.0))
                    .text_size(px(14.0))
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme.text_accent))
                    .child("About BananaTray"),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(4.0))
                    .text_size(px(14.0))
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme.text_accent))
                    .child("Quit")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.quit();
                    }),
            )
    }
}
