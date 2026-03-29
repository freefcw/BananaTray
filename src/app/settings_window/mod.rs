mod general_tab;
mod provider_detail;
mod provider_sidebar;
mod window_mgr;

use super::AppState;
use crate::app::widgets::render_icon_tab;
use crate::theme::Theme;
use gpui::*;
use log::info;
use std::cell::RefCell;
use std::rc::Rc;

pub use window_mgr::schedule_open_settings_window;

// ============================================================================
// Settings Tab 枚举
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Providers,
    Display,
    Advanced,
    About,
}

// ============================================================================
// 设置视图
// ============================================================================

pub(super) struct SettingsView {
    pub(super) state: Rc<RefCell<AppState>>,
}

impl SettingsView {
    pub(super) fn new(state: Rc<RefCell<AppState>>, _cx: &mut Context<Self>) -> Self {
        info!(target: "settings", "constructing settings view");
        Self { state }
    }

    pub(super) fn preferences_theme() -> Theme {
        Theme {
            bg_base: rgb(0xf2f2f7).into(),
            bg_panel: rgb(0xffffff).into(),
            bg_subtle: rgb(0xebebf0).into(),
            bg_card: rgb(0xe0ecfb).into(),
            text_primary: rgb(0x1c1c1e).into(),
            text_secondary: rgb(0x6e6e73).into(),
            text_muted: rgb(0x8e8e93).into(),
            text_accent: rgb(0x007aff).into(),
            text_accent_soft: rgb(0xc2dcf7).into(),
            border_subtle: rgb(0xdcdce2).into(),
            border_strong: rgb(0xc7c7cc).into(),
            element_active: rgb(0xffffff).into(),
            element_selected: rgb(0x007aff).into(),
            status_success: rgb(0x34c759).into(),
            status_error: rgb(0xff3b30).into(),
            status_warning: rgb(0xff9f0a).into(),
            progress_track: rgb(0xe5e5ea).into(),
        }
    }

    /// Render Providers settings tab — two-column layout
    fn render_providers_tab(
        &self,
        settings: &crate::models::AppSettings,
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

    /// Render a placeholder page for unimplemented tabs
    fn render_placeholder_tab(tab: SettingsTab, theme: &Theme) -> Div {
        let title = match tab {
            SettingsTab::Display => "Display",
            SettingsTab::Advanced => "Advanced",
            SettingsTab::About => "About",
            _ => "",
        };
        let desc = match tab {
            SettingsTab::Display => {
                "Customize appearance, menu bar icon style, and notification preferences."
            }
            SettingsTab::Advanced => "Debug logging, network proxy, and other advanced options.",
            SettingsTab::About => "BananaTray version info, licenses, and acknowledgements.",
            _ => "",
        };
        div()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .px(px(40.0))
            .child(
                div()
                    .flex_col()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(theme.text_muted)
                            .text_align(TextAlign::Center)
                            .line_height(relative(1.5))
                            .child(desc.to_string()),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .px(px(12.0))
                            .py(px(4.0))
                            .rounded(px(6.0))
                            .bg(theme.bg_subtle)
                            .text_size(px(11.5))
                            .text_color(theme.text_secondary)
                            .child("Coming soon"),
                    ),
            )
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Self::preferences_theme();
        let settings = self.state.borrow().settings.clone();
        let active_tab = self.state.borrow().settings_ui.active_tab;

        let tabs: &[(&str, &str, SettingsTab)] = &[
            ("src/icons/settings.svg", "General", SettingsTab::General),
            (
                "src/icons/overview.svg",
                "Providers",
                SettingsTab::Providers,
            ),
            ("src/icons/display.svg", "Display", SettingsTab::Display),
            ("src/icons/advanced.svg", "Advanced", SettingsTab::Advanced),
            ("src/icons/about.svg", "About", SettingsTab::About),
        ];

        let mut tab_bar = div()
            .flex()
            .justify_center()
            .pt(px(4.0))
            .border_b_1()
            .border_color(theme.border_subtle);

        for &(icon, label, tab) in tabs {
            let state = self.state.clone();
            tab_bar = tab_bar.child(
                render_icon_tab(icon, label, active_tab == tab, &theme).on_mouse_down(
                    MouseButton::Left,
                    move |_, window, _| {
                        state.borrow_mut().settings_ui.active_tab = tab;
                        window.refresh();
                    },
                ),
            );
        }

        // ── Content area (depends on active tab) ─────────────
        // Providers tab handles its own scrolling (sidebar + detail panel
        // scroll independently), so the content wrapper must NOT be a scroll
        // container — otherwise the inner flex_1 children can grow unbounded
        // and never overflow.  Other tabs keep the outer scroll.
        let content = div().id("settings-content").flex_col().flex_1();
        let content = if active_tab == SettingsTab::Providers {
            content.overflow_hidden()
        } else {
            content.overflow_y_scroll()
        };
        let content = content.child(match active_tab {
            SettingsTab::General => self.render_general_tab(&settings, &theme),
            SettingsTab::Providers => {
                self.render_providers_tab(&settings, &theme, window.viewport_size())
            }
            _ => Self::render_placeholder_tab(active_tab, &theme),
        });

        div()
            .size_full()
            .bg(theme.bg_base)
            .text_color(theme.text_primary)
            .child(div().flex_col().size_full().child(tab_bar).child(content))
    }
}
