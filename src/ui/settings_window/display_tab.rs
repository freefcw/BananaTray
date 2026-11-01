use super::components::{
    render_dark_card, render_divider, render_section_header, render_segmented_row,
    render_toggle_setting, TAB_PADDING_BOTTOM, TAB_PADDING_X,
};
use super::SettingsView;
use crate::application::{AppAction, SettingChange};
use crate::models::{
    AppSettings, AppTheme, NavTabStyle, QuotaCardHistoryRange, QuotaCardStyle, QuotaDisplayMode,
    TrayIconStyle,
};
use crate::theme::Theme;
use gpui::{div, px, Div, ParentElement, Styled};
use rust_i18n::t;

impl SettingsView {
    /// Render Display settings tab — 按「界面与菜单栏 → 托盘弹窗布局 → 配额卡片与图表」三层组织
    pub(super) fn render_display_tab(&self, settings: &AppSettings, theme: &Theme) -> Div {
        div()
            .flex_col()
            .px(px(TAB_PADDING_X))
            .pb(px(TAB_PADDING_BOTTOM))
            .child(render_section_header(
                &t!("settings.section.interface"),
                theme,
            ))
            .child(self.render_interface_card(settings, theme))
            .child(render_section_header(
                &t!("settings.section.popup_layout"),
                theme,
            ))
            .child(self.render_popup_layout_card(settings, theme))
            .child(render_section_header(
                &t!("settings.section.quota_cards"),
                theme,
            ))
            .child(self.render_quota_cards_card(settings, theme))
    }

    fn render_interface_card(&self, settings: &AppSettings, theme: &Theme) -> Div {
        render_dark_card(theme)
            .child(self.render_theme_setting(settings, theme))
            .child(render_divider(theme))
            .child(self.render_language_setting(settings, theme))
            .child(render_divider(theme))
            .child(self.render_tray_icon_setting(settings, theme))
    }

    fn render_popup_layout_card(&self, settings: &AppSettings, theme: &Theme) -> Div {
        render_dark_card(theme)
            .child(render_toggle_setting(
                &self.state,
                &t!("settings.show_overview"),
                &t!("settings.show_overview.desc"),
                settings.display.show_overview,
                theme,
                SettingChange::ToggleShowOverview,
            ))
            .child(render_divider(theme))
            .child(self.render_nav_tab_style_setting(settings, theme))
            .child(render_divider(theme))
            .child(render_toggle_setting(
                &self.state,
                &t!("settings.nav_status_dots"),
                &t!("settings.nav_status_dots.desc"),
                settings.display.nav_status_dots,
                theme,
                SettingChange::ToggleNavStatusDots,
            ))
            .child(render_divider(theme))
            .child(render_toggle_setting(
                &self.state,
                &t!("settings.show_account_info"),
                &t!("settings.show_account_info.desc"),
                settings.display.show_account_info,
                theme,
                SettingChange::ToggleShowAccountInfo,
            ))
            .child(render_divider(theme))
            .child(render_toggle_setting(
                &self.state,
                &t!("settings.show_dashboard"),
                &t!("settings.show_dashboard.desc"),
                settings.display.show_dashboard_button,
                theme,
                SettingChange::ToggleShowDashboardButton,
            ))
            .child(render_divider(theme))
            .child(render_toggle_setting(
                &self.state,
                &t!("settings.show_refresh"),
                &t!("settings.show_refresh.desc"),
                settings.display.show_refresh_button,
                theme,
                SettingChange::ToggleShowRefreshButton,
            ))
    }

    fn render_quota_cards_card(&self, settings: &AppSettings, theme: &Theme) -> Div {
        render_dark_card(theme)
            .child(self.render_quota_display_setting(settings, theme))
            .child(render_divider(theme))
            .child(self.render_quota_card_style_setting(settings, theme))
            .child(render_divider(theme))
            .child(self.render_quota_card_history_setting(settings, theme))
            .child(render_divider(theme))
            .child(render_toggle_setting(
                &self.state,
                &t!("settings.merge_history_charts"),
                &t!("settings.merge_history_charts.desc"),
                settings.display.merge_history_charts,
                theme,
                SettingChange::ToggleMergeHistoryCharts,
            ))
    }

