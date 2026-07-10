//! Custom Script Provider add/edit form.

use super::super::{FormInputsCache, ScriptProviderFormInputs, SettingsView};
use super::newapi_form::should_rebuild_form_inputs_cache;
use super::shared::{render_code_field, render_input_field, render_readonly_field, FormFieldSpec};
use crate::application::AppAction;
use crate::application::FormIdentity;
use crate::models::{
    unique_script_provider_id, ScriptProviderConfig, ScriptProviderEditData,
    ScriptProviderTestResult, DEFAULT_SCRIPT_TIMEOUT_MS,
};
use crate::theme::Theme;
use crate::ui::widgets::render_svg_icon;
use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, Context, Div, FontWeight, Hsla,
    InteractiveElement, IntoElement, MouseButton, ParentElement, StatefulInteractiveElement,
    Styled, Window,
};
use rust_i18n::t;

pub(in crate::ui::settings_window) struct ScriptProviderFormView<'a> {
    pub identity: FormIdentity,
    pub edit_data: Option<&'a ScriptProviderEditData>,
    pub test_result: Option<&'a ScriptProviderTestResult>,
    pub is_testing: bool,
}

impl ScriptProviderFormView<'_> {
    fn is_editing(&self) -> bool {
        self.edit_data.is_some()
    }
}

fn looks_like_cf_challenge(result: &ScriptProviderTestResult) -> bool {
    const SCAN_CHAR_LIMIT: usize = 24_000;
    let fields = [&result.message, &result.stdout, &result.stderr];

    const KEYWORDS: &[&str] = &[
        "cloudflare",
        "cf-ray",
        "cf_clearance",
        "__cf_bm",
        "challenge-platform",
        "just a moment",
        "checking your browser",
        "attention required",
        "cf-chl",
    ];

    fields.iter().any(|field| {
        let haystack = field
            .chars()
            .take(SCAN_CHAR_LIMIT)
            .collect::<String>()
            .to_ascii_lowercase();

        let has_cf_keyword = KEYWORDS.iter().any(|kw| haystack.contains(kw));
        let is_403_blocked = haystack.contains("403")
            && (haystack.contains("forbidden") || haystack.contains("blocked"));
        let is_503_unavailable = haystack.contains("503") && haystack.contains("unavailable");

        has_cf_keyword || is_403_blocked || is_503_unavailable
    })
}

fn should_show_cf_warning(result: &ScriptProviderTestResult) -> bool {
    !result.success && looks_like_cf_challenge(result)
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
        .when(should_show_cf_warning(result), |d| {
            d.child(
                div()
                    .text_size(px(11.5))
                    .text_color(theme.status.warning)
                    .child(t!("script_provider.test.cf_warning").to_string()),
            )
        })
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

fn render_script_provider_header(title: String, theme: &Theme) -> Div {
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
        )
}

