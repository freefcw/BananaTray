use super::components::{render_dark_card, render_divider, render_section_header};
use super::SettingsView;
use crate::app::widgets::{render_segmented_control, SegmentedSize};
use crate::application::{AppAction, SettingChange};
use crate::models::{AppSettings, AppTheme, QuotaDisplayMode, TrayIconStyle};
use crate::runtime;
use crate::theme::Theme;
use gpui::*;
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
            // ═══════ APPEARANCE ═══════
            .child(render_section_header(&t!("settings.section.theme"), theme))
            .child(
                render_dark_card(theme)
                    .px(px(14.0))
                    .py(px(4.0))
                    // Theme 选择器行
                    .child(render_inline_segmented_row(
                        &t!("settings.theme"),
                        vec![
                            (t!("theme.system").to_string(), AppTheme::System),
                            (t!("theme.light").to_string(), AppTheme::Light),
                            (t!("theme.dark").to_string(), AppTheme::Dark),
                        ],
                        &settings.display.theme,
                        theme,
                        {
                            let state = self.state.clone();
                            move |variant: AppTheme, window, cx| {
                                runtime::dispatch_in_window(
                                    &state,
                                    AppAction::UpdateSetting(SettingChange::Theme(variant)),
                                    window,
                                    cx,
                                );
                            }
                        },
                    ))
                    .child(render_divider(theme))
                    // Language 选择器行
                    .child(render_inline_segmented_row(
                        &t!("settings.language"),
                        crate::i18n::SUPPORTED_LANGUAGES
                            .iter()
                            .map(|&(code, name_key)| (t!(name_key).to_string(), code.to_string()))
                            .collect(),
                        &settings.display.language,
                        theme,
                        {
                            let state = self.state.clone();
                            move |code: String, window, cx| {
                                runtime::dispatch_in_window(
                                    &state,
                                    AppAction::UpdateSetting(SettingChange::Language(code)),
                                    window,
                                    cx,
                                );
                            }
                        },
                    ))
                    .child(render_divider(theme))
                    // Tray Icon Style 选择器行
                    .child(render_inline_segmented_row(
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
                        ],
                        &settings.display.tray_icon_style,
                        theme,
                        {
                            let state = self.state.clone();
                            move |style: TrayIconStyle, window, cx| {
                                runtime::dispatch_in_window(
                                    &state,
                                    AppAction::UpdateSetting(SettingChange::SetTrayIconStyle(
                                        style,
                                    )),
                                    window,
                                    cx,
                                );
                            }
                        },
                    ))
                    .child(render_divider(theme))
                    // Quota Display Mode 选择器行
                    .child(render_inline_segmented_row(
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
                        {
                            let state = self.state.clone();
                            move |mode: QuotaDisplayMode, window, cx| {
                                runtime::dispatch_in_window(
                                    &state,
                                    AppAction::UpdateSetting(SettingChange::SetQuotaDisplayMode(
                                        mode,
                                    )),
                                    window,
                                    cx,
                                );
                            }
                        },
                    )),
            )
            // ═══════ TOOLBAR ═══════
            .child(render_section_header(
                &t!("settings.section.toolbar"),
                theme,
            ))
            .child(
                render_dark_card(theme)
                    // Show Dashboard Button
                    .child(Self::render_icon_switch_row(
                        "src/icons/overview.svg",
                        rgb(ICON_FG).into(),
                        rgb(ICON_BG_DASHBOARD).into(),
                        &t!("settings.show_dashboard"),
                        &t!("settings.show_dashboard.desc"),
                        settings.display.show_dashboard_button,
                        theme,
                        {
                            let state = self.state.clone();
                            move |_, window, cx| {
                                runtime::dispatch_in_window(
                                    &state,
                                    AppAction::UpdateSetting(
                                        SettingChange::ToggleShowDashboardButton,
                                    ),
                                    window,
                                    cx,
                                );
                            }
                        },
                    ))
                    .child(render_divider(theme))
                    // Show Refresh Button
                    .child(Self::render_icon_switch_row(
                        "src/icons/refresh.svg",
                        rgb(ICON_FG).into(),
                        rgb(ICON_BG_REFRESH).into(),
                        &t!("settings.show_refresh"),
                        &t!("settings.show_refresh.desc"),
                        settings.display.show_refresh_button,
                        theme,
                        {
                            let state = self.state.clone();
                            move |_, window, cx| {
                                runtime::dispatch_in_window(
                                    &state,
                                    AppAction::UpdateSetting(
                                        SettingChange::ToggleShowRefreshButton,
                                    ),
                                    window,
                                    cx,
                                );
                            }
                        },
                    )),
            )
            // ═══════ DEVELOPER ═══════
            .child(render_section_header(
                &t!("settings.section.developer"),
                theme,
            ))
            .child(render_dark_card(theme).child(Self::render_icon_switch_row(
                "src/icons/advanced.svg",
                rgb(ICON_FG).into(),
                rgb(ICON_BG_DEBUG).into(),
                &t!("settings.show_debug_tab"),
                &t!("settings.show_debug_tab.desc"),
                settings.display.show_debug_tab,
                theme,
                {
                    let state = self.state.clone();
                    move |_, window, cx| {
                        runtime::dispatch_in_window(
                            &state,
                            AppAction::UpdateSetting(SettingChange::ToggleShowDebugTab),
                            window,
                            cx,
                        );
                    }
                },
            )))
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
        .child(render_segmented_control(
            &options,
            current,
            SegmentedSize::Inline,
            theme,
            on_select,
        ))
}
