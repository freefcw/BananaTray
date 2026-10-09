use crate::theme::{monospace_font_family, Theme};
use fc_ui::components::input_state::{wire_actions as wire_input_actions, InputState};
use fc_ui::components::textarea_state::{wire_actions as wire_textarea_actions, TextareaState};
use gpui::{
    div, hsla, px, relative, App, Div, Entity, FontWeight, Hsla, InteractiveElement, MouseButton,
    MouseDownEvent, ParentElement, Pixels, Stateful, Styled, Window,
};
use rust_i18n::t;

/// Provider 设置区的卡片外壳。Token 面板和自定义 provider 的编辑区共用这一套容器规格，
/// 保证设置区在两类 provider 下看起来一致。
pub(in crate::ui::settings_window) fn render_settings_card(theme: &Theme) -> Div {
    div()
        .flex_col()
        .w_full()
        .rounded(px(12.0))
        .bg(theme.bg.card_inner)
        .border_1()
        .border_color(theme.border.strong)
        .px(px(20.0))
        .py(px(20.0))
        .gap(px(14.0))
}

/// 设置卡片标题。
pub(in crate::ui::settings_window) fn render_settings_card_title(
    title: &str,
    theme: &Theme,
) -> Div {
    div()
        .text_size(px(15.0))
        .font_weight(FontWeight::BOLD)
        .text_color(theme.text.primary)
        .child(title.to_string())
}

/// 表单字段的共享布局规格。
#[derive(Clone)]
pub(in crate::ui::settings_window) struct FormFieldSpec<'a> {
    pub label: &'a str,
    pub hint: Option<&'a str>,
    pub is_focused: bool,
    pub margin_top: Pixels,
    /// 字段级校验错误文案（来自 fc-ui state 的 `validation_error`，经本地化映射）。
    /// `Some` 时输入框描边转为错误色，并在字段下方展示该文案。
    pub error: Option<String>,
}

/// 将 fc-ui state 校验错误映射为用户可见的本地化文案。
///
/// fc-ui 内置规则（required / min_length 等）的错误 message 是硬编码英文，
/// 这里按已知文案映射到 i18n key；`custom_validator` 返回的 message
/// 在构造时已本地化，原样透传。fc-ui 未来改动内置文案时最坏情况是英文透出，
/// 不会崩溃。
fn localize_validation_message(message: &str) -> String {
    // fc-ui validation::check_required_empty 的固定文案
    if message == "This field is required" {
        t!("common.validation.required").to_string()
    } else {
        message.to_string()
    }
}

/// 读取单行输入 state 的当前校验错误（已本地化）。
pub(in crate::ui::settings_window) fn input_field_error(
    state: &Entity<InputState>,
    cx: &App,
) -> Option<String> {
    state
        .read(cx)
        .validation_error
        .as_ref()
        .map(|e| localize_validation_message(e.message.as_ref()))
}

/// 读取多行输入 state 的当前校验错误（已本地化）。
pub(in crate::ui::settings_window) fn textarea_field_error(
    state: &Entity<TextareaState>,
    cx: &App,
) -> Option<String> {
    state
        .read(cx)
        .validation_error
        .as_ref()
        .map(|e| localize_validation_message(e.message.as_ref()))
}

/// 字段下方的校验错误文案行。
fn render_field_error(message: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(11.0))
        .text_color(theme.status.error)
        .child(message.to_string())
}

/// 输入框描边色：校验错误 > 聚焦高亮 > 默认。
fn field_border_color(has_error: bool, is_focused: bool, theme: &Theme) -> Hsla {
    if has_error {
        theme.status.error
    } else if is_focused {
        theme.text.accent
    } else {
        theme.border.strong
    }
}

fn render_field_label(label: &str, hint: Option<&str>, theme: &Theme) -> Div {
    let mut col = div().flex_col().gap(px(2.0)).child(
        div()
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text.primary)
            .child(label.to_string()),
    );

    if let Some(hint_text) = hint {
        col = col.child(
            div()
                .text_size(px(11.0))
                .text_color(theme.text.muted)
                .child(hint_text.to_string()),
        );
    }

    col
}

pub(in crate::ui::settings_window) fn render_input_field(
    field: FormFieldSpec<'_>,
    input_entity: &Entity<InputState>,
    theme: &Theme,
    window: &mut Window,
    cx: &App,
) -> Div {
    let has_error = field.error.is_some();
    let mut col = div()
        .flex_col()
        .gap(px(6.0))
        .mt(field.margin_top)
        .child(render_field_label(field.label, field.hint, theme))
        .child(render_input_box(
            field.is_focused,
            has_error,
            input_entity,
            theme,
            window,
            cx,
        ));
    if let Some(error) = field.error.as_deref() {
        col = col.child(render_field_error(error, theme));
    }
    col
}

/// 不带标签的单行输入框外壳（聚焦高亮 + 完整键盘编辑），供自定义标签布局复用。
/// 键盘 wiring 走 fc-ui 官方 `wire_actions`（自带 key_context / track_focus /
/// 全部编辑 action），这里只负责外壳样式与点击聚焦。
pub(in crate::ui::settings_window) fn render_input_box(
    is_focused: bool,
    has_error: bool,
    input_entity: &Entity<InputState>,
    theme: &Theme,
    window: &mut Window,
    cx: &App,
) -> Stateful<Div> {
    let focus_handle = input_entity.read(cx).focus_handle(cx);
    wire_input_actions(input_entity, window, cx)
        .w_full()
        .flex()
        .items_center()
        .px(px(12.0))
        .py(px(8.0))
        .h(px(36.0))
        .rounded(px(8.0))
        .bg(theme.bg.card)
        .border_1()
        .border_color(field_border_color(has_error, is_focused, theme))
        .text_size(px(13.0))
        .text_color(theme.text.primary)
        .on_mouse_down(MouseButton::Left, {
            let handle = focus_handle.clone();
            move |_, window, _| handle.focus(window)
        })
        .child(div().flex_1().overflow_hidden().child(input_entity.clone()))
}

