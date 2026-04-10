/// 底部全局操作栏：Sync Data + Settings + Close
use crate::application::{tray_global_actions_view_state, AppAction};
use crate::refresh::RefreshReason;
use crate::runtime;
use crate::theme::Theme;
use gpui::*;

use super::render_svg_icon;
use crate::ui::AppView;

impl AppView {
    pub(crate) fn render_global_actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let border_color = theme.border.subtle;
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
                .bg(theme.button.sync_bg)
                .border_1()
                .border_color(theme.button.sync_bg)
                .cursor_pointer()
                .hover(|style| style.opacity(0.8))
                .child(render_svg_icon(
                    "src/icons/refresh.svg",
                    px(14.0),
                    theme.button.sync_text,
                ))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.button.sync_text)
                        .child(refresh.label.clone()),
                );

            if refresh.id.is_some() && !refresh.is_refreshing {
                btn = btn.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    if let Some(ref id) = refresh.id {
                        let id = id.clone();
                        entity.update(cx, |view, cx| {
                            runtime::dispatch_in_context(
                                &view.state,
                                AppAction::RefreshProvider {
                                    id,
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
