use super::SettingsView;
use crate::app::persist_settings;
use crate::app::widgets::render_card;
use crate::models::AppTheme;
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;

impl SettingsView {
    /// Render language selector card with radio-button style options
    pub(super) fn render_language_selector(&self, current: &str, theme: &Theme) -> Div {
        use crate::i18n::SUPPORTED_LANGUAGES;

        let card = render_card(theme)
            .flex_col()
            .px(px(14.0))
            .py(px(10.0))
            .gap(px(2.0))
            .child(
                div()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(t!("settings.language").to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(relative(1.4))
                            .text_color(theme.text_secondary)
                            .child(t!("settings.language.desc").to_string()),
                    ),
            );

        let mut options = div().flex().flex_wrap().gap(px(6.0)).mt(px(8.0));
        for &(code, name_key) in SUPPORTED_LANGUAGES {
            let is_active = current == code;
            let state = self.state.clone();
            let code_owned = code.to_string();

            options = options.child(
                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded_full()
                    .bg(if is_active {
                        theme.element_selected
                    } else {
                        theme.bg_subtle
                    })
                    .border_1()
                    .border_color(if is_active {
                        theme.element_selected
                    } else {
                        theme.border_subtle
                    })
                    .text_size(px(12.0))
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
                    .cursor_pointer()
                    .child(t!(name_key).to_string())
                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                        let settings = {
                            let mut s = state.borrow_mut();
                            s.settings.language = code_owned.clone();
                            crate::i18n::apply_locale(&s.settings.language);
                            s.settings.clone()
                        };
                        persist_settings(&settings);
                        window.refresh();
                    }),
            );
        }

        card.child(options)
    }

    /// Render theme selector card with pill-style options (System / Light / Dark)
    pub(super) fn render_theme_selector(&self, current: AppTheme, theme: &Theme) -> Div {
        const THEME_OPTIONS: &[(AppTheme, &str)] = &[
            (AppTheme::System, "theme.system"),
            (AppTheme::Light, "theme.light"),
            (AppTheme::Dark, "theme.dark"),
        ];

        let card = render_card(theme)
            .flex_col()
            .px(px(14.0))
            .py(px(10.0))
            .gap(px(2.0))
            .child(
                div()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(t!("settings.theme").to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(relative(1.4))
                            .text_color(theme.text_secondary)
                            .child(t!("settings.theme.desc").to_string()),
                    ),
            );

        let mut options = div().flex().flex_wrap().gap(px(6.0)).mt(px(8.0));
        for &(variant, name_key) in THEME_OPTIONS {
            let is_active = current == variant;
            let state = self.state.clone();

            options = options.child(
                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded_full()
                    .bg(if is_active {
                        theme.element_selected
                    } else {
                        theme.bg_subtle
                    })
                    .border_1()
                    .border_color(if is_active {
                        theme.element_selected
                    } else {
                        theme.border_subtle
                    })
                    .text_size(px(12.0))
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
                    .cursor_pointer()
                    .child(t!(name_key).to_string())
                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                        let settings = {
                            let mut s = state.borrow_mut();
                            s.settings.theme = variant;
                            s.settings.clone()
                        };
                        persist_settings(&settings);
                        window.refresh();
                    }),
            );
        }

        card.child(options)
    }
}
