use super::components::{
    render_dark_card, render_divider, render_section_header, render_section_header_with_desc,
    render_setting_row_dimmable, render_switch_row_dimmable, render_toggle_setting,
    TAB_PADDING_BOTTOM, TAB_PADDING_X,
};
use super::SettingsView;
use crate::application::{AppAction, QuotaThresholdUnitViewState, SettingChange};
use crate::models::{AppSettings, QuotaThresholdTarget, QuotaThresholdUnit};
use crate::theme::Theme;
use gpui::{div, px, Context, Div, ParentElement, Styled, Window};
use rust_i18n::t;

impl SettingsView {
    /// Render Alerts & Thresholds settings tab — 桌面通知与全局状态/提醒阈值
    pub(super) fn render_alerts_tab(
        &mut self,
        settings: &AppSettings,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let state = self.state.clone();
        let notif_enabled = settings.notification.session_quota_notifications;
        let sound_enabled = settings.notification.notification_sound;
        let sound_state = state.clone();
        let usage_state = state.clone();

        let global_quota_units: Vec<QuotaThresholdUnitViewState> = QuotaThresholdUnit::ALL
            .iter()
            .map(|&unit| QuotaThresholdUnitViewState {
                unit,
                override_thresholds: None,
                effective: settings.quota.thresholds(unit),
            })
            .collect();

        div()
            .flex_col()
            .px(px(TAB_PADDING_X))
            .pb(px(TAB_PADDING_BOTTOM))
            // ═══════ 桌面通知 ═══════
            .child(render_section_header(
                &t!("settings.section.notifications"),
                theme,
            ))
            .child(
                render_dark_card(theme)
                    .child(render_toggle_setting(
                        &state,
                        &t!("settings.session_quota_notifications"),
                        &t!("settings.session_quota_notifications.desc"),
                        notif_enabled,
                        theme,
                        SettingChange::ToggleSessionQuotaNotifications,
                    ))
                    .child(render_divider(theme))
                    .child(render_setting_row_dimmable(
                        &t!("settings.quota_usage"),
                        &t!("settings.quota_usage.desc"),
                        !notif_enabled,
                        theme,
                        super::quota_usage::render_quota_usage_stepper(
                            Some(settings.notification.quota_usage_step_pct),
                            None,
                            theme,
                            move |step, window, cx| {
                                crate::bootstrap::dispatch_in_window(
                                    &usage_state,
                                    AppAction::UpdateSetting(SettingChange::SetQuotaUsageStep(
                                        step.unwrap_or(0),
                                    )),
                                    window,
                                    cx,
                                );
                            },
                        ),
                    ))
                    .child(render_divider(theme))
                    .child(render_switch_row_dimmable(
                        &t!("settings.notification_sound"),
                        &t!("settings.notification_sound.desc"),
                        sound_enabled,
                        !notif_enabled,
                        theme,
                        move |_, window, cx| {
                            crate::bootstrap::dispatch_in_window(
                                &sound_state,
                                AppAction::UpdateSetting(SettingChange::ToggleNotificationSound),
                                window,
                                cx,
                            );
                        },
                    )),
            )
            // ═══════ 全局额度状态与提醒阈值 ═══════
            .child(render_section_header_with_desc(
                &t!("settings.section.quota_thresholds"),
                &t!("settings.quota_thresholds.desc"),
                theme,
            ))
            .child(super::quota_thresholds::render_quota_thresholds_section(
                self,
                QuotaThresholdTarget::Global,
                &global_quota_units,
                theme,
                window,
                cx,
                {
                    let state = state.clone();
                    std::rc::Rc::new(
                        move |change: SettingChange, window: &mut Window, cx: &mut gpui::App| {
                            crate::bootstrap::dispatch_in_window(
                                &state,
                                AppAction::UpdateSetting(change),
                                window,
                                cx,
                            );
                        },
                    )
                },
            ))
    }
}
