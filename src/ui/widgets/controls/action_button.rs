/// 操作按钮组件
///
/// 圆角 + 边框 + 居中文字（可选图标）+ hover 反馈的按钮。
/// 主要用于设置窗口中的各种操作按钮。
use crate::theme::Theme;
use gpui::{
    div, hsla, px, transparent_black, App, Div, FontWeight, Hsla, InteractiveElement, MouseButton,
    MouseDownEvent, ParentElement, Styled, Window,
};

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

    // 紧凑按钮不要同时设 h + py：GPUI 高度含 padding 时内容区会被挤扁，
    // 文字行盒溢出后从底部裁切，看起来就像字往上浮。
    let mut btn = div()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(if full_width { 8.0 } else { 6.0 }))
        .rounded(px(if full_width { 12.0 } else { 6.0 }))
        .bg(bg)
        .border_1()
        .border_color(border_color)
        .cursor_pointer();

    if full_width {
        btn = btn.w_full().py(px(12.0));
    } else {
        btn = btn.h(px(32.0)).px(px(12.0));
    }

    // hover 效果
    if let Some(hbg) = hover_bg {
        btn = btn.hover(move |s| s.bg(hbg));
    } else {
        btn = btn.hover(|s| s.opacity(0.85));
    }

    // 图标和文字用同一行高。GPUI 基线是 (line_height - ascent - descent) / 2，
    // 行高小于 ascent+descent（约 1.3em）时 padding 为负，字形会往上冒。
    let icon_size = 16.0;
    let text_size = if full_width { 14.0 } else { 12.0 };
    let text_line = if full_width { 18.0 } else { 16.0 };

    if let Some((icon_path, icon_color)) = icon {
        btn = btn.child(
            div()
                .w(px(icon_size))
                .h(px(icon_size))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .child(crate::ui::widgets::render_svg_icon(
                    icon_path,
                    px(icon_size),
                    icon_color,
                )),
        );
    }

    // 截图实测：行盒已经和图标同高，但大写字母墨水中心仍比图标高约 1.5px
    // （descent 留白在基线下方，Inter/Helvetica 都这样）。往下挪 2px 做光学对齐。
    btn = btn.child(
        div()
            .h(px(text_line))
            .flex()
            .items_center()
            .mt(px(2.0))
            .text_size(px(text_size))
            .line_height(px(text_line))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(text_color)
            .child(label.to_string()),
    );

    btn.on_mouse_down(MouseButton::Left, on_click)
}
