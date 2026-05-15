//! Custom Script Provider add/edit form.

use super::super::{ScriptProviderFormInputs, SettingsView};
use crate::application::AppAction;
use crate::models::{
    unique_script_provider_id, ProviderId, ScriptProviderConfig, ScriptProviderEditData,
    ScriptProviderTestResult, DEFAULT_SCRIPT_TIMEOUT_MS,
};
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::{register_input_actions, render_svg_icon};
use adabraka_ui::components::input_state::InputState;
use adabraka_ui::components::textarea_state::TextareaState;
use gpui::{
    div, hsla, px, App, Context, Div, Entity, Focusable, FontWeight, InteractiveElement,
    MouseButton, ParentElement, Pixels, Stateful, StatefulInteractiveElement, Styled, Window,
};
use rust_i18n::t;

fn render_label(label: &str, hint: Option<&str>, theme: &Theme) -> Div {
    let mut col = div().flex_col().gap(px(2.0)).child(
        div()
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text.primary)
            .child(label.to_string()),
    );
    if let Some(hint) = hint {
        col = col.child(
            div()
                .text_size(px(11.0))
                .text_color(theme.text.muted)
                .child(hint.to_string()),
        );
    }
    col
}

pub(in crate::ui::settings_window) struct ScriptProviderFormView<'a> {
    pub edit_data: Option<&'a ScriptProviderEditData>,
    pub test_result: Option<&'a ScriptProviderTestResult>,
    pub is_testing: bool,
}

impl ScriptProviderFormView<'_> {
    fn is_editing(&self) -> bool {
        self.edit_data.is_some()
    }
}

struct InputFieldView<'a> {
    id: &'static str,
    label: &'a str,
    hint: Option<&'a str>,
    input_entity: &'a Entity<InputState>,
    is_focused: bool,
    margin_top: Pixels,
}

fn render_input_field(
    field: InputFieldView<'_>,
    theme: &Theme,
    window: &mut Window,
    cx: &App,
) -> Div {
    let focus_handle = field.input_entity.read(cx).focus_handle(cx);
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
    let input_div = register_input_actions(input_div, field.input_entity, window);

    div()
        .flex_col()
        .gap(px(6.0))
        .mt(field.margin_top)
        .child(render_label(field.label, field.hint, theme))
        .child(
            input_div.child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(field.input_entity.clone()),
            ),
        )
}

fn render_readonly_field(
    label: &str,
    hint: Option<&str>,
    value: &str,
    margin_top: Pixels,
    theme: &Theme,
) -> Div {
    div()
        .flex_col()
        .gap(px(6.0))
        .mt(margin_top)
        .child(render_label(label, hint, theme))
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
                .text_size(px(13.0))
                .text_color(theme.text.muted)
                .child(value.to_string()),
        )
}

