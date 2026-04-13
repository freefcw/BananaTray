mod about_tab;
mod components;
mod debug_tab;
mod display_tab;
mod general_tab;
mod providers;
mod window_mgr;

use super::AppState;
use crate::application::AppAction;
use crate::application::SettingsTab;
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::{render_svg_icon, SimpleInputState};
use gpui::{
    div, linear_color_stop, multi_stop_linear_gradient, px, rgba, svg, transparent_black, Context,
    Div, Entity, FocusHandle, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window,
    WindowAppearance,
};
use log::info;
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

pub use window_mgr::schedule_open_settings_window;

// ============================================================================
// 设置视图 — 匹配 Lumina Bar 设计稿
// ============================================================================

/// NewAPI 表单输入状态（不使用 adabraka-ui InputState，避免 IME 崩溃）
pub(crate) struct NewApiFormInputs {
    pub name: SimpleInputState,
    pub url: SimpleInputState,
    pub cookie: SimpleInputState,
    pub user_id: SimpleInputState,
    pub divisor: SimpleInputState,
    pub focus_handles: [FocusHandle; 5],
}

impl NewApiFormInputs {
    /// 新增模式：创建空表单
    pub fn new_add(cx: &mut Context<SettingsView>) -> Self {
        Self {
            name: SimpleInputState::new(t!("newapi.field.name.placeholder").to_string()),
            url: SimpleInputState::new(t!("newapi.field.url.placeholder").to_string()),
            cookie: SimpleInputState::new(t!("newapi.field.cookie.placeholder").to_string()),
            user_id: SimpleInputState::new(t!("newapi.field.user_id.placeholder").to_string()),
            divisor: SimpleInputState::new(t!("newapi.field.divisor.placeholder").to_string()),
            focus_handles: std::array::from_fn(|_| cx.focus_handle()),
        }
    }

    /// 编辑模式：用已有数据预填表单
    pub fn new_edit(
        data: &crate::providers::custom::generator::NewApiEditData,
        cx: &mut Context<SettingsView>,
    ) -> Self {
        Self {
            name: SimpleInputState::new_with_value(
                t!("newapi.field.name.placeholder").to_string(),
                &data.display_name,
            ),
            url: SimpleInputState::new_with_value(
                t!("newapi.field.url.placeholder").to_string(),
                &data.base_url,
            ),
            cookie: SimpleInputState::new_with_value(
                t!("newapi.field.cookie.placeholder").to_string(),
                &data.cookie,
            ),
            user_id: SimpleInputState::new_with_value(
                t!("newapi.field.user_id.placeholder").to_string(),
                data.user_id.as_deref().unwrap_or(""),
            ),
            divisor: SimpleInputState::new_with_value(
                t!("newapi.field.divisor.placeholder").to_string(),
                data.divisor
                    .map(|d| (d as u64).to_string())
                    .unwrap_or_default(),
            ),
            focus_handles: std::array::from_fn(|_| cx.focus_handle()),
        }
    }

    /// 按索引返回对应字段的可变引用（0=name, 1=url, 2=cookie, 3=user_id, 4=divisor）
    pub fn field_mut(&mut self, idx: usize) -> Option<&mut SimpleInputState> {
        match idx {
            0 => Some(&mut self.name),
            1 => Some(&mut self.url),
            2 => Some(&mut self.cookie),
            3 => Some(&mut self.user_id),
            4 => Some(&mut self.divisor),
            _ => None,
        }
    }

    /// 返回每个字段是否获得焦点的数组
    pub fn focused_states(&self, window: &Window) -> [bool; 5] {
        std::array::from_fn(|i| self.focus_handles[i].is_focused(window))
    }
}

pub(crate) struct SettingsView {
    pub(crate) state: Rc<RefCell<AppState>>,
    pub(crate) copilot_input: Option<Entity<adabraka_ui::components::input_state::InputState>>,
    /// 监听系统深色模式变化，自动切换主题
    pub(crate) _appearance_sub: Option<Subscription>,
    /// NewAPI 快速添加表单输入组（进入表单模式时创建，退出时置 None）
    pub(crate) newapi_inputs: Option<NewApiFormInputs>,
}

impl SettingsView {
    pub(crate) fn new(state: Rc<RefCell<AppState>>, _cx: &mut Context<Self>) -> Self {
        info!(target: "settings", "constructing settings view");
        Self {
            state,
            copilot_input: None,
            _appearance_sub: None,
            newapi_inputs: None,
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
                            .bg(theme.bg.subtle)
                            .child(render_svg_icon(
                                "src/icons/overview.svg",
                                px(18.0),
                                theme.text.accent,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text.primary)
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
                    .hover(|s| s.bg(theme.bg.subtle))
                    .child(render_svg_icon(
                        "src/icons/close.svg",
                        px(14.0),
                        theme.text.muted,
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
                    theme.nav.pill_active_bg,
                    theme.nav.pill_active_text,
                    theme.nav.pill_active_text,
                    theme.nav.pill_active_bg,
                )
            } else {
                (
                    transparent_black(),
                    theme.text.muted,
                    theme.text.muted,
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
                            style.bg(theme.bg.subtle)
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
                .child(self.render_providers_tab(&theme, window, cx))
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
            .bg(theme.bg.base)
            .text_color(theme.text.primary)
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
            .child(div().w_full().h(px(1.0)).bg(theme.border.subtle))
            // 内容区
            .child(content)
    }
}
