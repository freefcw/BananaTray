use super::components::{render_dark_card, render_divider, render_section_header};
use super::SettingsView;
use crate::app::widgets::{render_action_button, ButtonVariant};
use crate::application::{AppAction, SettingChange};
use crate::models::AppSettings;
use crate::runtime;
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;

// 设计稿颜色常量 — 各设置项的彩色图标背景
const ICON_BG_LOGIN: u32 = 0x3b30a6; // 紫蓝色 (Start at Login)
const ICON_BG_REFRESH: u32 = 0xb55a10; // 琥珀橙色 (Refresh Rate)
const ICON_BG_NOTIF: u32 = 0xa62828; // 深红色 (Quota Notifications)
const ICON_BG_SOUND: u32 = 0x6b3fa0; // 紫色 (Notification Sound)
const ICON_FG: u32 = 0xffffff; // 图标前景色统一白色

impl SettingsView {
    /// Render General settings tab — 匹配设计稿风格
    pub(super) fn render_general_tab(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();

        // ── SYSTEM section ───────────────────────────────────
        let login_state = state.clone();
        let login_checked = settings.start_at_login;

        // ── AUTOMATION section ───────────────────────────────
        let notif_state = state.clone();
        let notif_checked = settings.session_quota_notifications;
        let sound_state = state.clone();
        let sound_checked = settings.notification_sound;

        // Cadence dropdown (复用现有组件)
        let cadence_mins = if settings.refresh_interval_mins == 0 {
            None
        } else {
            Some(settings.refresh_interval_mins)
        };

        div()
            .flex_col()
            .px(px(16.0))
            .pb(px(16.0))
            // ═══════ SYSTEM ═══════
            .child(render_section_header(&t!("settings.section.system"), theme))
            .child(render_dark_card(theme).child(Self::render_icon_switch_row(
                "src/icons/switch.svg",
                rgb(ICON_FG).into(),
                rgb(ICON_BG_LOGIN).into(),
                &t!("settings.start_at_login"),
                &t!("settings.start_at_login.desc"),
                login_checked,
                theme,
                move |_, window, cx| {
                    runtime::dispatch_in_window(
                        &login_state,
                        AppAction::UpdateSetting(SettingChange::ToggleStartAtLogin),
                        window,
                        cx,
                    );
                },
            )))
            // ═══════ AUTOMATION ═══════
            .child(render_section_header(
                &t!("settings.section.automation"),
                theme,
            ))
            .child(
                render_dark_card(theme)
                    // Refresh Rate — 带下拉选择器
                    .child(Self::render_icon_dropdown_row(
                        "src/icons/refresh.svg",
                        rgb(ICON_FG).into(),
                        rgb(ICON_BG_REFRESH).into(),
                        &t!("settings.refresh_cadence"),
                        &t!("settings.refresh_cadence.desc"),
                        theme,
                        self.render_cadence_trigger(&state, cadence_mins, theme),
                    ))
                    .child(render_divider(theme))
                    // Quota Notifications
                    .child(Self::render_icon_switch_row(
                        "src/icons/status.svg",
                        rgb(ICON_FG).into(),
                        rgb(ICON_BG_NOTIF).into(),
                        &t!("settings.session_quota_notifications"),
                        &t!("settings.session_quota_notifications.desc"),
                        notif_checked,
                        theme,
                        move |_, window, cx| {
                            runtime::dispatch_in_window(
                                &notif_state,
                                AppAction::UpdateSetting(
                                    SettingChange::ToggleSessionQuotaNotifications,
                                ),
                                window,
                                cx,
                            );
                        },
                    ))
                    .child(render_divider(theme))
                    // Notification Sound
                    .child(Self::render_icon_switch_row(
                        "src/icons/usage.svg",
                        rgb(ICON_FG).into(),
                        rgb(ICON_BG_SOUND).into(),
                        &t!("settings.notification_sound"),
                        &t!("settings.notification_sound.desc"),
                        sound_checked,
                        theme,
                        move |_, window, cx| {
                            runtime::dispatch_in_window(
                                &sound_state,
                                AppAction::UpdateSetting(SettingChange::ToggleNotificationSound),
                                window,
                                cx,
                            );
                        },
                    )),
            )
            // ═══════ Quit ═══════
            .child(self.render_quit_button(theme))
    }