    fn render_theme_setting(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();
        render_segmented_row(
            &t!("settings.theme"),
            &t!("settings.theme.desc"),
            vec![
                (t!("theme.system").to_string(), AppTheme::System),
                (t!("theme.light").to_string(), AppTheme::Light),
                (t!("theme.dark").to_string(), AppTheme::Dark),
            ],
            &settings.display.theme,
            theme,
            move |variant, window, cx| {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::UpdateSetting(SettingChange::Theme(variant)),
                    window,
                    cx,
                );
                crate::ui::sync_fc_ui_theme(variant, window.appearance(), cx);
            },
        )
    }

    fn render_language_setting(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();
        let languages = crate::i18n::SUPPORTED_LANGUAGES
            .iter()
            .map(|&(code, name_key)| (t!(name_key).to_string(), code.to_string()))
            .collect();
        render_segmented_row(
            &t!("settings.language"),
            &t!("settings.language.desc"),
            languages,
            &settings.display.language,
            theme,
            move |code, window, cx| {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::UpdateSetting(SettingChange::Language(code)),
                    window,
                    cx,
                );
            },
        )
    }

    fn render_tray_icon_setting(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();
        render_segmented_row(
            &t!("settings.tray_icon_style"),
            &t!("settings.tray_icon_style.desc"),
            vec![
                (
                    t!("settings.tray_icon.monochrome").to_string(),
                    TrayIconStyle::Monochrome,
                ),
                (
                    t!("settings.tray_icon.yellow").to_string(),
                    TrayIconStyle::Yellow,
                ),
                (
                    t!("settings.tray_icon.colorful").to_string(),
                    TrayIconStyle::Colorful,
                ),
                (
                    t!("settings.tray_icon.dynamic").to_string(),
                    TrayIconStyle::Dynamic,
                ),
            ],
            &settings.display.tray_icon_style,
            theme,
            move |style, window, cx| {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::UpdateSetting(SettingChange::SetTrayIconStyle(style)),
                    window,
                    cx,
                );
            },
        )
    }

    fn render_quota_display_setting(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();
        render_segmented_row(
            &t!("settings.quota_display_mode"),
            &t!("settings.quota_display_mode.desc"),
            vec![
                (
                    t!("settings.quota_display_mode.remaining").to_string(),
                    QuotaDisplayMode::Remaining,
                ),
                (
                    t!("settings.quota_display_mode.used").to_string(),
                    QuotaDisplayMode::Used,
                ),
            ],
            &settings.display.quota_display_mode,
            theme,
            move |mode, window, cx| {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::UpdateSetting(SettingChange::SetQuotaDisplayMode(mode)),
                    window,
                    cx,
                );
            },
        )
    }

    fn render_quota_card_style_setting(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();
        render_segmented_row(
            &t!("settings.quota_card_style"),
            &t!("settings.quota_card_style.desc"),
            vec![
                (
                    t!("settings.quota_card_style.bar").to_string(),
                    QuotaCardStyle::Bar,
                ),
                (
                    t!("settings.quota_card_style.ring").to_string(),
                    QuotaCardStyle::Ring,
                ),
            ],
            &settings.display.quota_card_style,
            theme,
            move |style, window, cx| {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::UpdateSetting(SettingChange::SetQuotaCardStyle(style)),
                    window,
                    cx,
                );
            },
        )
    }

    fn render_quota_card_history_setting(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();
        render_segmented_row(
            &t!("settings.quota_card_history"),
            &t!("settings.quota_card_history.desc"),
            vec![
                (
                    t!("settings.quota_card_history.last_24h").to_string(),
                    QuotaCardHistoryRange::Last24Hours,
                ),
                (
                    t!("settings.quota_card_history.current_period").to_string(),
                    QuotaCardHistoryRange::CurrentPeriod,
                ),
            ],
            &settings.display.quota_card_history_range,
            theme,
            move |range, window, cx| {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::UpdateSetting(SettingChange::SetQuotaCardHistoryRange(range)),
                    window,
                    cx,
                );
            },
        )
    }

    fn render_nav_tab_style_setting(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();
        render_segmented_row(
            &t!("settings.nav_tab_style"),
            &t!("settings.nav_tab_style.desc"),
            vec![
                (
                    t!("settings.nav_tab_style.full").to_string(),
                    NavTabStyle::Full,
                ),
                (
                    t!("settings.nav_tab_style.compact").to_string(),
                    NavTabStyle::Compact,
                ),
            ],
            &settings.display.nav_tab_style,
            theme,
            move |style, window, cx| {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::UpdateSetting(SettingChange::SetNavTabStyle(style)),
                    window,
                    cx,
                );
            },
        )
    }
}
