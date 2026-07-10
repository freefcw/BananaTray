use super::components::{render_dark_card, render_divider, render_section_header, IconSwitchRow};
use super::SettingsView;
use crate::application::{AppAction, SettingChange};
use crate::models::{AppSettings, AppTheme, QuotaDisplayMode, TrayIconStyle};
use crate::theme::Theme;
use crate::ui::widgets::{render_segmented_control, SegmentedSize};
use gpui::{div, px, rgb, App, Div, FontWeight, ParentElement, Styled, Window};
use rust_i18n::t;

// 设计稿颜色常量
const ICON_BG_DASHBOARD: u32 = 0x3b30a6; // 紫蓝色 (Dashboard)
const ICON_BG_REFRESH: u32 = 0xb55a10; // 琥珀橙色 (Refresh)
const ICON_BG_DEBUG: u32 = 0x555555; // 灰色 (Debug Tab)
const ICON_FG: u32 = 0xffffff;

impl SettingsView {
    /// Render Display settings tab — 匹配设计稿风格
    pub(super) fn render_display_tab(&self, settings: &AppSettings, theme: &Theme) -> Div {
        div()
            .flex_col()
            .px(px(16.0))
            .pb(px(16.0))
            .child(render_section_header(&t!("settings.section.theme"), theme))
            .child(self.render_appearance_card(settings, theme))
            .child(render_section_header(
                &t!("settings.section.toolbar"),
                theme,
            ))
            .child(self.render_toolbar_card(settings, theme))
            .child(render_section_header(
                &t!("settings.section.developer"),
                theme,
            ))
            .child(self.render_developer_card(settings, theme))
    }

    fn render_appearance_card(&self, settings: &AppSettings, theme: &Theme) -> Div {
        render_dark_card(theme)
            .px(px(14.0))
            .py(px(4.0))
            .child(self.render_theme_setting(settings, theme))
            .child(render_divider(theme))
            .child(self.render_language_setting(settings, theme))
            .child(render_divider(theme))
            .child(self.render_tray_icon_setting(settings, theme))
            .child(render_divider(theme))
            .child(self.render_quota_display_setting(settings, theme))
    }

    fn render_theme_setting(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();
        render_inline_segmented_row(
            &t!("settings.theme"),
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
            },
        )
    }

    fn render_language_setting(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();
        let languages = crate::i18n::SUPPORTED_LANGUAGES
            .iter()
            .map(|&(code, name_key)| (t!(name_key).to_string(), code.to_string()))
            .collect();
        render_inline_segmented_row(
            &t!("settings.language"),
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
        render_inline_segmented_row(
            &t!("settings.tray_icon_style"),
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
        render_inline_segmented_row(
            &t!("settings.quota_display_mode"),
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

    fn render_toolbar_card(&self, settings: &AppSettings, theme: &Theme) -> Div {
        render_dark_card(theme)
            .child(self.render_toolbar_switch(
                IconSwitchRow {
                    icon_path: "src/icons/overview.svg",
                    icon_color: rgb(ICON_FG).into(),
                    icon_bg: rgb(ICON_BG_DASHBOARD).into(),
                    title: &t!("settings.show_dashboard"),
                    description: &t!("settings.show_dashboard.desc"),
                    enabled: settings.display.show_dashboard_button,
                },
                SettingChange::ToggleShowDashboardButton,
                theme,
            ))
            .child(render_divider(theme))
            .child(self.render_toolbar_switch(
                IconSwitchRow {
                    icon_path: "src/icons/refresh.svg",
                    icon_color: rgb(ICON_FG).into(),
                    icon_bg: rgb(ICON_BG_REFRESH).into(),
                    title: &t!("settings.show_refresh"),
                    description: &t!("settings.show_refresh.desc"),
                    enabled: settings.display.show_refresh_button,
                },
                SettingChange::ToggleShowRefreshButton,
                theme,
            ))
    }

    fn render_toolbar_switch(
        &self,
        row: IconSwitchRow<'_>,
        change: SettingChange,
        theme: &Theme,
    ) -> Div {
        let state = self.state.clone();
        Self::render_icon_switch_row(row, theme, move |_, window, cx| {
            crate::bootstrap::dispatch_in_window(
                &state,
                AppAction::UpdateSetting(change.clone()),
                window,
                cx,
            );
        })
    }

    fn render_developer_card(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();
        render_dark_card(theme).child(Self::render_icon_switch_row(
            IconSwitchRow {
                icon_path: "src/icons/advanced.svg",
                icon_color: rgb(ICON_FG).into(),
                icon_bg: rgb(ICON_BG_DEBUG).into(),
                title: &t!("settings.show_debug_tab"),
                description: &t!("settings.show_debug_tab.desc"),
                enabled: settings.display.show_debug_tab,
            },
            theme,
            move |_, window, cx| {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::UpdateSetting(SettingChange::ToggleShowDebugTab),
                    window,
                    cx,
                );
            },
        ))
    }
}

/// 渲染水平行式分段选择器行：左侧标签（13px MEDIUM）+ 右侧 Inline SegmentedControl
///
/// 用于 Appearance section 的 Theme / Language / Tray Icon Style 行。
fn render_inline_segmented_row<T, F>(
    label: &str,
    options: Vec<(String, T)>,
    current: &T,
    theme: &Theme,
    on_select: F,
) -> Div
where
    T: PartialEq + Clone + 'static,
    F: Fn(T, &mut Window, &mut App) + Clone + 'static,
{
    div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .py(px(10.0))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text.primary)
                .flex_shrink_0()
                .mr(px(16.0))
                .child(label.to_string()),
        )
        .child(div().flex_shrink_0().child(render_segmented_control(
            &options,
            current,
            SegmentedSize::Inline,
            theme,
            on_select,
        )))
}
