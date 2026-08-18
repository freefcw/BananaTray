use super::SettingsView;
use crate::application::{AppAction, AvailableProviderItem};
use crate::runtime;
use crate::theme::Theme;
use gpui::{
    div, hsla, px, relative, svg, AnyElement, Context, Div, Entity, FontWeight, InteractiveElement,
    IntoElement, MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
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
            .child(render_picker_header(&entity, theme))
            .child(render_available_providers(available, &entity, theme))
            .child(render_custom_provider_entries(&entity, theme));

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

fn render_picker_header(entity: &Entity<SettingsView>, theme: &Theme) -> Div {
    let cancel_entity = entity.clone();
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
        .child(
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
                    cancel_entity.update(cx, |view, cx| {
                        runtime::dispatch_in_context(&view.state, AppAction::CancelAddProvider, cx);
                    });
                }),
        )
}

fn render_available_providers(
    available: &[AvailableProviderItem],
    entity: &Entity<SettingsView>,
    theme: &Theme,
) -> AnyElement {
    if available.is_empty() {
        return div()
            .flex()
            .items_center()
            .justify_center()
            .py(px(40.0))
            .text_size(px(14.0))
            .text_color(theme.text.muted)
            .child(t!("provider.all_added").to_string())
            .into_any_element();
    }

    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .children(
            available
                .iter()
                .map(|item| render_available_provider(item, entity, theme)),
        )
        .into_any_element()
}

fn render_available_provider(
    item: &AvailableProviderItem,
    entity: &Entity<SettingsView>,
    theme: &Theme,
) -> AnyElement {
    let provider_id = item.id.clone();
    let item_entity = entity.clone();
    let background = theme.bg.subtle;

    div()
        .id(SharedString::from(format!("add-provider-{}", item.id)))
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(14.0))
        .py(px(12.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(theme.border.subtle)
        .cursor_pointer()
        .hover(move |style| style.bg(background))
        .child(
            svg()
                .path(SharedString::from(item.icon.clone()))
                .size(px(22.0))
                .text_color(theme.text.muted),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(14.0))
                .line_height(relative(1.3))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text.primary)
                .child(item.display_name.clone()),
        )
        .child(
            svg()
                .path("src/icons/plus.svg")
                .size(px(14.0))
                .text_color(theme.text.muted),
        )
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            item_entity.update(cx, |view, cx| {
                view.clear_token_input();
                runtime::dispatch_in_context(
                    &view.state,
                    AppAction::AddProviderToSidebar(provider_id.clone()),
                    cx,
                );
            });
        })
        .into_any_element()
}

fn render_custom_provider_entries(entity: &Entity<SettingsView>, theme: &Theme) -> Div {
    div()
        .flex_col()
        .mt(px(12.0))
        .pt(px(12.0))
        .border_t_1()
        .border_color(theme.border.subtle)
        .child(render_custom_provider_entry(
            "add-provider-newapi",
            "src/icons/provider-custom.svg",
            t!("newapi.add_button").to_string(),
            false,
            entity,
            theme,
            || AppAction::EnterAddNewApi,
        ))
        .child(render_custom_provider_entry(
            "add-provider-script",
            "src/icons/advanced.svg",
            t!("script_provider.add_button").to_string(),
            true,
            entity,
            theme,
            || AppAction::EnterAddScriptProvider,
        ))
}

fn render_custom_provider_entry<F>(
    id: &'static str,
    icon: &'static str,
    label: String,
    add_top_margin: bool,
    entity: &Entity<SettingsView>,
    theme: &Theme,
    action: F,
) -> AnyElement
where
    F: Fn() -> AppAction + 'static,
{
    let entry_entity = entity.clone();
    let accent = theme.text.accent;
    let background = theme.bg.subtle;
    let mut entry = div()
        .id(id)
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
        .hover(move |style| style.border_color(accent).bg(background))
        .child(svg().path(icon).size(px(22.0)).text_color(theme.text.muted))
        .child(
            div()
                .flex_1()
                .text_size(px(14.0))
                .line_height(relative(1.3))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text.muted)
                .child(label),
        )
        .child(
            svg()
                .path("src/icons/plus.svg")
                .size(px(14.0))
                .text_color(theme.text.muted),
        );
    if add_top_margin {
        entry = entry.mt(px(8.0));
    }

    entry
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            entry_entity.update(cx, |view, cx| {
                view.clear_token_input();
                runtime::dispatch_in_context(&view.state, action(), cx);
            });
        })
        .into_any_element()
}
