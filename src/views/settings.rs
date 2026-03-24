use gpui::*;

use crate::models::{AppSettings, AppTheme};
use crate::theme::Theme;

// ============================================================================
// 设置面板
// ============================================================================

/// 设置面板：主题切换、刷新间隔、Provider 管理
#[derive(IntoElement)]
pub struct SettingsPanel {
    settings: AppSettings,
}

impl SettingsPanel {
    pub fn new(settings: AppSettings) -> Self {
        Self { settings }
    }
}

impl RenderOnce for SettingsPanel {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.global::<Theme>();

        let section_bg = theme.bg_panel;
        let border_color = theme.border_subtle;
        let sub_text = theme.text_secondary;
        let label_color = theme.text_primary;

        let settings = &self.settings;

        div()
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(20.0))
            // 标题
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Settings"),
            )
            // 外观设置
            .child(self.render_section(
                "Appearance",
                "Customize the look and feel",
                section_bg,
                border_color,
                sub_text,
                vec![self.render_setting_row(
                    "Theme",
                    match settings.theme {
                        AppTheme::Dark => "🌙 Dark",
                        AppTheme::Light => "☀️ Light",
                    },
                    label_color,
                    sub_text,
                )],
            ))
            // 监控设置
            .child(self.render_section(
                "Monitoring",
                "Configure data refresh behavior",
                section_bg,
                border_color,
                sub_text,
                vec![
                    self.render_setting_row(
                        "Refresh Interval",
                        &format!("{}s", settings.refresh_interval_secs),
                        label_color,
                        sub_text,
                    ),
                    self.render_setting_row(
                        "Global Hotkey",
                        &settings.global_hotkey,
                        label_color,
                        sub_text,
                    ),
                ],
            ))
            // 关于
            .child(self.render_section(
                "About",
                "BananaTray v0.1.0",
                section_bg,
                border_color,
                sub_text,
                vec![
                    self.render_setting_row(
                        "Framework",
                        "adabraka-ui + adabraka-gpui",
                        label_color,
                        sub_text,
                    ),
                    self.render_setting_row(
                        "Platform",
                        std::env::consts::OS,
                        label_color,
                        sub_text,
                    ),
                ],
            ))
    }
}

impl SettingsPanel {
    /// 渲染设置区块
    fn render_section(
        &self,
        title: &str,
        subtitle: &str,
        bg: Hsla,
        border: Hsla,
        sub_text: Hsla,
        rows: Vec<Div>,
    ) -> impl IntoElement {
        div()
            .rounded(px(6.0))
            .bg(bg)
            .border_1()
            .border_color(border)
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(sub_text)
                            .child(subtitle.to_string()),
                    ),
            )
            .children(rows)
    }

    /// 渲染设置行：标签 + 值
    fn render_setting_row(
        &self,
        label: &str,
        value: &str,
        label_color: Hsla,
        value_color: Hsla,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .py(px(8.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(label_color)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(value_color)
                    .child(value.to_string()),
            )
    }
}
