//! 轻量级纯文本输入组件（不依赖 adabraka-ui InputState）
//!
//! 避免 InputState 的 NSTextInputClient / character_index_for_point 崩溃。
//! 仅支持 ASCII 输入，适用于 URL、Token、数字等技术型字段。

use gpui::*;

/// 简易输入框状态（存储文本 + 光标位置）
pub struct SimpleInputState {
    pub text: String,
    cursor: usize,
    pub placeholder: SharedString,
}

impl SimpleInputState {
    pub fn new(placeholder: impl Into<SharedString>) -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            placeholder: placeholder.into(),
        }
    }

    pub fn content(&self) -> &str {
        &self.text
    }

    pub(crate) fn insert_char(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub(crate) fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub(crate) fn backspace(&mut self) {
        if self.cursor > 0 {
            // 按字符边界删除
            let prev = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.text.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    pub(crate) fn delete(&mut self) {
        if self.cursor < self.text.len() {
            let next = self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.text.len());
            self.text.drain(self.cursor..next);
        }
    }

    pub(crate) fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub(crate) fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor = self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.text.len());
        }
    }

    pub(crate) fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub(crate) fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    pub(crate) fn select_all_and_copy(&self, cx: &mut App) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.text.clone()));
    }

    pub(crate) fn paste(&mut self, cx: &mut App) {
        if let Some(item) = cx.read_from_clipboard() {
            let text = item.text().unwrap_or_default();
            // 过滤控制字符（换行等）
            let clean: String = text.chars().filter(|c| !c.is_control()).collect();
            self.insert_str(&clean);
        }
    }
}

/// 渲染一个简易输入框
///
/// 返回一个已注册好键盘事件的 Div。
/// `text_color`、`bg`、`border_color` 由调用方通过 theme 控制。
#[allow(clippy::too_many_arguments)]
pub fn render_simple_input(
    id: &'static str,
    state: &SimpleInputState,
    focus_handle: &FocusHandle,
    is_focused: bool,
    theme_bg: Hsla,
    theme_text: Hsla,
    theme_muted: Hsla,
    theme_border: Hsla,
    theme_accent: Hsla,
) -> impl IntoElement {
    let border_color = if is_focused {
        theme_accent
    } else {
        theme_border
    };

    let display_text: SharedString = if state.text.is_empty() {
        state.placeholder.clone()
    } else {
        state.text.clone().into()
    };

    let text_color = if state.text.is_empty() {
        theme_muted
    } else {
        theme_text
    };

    // 构建光标和文本显示
    let content = if is_focused && !state.text.is_empty() {
        // 有焦点且有文本：显示文本 + 光标
        let before: String = state.text[..state.cursor].to_string();
        let after: String = state.text[state.cursor..].to_string();

        div()
            .flex()
            .items_center()
            .overflow_hidden()
            .child(
                div()
                    .text_color(text_color)
                    .text_size(px(13.0))
                    .child(before),
            )
            .child(div().w(px(1.0)).h(px(14.0)).bg(theme_accent).flex_none())
            .child(
                div()
                    .text_color(text_color)
                    .text_size(px(13.0))
                    .child(after),
            )
    } else if is_focused && state.text.is_empty() {
        // 有焦点但无文本：显示光标 + placeholder
        div()
            .flex()
            .items_center()
            .overflow_hidden()
            .child(div().w(px(1.0)).h(px(14.0)).bg(theme_accent).flex_none())
            .child(
                div()
                    .text_color(theme_muted)
                    .text_size(px(13.0))
                    .child(display_text),
            )
    } else {
        // 无焦点：显示文本或 placeholder
        div().flex().items_center().overflow_hidden().child(
            div()
                .text_color(text_color)
                .text_size(px(13.0))
                .child(display_text),
        )
    };

    div()
        .id(id)
        .track_focus(focus_handle)
        .key_context("simple_input")
        .w_full()
        .flex()
        .items_center()
        .px(px(12.0))
        .py(px(8.0))
        .h(px(36.0))
        .rounded(px(8.0))
        .bg(theme_bg)
        .border_1()
        .border_color(border_color)
        .on_mouse_down(MouseButton::Left, {
            let handle = focus_handle.clone();
            move |_, window, _| handle.focus(window)
        })
        .child(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_backspace() {
        let mut s = SimpleInputState::new("hint");
        s.insert_char('a');
        s.insert_char('b');
        assert_eq!(s.content(), "ab");
        s.backspace();
        assert_eq!(s.content(), "a");
        s.backspace();
        assert_eq!(s.content(), "");
        s.backspace(); // no-op
        assert_eq!(s.content(), "");
    }

    #[test]
    fn delete_forward() {
        let mut s = SimpleInputState::new("");
        s.insert_str("abc");
        s.move_home();
        s.delete();
        assert_eq!(s.content(), "bc");
    }

    #[test]
    fn cursor_movement() {
        let mut s = SimpleInputState::new("");
        s.insert_str("hello");
        assert_eq!(s.cursor, 5);
        s.move_left();
        assert_eq!(s.cursor, 4);
        s.move_home();
        assert_eq!(s.cursor, 0);
        s.move_right();
        assert_eq!(s.cursor, 1);
        s.move_end();
        assert_eq!(s.cursor, 5);
    }

    #[test]
    fn insert_at_cursor() {
        let mut s = SimpleInputState::new("");
        s.insert_str("ac");
        s.move_left(); // cursor at 1
        s.insert_char('b');
        assert_eq!(s.content(), "abc");
    }
}
