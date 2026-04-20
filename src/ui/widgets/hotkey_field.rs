//! 热键录入输入框 — 使用 BananaTray Theme 样式，与 NewAPI 表单输入框视觉一致
//!
//! 包裹 adabraka-ui 的 `HotkeyInputState` entity，提供 click-to-record / keydown 交互，
//! 同时使用 `theme.bg.card` / `theme.border.strong` / `theme.text.accent` 等 Token
//! 替代 adabraka-ui 内置主题，保证全局视觉一致性。

use crate::theme::Theme;
use adabraka_ui::components::hotkey_input::HotkeyInputState;
use gpui::{
    div, prelude::FluentBuilder, px, Entity, Focusable, InteractiveElement, MouseButton,
    ParentElement, SharedString, Stateful, Styled, Window,
};

/// 渲染一个与 NewAPI 表单输入框视觉一致的热键录入框
///
/// - `input_entity`: `HotkeyInputState` entity，管理录制生命周期
/// - `placeholder`: 未录入时的占位文本
/// - `on_capture`: 每次成功录入后的回调（用于通知父视图刷新）
pub(crate) fn render_hotkey_field(
    input_entity: &Entity<HotkeyInputState>,
    placeholder: SharedString,
    on_capture: impl Fn(&mut gpui::App) + 'static,
    theme: &Theme,
    window: &mut Window,
    cx: &gpui::App,
) -> Stateful<gpui::Div> {
    let state = input_entity.read(cx);
    let is_recording = state.is_recording();
    let hotkey = state.hotkey().cloned();
    let focus_handle = state.focus_handle(cx);
    let is_focused = focus_handle.is_focused(window);
    let tracked_focus = focus_handle.clone().tab_index(0).tab_stop(true);

    let display_text: SharedString = if is_recording {
        rust_i18n::t!("settings.global_hotkey.recording").into()
    } else if let Some(ref hk) = hotkey {
        hk.format_display().into()
    } else {
        placeholder
    };

    let text_color = if hotkey.is_some() && !is_recording {
        theme.text.primary
    } else {
        theme.text.muted
    };

    let border_color = if is_recording || is_focused {
        theme.text.accent
    } else {
        theme.border.strong
    };

    let state_for_click = input_entity.clone();
    let state_for_keydown = input_entity.clone();

    div()
        .id(("hotkey-input", input_entity.entity_id()))
        .key_context("HotkeyInput")
        .track_focus(&tracked_focus)
        .w_full()
        .flex()
        .items_center()
        .px(px(12.0))
        .py(px(8.0))
        .h(px(36.0))
        .rounded(px(8.0))
        .bg(theme.bg.card)
        .border_1()
        .border_color(border_color)
        .text_size(px(13.0))
        .text_color(text_color)
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, {
            let handle = focus_handle.clone();
            move |_, window, cx| {
                handle.focus(window);
                state_for_click.update(cx, |state, cx| {
                    if !state.is_recording() {
                        state.start_recording(cx);
                    }
                });
            }
        })
        .on_key_down(move |event, _window, cx| {
            let captured = state_for_keydown.update(cx, |state, cx| {
                state.capture_keystroke(&event.keystroke, cx)
            });
            if captured {
                on_capture(cx);
                cx.stop_propagation();
            }
        })
        .child(
            div()
                .flex_1()
                .when(is_recording, |d| d.opacity(0.7))
                .child(display_text),
        )
}
