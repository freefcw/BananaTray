use super::SettingsView;
use crate::application::{build_issue_report, build_issue_url, IssueReportContext};
use crate::platform::system::open_url;
use crate::theme::Theme;
use crate::ui::widgets::{render_action_button, render_kv_info_row, ButtonVariant};
use gpui::{
    div, hsla, img, px, relative, svg, Div, FontWeight, Hsla, InteractiveElement, MouseButton,
    ParentElement, Styled, TextAlign,
};
use rust_i18n::t;

// ============================================================================
// About 页 — 匹配 Lumina Bar 设计稿
// ============================================================================

const APP_NAME: &str = "BananaTray";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_REPO: &str = env!("CARGO_PKG_REPOSITORY");
const APP_WEBSITE: &str = env!("CARGO_PKG_HOMEPAGE");
const APP_LICENSE: &str = "MIT License";
const APP_AUTHOR: &str = "freefcw";
const APP_AUTHOR_URL: &str = "https://github.com/freefcw";
const GIT_HASH: &str = match option_env!("BANANATRAY_GIT_HASH") {
    Some(h) => h,
    None => "unknown",
};

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
            .child(self.render_action_buttons_row(theme))
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
                    .text_color(theme.text.primary)
                    .pt(px(2.0))
                    .child(APP_NAME),
            )
            .child(
                div()
                    .w_full()
                    .text_align(TextAlign::Center)
                    .text_size(px(13.0))
                    .text_color(theme.text.muted)
                    .line_height(relative(1.5))
                    .child(t!("settings.about.desc").to_string()),
            )
    }

    /// 带双层辉光的 App 图标 + 版本徽章
    fn render_glow_icon(theme: &Theme) -> Div {
        let accent = theme.text.accent;
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
                    .bg(theme.bg.card)
                    .child(img("src/icons/app_logo.png").w(px(56.0)).h(px(56.0))),
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
                    .bg(theme.text.accent)
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
            .border_color(theme.border.strong)
            .bg(theme.bg.card)
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg.subtle))
            .child(
                svg()
                    .path(icon_path)
                    .size(px(14.0))
                    .text_color(theme.text.secondary),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text.secondary)
                    .child(label.to_string()),
            )
            .on_mouse_down(MouseButton::Left, move |_, _, _| {
                open_url(&url_owned);
            })
    }

    // ========================================================================
    // 操作按钮行 — 检查更新 + 上报问题（同行并排）
    // ========================================================================

    fn render_action_buttons_row(&self, theme: &Theme) -> Div {
        let state = self.state.clone();
        let repo = APP_REPO.to_string();

        div()
            .w_full()
            .flex()
            .gap(px(10.0))
            .pb(px(12.0))
            // 检查更新
            .child(div().flex_1().child(render_action_button(
                &t!("about.check_updates"),
                None,
                ButtonVariant::Outlined,
                true,
                theme,
                move |_, _, _| {
                    open_url(&repo);
                },
            )))
            // 上报问题：复制诊断信息到剪贴板 + 打开 GitHub Issue 页
            .child(div().flex_1().child(render_action_button(
                &t!("about.report_issue"),
                Some(("src/icons/status.svg", theme.text.secondary)),
                ButtonVariant::Outlined,
                true,
                theme,
                move |_, _, _| {
                    let borrowed = state.borrow();
                    let log_path = borrowed.log_path.as_deref();
                    let ctx = IssueReportContext::collect(log_path);
                    let report = build_issue_report(&borrowed.session, &ctx);
                    let url = build_issue_url(&report);
                    drop(borrowed);
                    open_url(&url);
                },
            )))
    }

    // ========================================================================
    // 信息行区域 — 使用公共 render_kv_info_row
    // ========================================================================

    fn render_info_section(theme: &Theme) -> Div {
        let accent = theme.text.accent;
        let link_color = hsla(accent.h, accent.s, accent.l, 0.7);

        div()
            .flex_col()
            .w_full()
            .pt(px(2.0))
            .pb(px(8.0))
            .child(render_kv_info_row(
                &t!("about.build_version"),
                GIT_HASH,
                None,
                theme.text.secondary,
                theme,
            ))
            .child(div().h(px(0.5)).w_full().bg(theme.border.subtle))
            .child(render_kv_info_row(
                &t!("about.developed_by"),
                APP_AUTHOR,
                Some(APP_AUTHOR_URL),
                link_color,
                theme,
            ))
            .child(div().h(px(0.5)).w_full().bg(theme.border.subtle))
            .child(render_kv_info_row(
                &t!("about.license"),
                APP_LICENSE,
                None,
                theme.text.secondary,
                theme,
            ))
            .child(div().h(px(0.5)).w_full().bg(theme.border.subtle))
            .child(render_kv_info_row(
                &t!("about.website"),
                "BananaTray",
                Some(APP_WEBSITE),
                link_color,
                theme,
            ))
    }

    // ========================================================================
    // 版权信息
    // ========================================================================

    fn render_copyright(theme: &Theme) -> Div {
        let year = 2026;
        let c = theme.text.muted;
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
