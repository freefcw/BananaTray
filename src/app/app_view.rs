use crate::application::{header_view_state, tray_global_actions_view_state, AppAction};
use crate::models::NavTab;
use crate::refresh::RefreshReason;
use crate::runtime;
use crate::theme::Theme;
use gpui::*;
use log::debug;
use std::cell::RefCell;
use std::rc::Rc;

use super::app_state::AppState;
use super::widgets;

use crate::models::PopupLayout;

// ============================================================================
// 窗口视图 (可多次创建/销毁)
// ============================================================================
pub struct AppView {
    pub(crate) state: Rc<RefCell<AppState>>,
    pub(crate) _activation_sub: Option<gpui::Subscription>,
    pub(crate) nav_scroll_handle: gpui::ScrollHandle,
}

impl AppView {
    pub fn new(state: Rc<RefCell<AppState>>, cx: &mut Context<Self>) -> Self {
        let is_dark = crate::utils::platform::detect_system_dark_mode();
        let theme = match state.borrow().session.settings.theme.resolve(is_dark) {
            crate::models::AppTheme::Light => Theme::light(),
            crate::models::AppTheme::Dark => Theme::dark(),
            crate::models::AppTheme::System => unreachable!("resolve() never returns System"),
        };
        cx.set_global(theme);

        state.borrow_mut().view_entity = Some(cx.entity().downgrade());

        Self {
            state,
            _activation_sub: None,
            nav_scroll_handle: gpui::ScrollHandle::new(),
        }
    }

    // ========================================================================
    // 头部区域：应用名 + 连接状态徽章
    // ========================================================================

    pub(crate) fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let header = {
            let state = self.state.borrow();
            header_view_state(&state.session)
        };

