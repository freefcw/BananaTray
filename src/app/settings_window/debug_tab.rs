use super::components::{render_dark_card, render_divider, render_section_header};
use super::SettingsView;
use crate::notification::{send_system_notification, QuotaAlert};
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;

// 设计稿颜色常量
const ICON_BG_LOG: u32 = 0x2d6a4f; // 深绿色 (Log Level)
const ICON_BG_NOTIF: u32 = 0xa62828; // 深红色 (Test Notification)
const ICON_FG: u32 = 0xffffff;

/// 当前支持的日志级别
const LOG_LEVELS: &[(&str, &str)] = &[
    ("error", "Error"),
    ("warn", "Warn"),
    ("info", "Info"),
    ("debug", "Debug"),
    ("trace", "Trace"),
];

impl SettingsView {
    /// Render Debug settings tab
    pub(super) fn render_debug_tab(&self, theme: &Theme) -> Div {
        let current_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

        div()
            .flex_col()
            .px(px(16.0))
            .pb(px(16.0))
            // ═══════ LOG LEVEL ═══════
            .child(render_section_header(
                &t!("settings.section.debug_log"),
                theme,
            ))
            .child(render_dark_card(theme).child(Self::render_log_level_row(
                &current_level,
                theme,
                &self.state,
            )))
            // ═══════ TEST NOTIFICATIONS ═══════
            .child(render_section_header(
                &t!("settings.section.debug_notifications"),
                theme,
            ))
            .child(
                render_dark_card(theme)
                    .child(self.render_test_notification_button(
                        &t!("debug.test_low_quota"),
                        &t!("debug.test_low_quota.desc"),
                        "low",
                        theme,
                    ))
                    .child(render_divider(theme))
                    .child(self.render_test_notification_button(
                        &t!("debug.test_exhausted"),
                        &t!("debug.test_exhausted.desc"),
                        "exhausted",
                        theme,
                    ))
                    .child(render_divider(theme))
                    .child(self.render_test_notification_button(
                        &t!("debug.test_recovered"),
                        &t!("debug.test_recovered.desc"),
                        "recovered",
                        theme,
                    )),
            )
    }

    /// 日志级别选择行 — 图标 + 分段选择器
    fn render_log_level_row(
        current: &str,
        theme: &Theme,
        state: &std::rc::Rc<std::cell::RefCell<crate::app::AppState>>,
    ) -> Div {
        use crate::app::widgets::render_svg_icon;

        let mut row = div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(14.0))
            .py(px(12.0))
            // 彩色圆形图标
            .child(
                div()
                    .w(px(36.0))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(rgb(ICON_BG_LOG))
                    .flex_shrink_0()
                    .child(render_svg_icon(
                        "src/icons/advanced.svg",
                        px(18.0),
                        rgb(ICON_FG).into(),
                    )),
            )
            // 标题 + 描述
            .child(
                div()
                    .flex_col()
                    .flex_1()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(t!("debug.log_level").to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(t!("debug.log_level.desc").to_string()),
                    ),
            );

        // 日志级别分段选择器
        let mut control = div()
            .flex()
            .flex_shrink_0()
            .rounded(px(6.0))
            .bg(theme.bg_subtle)
            .border_1()
            .border_color(theme.border_subtle)
            .overflow_hidden();

        for &(level, label) in LOG_LEVELS {
            let is_active = current.eq_ignore_ascii_case(level);
            let level_owned = level.to_string();
            let state = state.clone();

            control = control.child(
                div()
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded(px(5.0))
                    .bg(if is_active {
                        theme.nav_pill_active_bg
                    } else {
                        transparent_black()
                    })
                    .text_size(px(11.0))
                    .font_weight(if is_active {
                        FontWeight::BOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if is_active {
                        theme.element_active
                    } else {
                        theme.text_secondary
                    })
                    .cursor_pointer()
                    .child(label)
                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                        // 设置环境变量并更新日志级别
                        std::env::set_var("RUST_LOG", &level_owned);
                        if let Ok(filter) = log::LevelFilter::from_str_exact(&level_owned) {
                            log::set_max_level(filter);
                            log::info!(target: "settings", "log level changed to: {}", level_owned);
                        }
                        // 刷新 UI
                        let _ = state.borrow();
                        window.refresh();
                    }),
            );
        }

        row = row.child(control);
        row
    }

    /// 测试通知按钮行
    fn render_test_notification_button(
        &self,
        title: &str,
        desc: &str,
        alert_type: &str,
        theme: &Theme,
    ) -> Div {
        use crate::app::widgets::render_svg_icon;

        let alert_type = alert_type.to_string();
        let state = self.state.clone();

        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(14.0))
            .py(px(12.0))
            // 彩色圆形图标
            .child(
                div()
                    .w(px(36.0))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(rgb(ICON_BG_NOTIF))
                    .flex_shrink_0()
                    .child(render_svg_icon(
                        "src/icons/status.svg",
                        px(18.0),
                        rgb(ICON_FG).into(),
                    )),
            )
            // 标题 + 描述
            .child(
                div()
                    .flex_col()
                    .flex_1()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(desc.to_string()),
                    ),
            )
            // 发送按钮
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .bg(theme.bg_subtle)
                    .border_1()
                    .border_color(theme.border_strong)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.85))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(t!("debug.send").to_string()),
                    )
                    .on_mouse_down(MouseButton::Left, move |_, _, _| {
                        let with_sound = state.borrow().settings.notification_sound;
                        let alert = match alert_type.as_str() {
                            "low" => QuotaAlert::LowQuota {
                                provider_name: "TestProvider".to_string(),
                                remaining_pct: 8.0,
                            },
                            "exhausted" => QuotaAlert::Exhausted {
                                provider_name: "TestProvider".to_string(),
                            },
                            _ => QuotaAlert::Recovered {
                                provider_name: "TestProvider".to_string(),
                                remaining_pct: 50.0,
                            },
                        };
                        std::thread::spawn(move || {
                            send_system_notification(&alert, with_sound);
                        });
                    }),
            )
    }
}

/// 从字符串精确匹配 log::LevelFilter（不区分大小写）
trait LevelFilterExt {
    fn from_str_exact(s: &str) -> Result<log::LevelFilter, ()>;
}

impl LevelFilterExt for log::LevelFilter {
    fn from_str_exact(s: &str) -> Result<log::LevelFilter, ()> {
        match s.to_lowercase().as_str() {
            "error" => Ok(log::LevelFilter::Error),
            "warn" => Ok(log::LevelFilter::Warn),
            "info" => Ok(log::LevelFilter::Info),
            "debug" => Ok(log::LevelFilter::Debug),
            "trace" => Ok(log::LevelFilter::Trace),
            _ => Err(()),
        }
    }
}
