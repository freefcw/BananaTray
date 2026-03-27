use crate::models::AppSettings;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

pub fn load() -> Result<AppSettings> {
    let path = config_path();
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read settings file at {}", path.display()))?;
    let settings = serde_json::from_str::<AppSettings>(&content)
        .with_context(|| format!("failed to parse settings file at {}", path.display()))?;
    Ok(settings)
}

pub fn save(settings: &AppSettings) -> Result<PathBuf> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create settings directory at {}",
                parent.display()
            )
        })?;
    }

    let content = serde_json::to_string_pretty(settings)?;
    fs::write(&path, content)
        .with_context(|| format!("failed to write settings file at {}", path.display()))?;
    Ok(path)
}

pub fn config_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("BananaTray")
                .join("settings.json");
        }
    } else if cfg!(target_os = "linux") {
        // XDG Base Directory: $XDG_CONFIG_HOME 或 ~/.config
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        return config_dir.join("bananatray").join("settings.json");
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("settings.json")
}
