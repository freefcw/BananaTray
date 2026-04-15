pub(crate) mod settings_window;
mod views;
pub(crate) mod widgets;

// 对外暴露的核心类型
pub use views::app_view::AppView;
pub(crate) use widgets::with_multiline_tooltip;
