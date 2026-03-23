use gpui::*;

use crate::models::{AppSettings, AppTheme, ProviderKind, ProviderStatus};
use crate::views::dashboard::Dashboard;
use crate::views::settings::SettingsPanel;

// ============================================================================
// 应用视图状态
// ============================================================================

/// 当前显示的面板
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    Dashboard,
    Settings,
}

// ============================================================================
// 主应用状态
// ============================================================================

/// 主应用实体，管理全局状态
pub struct AppState {
    pub providers: Vec<ProviderStatus>,
    pub settings: AppSettings,
    pub active_panel: ActivePanel,
    pub window_visible: bool,
}

impl AppState {
    pub fn new() -> Self {
        // 使用 mock 数据初始化所有 providers
        let providers = ProviderKind::all()
            .iter()
            .map(|kind| ProviderStatus::mock(*kind))
            .collect();

        Self {
            providers,
            settings: AppSettings::default(),
            active_panel: ActivePanel::Dashboard,
            window_visible: true,
        }
    }

    /// 切换到仪表盘面板
    pub fn show_dashboard(&mut self) {
        self.active_panel = ActivePanel::Dashboard;
    }

    /// 切换到设置面板
    pub fn show_settings(&mut self) {
        self.active_panel = ActivePanel::Settings;
    }

    /// 切换主题
    pub fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        self.settings.theme = match self.settings.theme {
            AppTheme::Light => AppTheme::Dark,
            AppTheme::Dark => AppTheme::Light,
        };
        cx.notify();
    }
}

// ============================================================================
// Render 实现
// ============================================================================

impl Render for AppState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.settings.theme;

        // 根据主题设置背景色和文字色 (Minimalist Premium style)
        let (bg_color, text_color, border_color) = match theme {
            AppTheme::Dark => (
                rgb(0x0a0a0a), // neutral-950
                rgb(0xfafafa), // neutral-50
                rgb(0x262626), // neutral-800
            ),
            AppTheme::Light => (
                rgb(0xffffff), // pure white
                rgb(0x0a0a0a), // neutral-950
                rgb(0xe5e5e5), // neutral-200
            ),
        };

        let active_panel = self.active_panel;
        let providers = self.providers.clone();
        let settings = self.settings.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(bg_color)
            .text_color(text_color)
            .child(
                // 顶部导航栏
                self.render_nav_bar(active_panel, border_color.into(), theme, cx),
            )
            .child(
                // 内容区域
                div()
                    .id("content")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(match active_panel {
                        ActivePanel::Dashboard => {
                            div().child(Dashboard::new(providers, theme))
                                .into_any_element()
                        }
                        ActivePanel::Settings => {
                            div().child(SettingsPanel::new(settings, theme))
                                .into_any_element()
                        }
                    }),
            )
    }
}

impl AppState {
    /// 渲染顶部导航栏
    fn render_nav_bar(
        &self,
        active_panel: ActivePanel,
        border_color: Hsla,
        theme: AppTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Premium minimalist nav styles (Text color emphasis instead of background pills)
        let (active_text, inactive_text) = match theme {
            AppTheme::Dark => (
                rgb(0xffffff), // pure white active
                rgb(0xa3a3a3), // neutral-400 inactive
            ),
            AppTheme::Light => (
                rgb(0x000000), // pure black active
                rgb(0x737373), // neutral-500 inactive
            ),
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .pl(px(88.0)) // Explicit left padding to dodge macOS window controls
            .pr(px(16.0))
            .py(px(12.0))
            .border_b_1()
            .border_color(border_color)
            .child(
                // 应用标题
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::BOLD)
                            .child("⚡ StarTray"),
                    ),
            )
            .child(
                // 导航按钮
                div()
                    .flex()
                    .gap(px(4.0))
                    .child(self.render_nav_button(
                        "Dashboard",
                        active_panel == ActivePanel::Dashboard,
                        active_text.into(),
                        inactive_text.into(),
                        cx.entity().clone(),
                        ActivePanel::Dashboard,
                    ))
                    .child(self.render_nav_button(
                        "Settings",
                        active_panel == ActivePanel::Settings,
                        active_text.into(),
                        inactive_text.into(),
                        cx.entity().clone(),
                        ActivePanel::Settings,
                    ))
                    .child(
                        // 分隔线
                        div().w(px(1.0)).h(px(16.0)).bg(inactive_text).mx(px(4.0)).opacity(0.3)
                    )
                    .child(
                        // 主题切换按钮
                        self.render_theme_toggle(
                            theme,
                            inactive_text.into(),
                            active_text.into(),
                            cx.entity().clone()
                        )
                    )
            )
    }

    /// 渲染单个导航按钮
    fn render_nav_button(
        &self,
        label: &'static str,
        is_active: bool,
        active_text: Hsla,
        inactive_text: Hsla,
        entity: Entity<AppState>,
        target_panel: ActivePanel,
    ) -> impl IntoElement {
        let mut btn = div()
            .px(px(8.0))
            .py(px(4.0))
            .cursor_pointer()
            .text_size(px(13.0))
            .font_weight(if is_active { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
            .child(label);

        if is_active {
            btn = btn.text_color(active_text);
        } else {
            btn = btn
                .text_color(inactive_text)
                .hover(|s| s.text_color(active_text));
        }

        btn.on_mouse_down(MouseButton::Left, move |_ev, _window, cx| {
            entity.update(cx, |state, cx| {
                match target_panel {
                    ActivePanel::Dashboard => state.show_dashboard(),
                    ActivePanel::Settings => state.show_settings(),
                }
                cx.notify();
            });
        })
    }

    /// 渲染主题切换按钮
    fn render_theme_toggle(
        &self,
        theme: AppTheme,
        inactive_text: Hsla,
        active_text: Hsla,
        entity: Entity<AppState>,
    ) -> impl IntoElement {
        let (icon, label) = match theme {
            AppTheme::Dark => ("☀️", "Light"),
            AppTheme::Light => ("🌙", "Dark"),
        };

        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(8.0))
            .py(px(4.0))
            .cursor_pointer()
            .text_size(px(13.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(inactive_text)
            .hover(|s| s.text_color(active_text))
            .child(icon)
            .child(label)
            .on_mouse_down(MouseButton::Left, move |_ev, _window, cx| {
                entity.update(cx, |state, cx| {
                    state.toggle_theme(cx);
                });
            })
    }
}
