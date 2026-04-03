use super::components::{render_dark_card, render_divider, render_section_header};
use super::SettingsView;
use crate::application::{
    build_debug_info_text, debug_tab_view_state, AppAction, DebugContext, DebugNotificationKind,
    ProviderDiagnosticItem, ProviderDiagnosticStatus,
};
use crate::runtime;
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;

// 设计稿颜色常量
const ICON_BG_LOG: u32 = 0x2d6a4f; // 深绿色 (Log Level)
const ICON_BG_FILE: u32 = 0x1a5276; // 深蓝色 (Log File)
const ICON_BG_NOTIF: u32 = 0xa62828; // 深红色 (Test Notification)
const ICON_BG_ENV: u32 = 0x4a1a6b; // 深紫色 (Environment)
const ICON_FG: u32 = 0xffffff;

// Provider 诊断状态颜色
const DOT_CONNECTED: u32 = 0x27ae60;
const DOT_REFRESHING: u32 = 0x3498db;
const DOT_ERROR: u32 = 0xe74c3c;
const DOT_DISCONNECTED: u32 = 0x95a5a6;
const DOT_DISABLED: u32 = 0x555555;

/// 当前支持的日志级别
const LOG_LEVELS: &[(&str, &str)] = &[
    ("error", "Error"),
    ("warn", "Warn"),
    ("info", "Info"),
    ("debug", "Debug"),
    ("trace", "Trace"),
];

