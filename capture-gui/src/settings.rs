use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const SETTINGS_FILE: &str = "settings.toml";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub fps: u32,
    pub screenshot_format: String,
    pub screenshot_quality: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fps: 60,
            screenshot_format: "jpg".to_string(),
            screenshot_quality: 80,
        }
    }
}

impl Settings {
    fn path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(SETTINGS_FILE)
    }

    pub fn load() -> Self {
        let path = Self::path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(s) => {
                        log::info!("Settings loaded from {}", path.display());
                        return s;
                    }
                    Err(e) => {
                        log::warn!("Failed to parse settings: {}, using defaults", e);
                    }
                },
                Err(e) => {
                    log::warn!("Failed to read settings: {}, using defaults", e);
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        log::info!("Settings saved to {}", path.display());
        Ok(())
    }
}
