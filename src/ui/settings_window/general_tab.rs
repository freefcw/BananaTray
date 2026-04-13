use super::components::{render_dark_card, render_divider, render_section_header};
use super::SettingsView;
use crate::application::{AppAction, SettingChange};
use crate::models::AppSettings;
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::{render_action_button, ButtonVariant};
use gpui::{div, px, rgb, Div, ParentElement, Styled};
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
        let login_checked = settings.system.start_at_login;

        // ── AUTOMATION section ───────────────────────────────
        let notif_state = state.clone();
        let notif_checked = settings.notification.session_quota_notifications;
        let sound_state = state.clone();
        let sound_checked = settings.notification.notification_sound;

        // Cadence dropdown (复用现有组件)
        let cadence_mins = if settings.system.refresh_interval_mins == 0 {
            None
        } else {
            Some(settings.system.refresh_interval_mins)
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
                        crate::ui::widgets::render_cadence_trigger(&state, cadence_mins, theme),
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

    /// 退出按钮 — 使用 render_action_button (Danger 变体)
    fn render_quit_button(&self, theme: &Theme) -> Div {
        let state = self.state.clone();
        div().mt(px(16.0)).child(render_action_button(
            &t!("settings.quit"),
            Some(("src/icons/switch.svg", theme.status.error)),
            ButtonVariant::Danger,
            true,
            theme,
            move |_, window, cx| {
                runtime::dispatch_in_window(&state, AppAction::QuitApp, window, cx);
            },
        ))
    }
}
