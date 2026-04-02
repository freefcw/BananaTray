use super::SettingsView;
use crate::theme::Theme;
use crate::utils::platform::open_url;
use gpui::*;
use rust_i18n::t;

// ============================================================================
// About 页 — 匹配 Lumina Bar 设计稿
// ============================================================================

const APP_NAME: &str = "BananaTray";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_REPO: &str = "https://github.com/freefcw/BananaTray";
const APP_WEBSITE: &str = "https://github.com/freefcw/BananaTray";
const APP_LICENSE: &str = "MIT License";
const APP_AUTHOR: &str = "BananaTray Team";

impl SettingsView {
    /// About Tab 入口
    pub(super) fn render_about_tab(&self, theme: &Theme) -> Div {
        div()
            .flex_col()
            .items_center()
            .w_full()
            .px(px(24.0))
            .py(px(12.0))
            .child(Self::render_app_hero(theme))
            .child(Self::render_link_buttons(theme))
            .child(Self::render_update_button(theme))
            .child(Self::render_info_section(theme))
            .child(Self::render_copyright(theme))
    }

    // ========================================================================
    // Hero 区域：图标 + 名称 + 描述
    // ========================================================================

    fn render_app_hero(theme: &Theme) -> Div {
        div()
            .flex_col()
            .items_center()
            .w_full()
            .pt(px(16.0))
            .pb(px(14.0))
            .gap(px(10.0))
            .child(Self::render_glow_icon(theme))
            .child(
                div()
                    .w_full()
                    .text_align(TextAlign::Center)
                    .text_size(px(22.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .pt(px(2.0))
                    .child(APP_NAME),
            )
            .child(
                div()
                    .w_full()
                    .text_align(TextAlign::Center)
                    .text_size(px(13.0))
                    .text_color(theme.text_muted)
                    .line_height(relative(1.5))
                    .child(t!("settings.about.desc").to_string()),
            )
    }

    /// 带双层辉光的 App 图标 + 版本徽章
    fn render_glow_icon(theme: &Theme) -> Div {
        let accent = theme.text_accent;
        let glow_outer = hsla(accent.h, accent.s, accent.l, 0.08);
        let glow_inner = hsla(accent.h, accent.s, accent.l, 0.15);
        let icon_border = hsla(accent.h, accent.s * 0.5, accent.l, 0.3);

        // 外层辉光（120px）→ 中层辉光（104px）→ 图标卡片（88px）
        div().flex().items_center().justify_center().w_full().child(
            div()
                .w(px(120.0))
                .h(px(120.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(30.0))
                .bg(glow_outer)
                .child(
                    div()
                        .w(px(104.0))
                        .h(px(104.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(26.0))
                        .bg(glow_inner)
                        .child(Self::render_icon_card_with_badge(theme, icon_border)),
                ),
        )
    }

    /// 图标卡片 + 右下角版本徽章
    fn render_icon_card_with_badge(theme: &Theme, border_color: Hsla) -> Div {
        div()
            .relative()
            // 图标卡片
            .child(
                div()
                    .w(px(88.0))
                    .h(px(88.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(22.0))
                    .border_1()
                    .border_color(border_color)
                    .bg(theme.bg_card)
                    .child(
                        svg()
                            .path("src/icons/tray_icon.svg")
                            .size(px(48.0))
                            .text_color(theme.text_accent),
                    ),
            )
            // 版本徽章
            .child(
                div()
                    .absolute()
                    .bottom(px(-6.0))
                    .right(px(-8.0))
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded(px(6.0))
                    .bg(theme.text_accent)
                    .text_size(px(10.5))
                    .font_weight(FontWeight::BOLD)
                    .text_color(hsla(0.0, 0.0, 1.0, 1.0))
                    .child(format!("v{APP_VERSION}")),
            )
    }

    // ========================================================================
    // 链接按钮
    // ========================================================================

    fn render_link_buttons(theme: &Theme) -> Div {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .pt(px(4.0))
            .pb(px(16.0))
            .child(Self::render_pill_link_button(
                "src/icons/overview.svg",
                "GitHub",
                APP_REPO,
                theme,
            ))
    }

    /// 胶囊链接按钮 — 圆角 + 边框 + 图标
    fn render_pill_link_button(
        icon_path: &'static str,
        label: &str,
        url: &str,
        theme: &Theme,
    ) -> Div {
        let url_owned = url.to_string();
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(16.0))
            .py(px(8.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.bg_card)
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg_subtle))
            .child(
                svg()
                    .path(icon_path)
                    .size(px(14.0))
                    .text_color(theme.text_secondary),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_secondary)
                    .child(label.to_string()),
            )
            .on_mouse_down(MouseButton::Left, move |_, _, _| {
                open_url(&url_owned);
            })
    }

    // ========================================================================
    // Check for Updates
    // ========================================================================

    fn render_update_button(theme: &Theme) -> Div {
        let accent = theme.text_accent;
        let border = hsla(accent.h, accent.s, accent.l, 0.5);
        let text = hsla(accent.h, accent.s, accent.l, 0.7);

        div().w_full().pb(px(14.0)).child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .py(px(10.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(border)
                .bg(transparent_black())
                .cursor_pointer()
                .hover(|s| s.bg(hsla(accent.h, accent.s, accent.l, 0.06)))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(text)
                        .child(t!("about.check_updates").to_string()),
                )
                .on_mouse_down(MouseButton::Left, move |_, _, _| {
                    open_url(APP_REPO);
                }),
        )
    }

    // ========================================================================
    // 信息行区域（Developed by / License / Website）
    // ========================================================================

    fn render_info_section(theme: &Theme) -> Div {
        let accent = theme.text_accent;
        let link_color = hsla(accent.h, accent.s, accent.l, 0.7);

        div()
            .flex_col()
            .w_full()
            .pt(px(2.0))
            .pb(px(8.0))
            .child(Self::render_info_row(
                &t!("about.developed_by"),
                APP_AUTHOR,
                None,
                theme.text_secondary,
                theme,
            ))
            .child(div().h(px(0.5)).w_full().bg(theme.border_subtle))
            .child(Self::render_info_row(
                &t!("about.license"),
                APP_LICENSE,
                None,
                theme.text_secondary,
                theme,
            ))
            .child(div().h(px(0.5)).w_full().bg(theme.border_subtle))
            .child(Self::render_info_row(
                &t!("about.website"),
                "BananaTray",
                Some(APP_WEBSITE),
                link_color,
                theme,
            ))
    }

    /// 信息行 — 左标签 + 右值（可选链接）
    fn render_info_row(
        label: &str,
        value: &str,
        url: Option<&str>,
        value_color: Hsla,
        theme: &Theme,
    ) -> Div {
        let value_str = value.to_string();

        let mut value_el = div()
            .text_size(px(12.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(value_color);

        if let Some(link) = url {
            let link_owned = link.to_string();
            value_el = value_el
                .flex()
                .items_center()
                .gap(px(4.0))
                .cursor_pointer()
                .child(value_str)
                .child(div().text_size(px(10.0)).text_color(value_color).child("↗"))
                .on_mouse_down(MouseButton::Left, move |_, _, _| {
                    open_url(&link_owned);
                });
        } else {
            value_el = value_el.child(value_str);
        }

        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .py(px(8.0))
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(theme.text_muted)
                    .child(label.to_string()),
            )
            .child(value_el)
    }

    // ========================================================================
    // 版权信息
    // ========================================================================

    fn render_copyright(theme: &Theme) -> Div {
        let year = 2026;
        let c = theme.text_muted;
        let copyright_color = hsla(c.h, c.s, c.l * 0.8, c.a * 0.7);

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .pt(px(6.0))
            .pb(px(4.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(copyright_color)
                    .child(format!("© {year} {APP_AUTHOR}. All rights reserved.")),
            )
    }
}
