use std::cell::RefCell;
use std::rc::Rc;

use log::info;

use crate::application::SettingsEffect;

use super::super::AppState;

pub(super) fn run(state: &Rc<RefCell<AppState>>, effect: SettingsEffect) {
    match effect {
        SettingsEffect::PersistSettings => {
            let s = state.borrow();
            s.settings_writer.schedule(s.session.settings.clone());
        }
        SettingsEffect::SyncAutoLaunch(enabled) => {
            sync_auto_launch(enabled);
        }
        SettingsEffect::ApplyLocale(language) => {
            crate::i18n::apply_locale(&language);
        }
        SettingsEffect::UpdateLogLevel(level) => {
            update_log_level(&level);
        }
    }
}

fn sync_auto_launch(enabled: bool) {
    std::thread::spawn(move || {
        crate::platform::auto_launch::sync(enabled);
    });
}

fn update_log_level(level: &str) {
    if let Some(filter) = parse_log_level(level) {
        log::set_max_level(filter);
        info!(target: "settings", "log level changed to: {}", level);
    }
}

fn parse_log_level(value: &str) -> Option<log::LevelFilter> {
    match value.to_lowercase().as_str() {
        "error" => Some(log::LevelFilter::Error),
        "warn" => Some(log::LevelFilter::Warn),
        "info" => Some(log::LevelFilter::Info),
        "debug" => Some(log::LevelFilter::Debug),
        "trace" => Some(log::LevelFilter::Trace),
        _ => None,
    }
}
