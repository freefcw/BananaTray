use super::components::{render_dark_card, render_divider, render_section_header};
use super::SettingsView;
use crate::app::widgets::{render_segmented_control, SegmentedSize};
use crate::application::{AppAction, SettingChange};
use crate::models::{AppSettings, AppTheme};
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
                    .py(px(14.0))
                    .gap(px(16.0))
                    // Theme 选择器
                    .child(
                        div()
                            .flex_col()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child(t!("settings.theme").to_string()),
                            )
                            .child({
                                let state = self.state.clone();
                                let options: Vec<(String, AppTheme)> = vec![
                                    (t!("theme.system").to_string(), AppTheme::System),
                                    (t!("theme.light").to_string(), AppTheme::Light),
                                    (t!("theme.dark").to_string(), AppTheme::Dark),
                                ];
                                render_segmented_control(
                                    &options,
                                    &settings.theme,
                                    SegmentedSize::Full,
                                    theme,
                                    move |variant: AppTheme, window, cx| {
                                        runtime::dispatch_in_window(
                                            &state,
                                            AppAction::UpdateSetting(SettingChange::Theme(variant)),
                                            window,
                                            cx,
                                        );
                                    },
                                )
                            }),
                    )
                    // Language 选择器
                    .child(
                        div()
                            .flex_col()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child(t!("settings.language").to_string()),
                            )
                            .child({
                                let state = self.state.clone();
                                let options: Vec<(String, String)> =
                                    crate::i18n::SUPPORTED_LANGUAGES
                                        .iter()
                                        .map(|&(code, name_key)| {
                                            (t!(name_key).to_string(), code.to_string())
                                        })
                                        .collect();
                                let current_lang = settings.language.clone();
                                render_segmented_control(
                                    &options,
                                    &current_lang,
                                    SegmentedSize::Full,
                                    theme,
                                    move |code: String, window, cx| {
                                        runtime::dispatch_in_window(
                                            &state,
                                            AppAction::UpdateSetting(SettingChange::Language(code)),
                                            window,
                                            cx,
                                        );
                                    },
                                )
                            }),
                    ),
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
                        settings.show_dashboard_button,
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
                        settings.show_refresh_button,
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
                settings.show_debug_tab,
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
