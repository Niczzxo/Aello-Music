//! Persisted user settings, stored as JSON in the app data directory.
//!
//! Deliberately plain `std::fs` I/O — no Tauri filesystem plugin is
//! needed because the file is read and written by the Rust core, never by
//! the frontend. A missing or corrupt file falls back to defaults instead
//! of failing startup.
//!
//! The Google sign-in cookie lives in a SEPARATE file (`auth.json`) and
//! is never part of `AppSettings`: `get_settings` exposes settings to the
//! web frontend, and the cookie must never reach JS storage.

use crate::models::EqProfile;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn default_accent_theme() -> String {
    "aello-red".to_string()
}
fn default_appearance() -> String {
    "system".to_string()
}
fn default_volume() -> f64 {
    0.8
}
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_audio_quality() -> String {
    "auto".to_string()
}
fn default_crossfade_secs() -> f64 {
    5.0
}
fn default_lt_server() -> String {
    "global".to_string()
}

/// User-visible preferences. Field names serialize as camelCase.
/// The default accent is the Aello logo color, matching the Android app.
///
/// Every field carries a `#[serde(default = ...)]`: settings files
/// written by v0.1.0 lack the 6.1.45 fields, and a missing field must
/// fall back to its default instead of failing the whole file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_accent_theme")]
    pub accent_theme: String,
    #[serde(default = "default_appearance")]
    pub appearance: String,
    #[serde(default = "default_volume")]
    pub volume: f64,
    #[serde(default = "default_true")]
    pub check_for_updates_on_startup: bool,
    // ── 6.1.45 additions ──
    /// "auto" | "high" | "low" (resolve picks itag 140 for high/auto,
    /// itag 139 for low — see select_aac_format in innertube/resolve.rs).
    #[serde(default = "default_audio_quality")]
    pub audio_quality: String,
    #[serde(default = "default_true")]
    pub crossfade_enabled: bool,
    #[serde(default = "default_crossfade_secs")]
    pub crossfade_secs: f64,
    #[serde(default = "default_true")]
    pub sound_enhance_enabled: bool,
    /// None = no custom EQ profile active.
    #[serde(default)]
    pub active_eq_profile_id: Option<String>,
    #[serde(default)]
    pub eq_profiles: Vec<EqProfile>,
    #[serde(default = "default_true")]
    pub persistent_queue: bool,
    #[serde(default = "default_false")]
    pub auto_download_on_like: bool,
    /// "global" | "usa" | custom Listen Together server URL.
    #[serde(default = "default_lt_server")]
    pub lt_server: String,
    #[serde(default)]
    pub lt_username: String,
    /// Android 6.1.45 `KEY_LAST_UPDATE_CHECK_FAILED`: the last update
    /// check failed, so Settings must show "Can't check for updates"
    /// instead of looking up-to-date. Set by the update check itself
    /// (lib.rs), never by the frontend form.
    #[serde(default = "default_false")]
    pub last_update_check_failed: bool,
    /// Android 6.1.45 `ListenTogetherControlModeKey`: the host's last
    /// control mode ("owner" | "everyone"), restored on the next room.
    #[serde(default = "default_lt_control_mode")]
    pub lt_control_mode: String,
}

fn default_lt_control_mode() -> String {
    "owner".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            accent_theme: default_accent_theme(),
            appearance: default_appearance(),
            volume: default_volume(),
            check_for_updates_on_startup: true,
            audio_quality: default_audio_quality(),
            crossfade_enabled: true,
            crossfade_secs: default_crossfade_secs(),
            sound_enhance_enabled: true,
            active_eq_profile_id: None,
            eq_profiles: Vec::new(),
            persistent_queue: true,
            auto_download_on_like: false,
            lt_server: default_lt_server(),
            lt_username: String::new(),
            last_update_check_failed: false,
            lt_control_mode: default_lt_control_mode(),
        }
    }
}

/// Google sign-in state, stored in `auth.json` (never in JS storage).
/// The cookie is the raw `Cookie` header value harvested from the
/// login WebView; `visitor_data` is the matching visitorData.
/// `data_sync_id` is the picked channel's identity token (Android
/// `DataSyncIdKey`); `channels` is the account's channel list from
/// `getAccountSwitcherEndpoint`, so the picker can re-show in Settings
/// without a network call. `avatar_url` is the account photo URL from
/// the `account_menu` response — never guessed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthState {
    #[serde(default)]
    pub cookie: Option<String>,
    #[serde(default)]
    pub visitor_data: Option<String>,
    #[serde(default)]
    pub account_name: Option<String>,
    #[serde(default)]
    pub data_sync_id: Option<String>,
    #[serde(default)]
    pub channel_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub channels: Vec<crate::models::AccountChannel>,
}

pub struct SettingsStore {
    path: PathBuf,
    settings: AppSettings,
    auth_path: PathBuf,
    auth: AuthState,
}

impl SettingsStore {
    /// Load settings from `<app data>/settings.json`, or defaults when the
    /// file does not exist or cannot be parsed. Auth state loads from
    /// `<app data>/auth.json` the same way.
    pub fn load(app_data_dir: &Path) -> Self {
        let path = app_data_dir.join("settings.json");
        let settings = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        let auth_path = app_data_dir.join("auth.json");
        let auth = fs::read_to_string(&auth_path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            path,
            settings,
            auth_path,
            auth,
        }
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

    pub fn auth(&self) -> &AuthState {
        &self.auth
    }

    /// Persist the update-check failure flag (called by the update
    /// check in lib.rs after every run: true on failure, false on a
    /// conclusive result). Writes settings.json directly — the
    /// frontend never round-trips this field.
    pub fn set_last_update_check_failed(&mut self, failed: bool) -> Result<(), String> {
        if self.settings.last_update_check_failed == failed {
            return Ok(());
        }
        self.settings.last_update_check_failed = failed;
        let text = serde_json::to_string_pretty(&self.settings).map_err(|e| e.to_string())?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&self.path, text).map_err(|e| e.to_string())
    }

    /// Persist sign-in state (cookie + visitorData + account name +
    /// avatar + channel list). The channel identity is NOT set here:
    /// a fresh sign-in acts as the primary identity until the user
    /// picks a channel via [`Self::set_channel`].
    pub fn set_auth(
        &mut self,
        cookie: String,
        visitor_data: Option<String>,
        account_name: String,
        avatar_url: Option<String>,
        channels: Vec<crate::models::AccountChannel>,
    ) -> Result<(), String> {
        self.auth = AuthState {
            cookie: Some(cookie),
            visitor_data,
            account_name: Some(account_name),
            data_sync_id: None,
            channel_name: None,
            avatar_url,
            channels,
        };
        self.write_auth()
    }

    /// Persist a picked channel (identity + display name) after a
    /// successful re-validation with that identity applied.
    pub fn set_channel(
        &mut self,
        data_sync_id: String,
        channel_name: String,
        account_name: String,
    ) -> Result<(), String> {
        self.auth.data_sync_id = Some(data_sync_id);
        self.auth.channel_name = Some(channel_name);
        self.auth.account_name = Some(account_name);
        self.write_auth()
    }

    /// Clear sign-in state locally (sign-out).
    pub fn clear_auth(&mut self) -> Result<(), String> {
        self.auth = AuthState::default();
        self.write_auth()
    }

    fn write_auth(&self) -> Result<(), String> {
        if let Some(parent) = self.auth_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(&self.auth).map_err(|e| e.to_string())?;
        fs::write(&self.auth_path, text).map_err(|e| e.to_string())?;
        Ok(())
    }
}
