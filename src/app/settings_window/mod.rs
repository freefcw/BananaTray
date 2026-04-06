mod about_tab;
mod components;
mod debug_tab;
mod display_tab;
mod general_tab;
mod providers;
mod window_mgr;

use super::AppState;
use crate::app::widgets::render_svg_icon;
use crate::app_state::SettingsTab;
use crate::application::AppAction;
use crate::runtime;
use crate::theme::Theme;
use gpui::*;
use log::info;
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

pub use window_mgr::schedule_open_settings_window;

// ============================================================================
// 设置视图 — 匹配 Lumina Bar 设计稿
// ============================================================================

pub(crate) struct SettingsView {
    pub(crate) state: Rc<RefCell<AppState>>,
    pub(crate) copilot_input: Option<Entity<adabraka_ui::components::input_state::InputState>>,
    /// 监听系统深色模式变化，自动切换主题
    pub(crate) _appearance_sub: Option<gpui::Subscription>,
}

impl SettingsView {
    pub(crate) fn new(state: Rc<RefCell<AppState>>, _cx: &mut Context<Self>) -> Self {
        info!(target: "settings", "constructing settings view");
        Self {
            state,
            copilot_input: None,
            _appearance_sub: None,
        }
    }

    /// 根据用户主题设置 + 窗口外观解析设置窗口主题
    pub(super) fn resolve_theme(
        state: &std::cell::RefCell<AppState>,
        appearance: WindowAppearance,
    ) -> Theme {
        let user_theme = state.borrow().session.settings.display.theme;
        Theme::resolve_for_settings(user_theme, appearance)
    }

    // ========================================================================
    // 自定义头部：图标 + "Settings" + ✕ 关闭按钮
    // ========================================================================

    fn render_header(&self, theme: &Theme) -> Div {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(20.0))
            .pt(px(20.0))
            .pb(px(12.0))
            // 左侧：图标 + 标题
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    // 网格图标
                    .child(
                        div()
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.0))
                            .bg(theme.bg_subtle)
                            .child(render_svg_icon(
                                "src/icons/overview.svg",
                                px(18.0),
                                theme.text_accent,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text_primary)
                            .child(t!("settings.title").to_string()),
                    ),
            )
            // 右侧：关闭按钮
            .child(
                div()
                    .w(px(28.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_subtle))
                    .child(render_svg_icon(
                        "src/icons/close.svg",
                        px(14.0),
                        theme.text_muted,
                    ))
                    .on_mouse_down(MouseButton::Left, |_, window, _| {
                        window.remove_window();
                    }),
            )
    }

    // ========================================================================
    // Tab 导航栏：水平 pill 风格
    // ========================================================================

    fn render_tab_bar(&self, active_tab: SettingsTab, theme: &Theme) -> Div {
        let show_debug = self.state.borrow().session.settings.display.show_debug_tab;

        let mut tabs: Vec<(&str, String, SettingsTab)> = vec![
            (
                "src/icons/settings.svg",
                t!("settings.tab.general").to_string(),
                SettingsTab::General,
            ),
            (
                "src/icons/overview.svg",
                t!("settings.tab.providers").to_string(),
                SettingsTab::Providers,
            ),
            (
                "src/icons/display.svg",
                t!("settings.tab.display").to_string(),
                SettingsTab::Display,
            ),
            (
                "src/icons/about.svg",
                t!("settings.tab.about").to_string(),
                SettingsTab::About,
            ),
        ];

        if show_debug {
            tabs.push((
                "src/icons/advanced.svg",
                t!("settings.tab.debug").to_string(),
                SettingsTab::Debug,
            ));
        }

        let mut bar = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(2.0))
            .px(px(16.0))
            .pb(px(10.0))
            .overflow_hidden();

        for (icon, label, tab) in tabs {
            let is_active = active_tab == tab;
            let state = self.state.clone();
            let (bg, text_color, icon_color, border_color) = if is_active {
                (
                    theme.nav_pill_active_bg,
                    theme.nav_pill_active_text,
                    theme.nav_pill_active_text,
                    theme.nav_pill_active_bg,
                )
            } else {
                (
                    transparent_black(),
                    theme.text_muted,
                    theme.text_muted,
                    transparent_black(),
                )
            };

            bar = bar.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(8.0))
                    .bg(bg)
                    .border_1()
                    .border_color(border_color)
                    .cursor_pointer()
                    .hover(|style| {
                        if is_active {
                            style
                        } else {
                            style.bg(theme.bg_subtle)
                        }
                    })
                    .child(
                        svg()
                            .path(icon)
                            .size(px(14.0))
                            .text_color(icon_color)
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(if is_active {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .text_color(text_color)
                            .whitespace_nowrap()
                            .child(label),
                    )
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        runtime::dispatch_in_window(
                            &state,
                            AppAction::SetSettingsTab(tab),
                            window,
                            cx,
                        );
                    }),
            );
        }

        bar
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Self::resolve_theme(&self.state, window.appearance());
        let active_tab = self.state.borrow().session.settings_ui.active_tab;
        let settings = self.state.borrow().session.settings.clone();
        let viewport = window.viewport_size();

        // ── Content area ─────────────
        // 头部 + tab 合计约 100px
        let content_h = viewport.height - px(100.0);

        let content = if active_tab == SettingsTab::Providers {
            div()
                .id("settings-content-providers")
                .flex_col()
                .h(content_h)
                .overflow_hidden()
                .child(self.render_providers_tab(&theme, viewport, cx))
        } else {
            div()
                .id("settings-content")
                .flex_col()
                .h(content_h)
                .overflow_y_scroll()
                .child(match active_tab {
                    SettingsTab::General => self.render_general_tab(&settings, &theme),
                    SettingsTab::Display => self.render_display_tab(&settings, &theme),
                    SettingsTab::About => self.render_about_tab(&theme),
                    SettingsTab::Debug => self.render_debug_tab(&theme),
                    _ => div(),
                })
        };

        // ── 整体布局 ──
        // 暗色背景 + 圆角 + 模拟发光边缘
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_base)
            .text_color(theme.text_primary)
            .rounded(px(14.0))
            .overflow_hidden()
            // 顶部微光效果 (amber glow)
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .h(px(2.0))
                    .bg(multi_stop_linear_gradient(
                        90.,
                        &[
                            linear_color_stop(transparent_black(), 0.),
                            linear_color_stop(rgba(0xff8c0040), 0.3),
                            linear_color_stop(rgba(0xff6b0060), 0.5),
                            linear_color_stop(rgba(0xff8c0040), 0.7),
                            linear_color_stop(transparent_black(), 1.),
                        ],
                    )),
            )
            // 头部
            .child(self.render_header(&theme))
            // Tab 栏
            .child(self.render_tab_bar(active_tab, &theme))
            // Tab 栏与内容区分隔线
            .child(div().w_full().h(px(1.0)).bg(theme.border_subtle))
            // 内容区
            .child(content)
    }
}
