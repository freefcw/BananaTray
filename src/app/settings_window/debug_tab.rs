use super::components::{render_dark_card, render_divider, render_section_header};
use super::SettingsView;
use crate::application::{
    build_debug_info_text, debug_tab_view_state, AppAction, DebugContext, DebugNotificationKind,
    DebugTabViewState, LogLevelColor,
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
            // ═══════ PROVIDER DEBUG CONSOLE ═══════
            .child(render_section_header(&t!("debug.section.console"), theme))
            .child(self.render_debug_console(&debug_state, theme))
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
    // Section 2: Environment
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

    // ═══════ PROVIDER DEBUG CONSOLE ═══════

    fn render_debug_console(&self, debug_state: &DebugTabViewState, theme: &Theme) -> Div {
        let console = &debug_state.console;
        let mut card = render_dark_card(theme);

        // ── Provider 选择 + Force Refresh 按钮 ──
        let mut toolbar = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(14.0))
            .py(px(10.0));

        // Provider 选择器（水平按钮组）
        for (kind, name) in &console.available_providers {
            let is_selected = console.selected_provider == Some(*kind);
            let kind_clone = *kind;
            let state_select = self.state.clone();

            toolbar = toolbar.child(
                div()
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .bg(if is_selected {
                        theme.bg_card_inner
                    } else {
                        theme.bg_subtle
                    })
                    .border_1()
                    .border_color(if is_selected {
                        theme.text_accent_soft
                    } else {
                        theme.border_strong
                    })
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.85))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(if is_selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if is_selected {
                                theme.text_accent
                            } else {
                                theme.text_secondary
                            })
                            .child(name.clone()),
                    )
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        runtime::dispatch_in_window(
                            &state_select,
                            AppAction::SelectDebugProvider(kind_clone),
                            window,
                            cx,
                        );
                    }),
            );
        }

        // 弹性空白
        toolbar = toolbar.child(div().flex_grow());

        // Force Refresh 按钮
        if console.selected_provider.is_some() {
            let is_active = console.refresh_active;
            let btn_label = if is_active {
                t!("debug.console.refreshing").to_string()
            } else {
                t!("debug.console.force_refresh").to_string()
            };

            if is_active {
                // 刷新中 — 禁用态
                toolbar = toolbar.child(
                    div()
                        .px(px(12.0))
                        .py(px(5.0))
                        .rounded(px(6.0))
                        .bg(theme.bg_subtle)
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_muted)
                                .child(btn_label),
                        ),
                );
            } else {
                // 可点击态
                let state_refresh = self.state.clone();
                toolbar = toolbar.child(
                    div()
                        .px(px(12.0))
                        .py(px(5.0))
                        .rounded(px(6.0))
                        .bg(rgb(0x2d6a4f))
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.85))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(ICON_FG))
                                .child(btn_label),
                        )
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            runtime::dispatch_in_window(
                                &state_refresh,
                                AppAction::DebugRefreshProvider,
                                window,
                                cx,
                            );
                        }),
                );
            }
        }

        card = card.child(toolbar);
        card = card.child(render_divider(theme));

        // ── 日志面板 ──
        if console.log_entries.is_empty() {
            card = card.child(
                div()
                    .w_full()
                    .py(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(t!("debug.console.empty").to_string()),
                    ),
            );
        } else {
            let mut log_panel = div()
                .id("debug-log-panel")
                .w_full()
                .max_h(px(280.0))
                .overflow_y_scroll()
                .px(px(14.0))
                .py(px(8.0));

            for entry in &console.log_entries {
                let level_color = match entry.level_color {
                    LogLevelColor::Error => rgb(0xe74c3c),
                    LogLevelColor::Warn => rgb(0xf39c12),
                    LogLevelColor::Info => rgb(0x27ae60),
                    LogLevelColor::Debug => rgb(0x3498db),
                    LogLevelColor::Trace => rgb(0x95a5a6),
                };

                log_panel = log_panel.child(
                    div()
                        .w_full()
                        .flex()
                        .gap(px(6.0))
                        .py(px(1.0))
                        // timestamp
                        .child(
                            div()
                                .text_size(px(10.0))
                                .font_family("SF Mono")
                                .text_color(theme.text_muted)
                                .flex_shrink_0()
                                .child(entry.timestamp.clone()),
                        )
                        // level badge
                        .child(
                            div()
                                .text_size(px(10.0))
                                .font_family("SF Mono")
                                .font_weight(FontWeight::BOLD)
                                .text_color(level_color)
                                .w(px(42.0))
                                .flex_shrink_0()
                                .child(format!("[{}]", entry.level)),
                        )
                        // target
                        .child(
                            div()
                                .text_size(px(10.0))
                                .font_family("SF Mono")
                                .text_color(theme.text_secondary)
                                .w(px(100.0))
                                .flex_shrink_0()
                                .child(entry.target.clone()),
                        )
                        // message
                        .child(
                            div()
                                .text_size(px(10.0))
                                .font_family("SF Mono")
                                .text_color(theme.text_primary)
                                .flex_grow()
                                .child(entry.message.clone()),
                        ),
                );
            }

            card = card.child(log_panel);

            // ── 底部工具栏：日志计数 + Copy/Clear 按钮 ──
            card = card.child(render_divider(theme));

            let log_text = console
                .log_entries
                .iter()
                .map(|e| format!("{} [{}] {} {}", e.timestamp, e.level, e.target, e.message))
                .collect::<Vec<_>>()
                .join("\n");

            let state_copy = self.state.clone();
            let state_clear = self.state.clone();
            let log_count = console.log_count;

            card = card.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(14.0))
                    .py(px(8.0))
                    // 日志计数
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .child(format!("{} {}", log_count, t!("debug.console.entries"))),
                    )
                    .child(div().flex_grow())
                    // Copy Logs
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .bg(theme.bg_subtle)
                            .border_1()
                            .border_color(theme.border_strong)
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.85))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme.text_secondary)
                                    .child(t!("debug.console.copy_logs").to_string()),
                            )
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                runtime::dispatch_in_window(
                                    &state_copy,
                                    AppAction::CopyToClipboard(log_text.clone()),
                                    window,
                                    cx,
                                );
                            }),
                    )
                    // Clear Logs
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .bg(theme.bg_subtle)
                            .border_1()
                            .border_color(theme.border_strong)
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.85))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme.text_secondary)
                                    .child(t!("debug.console.clear_logs").to_string()),
                            )
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                runtime::dispatch_in_window(
                                    &state_clear,
                                    AppAction::ClearDebugLogs,
                                    window,
                                    cx,
                                );
                            }),
                    ),
            );
        }

        card
    }
}
