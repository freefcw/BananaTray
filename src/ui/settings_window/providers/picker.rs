use super::SettingsView;
use crate::application::{AppAction, AvailableProviderItem};
use crate::runtime;
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;

impl SettingsView {
    /// 渲染 Provider 选择面板（右侧面板，添加 Provider 时展示）
    pub(in crate::ui::settings_window) fn render_provider_picker(
        &self,
        available: &[AvailableProviderItem],
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let entity = cx.entity().clone();

        let inner = div()
            .flex_col()
            .px(px(24.0))
            .pt(px(20.0))
            .pb(px(60.0))
            .child(
                // 标题栏
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(20.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text.primary)
                            .child(t!("provider.picker_title").to_string()),
                    )
                    .child({
                        let entity_cancel = entity.clone();
                        div()
                            .id("provider-picker-cancel")
                            .cursor_pointer()
                            .px(px(12.0))
                            .py(px(6.0))
                            .rounded(px(6.0))
                            .text_size(px(13.0))
                            .text_color(theme.text.muted)
                            .hover(|style| style.bg(theme.bg.subtle))
                            .child(t!("provider.cancel").to_string())
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                entity_cancel.update(cx, |view, cx| {
                                    runtime::dispatch_in_context(
                                        &view.state,
                                        AppAction::CancelAddProvider,
                                        cx,
                                    );
                                });
                            })
                    }),
            )
            // 内置 Provider 列表（或空状态提示）
            .child(if available.is_empty() {
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .py(px(40.0))
                    .text_size(px(14.0))
                    .text_color(theme.text.muted)
                    .child(t!("provider.all_added").to_string())
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(available.iter().map(|item| {
                        let id = item.id.clone();
                        let entity_item = entity.clone();
                        let item_icon = item.icon.clone();
                        let item_name = item.display_name.clone();
                        let text_color = theme.text.primary;
                        let text_muted = theme.text.muted;
                        let bg_hover = theme.bg.subtle;
                        let border_color = theme.border.subtle;

                        div()
                            .id(SharedString::from(format!("add-provider-{}", item.id)))
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(14.0))
                            .py(px(12.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(border_color)
                            .cursor_pointer()
                            .hover(|style| style.bg(bg_hover))
                            .child(
                                svg()
                                    .path(SharedString::from(item_icon))
                                    .size(px(22.0))
                                    .text_color(text_muted),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(text_color)
                                    .child(item_name),
                            )
                            .child(
                                svg()
                                    .path("src/icons/plus.svg")
                                    .size(px(14.0))
                                    .text_color(text_muted),
                            )
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                entity_item.update(cx, |view, cx| {
                                    runtime::dispatch_in_context(
                                        &view.state,
                                        AppAction::AddProviderToSidebar(id.clone()),
                                        cx,
                                    );
                                });
                            })
                    }))
                    .into_any_element()
            })
            // ── 分割线 + NewAPI 中转站入口（始终可见）──
            .child(
                div()
                    .mt(px(12.0))
                    .pt(px(12.0))
                    .border_t_1()
                    .border_color(theme.border.subtle)
                    .child({
                        let entity_newapi = entity.clone();
                        let accent = theme.text.accent;
                        let muted = theme.text.muted;
                        let bg_hover = theme.bg.subtle;

                        div()
                            .id("add-provider-newapi")
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(14.0))
                            .py(px(12.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_dashed()
                            .border_color(hsla(0.0, 0.0, 0.3, 0.3))
                            .cursor_pointer()
                            .hover(move |style| style.border_color(accent).bg(bg_hover))
                            .child(
                                svg()
                                    .path("src/icons/provider-custom.svg")
                                    .size(px(22.0))
                                    .text_color(muted),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(muted)
                                    .child(t!("newapi.add_button").to_string()),
                            )
                            .child(
                                svg()
                                    .path("src/icons/plus.svg")
                                    .size(px(14.0))
                                    .text_color(muted),
                            )
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                entity_newapi.update(cx, |view, cx| {
                                    runtime::dispatch_in_context(
                                        &view.state,
                                        AppAction::EnterAddNewApi,
                                        cx,
                                    );
                                });
                            })
                    }),
            );

        div().flex_col().flex_1().h_full().overflow_hidden().child(
            div()
                .id("provider-picker-scroll")
                .flex_col()
                .h_full()
                .overflow_y_scroll()
                .child(inner),
        )
    }
}
