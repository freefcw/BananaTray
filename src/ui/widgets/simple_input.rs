//! 轻量级纯文本输入组件（不依赖 adabraka-ui InputState）
//!
//! 避免 InputState 的 NSTextInputClient / character_index_for_point 崩溃。
//! 仅支持 ASCII 输入，适用于 URL、Token、数字等技术型字段。

use gpui::*;

/// 简易输入框状态（存储文本 + 光标位置 + 全选标记）
pub struct SimpleInputState {
    pub text: String,
    cursor: usize,
    pub placeholder: SharedString,
    /// 全选标记：Cmd+A 设置为 true，后续输入/删除时先清空文本
    all_selected: bool,
}

impl SimpleInputState {
    pub fn new(placeholder: impl Into<SharedString>) -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            placeholder: placeholder.into(),
            all_selected: false,
        }
    }

    /// 创建带有初始值的输入框（用于编辑模式回填）
    pub fn new_with_value(placeholder: impl Into<SharedString>, value: impl Into<String>) -> Self {
        let text: String = value.into();
        let cursor = text.len();
        Self {
            text,
            cursor,
            placeholder: placeholder.into(),
            all_selected: false,
        }
    }

    pub fn content(&self) -> &str {
        &self.text
    }

    pub fn is_all_selected(&self) -> bool {
        self.all_selected
    }

    /// 标记全选状态
    pub(crate) fn select_all(&mut self) {
        if !self.text.is_empty() {
            self.all_selected = true;
        }
    }

    /// 如果处于全选状态，先清空文本再执行操作
    fn clear_if_selected(&mut self) {
        if self.all_selected {
            self.text.clear();
            self.cursor = 0;
            self.all_selected = false;
        }
    }

    /// 取消全选状态（光标移动等场景）
    fn deselect(&mut self) {
        self.all_selected = false;
    }

    pub(crate) fn insert_char(&mut self, ch: char) {
        self.clear_if_selected();
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub(crate) fn insert_str(&mut self, s: &str) {
        self.clear_if_selected();
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub(crate) fn backspace(&mut self) {
        if self.all_selected {
            self.text.clear();
            self.cursor = 0;
            self.all_selected = false;
            return;
        }
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
        if self.all_selected {
            self.text.clear();
            self.cursor = 0;
            self.all_selected = false;
            return;
        }
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
        self.deselect();
        if self.cursor > 0 {
            self.cursor = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub(crate) fn move_right(&mut self) {
        self.deselect();
        if self.cursor < self.text.len() {
            self.cursor = self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.text.len());
        }
    }

    pub(crate) fn move_home(&mut self) {
        self.deselect();
        self.cursor = 0;
    }

    pub(crate) fn move_end(&mut self) {
        self.deselect();
        self.cursor = self.text.len();
    }

    pub(crate) fn select_all_and_copy(&self, cx: &mut App) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.text.clone()));
    }

    pub(crate) fn paste(&mut self, cx: &mut App) {
        self.clear_if_selected();
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
    let content = build_input_content(state, is_focused, theme_text, theme_muted, theme_accent);
    let border_color = if is_focused {
        theme_accent
    } else {
        theme_border
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

/// 渲染多行文本输入框（适用于 Cookie 等长文本字段）
///
/// 与 `render_simple_input` 共享同一个 `SimpleInputState`，
/// 区别在于：文本自动换行、最小 4 行高度、无固定高度限制。
#[allow(clippy::too_many_arguments)]
pub fn render_simple_textarea(
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

    // 多行文本框：全选高亮 or 正常显示（不显示光标，因为换行场景下光标定位不准确）
    let select_bg = if state.is_all_selected() && is_focused {
        hsla(210.0 / 360.0, 0.8, 0.5, 0.3) // 蓝色半透明选区
    } else {
        hsla(0.0, 0.0, 0.0, 0.0) // 透明
    };

    div()
        .id(id)
        .track_focus(focus_handle)
        .key_context("simple_input")
        .w_full()
        .flex_col()
        .px(px(12.0))
        .py(px(8.0))
        .min_h(px(80.0)) // 最小 4 行高度
        .rounded(px(8.0))
        .bg(theme_bg)
        .border_1()
        .border_color(border_color)
        .on_mouse_down(MouseButton::Left, {
            let handle = focus_handle.clone();
            move |_, window, _| handle.focus(window)
        })
        .child(
            div()
                .w_full()
                .bg(select_bg)
                .rounded(px(4.0))
                .text_size(px(12.0))
                .line_height(px(18.0))
                .text_color(text_color)
                .child(display_text),
        )
}

/// 构建输入框内容（共享逻辑：处理全选高亮、光标、placeholder）
fn build_input_content(
    state: &SimpleInputState,
    is_focused: bool,
    theme_text: Hsla,
    theme_muted: Hsla,
    theme_accent: Hsla,
) -> Div {
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

    // 全选状态：蓝色高亮背景 + 全部文本
    if state.is_all_selected() && is_focused && !state.text.is_empty() {
        let select_bg = hsla(210.0 / 360.0, 0.8, 0.5, 0.3);
        return div().flex().items_center().overflow_hidden().child(
            div()
                .bg(select_bg)
                .rounded(px(2.0))
                .text_color(theme_text)
                .text_size(px(13.0))
                .child(display_text),
        );
    }

    if is_focused && !state.text.is_empty() {
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
    }
}

#[cfg(test)]
mod tests {
    use super::SimpleInputState;

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

    // ── select_all ──────────────────────────────────────

    #[test]
    fn select_all_then_insert_replaces() {
        let mut s = SimpleInputState::new("");
        s.insert_str("old content");
        s.select_all();
        assert!(s.is_all_selected());

        // 输入新字符 → 先清空再插入
        s.insert_char('N');
        assert_eq!(s.content(), "N");
        assert!(!s.is_all_selected());
    }

    #[test]
    fn select_all_then_backspace_clears() {
        let mut s = SimpleInputState::new("");
        s.insert_str("some text");
        s.select_all();

        s.backspace();
        assert_eq!(s.content(), "");
        assert!(!s.is_all_selected());
    }

    #[test]
    fn select_all_then_delete_clears() {
        let mut s = SimpleInputState::new("");
        s.insert_str("some text");
        s.select_all();

        s.delete();
        assert_eq!(s.content(), "");
        assert!(!s.is_all_selected());
    }

    #[test]
    fn select_all_then_move_deselects() {
        let mut s = SimpleInputState::new("");
        s.insert_str("hello");
        s.select_all();
        assert!(s.is_all_selected());

        s.move_left();
        assert!(!s.is_all_selected());
        // 文本保持不变
        assert_eq!(s.content(), "hello");
    }

    #[test]
    fn select_all_empty_text_is_noop() {
        let mut s = SimpleInputState::new("");
        s.select_all();
        assert!(!s.is_all_selected());
    }

    #[test]
    fn select_all_then_insert_str_replaces() {
        let mut s = SimpleInputState::new("");
        s.insert_str("old cookie value");
        s.select_all();

        s.insert_str("new_cookie=abc123");
        assert_eq!(s.content(), "new_cookie=abc123");
        assert!(!s.is_all_selected());
    }

    #[test]
    fn new_with_value_prefills() {
        let s = SimpleInputState::new_with_value("hint", "prefilled");
        assert_eq!(s.content(), "prefilled");
        assert_eq!(s.cursor, 9);
        assert!(!s.is_all_selected());
    }
}
