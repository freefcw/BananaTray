//! NewAPI 中转站快速添加 / 编辑表单
//!
//! 内嵌在 Settings → Providers 右侧 detail 面板中，
//! 用户填写必要字段后自动生成 YAML 配置文件。
//! 编辑模式下从磁盘读取已有配置回填表单，URL 字段只读。
//!
//! 使用自建 SimpleInputState 替代 adabraka-ui InputState，
//! 避免 macOS IME 触发 character_index_for_point 崩溃。

use super::super::{NewApiFormInputs, SettingsView};
use crate::application::AppAction;
use crate::providers::custom::generator::NewApiEditData;
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::{
    render_simple_input, render_simple_textarea, render_svg_icon, SimpleInputState,
};
use gpui::*;
use rust_i18n::t;

/// 渲染字段标签 + 描述（如果有）
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

/// 渲染单个表单字段（标签 + 输入框）
#[allow(clippy::too_many_arguments)]
fn render_form_field(
    id: &'static str,
    label: &str,
    hint: Option<&str>,
    state: &SimpleInputState,
    focus_handle: &FocusHandle,
    is_focused: bool,
    margin_top: Pixels,
    theme: &Theme,
) -> Div {
    div()
        .flex_col()
        .gap(px(6.0))
        .mt(margin_top)
        .child(render_field_label(label, hint, theme))
        .child(render_simple_input(
            id,
            state,
            focus_handle,
            is_focused,
            theme.bg.card,
            theme.text.primary,
            theme.text.muted,
            theme.border.strong,
            theme.text.accent,
        ))
}

/// 渲染多行文本表单字段（标签 + 多行输入框，适用于 Cookie 等长文本）
#[allow(clippy::too_many_arguments)]
fn render_form_field_textarea(
    id: &'static str,
    label: &str,
    hint: Option<&str>,
    state: &SimpleInputState,
    focus_handle: &FocusHandle,
    is_focused: bool,
    margin_top: Pixels,
    theme: &Theme,
) -> Div {
    div()
        .flex_col()
        .gap(px(6.0))
        .mt(margin_top)
        .child(render_field_label(label, hint, theme))
        .child(render_simple_textarea(
            id,
            state,
            focus_handle,
            is_focused,
            theme.bg.card,
            theme.text.primary,
            theme.text.muted,
            theme.border.strong,
            theme.text.accent,
        ))
}

