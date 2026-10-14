mod fc_ui_theme;
pub(crate) mod settings_window;
mod views;
pub(crate) mod widgets;

// 对外暴露的核心类型
pub(crate) use fc_ui_theme::sync_fc_ui_theme;
pub use views::app_view::AppView;
pub(crate) use widgets::with_multiline_tooltip;

pub(crate) use views::app_view::{clear_current_view, notify_current_view};
