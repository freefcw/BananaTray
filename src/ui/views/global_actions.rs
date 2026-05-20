/// 底部全局操作栏：Refresh + Settings + Close
use crate::application::{tray_global_actions_view_state, AppAction, RefreshTarget};
use crate::refresh::RefreshReason;
use crate::runtime;
use crate::theme::Theme;
use gpui::{
    div, px, Context, Div, ElementId, Hsla, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled,
};

use crate::ui::widgets::{render_svg_icon, with_tooltip};
use crate::ui::AppView;

impl AppView {
    pub(crate) fn render_global_actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let border_color = theme.border.subtle;
        let actions = {
            let state = self.state.borrow();
            tray_global_actions_view_state(&state.session)
        };

        // 刷新按钮（图标 + tooltip；Overview 全部刷新 / Provider 单个刷新）
        let sync_btn = {
            let entity = cx.entity().clone();
            let refresh = actions.refresh.clone();
            let theme = cx.global::<Theme>();
            let tooltip = refresh.label.clone();

            let mut btn = render_circle_button(
                "src/icons/refresh.svg",
                theme.button.sync_text,
                theme.button.sync_bg,
                theme.button.sync_bg,
            );

            if let (Some(target), false) = (refresh.target, refresh.is_refreshing) {
                btn = btn.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    entity.update(cx, |view, cx| {
                        let action = match target.clone() {
                            RefreshTarget::All => AppAction::RefreshAll,
                            RefreshTarget::One(id) => AppAction::RefreshProvider {
                                id,
                                reason: RefreshReason::Manual,
                            },
                        };
                        runtime::dispatch_in_context(&view.state, action, cx);
                    });
                });
            } else if refresh.is_refreshing {
                btn = btn.opacity(0.6);
            }

            with_tooltip(
                ElementId::Name("tray-global-refresh".into()),
                &tooltip,
                theme,
                btn,
            )
        };

        // 设置按钮（圆形）
        let settings_btn = render_circle_button(
            "src/icons/settings.svg",
            cx.global::<Theme>().text.secondary,
            cx.global::<Theme>().bg.subtle,
            cx.global::<Theme>().border.subtle,
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
        let close_btn = render_circle_button(
            "src/icons/close.svg",
            cx.global::<Theme>().status.error,
            cx.global::<Theme>().button.danger_bg,
            cx.global::<Theme>().button.danger_bg,
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
}

/// 圆形工具栏按钮（纯函数，不依赖 AppView）
pub(crate) fn render_circle_button(
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
        .child(render_svg_icon(icon, px(16.0), icon_color))
}
