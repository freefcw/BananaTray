//! NewAPI 中转站快速添加 / 编辑表单
//!
//! 内嵌在 Settings → Providers 右侧 detail 面板中，
//! 用户填写必要字段后自动生成 YAML 配置文件。
//! 编辑模式下从磁盘读取已有配置回填表单，URL 字段只读。
//!
//! 使用 adabraka-ui InputState / TextareaState，
//! 支持鼠标选择、光标闪烁、Alt+方向键按单词跳转等标准编辑功能。
//! Cookie 字段使用 Textarea 多行编辑组件，便于查看和编辑长字符串。

use super::super::{NewApiFormInputs, SettingsView};
use crate::application::AppAction;
use crate::providers::custom::generator::NewApiEditData;
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::{register_input_actions, render_svg_icon};
use adabraka_ui::components::input_state::InputState;
use adabraka_ui::components::textarea_state::TextareaState;
use gpui::{
    div, hsla, px, App, Context, Div, Entity, Focusable, FontWeight,
    InteractiveElement, MouseButton, ParentElement, Pixels, Stateful, StatefulInteractiveElement,
    Styled, Window,
};
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

/// 渲染单个表单字段（标签 + InputState 输入框）
#[allow(clippy::too_many_arguments)]
fn render_form_field(
    id: &'static str,
    label: &str,
    hint: Option<&str>,
    input_entity: &Entity<InputState>,
    is_focused: bool,
    margin_top: Pixels,
    theme: &Theme,
    window: &mut Window,
    cx: &App,
) -> Div {
    let focus_handle = input_entity.read(cx).focus_handle(cx);

    let input_div = div()
        .id(id)
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
        .border_color(if is_focused {
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
        .mt(margin_top)
        .child(render_field_label(label, hint, theme))
        .child(input_div.child(div().flex_1().overflow_hidden().child(input_entity.clone())))
}

/// 渲染 Textarea 表单字段（标签 + 多行文本编辑框）
///
/// Cookie 等长文本字段使用 TextareaState entity 直接渲染，样式与 render_form_field 对齐，
/// 使用 BananaTray 的 Theme 而非 adabraka-ui 的内置主题，保证视觉一致性。
#[allow(clippy::too_many_arguments)]
fn render_textarea_field(
    id: &'static str,
    label: &str,
    hint: Option<&str>,
    textarea_entity: &Entity<TextareaState>,
    is_focused: bool,
    margin_top: Pixels,
    theme: &Theme,
    window: &mut Window,
    cx: &App,
) -> Div {
    let focus_handle = textarea_entity.read(cx).focus_handle(cx);

    let textarea_div = div()
        .id(id)
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
        .border_color(if is_focused {
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
        .mt(margin_top)
        .child(render_field_label(label, hint, theme))
        .child(textarea_div.child(textarea_entity.clone()))
}

/// 注册 TextareaState 的所有键盘事件处理器
///
/// 与 `register_input_actions` 对称，但针对 TextareaState（多行编辑），
/// 额外支持上下方向键导航、Enter 换行、Tab 缩进等。
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

        let focused = inputs.focused_states(window, cx);

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
                focused[0],
                px(24.0),
                theme,
                window,
                cx,
            ))
            // URL 字段：编辑模式下显示为只读文本
            .child(if is_editing {
                render_readonly_field(
                    &t!("newapi.field.url"),
                    Some(&t!("newapi.field.url.readonly_hint")),
                    inputs.url.read(cx).content(),
                    px(16.0),
                    theme,
                )
            } else {
                render_form_field(
                    "newapi-url",
                    &t!("newapi.field.url"),
                    Some(&t!("newapi.field.url.placeholder")),
                    &inputs.url,
                    focused[1],
                    px(16.0),
                    theme,
                    window,
                    cx,
                )
            })
            .child(render_textarea_field(
                "newapi-cookie",
                &t!("newapi.field.cookie"),
                Some(&t!("newapi.field.cookie.hint")),
                &inputs.cookie,
                focused[2],
                px(16.0),
                theme,
                window,
                cx,
            ))
            .child(render_form_field(
                "newapi-userid",
                &t!("newapi.field.user_id"),
                Some(&t!("newapi.field.user_id.placeholder")),
                &inputs.user_id,
                focused[3],
                px(16.0),
                theme,
                window,
                cx,
            ))
            .child(render_form_field(
                "newapi-divisor",
                &t!("newapi.field.divisor"),
                Some(&t!("newapi.field.divisor.placeholder")),
                &inputs.divisor,
                focused[4],
                px(16.0),
                theme,
                window,
                cx,
            ))
            // ── 操作按钮 ──
            .child(self.render_form_buttons(theme, cx));

        // ── 外层容器 ──
        div().flex_col().flex_1().h_full().overflow_hidden().child(
            div()
                .id("newapi-form-scroll")
                .flex_col()
                .h_full()
                .overflow_y_scroll()
                .child(inner),
        )
    }

    /// 从表单当前值构造提交 Action；必填字段缺失时返回 None
    fn collect_submit_action(&self, cx: &App) -> Option<AppAction> {
        let inputs = self.newapi_inputs.as_ref()?;
        let name_val = inputs.name.read(cx).content().trim().to_string();
        let url_val = inputs.url.read(cx).content().trim().to_string();
        let cookie_val = inputs.cookie.read(cx).content().trim().to_string();

        if name_val.is_empty() || url_val.is_empty() || cookie_val.is_empty() {
            log::warn!(target: "settings", "NewAPI save: required fields missing");
            return None;
        }

        let user_id_val = inputs.user_id.read(cx).content().trim().to_string();
        let divisor_val = inputs.divisor.read(cx).content().trim().to_string();

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
    fn render_form_buttons(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let state_save = self.state.clone();
        let state_cancel = self.state.clone();
        let view = cx.entity().clone();

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
                        let ok =
                            view.update(cx, |view: &mut Self, cx| view.collect_submit_action(cx));
                        if let Some(action) = ok {
                            runtime::dispatch_in_window(&state_save, action, window, cx);
                        }
                    })
            })
    }
}