/// 渲染只读字段（编辑模式下身份标识字段不可修改）
fn render_readonly_field(
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

impl SettingsView {
    /// 确保 NewAPI 表单输入状态已创建（编辑模式时预填已有配置数据）
    fn ensure_newapi_inputs(&mut self, edit_data: Option<&NewApiEditData>, cx: &mut Context<Self>) {
        if self.newapi_inputs.is_some() {
            return;
        }
        self.newapi_inputs = Some(match edit_data {
            Some(data) => NewApiFormInputs::new_edit(data, cx),
            None => NewApiFormInputs::new_add(cx),
        });
    }

    /// 清除所有 NewAPI 表单输入状态
    pub(in crate::ui::settings_window) fn clear_newapi_inputs(&mut self) {
        self.newapi_inputs = None;
    }

    /// 渲染 NewAPI 添加/编辑表单（右侧 detail 面板内嵌）
    pub(in crate::ui::settings_window) fn render_newapi_form(
        &mut self,
        is_editing: bool,
        edit_data: Option<&NewApiEditData>,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        self.ensure_newapi_inputs(edit_data, cx);
        let inputs = self.newapi_inputs.as_ref().unwrap();

        let focused = inputs.focused_states(window);

        let title = if is_editing {
            t!("newapi.edit_title").to_string()
        } else {
            t!("newapi.add_title").to_string()
        };

        let inner = div()
            .flex_col()
            .px(px(24.0))
            .pt(px(20.0))
            .pb(px(60.0))
            // ── Header ──
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
                                "src/icons/provider-custom.svg",
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
                                    .child("NewAPI / OneAPI"),
                            ),
                    ),
            )
            // ── 表单字段 ──
            .child(render_form_field(
                "newapi-name",
                &t!("newapi.field.name"),
                Some(&t!("newapi.field.name.placeholder")),
                &inputs.name,
                &inputs.focus_handles[0],
                focused[0],
                px(24.0),
                theme,
            ))
            // URL 字段：编辑模式下显示为只读文本
            .child(if is_editing {
                render_readonly_field(
                    &t!("newapi.field.url"),
                    Some(&t!("newapi.field.url.readonly_hint")),
                    inputs.url.content(),
                    px(16.0),
                    theme,
                )
            } else {
                render_form_field(
                    "newapi-url",
                    &t!("newapi.field.url"),
                    Some(&t!("newapi.field.url.placeholder")),
                    &inputs.url,
                    &inputs.focus_handles[1],
                    focused[1],
                    px(16.0),
                    theme,
                )
            })
            .child(render_form_field_textarea(
                "newapi-cookie",
                &t!("newapi.field.cookie"),
                Some(&t!("newapi.field.cookie.hint")),
                &inputs.cookie,
                &inputs.focus_handles[2],
                focused[2],
                px(16.0),
                theme,
            ))
            .child(render_form_field(
                "newapi-userid",
                &t!("newapi.field.user_id"),
                Some(&t!("newapi.field.user_id.placeholder")),
                &inputs.user_id,
                &inputs.focus_handles[3],
                focused[3],
                px(16.0),
                theme,
            ))
            .child(render_form_field(
                "newapi-divisor",
                &t!("newapi.field.divisor"),
                Some(&t!("newapi.field.divisor.placeholder")),
                &inputs.divisor,
                &inputs.focus_handles[4],
                focused[4],
                px(16.0),
                theme,
            ))
            // ── 操作按钮 ──
            .child(self.render_form_buttons(inputs, theme));

        // ── 键盘事件处理 ──
        div()
            .flex_col()
            .flex_1()
            .h_full()
            .overflow_hidden()
            .on_key_down(cx.listener(|view, ev: &KeyDownEvent, window, cx| {
                Self::handle_form_key(view, ev, window, cx);
            }))
            .child(
                div()
                    .id("newapi-form-scroll")
                    .flex_col()
                    .h_full()
                    .overflow_y_scroll()
                    .child(inner),
            )
    }

    /// 处理表单键盘事件
    fn handle_form_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let inputs = match self.newapi_inputs.as_mut() {
            Some(i) => i,
            None => return,
        };

        // 找到当前获得焦点的输入框
        let focused_idx = inputs
            .focus_handles
            .iter()
            .position(|h| h.is_focused(window));

        let focused_idx = match focused_idx {
            Some(i) => i,
            None => return, // 没有任何输入框有焦点
        };

        let state = match inputs.field_mut(focused_idx) {
            Some(s) => s,
            None => return,
        };

        let keystroke = &ev.keystroke;

        // Cmd+V 粘贴
        if keystroke.modifiers.platform && keystroke.key.as_str() == "v" {
            state.paste(cx);
            cx.notify();
            return;
        }

        // Cmd+C 复制
        if keystroke.modifiers.platform && keystroke.key.as_str() == "c" {
            state.select_all_and_copy(cx);
            return;
        }

        // Cmd+A 全选
        if keystroke.modifiers.platform && keystroke.key.as_str() == "a" {
            state.select_all();
            cx.notify();
            return;
        }

        match keystroke.key.as_str() {
            "backspace" => {
                state.backspace();
                cx.notify();
            }
            "delete" => {
                state.delete();
                cx.notify();
            }
            "left" => {
                state.move_left();
                cx.notify();
            }
            "right" => {
                state.move_right();
                cx.notify();
            }
            "home" => {
                state.move_home();
                cx.notify();
            }
            "end" => {
                state.move_end();
                cx.notify();
            }
            "tab" => {
                // Tab 切换到下一个输入框
                let next = (focused_idx + 1) % 5;
                inputs.focus_handles[next].focus(window);
                cx.notify();
            }
            "enter" => {
                if let Some(action) = self.collect_submit_action() {
                    runtime::dispatch_in_window(&self.state.clone(), action, window, cx);
                }
            }
            _ => {
                // 普通字符输入
                if !keystroke.modifiers.platform && !keystroke.modifiers.control {
                    if let Some(ref text) = keystroke.key_char {
                        for ch in text.chars() {
                            if !ch.is_control() {
                                state.insert_char(ch);
                            }
                        }
                        cx.notify();
                    }
                }
            }
        }
    }

    /// 从表单当前值构造提交 Action；必填字段缺失时返回 None
    fn collect_submit_action(&self) -> Option<AppAction> {
        let inputs = self.newapi_inputs.as_ref()?;
        let name_val = inputs.name.content().trim().to_string();
        let url_val = inputs.url.content().trim().to_string();
        let cookie_val = inputs.cookie.content().trim().to_string();

        if name_val.is_empty() || url_val.is_empty() || cookie_val.is_empty() {
            log::warn!(target: "settings", "NewAPI save: required fields missing");
            return None;
        }

        let user_id_val = inputs.user_id.content().trim().to_string();
        let divisor_val = inputs.divisor.content().trim().to_string();

        Some(AppAction::SubmitNewApi {
            display_name: name_val,
            base_url: url_val,
            cookie: cookie_val,
            user_id: if user_id_val.is_empty() {
                None
            } else {
                Some(user_id_val)
            },
            divisor: if divisor_val.is_empty() {
                None
            } else {
                divisor_val.parse::<f64>().ok()
            },
        })
    }

    /// 渲染取消 + 保存按钮
    fn render_form_buttons(&self, inputs: &NewApiFormInputs, theme: &Theme) -> Div {
        let state_save = self.state.clone();
        let state_cancel = self.state.clone();

        let name_val = inputs.name.content().trim().to_string();
        let url_val = inputs.url.content().trim().to_string();
        let cookie_val = inputs.cookie.content().trim().to_string();
        let user_id_val = inputs.user_id.content().trim().to_string();
        let divisor_val = inputs.divisor.content().trim().to_string();

        div()
            .flex()
            .gap(px(12.0))
            .mt(px(28.0))
            // 取消按钮
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(16.0))
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
                    .child(t!("newapi.cancel").to_string())
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        runtime::dispatch_in_window(
                            &state_cancel,
                            AppAction::CancelAddNewApi,
                            window,
                            cx,
                        );
                    }),
            )
            // 保存按钮
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(16.0))
                    .py(px(10.0))
                    .rounded(px(8.0))
                    .bg(theme.text.accent)
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.element.active)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.9))
                    .child(t!("newapi.save").to_string())
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        if name_val.is_empty() || url_val.is_empty() || cookie_val.is_empty() {
                            log::warn!(
                                target: "settings",
                                "NewAPI save: required fields missing"
                            );
                            return;
                        }

                        let user_id = if user_id_val.is_empty() {
                            None
                        } else {
                            Some(user_id_val.clone())
                        };

                        let divisor = if divisor_val.is_empty() {
                            None
                        } else {
                            divisor_val.parse::<f64>().ok()
                        };

                        runtime::dispatch_in_window(
                            &state_save,
                            AppAction::SubmitNewApi {
                                display_name: name_val.clone(),
                                base_url: url_val.clone(),
                                cookie: cookie_val.clone(),
                                user_id,
                                divisor,
                            },
                            window,
                            cx,
                        );
                    }),
            )
    }
}