    /// 内联刷新频率触发按钮 — 风格与设计稿一致
    fn render_cadence_trigger(
        &self,
        state: &std::rc::Rc<std::cell::RefCell<crate::app::AppState>>,
        cadence_mins: Option<u64>,
        theme: &Theme,
    ) -> Div {
        let dropdown_open = state.borrow().session.settings_ui.cadence_dropdown_open;
        let toggle_state = state.clone();

        let label = match cadence_mins {
            None => t!("cadence.manual").to_string(),
            Some(1) => t!("cadence.1_minute").to_string(),
            Some(m) => t!("cadence.n_minutes", n = m).to_string(),
        };

        let mut trigger = div()
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(4.0))
            .px(px(10.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .bg(theme.bg_subtle)
            .border_1()
            .border_color(if dropdown_open {
                theme.element_selected
            } else {
                theme.border_strong
            })
            .cursor_pointer()
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(label),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.text_muted)
                    .ml(px(4.0))
                    .child(if dropdown_open { "▲" } else { "▼" }),
            )
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                runtime::dispatch_in_window(
                    &toggle_state,
                    AppAction::ToggleCadenceDropdown,
                    window,
                    cx,
                );
            });

        if dropdown_open {
            trigger = trigger.child(self.render_cadence_options(state, cadence_mins, theme));
        }

        trigger
    }

    /// 下拉选项列表
    fn render_cadence_options(
        &self,
        state: &std::rc::Rc<std::cell::RefCell<crate::app::AppState>>,
        cadence_mins: Option<u64>,
        theme: &Theme,
    ) -> Deferred {
        use std::ops::Range;

        const OPTIONS: &[Option<u64>] = &[
            None,
            Some(1),
            Some(2),
            Some(3),
            Some(5),
            Some(10),
            Some(15),
            Some(30),
        ];

        let bg = theme.bg_base;
        let border = theme.border_strong;
        let state = state.clone();
        let theme = theme.clone();

        deferred(
            div()
                .occlude()
                .absolute()
                .top(px(32.0))
                .right(px(0.0))
                .w(px(100.0))
                .h(px(180.0))
                .rounded(px(8.0))
                .bg(bg)
                .border_1()
                .border_color(border)
                .shadow_lg()
                .child(
                    uniform_list(
                        "cadence-options-list",
                        OPTIONS.len(),
                        move |range: Range<usize>, _window: &mut Window, _cx: &mut App| {
                            range
                                .map(|i| {
                                    let mins = OPTIONS[i];
                                    let is_active = cadence_mins == mins;
                                    let opt_state = state.clone();
                                    let label = match mins {
                                        None => t!("cadence.manual").to_string(),
                                        Some(1) => t!("cadence.1_minute").to_string(),
                                        Some(m) => t!("cadence.n_minutes", n = m).to_string(),
                                    };

                                    let mut row = div()
                                        .w_full()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .px(px(12.0))
                                        .py(px(7.0))
                                        .cursor_pointer()
                                        .bg(if is_active {
                                            theme.element_selected
                                        } else {
                                            bg
                                        })
                                        .child(
                                            div()
                                                .text_size(px(12.5))
                                                .font_weight(if is_active {
                                                    FontWeight::SEMIBOLD
                                                } else {
                                                    FontWeight::MEDIUM
                                                })
                                                .text_color(if is_active {
                                                    theme.element_active
                                                } else {
                                                    theme.text_primary
                                                })
                                                .child(label),
                                        );

                                    if is_active {
                                        row = row.child(
                                            div()
                                                .text_size(px(11.0))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(theme.element_active)
                                                .child("✓"),
                                        );
                                    }

                                    row = row.on_mouse_down(
                                        MouseButton::Left,
                                        move |_: &MouseDownEvent,
                                              window: &mut Window,
                                              cx: &mut App| {
                                            runtime::dispatch_in_window(
                                                &opt_state,
                                                AppAction::UpdateSetting(
                                                    SettingChange::RefreshCadence(mins),
                                                ),
                                                window,
                                                cx,
                                            );
                                        },
                                    );

                                    if i > 0 {
                                        row = row.border_t_1().border_color(theme.border_strong);
                                    }
                                    row
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                    .size_full(),
                ),
        )
        .with_priority(1)
    }

    /// 退出按钮 — 使用 render_action_button (Danger 变体)
    fn render_quit_button(&self, theme: &Theme) -> Div {
        let state = self.state.clone();
        div().mt(px(16.0)).child(render_action_button(
            &t!("settings.quit"),
            Some(("src/icons/switch.svg", theme.status_error)),
            ButtonVariant::Danger,
            true,
            theme,
            move |_, window, cx| {
                runtime::dispatch_in_window(&state, AppAction::QuitApp, window, cx);
            },
        ))
    }
}
