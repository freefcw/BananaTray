use crate::theme::Theme;
use crate::ui::widgets::register_input_actions;
use adabraka_ui::components::input_state::InputState;
use adabraka_ui::components::textarea_state::TextareaState;
use gpui::{
    div, hsla, px, relative, App, Div, Entity, Focusable, FontWeight, InteractiveElement,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Stateful, StatefulInteractiveElement,
    Styled, Window,
};

/// Provider 设置区的卡片外壳。Token 面板和自定义 provider 的编辑区共用这一套容器规格，
/// 保证设置区在两类 provider 下看起来一致。
pub(super) fn render_settings_card(theme: &Theme) -> Div {
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
pub(super) fn render_settings_card_title(title: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(15.0))
        .font_weight(FontWeight::BOLD)
        .text_color(theme.text.primary)
        .child(title.to_string())
}

/// 表单字段的共享布局规格。
#[derive(Clone, Copy)]
pub(super) struct FormFieldSpec<'a> {
    pub id: &'static str,
    pub label: &'a str,
    pub hint: Option<&'a str>,
    pub is_focused: bool,
    pub margin_top: Pixels,
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

pub(super) fn render_input_field(
    field: FormFieldSpec<'_>,
    input_entity: &Entity<InputState>,
    theme: &Theme,
    window: &mut Window,
    cx: &App,
) -> Div {
    let focus_handle = input_entity.read(cx).focus_handle(cx);
    let input_div = div()
        .id(field.id)
        .key_context("Input")
        .track_focus(&focus_handle)
        .w_full()
        .flex()
        .items_center()
        .px(px(12.0))
        .py(px(8.0))
        .h(px(36.0))
        .rounded(px(8.0))
        .bg(theme.bg.card)
        .border_1()
        .border_color(if field.is_focused {
            theme.text.accent
        } else {
            theme.border.strong
        })
        .text_size(px(13.0))
        .text_color(theme.text.primary)
        .on_mouse_down(MouseButton::Left, {
            let handle = focus_handle.clone();
            move |_, window, _| handle.focus(window)
        });

    let input_div = register_input_actions(input_div, input_entity, window);

    div()
        .flex_col()
        .gap(px(6.0))
        .mt(field.margin_top)
        .child(render_field_label(field.label, field.hint, theme))
        .child(input_div.child(div().flex_1().overflow_hidden().child(input_entity.clone())))
}

pub(super) fn render_textarea_field(
    field: FormFieldSpec<'_>,
    textarea_entity: &Entity<TextareaState>,
    theme: &Theme,
    window: &mut Window,
    cx: &App,
) -> Div {
    let focus_handle = textarea_entity.read(cx).focus_handle(cx);

    let textarea_div = div()
        .id(field.id)
        .key_context("Textarea")
        .track_focus(&focus_handle)
        .w_full()
        .px(px(12.0))
        .py(px(8.0))
        .min_h(px(72.0))
        .max_h(px(140.0))
        .rounded(px(8.0))
        .bg(theme.bg.card)
        .border_1()
        .border_color(if field.is_focused {
            theme.text.accent
        } else {
            theme.border.strong
        })
        .text_size(px(13.0))
        .text_color(theme.text.primary)
        .overflow_y_scroll()
        .on_mouse_down(MouseButton::Left, {
            let handle = focus_handle.clone();
            move |_, window, _| handle.focus(window)
        });

    let textarea_div = register_textarea_actions(textarea_div, textarea_entity, window);

    div()
        .flex_col()
        .gap(px(6.0))
        .mt(field.margin_top)
        .child(render_field_label(field.label, field.hint, theme))
        .child(textarea_div.child(textarea_entity.clone()))
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
    let focus_handle = textarea_entity.read(cx).focus_handle(cx);

    let textarea_div = div()
        .id(field.id)
        .key_context("Textarea")
        .track_focus(&focus_handle)
        .w_full()
        .px(px(12.0))
        .py(px(10.0))
        .min_h(px(260.0))
        .max_h(px(420.0))
        .rounded(px(8.0))
        .bg(theme.bg.card)
        .border_1()
        .border_color(if field.is_focused {
            theme.text.accent
        } else {
            theme.border.strong
        })
        .font_family("SF Mono")
        .text_size(px(12.0))
        .text_color(theme.text.primary)
        .overflow_y_scroll()
        .on_mouse_down(MouseButton::Left, {
            let handle = focus_handle.clone();
            move |_, window, _| handle.focus(window)
        });

    let textarea_div = register_textarea_actions(textarea_div, textarea_entity, window);

    div()
        .flex_col()
        .gap(px(6.0))
        .mt(field.margin_top)
        .child(render_field_label(field.label, field.hint, theme))
        .child(textarea_div.child(textarea_entity.clone()))
        .child(
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

pub(super) fn register_textarea_actions(
    div: Stateful<Div>,
    entity: &Entity<TextareaState>,
    window: &mut Window,
) -> Stateful<Div> {
    div.on_action(window.listener_for(entity, TextareaState::backspace))
        .on_action(window.listener_for(entity, TextareaState::delete))
        .on_action(window.listener_for(entity, TextareaState::left))
        .on_action(window.listener_for(entity, TextareaState::right))
        .on_action(window.listener_for(entity, TextareaState::up))
        .on_action(window.listener_for(entity, TextareaState::down))
        .on_action(window.listener_for(entity, TextareaState::select_left))
        .on_action(window.listener_for(entity, TextareaState::select_right))
        .on_action(window.listener_for(entity, TextareaState::select_up))
        .on_action(window.listener_for(entity, TextareaState::select_down))
        .on_action(window.listener_for(entity, TextareaState::select_all))
        .on_action(window.listener_for(entity, TextareaState::home))
        .on_action(window.listener_for(entity, TextareaState::end))
        .on_action(window.listener_for(entity, TextareaState::copy))
        .on_action(window.listener_for(entity, TextareaState::cut))
        .on_action(window.listener_for(entity, TextareaState::paste))
        .on_action(window.listener_for(entity, TextareaState::enter))
        .on_action(window.listener_for(entity, TextareaState::shift_enter))
        .on_action(window.listener_for(entity, TextareaState::tab))
        .on_action(window.listener_for(entity, TextareaState::shift_tab))
        .on_action(window.listener_for(entity, TextareaState::escape))
        .on_action(window.listener_for(entity, TextareaState::word_left))
        .on_action(window.listener_for(entity, TextareaState::word_right))
        .on_action(window.listener_for(entity, TextareaState::select_word_left))
        .on_action(window.listener_for(entity, TextareaState::select_word_right))
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
