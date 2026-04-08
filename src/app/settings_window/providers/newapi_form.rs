//! NewAPI 中转站快速添加表单
//!
//! 内嵌在 Settings → Providers 右侧 detail 面板中，
//! 用户填写必要字段后自动生成 YAML 配置文件。
//!
//! 使用自建 SimpleInputState 替代 adabraka-ui InputState，
//! 避免 macOS IME 触发 character_index_for_point 崩溃。

use super::super::{NewApiFormInputs, SettingsView};
use crate::app::widgets::{render_simple_input, render_svg_icon, SimpleInputState};
use crate::application::AppAction;
use crate::runtime;
use crate::theme::Theme;
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

impl SettingsView {
    /// 确保 NewAPI 表单输入状态已创建
    fn ensure_newapi_inputs(&mut self, cx: &mut Context<Self>) {
        if self.newapi_inputs.is_some() {
            return;
        }

        self.newapi_inputs = Some(NewApiFormInputs {
            name: SimpleInputState::new(t!("newapi.field.name.placeholder").to_string()),
            url: SimpleInputState::new(t!("newapi.field.url.placeholder").to_string()),
            cookie: SimpleInputState::new(t!("newapi.field.cookie.placeholder").to_string()),
            user_id: SimpleInputState::new(t!("newapi.field.user_id.placeholder").to_string()),
            divisor: SimpleInputState::new(t!("newapi.field.divisor.placeholder").to_string()),
            focus_handles: [
                cx.focus_handle(),
                cx.focus_handle(),
                cx.focus_handle(),
                cx.focus_handle(),
                cx.focus_handle(),
            ],
        });
    }

    /// 清除所有 NewAPI 表单输入状态
    pub(in crate::app::settings_window) fn clear_newapi_inputs(&mut self) {
        self.newapi_inputs = None;
    }

    /// 渲染 NewAPI 添加表单（右侧 detail 面板内嵌）
    pub(in crate::app::settings_window) fn render_newapi_form(
        &mut self,
        theme: &Theme,
        viewport: Size<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        self.ensure_newapi_inputs(cx);
        let inputs = self.newapi_inputs.as_ref().unwrap();

        let focused: [bool; 5] = [
            inputs.focus_handles[0].is_focused(window),
            inputs.focus_handles[1].is_focused(window),
            inputs.focus_handles[2].is_focused(window),
            inputs.focus_handles[3].is_focused(window),
            inputs.focus_handles[4].is_focused(window),
        ];

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
                                    .child(t!("newapi.add_title").to_string()),
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
            .child(render_form_field(
                "newapi-url",
                &t!("newapi.field.url"),
                Some(&t!("newapi.field.url.placeholder")),
                &inputs.url,
                &inputs.focus_handles[1],
                focused[1],
                px(16.0),
                theme,
            ))
            .child(render_form_field(
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
            .child(self.render_form_buttons(theme));

        // ── 键盘事件处理 ──
        let detail_scroll_h = viewport.height - px(65.0);

        div()
            .flex_col()
            .flex_1()
            .overflow_hidden()
            .on_key_down(cx.listener(|view, ev: &KeyDownEvent, window, cx| {
                Self::handle_form_key(view, ev, window, cx);
            }))
            .child(
                div()
                    .id("newapi-form-scroll")
                    .flex_col()
                    .h(detail_scroll_h)
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

        let state = match focused_idx {
            0 => &mut inputs.name,
            1 => &mut inputs.url,
            2 => &mut inputs.cookie,
            3 => &mut inputs.user_id,
            4 => &mut inputs.divisor,
            _ => return,
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

        // Cmd+A 全选（visual only, 简化处理不做选区）
        if keystroke.modifiers.platform && keystroke.key.as_str() == "a" {
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
                // 回车触发保存（交给按钮逻辑）
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

    /// 渲染取消 + 保存按钮
    fn render_form_buttons(&self, theme: &Theme) -> Div {
        let state_save = self.state.clone();
        let state_cancel = self.state.clone();

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
            .child({
                let inputs = self.newapi_inputs.as_ref().unwrap();
                let name_val = inputs.name.content().trim().to_string();
                let url_val = inputs.url.content().trim().to_string();
                let cookie_val = inputs.cookie.content().trim().to_string();
                let user_id_val = inputs.user_id.content().trim().to_string();
                let divisor_val = inputs.divisor.content().trim().to_string();

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
                    })
            })
    }
}
