use gpui::*;

use crate::models::{AppSettings, AppTheme, ProviderKind, ProviderStatus};
use crate::theme::Theme;
use crate::views::dashboard::Dashboard;
use crate::views::settings::SettingsPanel;

// ============================================================================
// 应用状态模型 (纯业务状态逻辑)
// ============================================================================

/// 当前显示的面板
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    Dashboard,
    Settings,
}

/// 主应用模型，负责保存状态和修改逻辑
pub struct AppModel {
    pub providers: Vec<ProviderStatus>,
    pub settings: AppSettings,
    pub active_panel: ActivePanel,
}

impl AppModel {
    pub fn new() -> Self {
        // 使用 mock 数据初始化所有 providers
        let providers = ProviderKind::all()
            .iter()
            .map(|kind| ProviderStatus::mock(*kind))
            .collect();

        Self {
            providers,
            settings: AppSettings {
                theme: AppTheme::Dark,
                ..Default::default()
            },
            active_panel: ActivePanel::Dashboard,
        }
    }

    pub fn show_dashboard(&mut self) {
        self.active_panel = ActivePanel::Dashboard;
    }

    pub fn show_settings(&mut self) {
        self.active_panel = ActivePanel::Settings;
    }

    pub fn toggle_theme<V>(&mut self, cx: &mut Context<V>) {
        self.settings.theme = match self.settings.theme {
            AppTheme::Light => {
                cx.set_global(Theme::dark());
                AppTheme::Dark
            }
            AppTheme::Dark => {
                cx.set_global(Theme::light());
                AppTheme::Light
            }
        };
    }
}

// ============================================================================
// 根视图控制 (UI 视图层)
// ============================================================================

pub struct AppView {
    pub model: AppModel,
}

impl AppView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.set_global(Theme::dark());
        Self {
            model: AppModel::new(),
        }
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let active_panel = self.model.active_panel;
        let current_app_theme = self.model.settings.theme;

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_base)
            .text_color(theme.text_primary)
            .child(
                // 顶部导航栏
                self.render_nav_bar(active_panel, current_app_theme, cx),
            )
            .child(
                // 内容区域
                div()
                    .id("content")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(match active_panel {
                        ActivePanel::Dashboard => {
                            div().child(Dashboard::new(self.model.providers.clone())).into_any_element()
                        }
                        ActivePanel::Settings => {
                            div().child(SettingsPanel::new(self.model.settings.clone())).into_any_element()
                        }
                    }),
            )
    }
}

impl AppView {
    /// 渲染顶部导航栏
    fn render_nav_bar(
        &self,
        active_panel: ActivePanel,
        current_app_theme: AppTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let border_color = theme.border_subtle;
        let active_text = theme.element_active;
        let inactive_text = theme.element_inactive;

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
                            .child("🍌 BananaTray"),
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
                        active_text,
                        inactive_text,
                        ActivePanel::Dashboard,
                        cx.entity().clone()
                    ))
                    .child(self.render_nav_button(
                        "Settings",
                        active_panel == ActivePanel::Settings,
                        active_text,
                        inactive_text,
                        ActivePanel::Settings,
                        cx.entity().clone()
                    ))
                    .child(
                        // 分隔线
                        div().w(px(1.0)).h(px(16.0)).bg(inactive_text).mx(px(4.0)).opacity(0.3)
                    )
                    .child(
                        // 主题切换按钮
                        self.render_theme_toggle(
                            current_app_theme,
                            inactive_text,
                            active_text,
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
        target_panel: ActivePanel,
        entity: Entity<AppView>,
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
            entity.update(cx, |view, cx| {
                match target_panel {
                    ActivePanel::Dashboard => view.model.show_dashboard(),
                    ActivePanel::Settings => view.model.show_settings(),
                }
                cx.notify();
            });
        })
    }

    /// 渲染主题切换按钮
    fn render_theme_toggle(
        &self,
        current_theme: AppTheme,
        inactive_text: Hsla,
        active_text: Hsla,
        entity: Entity<AppView>,
    ) -> impl IntoElement {
        let (icon, label) = match current_theme {
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
                entity.update(cx, |view, cx| {
                    view.model.toggle_theme(cx);
                    cx.notify();
                });
            })
    }
}
