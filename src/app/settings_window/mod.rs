#[allow(dead_code)]
mod components;
mod display_tab;
mod general_tab;
mod provider_detail;
mod provider_sidebar;
mod window_mgr;

use super::AppState;
use crate::app::widgets::{render_svg_icon, render_toggle_switch};
use crate::app_state::SettingsTab;
use crate::theme::Theme;
use gpui::*;
use log::info;
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

pub use window_mgr::schedule_open_settings_window;
pub use window_mgr::schedule_open_settings_window_with_provider;

// ============================================================================
// 设置视图 — 匹配 Lumina Bar 设计稿
// ============================================================================

pub(super) struct SettingsView {
    pub(super) state: Rc<RefCell<AppState>>,
}

impl SettingsView {
    pub(super) fn new(state: Rc<RefCell<AppState>>, _cx: &mut Context<Self>) -> Self {
        info!(target: "settings", "constructing settings view");
        Self { state }
    }

    /// 根据用户主题设置解析设置窗口主题（与主面板保持一致）
    pub(super) fn resolve_theme(state: &std::cell::RefCell<AppState>) -> Theme {
        use crate::models::AppTheme;
        match state.borrow().settings.theme.resolve() {
            AppTheme::Light => Theme::light(),
            AppTheme::Dark => Theme::dark(),
            AppTheme::System => unreachable!("resolve() never returns System"),
        }
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
        let tabs: Vec<(&str, String, SettingsTab)> = vec![
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
                    theme.border_strong,
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
                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                        state.borrow_mut().settings_ui.active_tab = tab;
                        window.refresh();
                    }),
            );
        }

        bar
    }

    // ========================================================================
    // 底部 "Save & Return" 按钮
    // ========================================================================

    fn render_save_button(theme: &Theme) -> Div {
        // 渐变紫蓝色
        let gradient_start: Hsla = rgb(0x6366f1).into(); // indigo
        let gradient_end: Hsla = rgb(0x3b82f6).into(); // blue

        div().w_full().px(px(16.0)).pb(px(16.0)).pt(px(8.0)).child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .py(px(14.0))
                .rounded(px(12.0))
                .bg(multi_stop_linear_gradient(
                    90.,
                    &[
                        linear_color_stop(gradient_start, 0.),
                        linear_color_stop(gradient_end, 1.),
                    ],
                ))
                .cursor_pointer()
                .hover(|s| s.opacity(0.85))
                .child(
                    div()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.element_active)
                        .child(t!("settings.save_return").to_string()),
                )
                .on_mouse_down(MouseButton::Left, |_, window, _| {
                    window.remove_window();
                }),
        )
    }

    // ========================================================================
    // Providers tab (保留双栏布局)
    // ========================================================================

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

    // ========================================================================
    // About placeholder
    // ========================================================================

    fn render_placeholder_tab(tab: SettingsTab, theme: &Theme) -> Div {
        let title = match tab {
            SettingsTab::About => t!("settings.tab.about").to_string(),
            _ => String::new(),
        };
        let desc = match tab {
            SettingsTab::About => t!("settings.about.desc").to_string(),
            _ => String::new(),
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
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(theme.text_muted)
                            .text_align(TextAlign::Center)
                            .line_height(relative(1.5))
                            .child(desc),
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
                            .child(t!("settings.coming_soon").to_string()),
                    ),
            )
    }

    // ========================================================================
    // 带彩色圆形图标的设置行（设计稿核心组件）
    // ========================================================================

    /// 渲染带彩色圆形背景图标的开关行
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_icon_switch_row<F>(
        icon_path: &'static str,
        icon_color: Hsla,
        icon_bg: Hsla,
        title: &str,
        description: &str,
        enabled: bool,
        theme: &Theme,
        on_click: F,
    ) -> Div
    where
        F: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    {
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(14.0))
            .py(px(12.0))
            // 彩色圆形图标
            .child(
                div()
                    .w(px(36.0))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(icon_bg)
                    .flex_shrink_0()
                    .child(render_svg_icon(icon_path, px(18.0), icon_color)),
            )
            // 标题 + 描述
            .child(
                div()
                    .flex_col()
                    .flex_1()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(description.to_string()),
                    ),
            )
            // 开关
            .child(
                render_toggle_switch(enabled, px(44.0), px(24.0), px(18.0), theme)
                    .flex_shrink_0()
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, on_click),
            )
    }

    /// 渲染带彩色圆形背景图标的下拉行
    pub(super) fn render_icon_dropdown_row(
        icon_path: &'static str,
        icon_color: Hsla,
        icon_bg: Hsla,
        title: &str,
        description: &str,
        theme: &Theme,
        dropdown: Div,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(14.0))
            .py(px(12.0))
            // 彩色圆形图标
            .child(
                div()
                    .w(px(36.0))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(icon_bg)
                    .flex_shrink_0()
                    .child(render_svg_icon(icon_path, px(18.0), icon_color)),
            )
            // 标题 + 描述
            .child(
                div()
                    .flex_col()
                    .flex_1()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(description.to_string()),
                    ),
            )
            // 下拉控件
            .child(dropdown)
    }
}

// ============================================================================
// 设计稿风格的段落标题和卡片
// ============================================================================

/// 段落标题（如 "SYSTEM"、"AUTOMATION"）— 大写、小号、间距
pub(super) fn render_section_header(title: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(11.0))
        .font_weight(FontWeight::BOLD)
        .text_color(theme.text_muted)
        .px(px(4.0))
        .pt(px(16.0))
        .pb(px(8.0))
        .child(title.to_uppercase())
}

/// 深色卡片容器（与设计稿匹配的暗色圆角卡片）
pub(super) fn render_dark_card(theme: &Theme) -> Div {
    div()
        .flex_col()
        .w_full()
        .rounded(px(14.0))
        .bg(theme.bg_card)
        .border_1()
        .border_color(theme.border_subtle)
        .overflow_hidden()
}

/// 卡片内分隔线
pub(super) fn render_divider(theme: &Theme) -> Div {
    div().h(px(0.5)).w_full().bg(theme.border_subtle)
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Self::resolve_theme(&self.state);
        let settings = self.state.borrow().settings.clone();
        let active_tab = self.state.borrow().settings_ui.active_tab;
        let viewport = window.viewport_size();

        // ── Content area ─────────────
        // 头部 + tab 合计约 100px, 底部 Save 按钮约 60px
        let content_h = viewport.height - px(160.0);

        let content = if active_tab == SettingsTab::Providers {
            div()
                .id("settings-content-providers")
                .flex_col()
                .h(content_h)
                .overflow_hidden()
                .child(self.render_providers_tab(&settings, &theme, viewport))
        } else {
            div()
                .id("settings-content")
                .flex_col()
                .h(content_h)
                .overflow_y_scroll()
                .child(match active_tab {
                    SettingsTab::General => self.render_general_tab(&settings, &theme),
                    SettingsTab::Display => self.render_display_tab(&settings, &theme),
                    _ => Self::render_placeholder_tab(active_tab, &theme),
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
            // 分隔线
            .child(div().w_full().h(px(0.5)).bg(theme.border_subtle))
            // 内容区
            .child(content)
            // 底部 "Save & Return"
            .child(Self::render_save_button(&theme))
    }
}