impl SettingsView {
    /// Render Debug settings tab — 开发者诊断中心
    pub(super) fn render_debug_tab(&self, theme: &Theme) -> Div {
        // 在 UI 层收集运行时上下文（含 I/O），然后传给纯函数 selector
        let debug_state = {
            let state = self.state.borrow();
            let ctx = DebugContext::collect(state.log_path.clone());
            debug_tab_view_state(&state.session, &ctx)
        };

        div()
            .flex_col()
            .px(px(16.0))
            .pb(px(16.0))
            // ═══════ LOG LEVEL ═══════
            .child(render_section_header(
                &t!("settings.section.debug_log"),
                theme,
            ))
            .child(
                render_dark_card(theme)
                    .child(Self::render_log_level_row(
                        &debug_state.log.current_level,
                        theme,
                        &self.state,
                    ))
                    .child(render_divider(theme))
                    .child(self.render_log_file_row(&debug_state, theme)),
            )
            // ═══════ PROVIDER DIAGNOSTICS ═══════
            .child(render_section_header(&t!("debug.section.providers"), theme))
            .child(self.render_provider_diagnostics(&debug_state, theme))
            // ═══════ ENVIRONMENT ═══════
            .child(render_section_header(
                &t!("debug.section.environment"),
                theme,
            ))
            .child(self.render_environment_card(&debug_state, theme))
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

    // ========================================================================
    // Section 1: Log — 日志级别 + 文件信息
    // ========================================================================

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
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        runtime::dispatch_in_window(
                            &state,
                            AppAction::UpdateLogLevel(level_owned.clone()),
                            window,
                            cx,
                        );
                    }),
            );
        }

        row = row.child(control);
        row
    }

    /// 日志文件信息行 — 路径 + 大小 + Open/Copy 按钮
    fn render_log_file_row(
        &self,
        debug_state: &crate::application::DebugTabViewState,
        theme: &Theme,
    ) -> Div {
        use crate::app::widgets::render_svg_icon;

        let log = &debug_state.log;
        let path_display = log.log_path.as_deref().unwrap_or("—");
        let size_display = log.log_file_size.as_deref().unwrap_or("—");
        let subtitle = format!("{} · {}", path_display, size_display);

        let state_open = self.state.clone();
        let state_copy = self.state.clone();
        let path_for_copy = log.log_path.clone().unwrap_or_default();

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
                    .bg(rgb(ICON_BG_FILE))
                    .flex_shrink_0()
                    .child(render_svg_icon(
                        "src/icons/status.svg",
                        px(18.0),
                        rgb(ICON_FG).into(),
                    )),
            )
            // 标题 + 路径描述
            .child(
                div()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(t!("debug.log_file").to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(subtitle),
                    ),
            )
            // Open 按钮
            .child(
                self.render_mini_button(&t!("debug.open"), theme)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        runtime::dispatch_in_window(
                            &state_open,
                            AppAction::OpenLogDirectory,
                            window,
                            cx,
                        );
                    }),
            )
            // Copy Path 按钮
            .child(
                self.render_mini_button(&t!("debug.copy_path"), theme)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        runtime::dispatch_in_window(
                            &state_copy,
                            AppAction::CopyToClipboard(path_for_copy.clone()),
                            window,
                            cx,
                        );
                    }),
            )
    }

    // ========================================================================
    // Section 2: Provider Diagnostics
    // ========================================================================

    fn render_provider_diagnostics(
        &self,
        debug_state: &crate::application::DebugTabViewState,
        theme: &Theme,
    ) -> Div {
        let mut card = render_dark_card(theme);

        for (i, provider) in debug_state.providers.iter().enumerate() {
            if i > 0 {
                card = card.child(render_divider(theme));
            }
            card = card.child(self.render_provider_diagnostic_row(provider, theme));
        }

        if debug_state.providers.is_empty() {
            card = card.child(
                div()
                    .px(px(14.0))
                    .py(px(16.0))
                    .text_size(px(13.0))
                    .text_color(theme.text_muted)
                    .child("No providers registered"),
            );
        }

        card
    }

    fn render_provider_diagnostic_row(&self, item: &ProviderDiagnosticItem, theme: &Theme) -> Div {
        let dot_color = match item.status_dot {
            ProviderDiagnosticStatus::Connected => rgb(DOT_CONNECTED),
            ProviderDiagnosticStatus::Refreshing => rgb(DOT_REFRESHING),
            ProviderDiagnosticStatus::Error => rgb(DOT_ERROR),
            ProviderDiagnosticStatus::Disconnected => rgb(DOT_DISCONNECTED),
            ProviderDiagnosticStatus::Disabled => rgb(DOT_DISABLED),
        };

        let quota_text = if item.quota_count > 0 {
            t!("debug.quotas_loaded", n = item.quota_count).to_string()
        } else {
            t!("debug.no_quotas").to_string()
        };

        let is_disabled = item.status_dot == ProviderDiagnosticStatus::Disabled;
        let text_color = if is_disabled {
            theme.text_muted
        } else {
            theme.text_primary
        };

        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(14.0))
            .py(px(10.0))
            // Provider 图标
            .child(
                svg()
                    .path(item.icon.clone())
                    .size(px(18.0))
                    .text_color(if is_disabled {
                        theme.text_muted
                    } else {
                        theme.text_secondary
                    })
                    .flex_shrink_0(),
            )
            // 状态点
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded_full()
                    .bg(dot_color)
                    .flex_shrink_0(),
            )
            // Provider 名称 + 状态描述
            .child(
                div()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(1.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(text_color)
                                    .child(item.display_name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_muted)
                                    .child(format!("· {}", item.source)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(item.status_text.clone()),
                    ),
            )
            // 配额数
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_muted)
                    .child(quota_text),
            )
    }

    // ========================================================================
    // Section 3: Environment
    // ========================================================================

    fn render_environment_card(
        &self,
        debug_state: &crate::application::DebugTabViewState,
        theme: &Theme,
    ) -> Div {
        use crate::app::widgets::render_svg_icon;

        let env = &debug_state.environment;

        let env_rows: Vec<(String, String)> = vec![
            (t!("debug.env.version").to_string(), env.app_version.clone()),
            (t!("debug.env.os").to_string(), env.os_info.clone()),
            (t!("debug.env.log_level").to_string(), env.log_level.clone()),
            (t!("debug.env.locale").to_string(), env.locale.clone()),
            (
                t!("debug.env.settings_path").to_string(),
                env.settings_path.clone(),
            ),
            (t!("debug.env.log_path").to_string(), env.log_path.clone()),
            (
                t!("debug.env.providers").to_string(),
                env.providers_summary.clone(),
            ),
            (
                t!("debug.env.refresh").to_string(),
                env.refresh_interval.clone(),
            ),
        ];

        let mut card = render_dark_card(theme);

        // 头部图标行
        card = card.child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .px(px(14.0))
                .pt(px(12.0))
                .pb(px(8.0))
                .child(
                    div()
                        .w(px(28.0))
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(rgb(ICON_BG_ENV))
                        .flex_shrink_0()
                        .child(render_svg_icon(
                            "src/icons/about.svg",
                            px(14.0),
                            rgb(ICON_FG).into(),
                        )),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child(t!("debug.section.environment").to_string()),
                ),
        );

        // 键值对行
        for (label, value) in &env_rows {
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(14.0))
                    .py(px(5.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_muted)
                            .child(label.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_secondary)
                            .max_w(px(280.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(value.clone()),
                    ),
            );
        }

        // Copy Debug Info 按钮
        let debug_text = build_debug_info_text(debug_state);
        let state = self.state.clone();

        card = card.child(
            div().px(px(14.0)).pt(px(8.0)).pb(px(12.0)).child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(6.0))
                    .px(px(16.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .bg(theme.bg_subtle)
                    .border_1()
                    .border_color(theme.border_strong)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.85))
                    .child(render_svg_icon(
                        "src/icons/overview.svg",
                        px(12.0),
                        theme.text_secondary,
                    ))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(t!("debug.copy_debug_info").to_string()),
                    )
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        runtime::dispatch_in_window(
                            &state,
                            AppAction::CopyToClipboard(debug_text.clone()),
                            window,
                            cx,
                        );
                    }),
            ),
        );

        card
    }

    // ========================================================================
    // Section 4: Test Notifications (保留现有)
    // ========================================================================

    /// 测试通知按钮行
    fn render_test_notification_button(
        &self,
        title: &str,
        desc: &str,
        alert_type: &str,
        theme: &Theme,
    ) -> Div {
        use crate::app::widgets::render_svg_icon;

        let alert_kind = match alert_type {
            "low" => DebugNotificationKind::Low,
            "exhausted" => DebugNotificationKind::Exhausted,
            _ => DebugNotificationKind::Recovered,
        };
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
                self.render_mini_button(&t!("debug.send"), theme)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        runtime::dispatch_in_window(
                            &state,
                            AppAction::SendDebugNotification(alert_kind),
                            window,
                            cx,
                        );
                    }),
            )
    }

    // ========================================================================
    // 共享组件
    // ========================================================================

    /// 小型操作按钮（Send / Open / Copy Path）
    fn render_mini_button(&self, label: &str, theme: &Theme) -> Div {
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
                    .child(label.to_string()),
            )
    }
}
