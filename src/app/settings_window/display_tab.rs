use super::SettingsView;
use crate::app::persist_settings;
use crate::app::widgets::{
    render_card, render_card_separator, render_section_label, render_switch_row,
};
use crate::models::AppSettings;
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;

impl SettingsView {
    /// Render Display settings tab
    pub(super) fn render_display_tab(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();
        let dash_state = state.clone();
        let refresh_btn_state = state.clone();

        div()
            .flex_col()
            .px(px(16.0))
            .pt(px(16.0))
            .pb(px(20.0))
            // ═══════ LANGUAGE ═══════
            .child(
                div()
                    .flex_col()
                    .child(render_section_label(
                        &t!("settings.section.language"),
                        theme,
                    ))
                    .child(self.render_language_selector(&settings.language, theme)),
            )
            // ═══════ TOOLBAR ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(render_section_label(&t!("settings.section.toolbar"), theme))
                    .child(
                        render_card(theme)
                            .child(render_switch_row(
                                &t!("settings.show_dashboard"),
                                &t!("settings.show_dashboard.desc"),
                                settings.show_toolbar_dashboard,
                                theme,
                                move |_, window, _| {
                                    let settings = {
                                        let mut s = dash_state.borrow_mut();
                                        s.settings.show_toolbar_dashboard =
                                            !s.settings.show_toolbar_dashboard;
                                        s.settings.clone()
                                    };
                                    persist_settings(&settings);
                                    window.refresh();
                                },
                            ))
                            .child(render_card_separator(theme))
                            .child(render_switch_row(
                                &t!("settings.show_refresh"),
                                &t!("settings.show_refresh.desc"),
                                settings.show_toolbar_refresh,
                                theme,
                                move |_, window, _| {
                                    let settings = {
                                        let mut s = refresh_btn_state.borrow_mut();
                                        s.settings.show_toolbar_refresh =
                                            !s.settings.show_toolbar_refresh;
                                        s.settings.clone()
                                    };
                                    persist_settings(&settings);
                                    window.refresh();
                                },
                            )),
                    ),
            )
    }
}