fn script_form_button(label: String, background: Hsla, border: Option<Hsla>, text: Hsla) -> Div {
    let button = div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .px(px(14.0))
        .py(px(10.0))
        .rounded(px(8.0))
        .bg(background)
        .text_size(px(13.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(text)
        .cursor_pointer()
        .hover(|style| style.opacity(0.9))
        .child(label);
    match border {
        Some(color) => button.border_1().border_color(color),
        None => button,
    }
}

impl SettingsView {
    fn ensure_script_provider_inputs(
        &mut self,
        identity: FormIdentity,
        edit_data: Option<&ScriptProviderEditData>,
        cx: &mut Context<Self>,
    ) {
        let should_rebuild =
            should_rebuild_form_inputs_cache(self.script_provider_inputs.as_ref(), &identity);
        if should_rebuild {
            let inputs = match edit_data {
                Some(data) => ScriptProviderFormInputs::new_edit(data, cx),
                None => ScriptProviderFormInputs::new_add(cx),
            };
            self.script_provider_inputs = Some(FormInputsCache { identity, inputs });
        }
    }

    pub(in crate::ui::settings_window) fn render_script_provider_form(
        &mut self,
        form: ScriptProviderFormView<'_>,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        self.ensure_script_provider_inputs(form.identity.clone(), form.edit_data, cx);
        let inputs = &self.script_provider_inputs.as_ref().unwrap().inputs;
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
            .child(render_script_provider_header(title, theme))
            .child(render_input_field(
                FormFieldSpec {
                    id: "script-provider-name",
                    label: &t!("script_provider.field.name"),
                    hint: Some(&t!("script_provider.field.name.placeholder")),
                    is_focused: focused[0],
                    margin_top: px(24.0),
                },
                &inputs.name,
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
                FormFieldSpec {
                    id: "script-provider-interpreter",
                    label: &t!("script_provider.field.interpreter"),
                    hint: Some(&t!("script_provider.field.interpreter.hint")),
                    is_focused: focused[2],
                    margin_top: px(16.0),
                },
                &inputs.interpreter,
                theme,
                window,
                cx,
            ))
            .child(render_input_field(
                FormFieldSpec {
                    id: "script-provider-timeout",
                    label: &t!("script_provider.field.timeout"),
                    hint: Some(&t!("script_provider.field.timeout.hint")),
                    is_focused: focused[3],
                    margin_top: px(16.0),
                },
                &inputs.timeout,
                theme,
                window,
                cx,
            ))
            .child(render_code_field(
                FormFieldSpec {
                    id: "script-provider-code",
                    label: &t!("script_provider.field.script"),
                    hint: Some(&t!("script_provider.field.script.hint")),
                    is_focused: focused[4],
                    margin_top: px(16.0),
                },
                &inputs.script,
                &t!("script_provider.field.script.cf_hint"),
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
        let inputs = &inputs.inputs;
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
            || timeout_secs == 0
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
            state.session.is_script_provider_id_occupied(id)
        })
    }

    fn render_script_provider_buttons(
        &self,
        theme: &Theme,
        is_testing: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .gap(px(10.0))
            .mt(px(28.0))
            .child(self.render_script_cancel_button(theme))
            .child(self.render_script_test_button(theme, is_testing, cx))
            .child(self.render_script_save_button(theme, cx))
    }

    fn render_script_cancel_button(&self, theme: &Theme) -> AnyElement {
        let state = self.state.clone();
        script_form_button(
            t!("script_provider.cancel").to_string(),
            theme.bg.subtle,
            Some(theme.border.strong),
            theme.text.primary,
        )
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            crate::bootstrap::dispatch_in_window(
                &state,
                AppAction::CancelAddScriptProvider,
                window,
                cx,
            );
        })
        .into_any_element()
    }

    fn render_script_test_button(
        &self,
        theme: &Theme,
        is_testing: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state.clone();
        let view = cx.entity().clone();
        let label = if is_testing {
            t!("script_provider.testing").to_string()
        } else {
            t!("script_provider.test").to_string()
        };
        script_form_button(
            label,
            theme.bg.subtle,
            Some(theme.text.accent),
            theme.text.accent,
        )
        .opacity(if is_testing { 0.55 } else { 1.0 })
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if is_testing {
                return;
            }
            let config = view.update(cx, |view: &mut Self, cx| {
                view.collect_script_provider_config(cx)
            });
            if let Some(config) = config {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::TestScriptProvider(config),
                    window,
                    cx,
                );
            }
        })
        .into_any_element()
    }

    fn render_script_save_button(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let state = self.state.clone();
        let view = cx.entity().clone();
        script_form_button(
            t!("script_provider.save").to_string(),
            theme.text.accent,
            None,
            theme.element.active,
        )
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            let config = view.update(cx, |view: &mut Self, cx| {
                view.collect_script_provider_config(cx)
            });
            if let Some(config) = config {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::SubmitScriptProvider(config),
                    window,
                    cx,
                );
            }
        })
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{compact_text, looks_like_cf_challenge, should_show_cf_warning};
    use crate::models::ScriptProviderTestResult;

    fn make_result(message: &str, stdout: &str, stderr: &str) -> ScriptProviderTestResult {
        ScriptProviderTestResult {
            success: false,
            message: message.to_string(),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            preview: None,
        }
    }

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

    #[test]
    fn cf_detect_matches_cloudflare_html() {
        let result = make_result(
            "script exited with a non-zero status",
            "<title>Just a moment...</title>",
            "",
        );
        assert!(looks_like_cf_challenge(&result));
    }

    #[test]
    fn cf_detect_matches_cf_ray_header() {
        let result = make_result("", "", "HTTP/2 403\nCF-RAY: 8a1b2c3d");
        assert!(looks_like_cf_challenge(&result));
    }

    #[test]
    fn cf_detect_matches_403_forbidden() {
        let result = make_result("", "", "curl: 403 Forbidden");
        assert!(looks_like_cf_challenge(&result));
    }

    #[test]
    fn cf_detect_matches_cf_clearance_cookie() {
        let result = make_result("", "", "Set-Cookie: cf_clearance=abc123; Path=/");
        assert!(looks_like_cf_challenge(&result));
    }

    #[test]
    fn cf_detect_matches_cf_bm_cookie() {
        let result = make_result("", "Set-Cookie: __cf_bm=xyz", "");
        assert!(looks_like_cf_challenge(&result));
    }

    #[test]
    fn cf_detect_matches_challenge_platform() {
        let result = make_result(
            "",
            "<script src=\"/cdn-cgi/challenge-platform/h/g/orchestrate/jsch/v1\">",
            "",
        );
        assert!(looks_like_cf_challenge(&result));
    }

    #[test]
    fn cf_detect_matches_503_unavailable() {
        let result = make_result("", "", "HTTP/1.1 503 Service Unavailable");
        assert!(looks_like_cf_challenge(&result));
    }

    #[test]
    fn cf_detect_is_case_insensitive() {
        let result = make_result("", "", "<TITLE>JUST A MOMENT...</TITLE>");
        assert!(looks_like_cf_challenge(&result));
    }

    #[test]
    fn cf_detect_ignores_bare_403_without_forbidden_keyword() {
        let result = make_result("", "rate-limit=403/min", "");
        assert!(!looks_like_cf_challenge(&result));
    }

    #[test]
    fn cf_detect_ignores_normal_failure() {
        let result = make_result("invalid JSON", "{\"foo\":1}", "");
        assert!(!looks_like_cf_challenge(&result));
    }

    #[test]
    fn cf_warning_is_hidden_for_successful_result() {
        let result = ScriptProviderTestResult {
            success: true,
            message: "OK".to_string(),
            stdout: "<title>Just a moment...</title>".to_string(),
            stderr: String::new(),
            preview: None,
        };

        assert!(!should_show_cf_warning(&result));
    }
}
