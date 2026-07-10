use super::components::{render_dark_card, render_divider, render_section_header};
use super::SettingsView;
use crate::application::{
    build_debug_info_text, debug_tab_view_state, format_debug_console_logs, AppAction,
    DebugNotificationKind, DebugTabViewState, EnvironmentRowKind, LogLevelColor,
};
use crate::models::ProviderId;
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::{
    render_action_button, render_colored_icon_sized, render_icon_row, render_icon_tooltip_button,
    render_info_cell, render_path_info_cell, render_segmented_control, ButtonVariant,
    IconTooltipButtonOptions, SegmentedSize,
};
use gpui::{
    div, px, rgb, AnyElement, Context, Div, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, StatefulInteractiveElement, Styled,
};
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
    /// 在后台刷新 Debug Tab 的阻塞式系统诊断快照。
    pub(in crate::ui::settings_window) fn refresh_debug_diagnostics(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.debug_diagnostics_loading {
            return;
        }

        self.debug_diagnostics_loading = true;
        let log_path = self.state.borrow().log_path.clone();
        let collect_task = cx
            .background_executor()
            .spawn(async move { runtime::collect_debug_diagnostics(log_path) });
        self._debug_diagnostics_task = Some(cx.spawn(async move |view, cx| {
            let diagnostics = collect_task.await;
            let _ = view.update(cx, |view, cx| {
                view.debug_diagnostics = Some(diagnostics);
                view.debug_diagnostics_loading = false;
                cx.notify();
            });
        }));
    }

    /// Render Debug settings tab — 开发者诊断中心
    pub(super) fn render_debug_tab(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let log_path = self.state.borrow().log_path.clone();
        let ctx =
            runtime::debug_context_from_diagnostics(log_path, self.debug_diagnostics.as_ref());
        let debug_state = {
            let state = self.state.borrow();
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
            .child(self.render_environment_card(&debug_state, theme, cx))
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
                        DebugNotificationKind::Low,
                        theme,
                    ))
                    .child(render_divider(theme))
                    .child(self.render_test_notification_button(
                        &t!("debug.test_exhausted"),
                        &t!("debug.test_exhausted.desc"),
                        DebugNotificationKind::Exhausted,
                        theme,
                    ))
                    .child(render_divider(theme))
                    .child(self.render_test_notification_button(
                        &t!("debug.test_recovered"),
                        &t!("debug.test_recovered.desc"),
                        DebugNotificationKind::Recovered,
                        theme,
                    )),
            )
    }

    // ========================================================================
    // Section 1: Log — 日志级别 + 文件信息
    // ========================================================================

    /// 日志级别选择行 — 使用 render_icon_row + render_segmented_control
    fn render_log_level_row(
        current: &str,
        theme: &Theme,
        state: &std::rc::Rc<std::cell::RefCell<crate::runtime::AppState>>,
    ) -> Div {
        let options: Vec<(String, String)> = LOG_LEVELS
            .iter()
            .map(|&(level, label)| (label.to_string(), level.to_string()))
            .collect();
        let current_owned = current.to_lowercase();
        let state_clone = state.clone();

        render_icon_row(
            "src/icons/advanced.svg",
            rgb(ICON_FG).into(),
            rgb(ICON_BG_LOG).into(),
            &t!("debug.log_level"),
            &t!("debug.log_level.desc"),
            theme,
            div().flex_shrink_0().child(render_segmented_control(
                &options,
                &current_owned,
                SegmentedSize::Compact,
                theme,
                move |level: String, window, cx| {
                    crate::bootstrap::dispatch_in_window(
                        &state_clone,
                        AppAction::UpdateLogLevel(level),
                        window,
                        cx,
                    );
                },
            )),
        )
    }

    /// 日志文件信息行 — 使用 render_icon_row + render_action_button 组合
    fn render_log_file_row(
        &self,
        debug_state: &crate::application::DebugTabViewState,
        theme: &Theme,
    ) -> Div {
        let log = &debug_state.log;
        let path_display = log.log_path.as_deref().unwrap_or("—");
        let size_display = log.log_file_size.as_deref().unwrap_or("—");
        let subtitle = format!("{} · {}", path_display, size_display);

        let state_open = self.state.clone();
        let state_copy = self.state.clone();
        let path_for_copy = log.log_path.clone().unwrap_or_default();

        // 右侧操作按钮组：Open + Copy Path
        let trailing = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(6.0))
            .child(render_action_button(
                &t!("debug.open"),
                None,
                ButtonVariant::Subtle,
                false,
                theme,
                move |_, window, cx| {
                    crate::bootstrap::dispatch_in_window(
                        &state_open,
                        AppAction::OpenLogDirectory,
                        window,
                        cx,
                    );
                },
            ))
            .child(render_action_button(
                &t!("debug.copy_path"),
                None,
                ButtonVariant::Subtle,
                false,
                theme,
                move |_, window, cx| {
                    crate::bootstrap::dispatch_in_window(
                        &state_copy,
                        AppAction::CopyToClipboard(path_for_copy.clone()),
                        window,
                        cx,
                    );
                },
            ));

        // render_icon_row 的 description 是纯文字，而这里需要 overflow_hidden 的路径信息，
        // 因此用 render_colored_icon + 手动中间区域 + trailing 保持布局一致
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(14.0))
            .py(px(12.0))
            .child(crate::ui::widgets::render_colored_icon(
                "src/icons/status.svg",
                rgb(ICON_FG).into(),
                rgb(ICON_BG_FILE).into(),
            ))
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
                            .text_color(theme.text.primary)
                            .child(t!("debug.log_file").to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text.muted)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(subtitle),
                    ),
            )
            .child(trailing)
    }

    // ========================================================================
    // Section 2: Environment — 使用 render_colored_icon_sized + render_info_cell
    // ========================================================================

    fn render_environment_card(
        &self,
        debug_state: &crate::application::DebugTabViewState,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let env = &debug_state.environment;
        let env_rows = env.rows();
        let mut card = render_dark_card(theme).child(self.render_environment_header(theme, cx));

        // 键值对行 — 使用 render_info_cell，配置文件/日志路径行可点击打开所在目录
        for row in &env_rows {
            if matches!(
                row.kind,
                EnvironmentRowKind::SettingsPath | EnvironmentRowKind::LogPath
            ) {
                let tooltip_id = match row.kind {
                    EnvironmentRowKind::SettingsPath => "env-settings-path-tooltip",
                    EnvironmentRowKind::LogPath => "env-log-path-tooltip",
                    _ => unreachable!("only path rows have tooltips"),
                };
                card = card.child(div().px(px(14.0)).py(px(5.0)).child(render_path_info_cell(
                    tooltip_id, &row.label, &row.value, theme,
                )));
            } else {
                card = card.child(div().px(px(14.0)).py(px(5.0)).child(render_info_cell(
                    &row.label,
                    &row.value,
                    theme.text.secondary,
                    theme,
                )));
            }
        }

        // Copy Debug Info 按钮 — 使用 render_action_button
        let debug_text = build_debug_info_text(debug_state);
        let state = self.state.clone();

        card = card.child(
            div()
                .px(px(14.0))
                .pt(px(8.0))
                .pb(px(12.0))
                .child(render_action_button(
                    &t!("debug.copy_debug_info"),
                    Some(("src/icons/overview.svg", theme.text.secondary)),
                    ButtonVariant::Subtle,
                    true,
                    theme,
                    move |_, window, cx| {
                        crate::bootstrap::dispatch_in_window(
                            &state,
                            AppAction::CopyToClipboard(debug_text.clone()),
                            window,
                            cx,
                        );
                    },
                )),
        );

        card
    }

    fn render_environment_header(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let entity = cx.entity().clone();
        let loading = self.debug_diagnostics_loading;

        div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(14.0))
            .pt(px(8.0))
            .pb(px(4.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(render_colored_icon_sized(
                        "src/icons/about.svg",
                        rgb(ICON_FG).into(),
                        rgb(ICON_BG_ENV).into(),
                        28.0,
                        14.0,
                    ))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text.primary)
                            .child(t!("debug.section.environment").to_string()),
                    ),
            )
            .child(render_icon_tooltip_button(
                "debug-environment-refresh".into(),
                "src/icons/refresh.svg",
                IconTooltipButtonOptions {
                    tooltip_text: Some(if loading {
                        t!("debug.console.refreshing").to_string()
                    } else {
                        t!("tooltip.refresh").to_string()
                    }),
                    enabled: !loading,
                    icon_color: theme.text.secondary,
                    disabled_icon_color: theme.text.muted,
                    hover_bg: theme.bg.subtle,
                },
                theme,
                move |_, _, cx| {
                    entity.update(cx, |view, cx| {
                        view.refresh_debug_diagnostics(cx);
                    });
                },
            ))
    }

    // ========================================================================
    // Section 3: Test Notifications
    // ========================================================================

    /// 测试通知按钮行 — 使用 render_icon_row + render_action_button
    fn render_test_notification_button(
        &self,
        title: &str,
        desc: &str,
        alert_kind: DebugNotificationKind,
        theme: &Theme,
    ) -> Div {
        let state = self.state.clone();

        render_icon_row(
            "src/icons/status.svg",
            rgb(ICON_FG).into(),
            rgb(ICON_BG_NOTIF).into(),
            title,
            desc,
            theme,
            div().flex_shrink_0().child(render_action_button(
                &t!("debug.send"),
                None,
                ButtonVariant::Subtle,
                false,
                theme,
                move |_, window, cx| {
                    crate::bootstrap::dispatch_in_window(
                        &state,
                        AppAction::SendDebugNotification(alert_kind),
                        window,
                        cx,
                    );
                },
            )),
        )
    }

    // ═══════ PROVIDER DEBUG CONSOLE ═══════

    fn render_debug_console(&self, debug_state: &DebugTabViewState, theme: &Theme) -> Div {
        let mut card = render_dark_card(theme)
            .child(self.render_debug_console_toolbar(debug_state, theme))
            .child(render_divider(theme))
            .child(Self::render_debug_console_log_body(debug_state, theme));

        if !debug_state.console.log_entries.is_empty() {
            card = card
                .child(render_divider(theme))
                .child(self.render_debug_console_footer(debug_state, theme));
        }

        card
    }

    fn render_debug_console_toolbar(&self, debug_state: &DebugTabViewState, theme: &Theme) -> Div {
        let console = &debug_state.console;
        let mut toolbar = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(14.0))
            .py(px(10.0));

        for (kind, name) in &console.available_providers {
            toolbar = toolbar.child(self.render_debug_provider_chip(
                kind,
                name,
                console.selected_provider.as_ref() == Some(kind),
                theme,
            ));
        }

        toolbar = toolbar.child(div().flex_grow());

        if console.selected_provider.is_some() {
            toolbar =
                toolbar.child(self.render_debug_refresh_button(console.refresh_active, theme));
        }

        toolbar
    }

    fn render_debug_provider_chip(
        &self,
        provider_id: &ProviderId,
        name: &str,
        is_selected: bool,
        theme: &Theme,
    ) -> Div {
        let provider_id = provider_id.clone();
        let state = self.state.clone();

        div()
            .px(px(10.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(if is_selected {
                theme.bg.card_inner
            } else {
                theme.bg.subtle
            })
            .border_1()
            .border_color(if is_selected {
                theme.text.accent_soft
            } else {
                theme.border.strong
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
                        theme.text.accent
                    } else {
                        theme.text.secondary
                    })
                    .child(name.to_string()),
            )
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::SelectDebugProvider(provider_id.clone()),
                    window,
                    cx,
                );
            })
    }

    fn render_debug_refresh_button(&self, is_active: bool, theme: &Theme) -> Div {
        let label = if is_active {
            t!("debug.console.refreshing").to_string()
        } else {
            t!("debug.console.force_refresh").to_string()
        };

        let button = div()
            .px(px(12.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .bg(if is_active {
                theme.bg.subtle
            } else {
                theme.button.action_bg
            })
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if is_active {
                        theme.text.muted
                    } else {
                        theme.button.action_text
                    })
                    .child(label),
            );

        if is_active {
            return button;
        }

        let state = self.state.clone();
        button
            .cursor_pointer()
            .hover(|s| s.opacity(0.85))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::DebugRefreshProvider,
                    window,
                    cx,
                );
            })
    }

    fn render_debug_console_log_body(debug_state: &DebugTabViewState, theme: &Theme) -> AnyElement {
        let console = &debug_state.console;
        if console.log_entries.is_empty() {
            return div()
                .w_full()
                .py(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.text.muted)
                        .child(t!("debug.console.empty").to_string()),
                )
                .into_any_element();
        }

        let mut log_panel = div()
            .id("debug-log-panel")
            .w_full()
            .max_h(px(280.0))
            .overflow_y_scroll()
            .px(px(14.0))
            .py(px(8.0));

        for entry in &console.log_entries {
            log_panel = log_panel.child(Self::render_debug_console_log_entry(
                &entry.timestamp,
                &entry.level,
                entry.level_color,
                &entry.target,
                &entry.message,
                theme,
            ));
        }

        log_panel.into_any_element()
    }

    fn render_debug_console_log_entry(
        timestamp: &str,
        level: &str,
        level_color: LogLevelColor,
        target: &str,
        message: &str,
        theme: &Theme,
    ) -> Div {
        let level_color = match level_color {
            LogLevelColor::Error => theme.log.error,
            LogLevelColor::Warn => theme.log.warn,
            LogLevelColor::Info => theme.log.info,
            LogLevelColor::Debug => theme.log.debug,
            LogLevelColor::Trace => theme.log.trace,
        };

        div()
            .w_full()
            .flex()
            .gap(px(6.0))
            .py(px(1.0))
            .child(
                div()
                    .text_size(px(10.0))
                    .font_family("SF Mono")
                    .text_color(theme.text.muted)
                    .flex_shrink_0()
                    .child(timestamp.to_string()),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .font_family("SF Mono")
                    .font_weight(FontWeight::BOLD)
                    .text_color(level_color)
                    .w(px(42.0))
                    .flex_shrink_0()
                    .child(format!("[{}]", level)),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .font_family("SF Mono")
                    .text_color(theme.text.secondary)
                    .w(px(100.0))
                    .flex_shrink_0()
                    .child(target.to_string()),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .font_family("SF Mono")
                    .text_color(theme.text.primary)
                    .flex_grow()
                    .child(message.to_string()),
            )
    }

    fn render_debug_console_footer(&self, debug_state: &DebugTabViewState, theme: &Theme) -> Div {
        let console = &debug_state.console;
        let log_text = format_debug_console_logs(&console.log_entries);
        let state_copy = self.state.clone();
        let state_clear = self.state.clone();

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(14.0))
            .py(px(8.0))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.text.muted)
                    .child(format!(
                        "{} {}",
                        console.log_count,
                        t!("debug.console.entries")
                    )),
            )
            .child(div().flex_grow())
            .child(
                Self::render_debug_console_footer_button(&t!("debug.console.copy_logs"), theme)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        crate::bootstrap::dispatch_in_window(
                            &state_copy,
                            AppAction::CopyToClipboard(log_text.clone()),
                            window,
                            cx,
                        );
                    }),
            )
            .child(
                Self::render_debug_console_footer_button(&t!("debug.console.clear_logs"), theme)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        crate::bootstrap::dispatch_in_window(
                            &state_clear,
                            AppAction::ClearDebugLogs,
                            window,
                            cx,
                        );
                    }),
            )
    }

    fn render_debug_console_footer_button(label: &str, theme: &Theme) -> Div {
        div()
            .px(px(8.0))
            .py(px(3.0))
            .rounded(px(4.0))
            .bg(theme.bg.subtle)
            .border_1()
            .border_color(theme.border.strong)
            .cursor_pointer()
            .hover(|s| s.opacity(0.85))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.text.secondary)
                    .child(label.to_string()),
            )
    }
}
