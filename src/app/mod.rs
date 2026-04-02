mod app_state;
mod app_view;
mod nav;
mod provider_logic;
mod provider_panel;
pub(crate) mod settings_window;
mod tray_settings;
mod widgets;

// 对外暴露的核心类型
pub(crate) use app_state::persist_settings;
pub use app_state::AppState;
pub use app_view::AppView;

pub use settings_window::schedule_open_settings_window;
pub(crate) use widgets::with_multiline_tooltip;
