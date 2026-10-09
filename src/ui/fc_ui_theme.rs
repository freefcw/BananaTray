//! fc-ui 全局主题与 BananaTray 主题的同步。
//!
//! fc-ui 组件（输入框的光标 / 选区 / 占位色）只读它自己的全局 `ThemeState`，
//! 不认识 BananaTray 的 `&Theme` 体系；0.9 起光标 / 选区用 `tokens.primary` 绘制，
//! 而 light preset 的 primary 是纯黑，装在深色输入框底色上不可见。
//! 因此在每次"解析后的 BananaTray 主题可能变化"的时机调用本函数：
//! 按用户主题偏好 + 系统外观选 fc-ui preset，并把 primary 统一覆盖为
//! BananaTray accent，保证光标 / 选区在两套外观下都清晰可读。

use crate::models::AppTheme;
use crate::theme::{is_dark_appearance, Theme};
use gpui::{App, WindowAppearance};

pub(crate) fn sync_fc_ui_theme(user_theme: AppTheme, appearance: WindowAppearance, cx: &mut App) {
    let dark = matches!(
        user_theme.resolve(is_dark_appearance(appearance)),
        AppTheme::Dark
    );
    let banana_theme = if dark { Theme::dark() } else { Theme::light() };
    let mut fc_theme = if dark {
        fc_ui::theme::Theme::dark()
    } else {
        fc_ui::theme::Theme::light()
    };
    fc_theme.tokens.primary = banana_theme.text.accent;
    fc_ui::theme::install_theme(cx, fc_theme);
}