fn render_code_field(
    textarea_entity: &Entity<TextareaState>,
    is_focused: bool,
    theme: &Theme,
    window: &mut Window,
    cx: &App,
) -> Div {
    let focus_handle = textarea_entity.read(cx).focus_handle(cx);
    let textarea_div = div()
        .id("script-provider-code")
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
        .border_color(if is_focused {
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
        .mt(px(16.0))
        .child(render_label(
            &t!("script_provider.field.script"),
            Some(&t!("script_provider.field.script.hint")),
            theme,
        ))
        .child(textarea_div.child(textarea_entity.clone()))
}

fn register_textarea_actions(
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

fn render_test_result(result: Option<&ScriptProviderTestResult>, theme: &Theme) -> Div {
    let Some(result) = result else {
        return div();
    };
    let color = if result.success {
        theme.status.success
    } else {
        theme.status.error
    };
    let preview = result.preview.as_ref().map(|p| {
        format!(
            "{}  {:.2} {}",
            p.label,
            p.remaining,
            if p.unit.is_empty() { "USD" } else { &p.unit }
        )
    });

    div()
        .flex_col()
        .gap(px(8.0))
        .mt(px(18.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(theme.border.subtle)
        .bg(theme.bg.subtle)
        .px(px(12.0))
        .py(px(10.0))
        .child(
            div()
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .child(if result.success {
                    t!("script_provider.test.ok").to_string()
                } else {
                    t!("script_provider.test.failed").to_string()
                }),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text.secondary)
                .child(preview.unwrap_or_else(|| result.message.clone())),
        )
        .child(
            div()
                .font_family("SF Mono")
                .text_size(px(11.0))
                .text_color(theme.text.muted)
                .child(format!("stdout: {}", compact_text(&result.stdout))),
        )
        .child(
            div()
                .font_family("SF Mono")
                .text_size(px(11.0))
                .text_color(theme.text.muted)
                .child(format!("stderr: {}", compact_text(&result.stderr))),
        )
}

fn compact_text(text: &str) -> String {
    let trimmed = text.trim();
    let mut chars = trimmed.chars();
    let compact = chars.by_ref().take(220).collect::<String>();
    if chars.next().is_some() {
        format!("{}...", compact)
    } else if trimmed.is_empty() {
        "-".to_string()
    } else {
        compact
    }
}

#[cfg(test)]
mod tests {
    use super::compact_text;

    #[test]
    fn compact_text_truncates_on_char_boundary() {
        let text = "余".repeat(260);
        let compact = compact_text(&text);

        assert!(compact.ends_with("..."));
        assert_eq!(compact.trim_end_matches("...").chars().count(), 220);
    }

    #[test]
    fn compact_text_empty_falls_back_to_dash() {
        assert_eq!(compact_text(" \n "), "-");
    }
}

impl SettingsView {
    fn ensure_script_provider_inputs(
        &mut self,
        edit_data: Option<&ScriptProviderEditData>,
        cx: &mut Context<Self>,
    ) {
        if self.script_provider_inputs.is_some() {
            return;
        }
        self.script_provider_inputs = Some(match edit_data {
            Some(data) => ScriptProviderFormInputs::new_edit(data, cx),
            None => ScriptProviderFormInputs::new_add(cx),
        });
    }

    pub(in crate::ui::settings_window) fn clear_script_provider_inputs(&mut self) {
        self.script_provider_inputs = None;
    }

    pub(in crate::ui::settings_window) fn render_script_provider_form(
        &mut self,
        form: ScriptProviderFormView<'_>,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        self.ensure_script_provider_inputs(form.edit_data, cx);
        let inputs = self.script_provider_inputs.as_ref().unwrap();
        let focused = inputs.focused_states(window, cx);
        let title = if form.is_editing() {
            t!("script_provider.edit_title").to_string()
        } else {
            t!("script_provider.add_title").to_string()
        };
        let provider_id_display = if form.is_editing() {
            inputs.provider_id.read(cx).content().to_string()
        } else {
            let name = inputs.name.read(cx).content().trim().to_string();
            self.unique_script_provider_id_for_name(&name)
        };

        let inner = div()
            .flex_col()
            .px(px(24.0))
            .pt(px(20.0))
            .pb(px(60.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(
                        div()
                            .w(px(48.0))
                            .h(px(48.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(14.0))
                            .bg(theme.bg.subtle)
                            .border_1()
                            .border_color(theme.border.subtle)
                            .child(render_svg_icon(
                                "src/icons/advanced.svg",
                                px(28.0),
                                theme.text.accent,
                            )),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.text.primary)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(theme.text.muted)
                                    .child(t!("script_provider.subtitle").to_string()),
                            ),
                    ),
            )
            .child(render_input_field(
                InputFieldView {
                    id: "script-provider-name",
                    label: &t!("script_provider.field.name"),
                    hint: Some(&t!("script_provider.field.name.placeholder")),
                    input_entity: &inputs.name,
                    is_focused: focused[0],
                    margin_top: px(24.0),
                },
                theme,
                window,
                cx,
            ))
            .child(render_readonly_field(
                &t!("script_provider.field.provider_id"),
                Some(&t!("script_provider.field.provider_id.readonly_hint")),
                &provider_id_display,
                px(16.0),
                theme,
            ))
            .child(render_input_field(
                InputFieldView {
                    id: "script-provider-interpreter",
                    label: &t!("script_provider.field.interpreter"),
                    hint: Some(&t!("script_provider.field.interpreter.hint")),
                    input_entity: &inputs.interpreter,
                    is_focused: focused[2],
                    margin_top: px(16.0),
                },
                theme,
                window,
                cx,
            ))
            .child(render_input_field(
                InputFieldView {
                    id: "script-provider-timeout",
                    label: &t!("script_provider.field.timeout"),
                    hint: Some(&t!("script_provider.field.timeout.hint")),
                    input_entity: &inputs.timeout,
                    is_focused: focused[3],
                    margin_top: px(16.0),
                },
                theme,
                window,
                cx,
            ))
            .child(render_code_field(
                &inputs.script,
                focused[4],
                theme,
                window,
                cx,
            ))
            .child(render_test_result(form.test_result, theme))
            .child(self.render_script_provider_buttons(theme, form.is_testing, cx));

        div().flex_col().flex_1().h_full().overflow_hidden().child(
            div()
                .id("script-provider-form-scroll")
                .flex_col()
                .h_full()
                .overflow_y_scroll()
                .child(inner),
        )
    }

    fn collect_script_provider_config(&self, cx: &App) -> Option<ScriptProviderConfig> {
        let inputs = self.script_provider_inputs.as_ref()?;
        let display_name = inputs.name.read(cx).content().trim().to_string();
        let is_editing = self
            .state
            .borrow()
            .session
            .settings_ui
            .modal
            .script_provider_edit_data()
            .is_some();
        let provider_id = if is_editing {
            inputs.provider_id.read(cx).content().trim().to_string()
        } else {
            self.unique_script_provider_id_for_name(&display_name)
        };
        let interpreter = inputs.interpreter.read(cx).content().trim().to_string();
        let timeout_secs = inputs
            .timeout
            .read(cx)
            .content()
            .trim()
            .parse::<u64>()
            .ok()
            .unwrap_or(DEFAULT_SCRIPT_TIMEOUT_MS / 1000);
        let script = inputs.script.read(cx).content().to_string();

        if display_name.is_empty()
            || provider_id.is_empty()
            || interpreter.is_empty()
            || script.trim().is_empty()
        {
            return None;
        }

        Some(ScriptProviderConfig {
            display_name,
            provider_id,
            interpreter,
            timeout_ms: timeout_secs.saturating_mul(1000),
            script,
        })
    }

    fn unique_script_provider_id_for_name(&self, display_name: &str) -> String {
        let state = self.state.borrow();
        unique_script_provider_id(display_name, |id| {
            let provider_id = ProviderId::Custom(id.to_string());
            let key = provider_id.id_key();

            state
                .session
                .provider_store
                .custom_provider_ids()
                .iter()
                .any(|existing| existing.id_key() == key)
                || state
                    .session
                    .settings
                    .provider
                    .enabled_providers
                    .contains_key(&key)
                || state
                    .session
                    .settings
                    .provider
                    .provider_order
                    .contains(&key)
                || state
                    .session
                    .settings
                    .provider
                    .sidebar_providers
                    .contains(&key)
        })
    }

    fn render_script_provider_buttons(
        &self,
        theme: &Theme,
        is_testing: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let state_cancel = self.state.clone();
        let state_test = self.state.clone();
        let state_save = self.state.clone();
        let view = cx.entity().clone();
        let view_test = view.clone();

        div()
            .flex()
            .gap(px(10.0))
            .mt(px(28.0))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(14.0))
                    .py(px(10.0))
                    .rounded(px(8.0))
                    .bg(theme.bg.subtle)
                    .border_1()
                    .border_color(theme.border.strong)
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text.primary)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.9))
                    .child(t!("script_provider.cancel").to_string())
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        runtime::dispatch_in_window(
                            &state_cancel,
                            AppAction::CancelAddScriptProvider,
                            window,
                            cx,
                        );
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(14.0))
                    .py(px(10.0))
                    .rounded(px(8.0))
                    .bg(theme.bg.subtle)
                    .border_1()
                    .border_color(theme.text.accent)
                    .opacity(if is_testing { 0.55 } else { 1.0 })
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text.accent)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.9))
                    .child(if is_testing {
                        t!("script_provider.testing").to_string()
                    } else {
                        t!("script_provider.test").to_string()
                    })
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        if is_testing {
                            return;
                        }
                        let config = view_test.update(cx, |view: &mut Self, cx| {
                            view.collect_script_provider_config(cx)
                        });
                        if let Some(config) = config {
                            runtime::dispatch_in_window(
                                &state_test,
                                AppAction::TestScriptProvider(config),
                                window,
                                cx,
                            );
                        }
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(14.0))
                    .py(px(10.0))
                    .rounded(px(8.0))
                    .bg(theme.text.accent)
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.element.active)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.9))
                    .child(t!("script_provider.save").to_string())
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        let config = view.update(cx, |view: &mut Self, cx| {
                            view.collect_script_provider_config(cx)
                        });
                        if let Some(config) = config {
                            runtime::dispatch_in_window(
                                &state_save,
                                AppAction::SubmitScriptProvider(config),
                                window,
                                cx,
                            );
                        }
                    }),
            )
    }
}
