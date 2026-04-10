mod bridge;
pub(crate) mod settings_window;
mod views;
pub(crate) mod widgets;

// 对外暴露的核心类型
pub(crate) use bridge::persist_settings;
pub use bridge::AppState;
pub use views::app_view::AppView;

pub use settings_window::schedule_open_settings_window;
pub(crate) use widgets::with_multiline_tooltip;
