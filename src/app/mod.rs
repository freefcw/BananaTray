mod app_view;
mod gpui_bridge;
mod nav;
mod provider_panel;
pub(crate) mod settings_window;
mod tray_settings;
pub(crate) mod widgets;

// 对外暴露的核心类型
pub use app_view::AppView;
pub(crate) use gpui_bridge::persist_settings;
pub use gpui_bridge::AppState;

pub use settings_window::schedule_open_settings_window;
pub(crate) use widgets::with_multiline_tooltip;
