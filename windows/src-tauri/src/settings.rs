//! Persisted user settings, stored as JSON in the app data directory.
//!
//! Deliberately plain `std::fs` I/O — no Tauri filesystem plugin is
//! needed because the file is read and written by the Rust core, never by
//! the frontend. A missing or corrupt file falls back to defaults instead
//! of failing startup.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// User-visible preferences. Field names serialize as camelCase.
/// The default accent is the Aello logo color, matching the Android app.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub accent_theme: String,
    pub appearance: String,
    pub volume: f64,
    pub check_for_updates_on_startup: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            accent_theme: "aello-red".to_string(),
            appearance: "system".to_string(),
            volume: 0.8,
            check_for_updates_on_startup: true,
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    settings: AppSettings,
}

impl SettingsStore {
    /// Load settings from `<app data>/settings.json`, or defaults when the
    /// file does not exist or cannot be parsed.
    pub fn load(app_data_dir: &Path) -> Self {
        let path = app_data_dir.join("settings.json");
        let settings = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { path, settings }
    }

    pub fn current(&self) -> &AppSettings {
        &self.settings
    }

    /// Persist settings, creating the parent directory if needed.
    pub fn save(&mut self, settings: AppSettings) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
        fs::write(&self.path, text).map_err(|e| e.to_string())?;
        self.settings = settings;
        Ok(())
    }
}
