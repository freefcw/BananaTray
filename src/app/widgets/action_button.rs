/// 操作按钮组件
///
/// 圆角 + 边框 + 居中文字（可选图标）+ hover 反馈的按钮。
/// 主要用于设置窗口中的各种操作按钮。
use crate::theme::Theme;
use gpui::*;

/// 按钮风格变体
pub(crate) enum ButtonVariant {
    /// 危险操作（红色背景 + 红色边框），如 Quit 按钮
    Danger,
    /// 主题色边框 + 透明背景，如 Check for Updates 按钮
    Outlined,
    /// 微妙风格（bg_subtle + border_strong），如 Send 按钮
    Subtle,
}

/// 渲染操作按钮
///
/// # 参数
/// - `label` — 按钮文字
/// - `icon` — 可选 SVG 图标路径
/// - `variant` — 按钮风格变体
/// - `full_width` — 是否全宽
/// - `theme` — 主题
/// - `on_click` — 点击回调
///
/// # 使用场景
/// - `settings_window/general_tab.rs` — Quit 按钮 (Danger)
/// - `settings_window/about_tab.rs` — Check for Updates 按钮 (Outlined)
/// - `settings_window/debug_tab.rs` — Send 按钮 (Subtle)
pub(crate) fn render_action_button<F>(
    label: &str,
    icon: Option<(&'static str, Hsla)>,
    variant: ButtonVariant,
    full_width: bool,
    theme: &Theme,
    on_click: F,
) -> Div
where
    F: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    let (bg, border_color, text_color, hover_bg) = match variant {
        ButtonVariant::Danger => (
            theme.button.danger_bg,
            theme.status.error,
            theme.status.error,
            Some(hsla(0.0, 0.0, 0.0, 0.15)),
        ),
        ButtonVariant::Outlined => {
            let accent = theme.text.accent;
            let border = hsla(accent.h, accent.s, accent.l, 0.5);
            let text = hsla(accent.h, accent.s, accent.l, 0.7);
            (
                transparent_black(),
                border,
                text,
                Some(hsla(accent.h, accent.s, accent.l, 0.06)),
            )
        }
        ButtonVariant::Subtle => (
            theme.bg.subtle,
            theme.border.strong,
            theme.text.primary,
            None, // 使用 opacity 替代
        ),
    };

    let mut btn = div()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .py(px(if full_width { 12.0 } else { 6.0 }))
        .rounded(px(if full_width { 12.0 } else { 6.0 }))
        .bg(bg)
        .border_1()
        .border_color(border_color)
        .cursor_pointer();

    if full_width {
        btn = btn.w_full();
    } else {
        btn = btn.px(px(12.0));
    }

    // hover 效果
    if let Some(hbg) = hover_bg {
        btn = btn.hover(move |s| s.bg(hbg));
    } else {
        btn = btn.hover(|s| s.opacity(0.85));
    }

    // 可选图标
    if let Some((icon_path, icon_color)) = icon {
        btn = btn.child(super::render_svg_icon(icon_path, px(16.0), icon_color));
    }

    // 文字标签
    btn = btn.child(
        div()
            .text_size(px(if full_width { 14.0 } else { 12.0 }))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(text_color)
            .child(label.to_string()),
    );

    btn.on_mouse_down(MouseButton::Left, on_click)
}
