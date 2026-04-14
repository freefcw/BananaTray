use crate::application::{AppAction, SettingChange};
use crate::runtime;
use crate::theme::Theme;
use crate::ui::AppView;
use gpui::{
    div, px, AnyElement, App, Context, Div, FontWeight, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, Styled, Window,
};
use rust_i18n::t;

const AUTO_HIDE_ICON: &str = "src/icons/display.svg";
const ACCOUNT_INFO_ICON: &str = "src/icons/about.svg";
const OVERVIEW_ICON: &str = "src/icons/overview.svg";

impl AppView {
    pub(crate) fn render_settings_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.state.borrow().session.settings.clone();
        let theme = cx.global::<Theme>();

        let entity_auto_hide = cx.entity().clone();
        let entity_account_info = cx.entity().clone();
        let entity_overview = cx.entity().clone();

        div()
            .px(px(12.0))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            // Auto-hide toggle
            .child(self.render_settings_toggle_row(
                AUTO_HIDE_ICON,
                &t!("settings.auto_hide"),
                &t!("settings.auto_hide.desc"),
                settings.system.auto_hide_window,
                theme,
                move |_, _, cx| {
                    entity_auto_hide.update(cx, |view, cx| {
                        runtime::dispatch_in_context(
                            &view.state,
                            AppAction::UpdateSetting(SettingChange::ToggleAutoHideWindow),
                            cx,
                        );
                    });
                },
            ))
            // Show account info toggle
            .child(self.render_settings_toggle_row(
                ACCOUNT_INFO_ICON,
                &t!("settings.show_account_info"),
                &t!("settings.show_account_info.desc"),
                settings.display.show_account_info,
                theme,
                move |_, _, cx| {
                    entity_account_info.update(cx, |view, cx| {
                        runtime::dispatch_in_context(
                            &view.state,
                            AppAction::UpdateSetting(SettingChange::ToggleShowAccountInfo),
                            cx,
                        );
                    });
                },
            ))
            // Overview toggle
            .child(self.render_settings_toggle_row(
                OVERVIEW_ICON,
                &t!("settings.show_overview"),
                &t!("settings.show_overview.desc"),
                settings.display.show_overview,
                theme,
                move |_, _, cx| {
                    entity_overview.update(cx, |view, cx| {
                        runtime::dispatch_in_context(
                            &view.state,
                            AppAction::UpdateSetting(SettingChange::ToggleShowOverview),
                            cx,
                        );
                    });
                },
            ))
            .into_any_element()
    }

    /// 设置页的 toggle 行通用组件
    fn render_settings_toggle_row(
        &self,
        icon: &'static str,
        title: &str,
        desc: &str,
        enabled: bool,
        theme: &Theme,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(14.0))
            .bg(theme.bg.card)
            .border_1()
            .border_color(theme.border.subtle)
            .px(px(14.0))
            .py(px(12.0))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .child(crate::ui::widgets::render_footer_glyph(icon, theme))
                    .child(
                        div()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text.primary)
                                    .child(title.to_string()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.text.secondary)
                                    .child(desc.to_string()),
                            ),
                    ),
            )
            .child(self.render_toggle_switch_small(enabled, theme))
            .on_mouse_down(MouseButton::Left, handler)
    }
}
