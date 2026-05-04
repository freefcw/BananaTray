use crate::application::{header_view_state, HeaderStatusKind};
use crate::models::{NavTab, ProviderId};
use crate::theme::Theme;
#[cfg(target_os = "linux")]
use gpui::MouseButton;
use gpui::{
    div, img, px, size, Context, Div, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window, WindowBounds,
};
use log::debug;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::runtime::AppState;

use crate::models::PopupLayout;

thread_local! {
    static APP_VIEW_ENTITY: RefCell<Option<gpui::WeakEntity<AppView>>> = const { RefCell::new(None) };
}

#[cfg(target_os = "linux")]
const LINUX_POPUP_DRAG_SUPPRESSION: std::time::Duration = std::time::Duration::from_millis(2_000);

pub(crate) fn register_current_view(entity: gpui::WeakEntity<AppView>) {
    APP_VIEW_ENTITY.with(|slot| *slot.borrow_mut() = Some(entity));
}

pub(crate) fn clear_current_view() {
    APP_VIEW_ENTITY.with(|slot| *slot.borrow_mut() = None);
}

#[allow(dead_code)]
pub(crate) fn notify_current_view(cx: &mut gpui::App) {
    let entity = APP_VIEW_ENTITY.with(|slot| slot.borrow().clone());
    if let Some(entity) = entity {
        let _ = entity.update(cx, |_, cx| {
            cx.notify();
        });
    }
}

// ============================================================================
// 窗口视图 (可多次创建/销毁)
// ============================================================================
pub struct AppView {
    pub(crate) state: Rc<RefCell<AppState>>,
    pub(crate) _activation_sub: Option<gpui::Subscription>,
    /// 监听系统深色模式变化，自动切换主题
    pub(crate) _appearance_sub: Option<gpui::Subscription>,
    #[cfg(target_os = "linux")]
    pub(crate) _bounds_sub: Option<gpui::Subscription>,
    pub(crate) nav_scroll_handle: gpui::ScrollHandle,
    /// Overview 面板中展开了配额详情的 Provider 集合（UI-only 状态）
    pub(crate) overview_expanded: HashSet<ProviderId>,
}

impl AppView {
    pub fn new(state: Rc<RefCell<AppState>>, cx: &mut Context<Self>) -> Self {
        // 初次创建时通过子进程检测深色模式并设置主题
        // 后续变化通过 observe_window_appearance 自动跟随
        let is_dark = crate::platform::system::detect_system_dark_mode();
        let user_theme = state.borrow().session.settings.display.theme;
        let theme = match user_theme.resolve(is_dark) {
            crate::models::AppTheme::Light => Theme::light(),
            crate::models::AppTheme::Dark => Theme::dark(),
            crate::models::AppTheme::System => unreachable!("resolve() never returns System"),
        };
        cx.set_global(theme);

        register_current_view(cx.entity().downgrade());

        Self {
            state,
            _activation_sub: None,
            _appearance_sub: None,
            #[cfg(target_os = "linux")]
            _bounds_sub: None,
            nav_scroll_handle: gpui::ScrollHandle::new(),
            overview_expanded: HashSet::new(),
        }
    }

    // ========================================================================
    // 头部区域：应用名 + 连接状态徽章
    // ========================================================================

    pub(crate) fn render_header(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.global::<Theme>();
        let header = {
            let state = self.state.borrow();
            header_view_state(&state.session)
        };

        let (dot_color, badge_bg) = match header.status_kind {
            HeaderStatusKind::Synced => (theme.badge.healthy, theme.badge.synced_bg),
            HeaderStatusKind::Syncing => (theme.text.accent, theme.badge.syncing_bg),
            HeaderStatusKind::Stale => (theme.text.muted, theme.bg.subtle),
            HeaderStatusKind::Offline => (theme.badge.offline, theme.button.danger_bg),
        };

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(16.0))
            .py(px(12.0))
            .border_b_1()
            .border_color(theme.border.subtle)
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
                            .bg(theme.bg.subtle)
                            .border_1()
                            .border_color(theme.border.subtle)
                            .child(img("src/icons/app_logo.png").w(px(24.0)).h(px(24.0))),
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
                                    .text_color(theme.text.primary)
                                    .child("BananaTray"),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text.muted)
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
                            .text_color(theme.text.secondary)
                            .child(header.status_text),
                    ),
            );

        #[cfg(target_os = "linux")]
        let header = {
            let state = self.state.clone();
            header.cursor(gpui::CursorStyle::OpenHand).on_mouse_down(
                MouseButton::Left,
                move |_, window, _| {
                    state
                        .borrow_mut()
                        .begin_linux_popup_drag(LINUX_POPUP_DRAG_SUPPRESSION);
                    window.start_window_move();
                },
            )
        };

        header
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.borrow();
        let active_tab = state.session.nav.active_tab.clone();
        #[cfg(target_os = "linux")]
        let popup_visible = state.session.popup_visible;
        // 在每次渲染时动态调整窗口高度
        let desired_height = state.session.popup_height();
        drop(state);

        #[cfg(target_os = "linux")]
        if !popup_visible {
            return div().size_full();
        }

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
            .bg(theme.bg.panel)
            .text_color(theme.text.primary)
            .rounded(px(14.0))
            .overflow_hidden()
            // 头部
            .child(self.render_header(cx))
            // 导航
            .child(self.render_top_nav(active_tab.clone(), cx))
            // 内容区
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(match &active_tab {
                        NavTab::Overview => div()
                            .px(px(12.0))
                            .pt(px(10.0))
                            .pb(px(8.0))
                            .child(self.render_overview_panel(cx))
                            .into_any_element(),
                        NavTab::Provider(id) => div()
                            .px(px(12.0))
                            .pt(px(10.0))
                            .pb(px(8.0))
                            .child(self.render_provider_detail(id, cx))
                            .into_any_element(),
                        NavTab::Settings => self.render_settings_content(cx),
                    }),
            )
            // 底部工具栏
            .child(self.render_global_actions(cx))
    }
}

impl Drop for AppView {
    fn drop(&mut self) {
        clear_current_view();
    }
}