pub(super) fn render_textarea_field(
    field: FormFieldSpec<'_>,
    textarea_entity: &Entity<TextareaState>,
    theme: &Theme,
    window: &mut Window,
    cx: &App,
) -> Div {
    let has_error = field.error.is_some();
    // 官方 wiring 自带 key_context / track_focus / track_scroll / 点击聚焦 / 全部编辑 action
    let textarea_div = wire_textarea_actions(textarea_entity, window, cx)
        .w_full()
        .px(px(12.0))
        .py(px(8.0))
        .min_h(px(72.0))
        .max_h(px(140.0))
        .rounded(px(8.0))
        .bg(theme.bg.card)
        .border_1()
        .border_color(field_border_color(has_error, field.is_focused, theme))
        .text_size(px(13.0))
        .text_color(theme.text.primary);

    let mut col = div()
        .flex_col()
        .gap(px(6.0))
        .mt(field.margin_top)
        .child(render_field_label(field.label, field.hint, theme))
        .child(textarea_div.child(textarea_entity.clone()));
    if let Some(error) = field.error.as_deref() {
        col = col.child(render_field_error(error, theme));
    }
    col
}

/// 代码编辑专用 textarea：等宽字体、更大的编辑区、附带 cf_hint 提示。
pub(super) fn render_code_field(
    field: FormFieldSpec<'_>,
    textarea_entity: &Entity<TextareaState>,
    cf_hint: &str,
    theme: &Theme,
    window: &mut Window,
    cx: &App,
) -> Div {
    let has_error = field.error.is_some();
    let textarea_div = wire_textarea_actions(textarea_entity, window, cx)
        .w_full()
        .px(px(12.0))
        .py(px(10.0))
        .min_h(px(260.0))
        .max_h(px(420.0))
        .rounded(px(8.0))
        .bg(theme.bg.card)
        .border_1()
        .border_color(field_border_color(has_error, field.is_focused, theme))
        .font_family(monospace_font_family())
        .text_size(px(12.0))
        .text_color(theme.text.primary);

    let mut col = div()
        .flex_col()
        .gap(px(6.0))
        .mt(field.margin_top)
        .child(render_field_label(field.label, field.hint, theme))
        .child(textarea_div.child(textarea_entity.clone()));
    if let Some(error) = field.error.as_deref() {
        col = col.child(render_field_error(error, theme));
    }
    col.child(
        div()
            .text_size(px(11.0))
            .text_color(theme.text.muted)
            .child(cf_hint.to_string()),
    )
}

pub(super) fn render_readonly_field(
    label: &str,
    hint: Option<&str>,
    value: &str,
    margin_top: Pixels,
    theme: &Theme,
) -> Div {
    let muted = theme.text.muted;
    div()
        .flex_col()
        .gap(px(6.0))
        .mt(margin_top)
        .child(render_field_label(label, hint, theme))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .px(px(12.0))
                .py(px(8.0))
                .h(px(36.0))
                .rounded(px(8.0))
                .bg(hsla(0.0, 0.0, 0.2, 0.5))
                .border_1()
                .border_color(theme.border.subtle)
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(muted)
                        .overflow_hidden()
                        .child(value.to_string()),
                ),
        )
}

pub(super) fn render_confirm_cancel_buttons(
    confirm_label: &str,
    cancel_label: &str,
    on_confirm: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    theme: &Theme,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(
            div()
                .h(px(24.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(4.0))
                .rounded(px(6.0))
                .bg(theme.status.error)
                .cursor_pointer()
                .hover(|s| s.opacity(0.85))
                .child(crate::ui::widgets::render_svg_icon(
                    "src/icons/trash.svg",
                    px(12.0),
                    gpui::white(),
                ))
                .child(
                    div()
                        .text_size(px(11.0))
                        .line_height(relative(1.3))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(gpui::white())
                        .child(confirm_label.to_string()),
                )
                .on_mouse_down(MouseButton::Left, on_confirm),
        )
        .child(
            div()
                .h(px(24.0))
                .px(px(6.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .bg(theme.bg.subtle)
                .cursor_pointer()
                .hover(|s| s.opacity(0.8))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.text.muted)
                        .child(cancel_label.to_string()),
                )
                .on_mouse_down(MouseButton::Left, on_cancel),
        )
}

#[cfg(test)]
mod tests {
    use super::localize_validation_message;
    use crate::i18n::test_locale_guard;

    /// fc-ui 内置 required 规则的错误文案映射到本地化 key；
    /// 该映射按字面量匹配 fc-ui 源码里的固定英文 message。
    #[test]
    fn maps_fc_ui_required_message_to_localized_text() {
        let _guard = test_locale_guard("en");
        assert_eq!(
            localize_validation_message("This field is required"),
            "This field is required."
        );
        rust_i18n::set_locale("zh-CN");
        assert_eq!(
            localize_validation_message("This field is required"),
            "该字段为必填项。"
        );
    }

    /// custom_validator 的 message 构造时已本地化，原样透传。
    #[test]
    fn passes_custom_validator_messages_through() {
        let _guard = test_locale_guard("en");
        assert_eq!(
            localize_validation_message("Credit ratio must be a positive number."),
            "Credit ratio must be a positive number."
        );
    }
}
