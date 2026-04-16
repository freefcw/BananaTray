use std::path::PathBuf;

use super::{APP_ID_LOWER, APP_NAME};

pub fn app_config_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(APP_NAME);
        }
    } else if cfg!(target_os = "linux") {
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        return config_dir.join(APP_ID_LOWER);
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn settings_path() -> PathBuf {
    app_config_dir().join("settings.json")
}

pub fn custom_providers_dir() -> PathBuf {
    app_config_dir().join("providers")
}

pub fn custom_provider_path(filename: &str) -> PathBuf {
    custom_providers_dir().join(filename)
}
