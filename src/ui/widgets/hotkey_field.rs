//! 热键录入输入框 — 使用 BananaTray Theme 样式
//!
//! 包裹 adabraka-ui 的 `HotkeyInputState` entity，提供 click-to-record / keydown 交互，
//! 同时使用 `theme.bg.card` / `theme.border.strong` / `theme.text.accent` 等 Token
//! 替代 adabraka-ui 内置主题，保证全局视觉一致性。
//!
//! 紧凑内联 chip 样式 — 用于设置行 trailing 控件位置，与 toggle / dropdown 行保持一致风格。

use crate::theme::Theme;
use adabraka_ui::components::hotkey_input::HotkeyInputState;
use gpui::{
    div, prelude::FluentBuilder, px, Entity, Focusable, InteractiveElement, MouseButton,
    ParentElement, SharedString, Stateful, Styled, Window,
};

/// 紧凑内联热键录入控件 — 用于设置行 trailing 位置（类似 dropdown trigger 的 chip 风格）
///
/// 固定最小宽度，点击进入录制模式；与 `render_icon_row` 的 trailing 槽位配合使用。
///
/// - `input_entity`: `HotkeyInputState` entity，管理录制生命周期
/// - `placeholder`: 未录入时的占位文本
/// - `on_capture`: 每次成功录入后的回调（用于通知父视图刷新）
pub(crate) fn render_hotkey_field_inline(
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
        rust_i18n::t!("settings.global_hotkey.recording")
            .to_string()
            .into()
    } else if let Some(ref hk) = hotkey {
        hk.format_display().into()
    } else {
        placeholder
    };

    let text_color = if is_recording {
        theme.text.accent
    } else if hotkey.is_some() {
        theme.text.primary
    } else {
        theme.text.muted
    };

    let border_color = if is_recording || is_focused {
        theme.text.accent
    } else {
        theme.border.strong
    };

    let bg = if is_recording {
        theme.bg.subtle
    } else {
        theme.bg.base
    };

    let state_for_click = input_entity.clone();
    let state_for_keydown = input_entity.clone();

    div()
        .id(("hotkey-inline", input_entity.entity_id()))
        .key_context("HotkeyInput")
        .track_focus(&tracked_focus)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .px(px(12.0))
        .py(px(6.0))
        .h(px(32.0))
        .min_w(px(80.0))
        .rounded(px(6.0))
        .bg(bg)
        .border_1()
        .border_color(border_color)
        .text_size(px(13.0))
        .font_weight(gpui::FontWeight::MEDIUM)
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
                .whitespace_nowrap()
                .when(is_recording, |d| d.opacity(0.7))
                .child(display_text),
        )
}