        let (dot_color, badge_bg) = match header.status_kind {
            crate::app_state::HeaderStatusKind::Synced => {
                (theme.badge_healthy, theme.badge_synced_bg)
            }
            crate::app_state::HeaderStatusKind::Syncing => {
                (theme.text_accent, rgba(0x3b82f61a).into())
            }
            crate::app_state::HeaderStatusKind::Stale => (theme.text_muted, theme.bg_subtle),
            crate::app_state::HeaderStatusKind::Offline => {
                (theme.badge_offline, theme.btn_danger_bg)
            }
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(16.0))
            .py(px(12.0))
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(
                // 左侧：应用图标 + 名称
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    // 应用图标
                    .child(
                        div()
                            .w(px(36.0))
                            .h(px(36.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(10.0))
                            .bg(theme.bg_subtle)
                            .border_1()
                            .border_color(theme.border_subtle)
                            .child(widgets::render_svg_icon(
                                "src/icons/tray_icon.svg",
                                px(20.0),
                                theme.text_accent,
                            )),
                    )
                    // 应用名称
                    .child(
                        div()
                            .flex_col()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.text_primary)
                                    .child("BananaTray"),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_muted)
                                    .child("AI USAGE MONITOR"),
                            ),
                    ),
            )
            .child(
                // 右侧：状态徽章
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(12.0))
                    .bg(badge_bg)
                    .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(dot_color))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_secondary)
                            .child(header.status_text),
                    ),
            )
    }

    // ========================================================================
    // 底部工具栏：Sync Data + Settings + Close
    // ========================================================================

    pub(crate) fn render_global_actions(
        &self,
        _active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let border_color = theme.border_subtle;
        let actions = {
            let state = self.state.borrow();
            tray_global_actions_view_state(&state.session)
        };

        // Sync Data 按钮（触发当前 provider 的刷新）
        let sync_btn = {
            let entity = cx.entity().clone();
            let refresh = actions.refresh.clone();
            let theme = cx.global::<Theme>();

            let mut btn = div()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .px(px(20.0))
                .py(px(10.0))
                .rounded(px(10.0))
                .bg(theme.btn_sync_bg)
                .border_1()
                .border_color(theme.btn_sync_bg)
                .cursor_pointer()
                .hover(|style| style.opacity(0.8))
                .child(widgets::render_svg_icon(
                    "src/icons/refresh.svg",
                    px(14.0),
                    theme.btn_sync_text,
                ))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.btn_sync_text)
                        .child(refresh.label.clone()),
                );

            if refresh.kind.is_some() && !refresh.is_refreshing {
                btn = btn.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    if let Some(k) = refresh.kind {
                        entity.update(cx, |view, cx| {
                            runtime::dispatch_in_context(
                                &view.state,
                                AppAction::RefreshProvider {
                                    kind: k,
                                    reason: RefreshReason::Manual,
                                },
                                cx,
                            );
                        });
                    }
                });
            }

            btn
        };

        // 设置按钮（圆形）
        let settings_btn = self.render_circle_button(
            "src/icons/settings.svg",
            cx.global::<Theme>().text_secondary,
            cx.global::<Theme>().bg_subtle,
            cx.global::<Theme>().border_subtle,
        );
        let settings_state = self.state.clone();
        let settings_btn = settings_btn.on_mouse_down(MouseButton::Left, move |_, window, cx| {
            runtime::dispatch_in_window(
                &settings_state,
                AppAction::OpenSettings { provider: None },
                window,
                cx,
            );
        });

        // 关闭按钮（圆形，红色调）
        let close_btn = self.render_circle_button(
            "src/icons/close.svg",
            cx.global::<Theme>().status_error,
            cx.global::<Theme>().btn_danger_bg,
            cx.global::<Theme>().btn_danger_bg,
        );
        let close_state = self.state.clone();
        let close_btn = close_btn.on_mouse_down(MouseButton::Left, move |_, window, cx| {
            runtime::dispatch_in_window(&close_state, AppAction::QuitApp, window, cx);
        });

        let mut footer = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(14.0))
            .py(px(10.0))
            .border_t_1()
            .border_color(border_color);

        if actions.show_refresh {
            footer = footer.child(sync_btn);
        }

        footer
            // 弹性空白，将设置和关闭按钮推到右侧
            .child(div().flex_1())
            .child(settings_btn)
            .child(close_btn)
    }

    /// 圆形工具栏按钮
    pub(crate) fn render_circle_button(
        &self,
        icon: &'static str,
        icon_color: Hsla,
        bg_color: Hsla,
        border_color: Hsla,
    ) -> Div {
        div()
            .w(px(38.0))
            .h(px(38.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(10.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .cursor_pointer()
            .child(widgets::render_svg_icon(icon, px(16.0), icon_color))
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.borrow();
        let active_tab = state.session.nav.active_tab;
        // 在每次渲染时动态调整窗口高度
        let desired_height = state.session.popup_height();
        drop(state);

        // 仅对 Windowed 类型窗口执行 resize（避免影响全屏/最大化窗口）
        let bounds = window.window_bounds();
        if let WindowBounds::Windowed(current_bounds) = bounds {
            let new_height = px(desired_height);
            let diff = current_bounds.size.height - new_height;
            if diff.abs() > px(2.0) {
                // 仅在高度变化时输出布局明细（避免每帧刷屏）
                let content_area = desired_height
                    - PopupLayout::HEADER_HEIGHT
                    - PopupLayout::NAV_HEIGHT
                    - PopupLayout::FOOTER_HEIGHT;
                debug!(
                    target: "layout",
                    "window resize: {:?} -> {:?} (diff={:?}), content_area={:.0}px",
                    current_bounds.size.height,
                    new_height,
                    diff,
                    content_area
                );
                window.resize(size(px(PopupLayout::WIDTH), new_height));
            }
        }

        let theme = cx.global::<Theme>();
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_panel)
            .text_color(theme.text_primary)
            .rounded(px(14.0))
            .overflow_hidden()
            // 头部
            .child(self.render_header(cx))
            // 导航
            .child(self.render_top_nav(active_tab, cx))
            // 内容区
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(match active_tab {
                        NavTab::Provider(kind) => div()
                            .px(px(12.0))
                            .pt(px(10.0))
                            .pb(px(8.0))
                            .child(self.render_provider_detail(kind, cx))
                            .into_any_element(),
                        NavTab::Settings => self.render_settings_content(cx),
                    }),
            )
            // 底部工具栏
            .child(self.render_global_actions(active_tab, cx))
    }
}

impl Drop for AppView {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.view_entity = None;
        }
    }
}
