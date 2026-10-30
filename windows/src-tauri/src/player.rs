//! Playback snapshot shared between the web frontend and the Rust core.
//!
//! Audio itself renders in the frontend through the `<audio>` element
//! (WebView2). The Rust side keeps a snapshot of what is playing so later
//! work can wire Windows System Media Transport Controls (SMTC), media
//! keys, and a mini-player without re-architecting. SMTC is NOT
//! implemented in this scaffold — the snapshot only exists to be read and
//! written through the `get_player_snapshot` / `update_player_snapshot`
//! commands.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Point-in-time playback state. Field names serialize as camelCase.
/// `current_title` / `current_artist` / `current_artwork` are 6.1.45
/// additions for SMTC (Windows media controls); all `#[serde(default)]`
/// so older frontend payloads still parse.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSnapshot {
    pub current_song_id: Option<String>,
    #[serde(default)]
    pub current_title: Option<String>,
    #[serde(default)]
    pub current_artist: Option<String>,
    #[serde(default)]
    pub current_artwork: Option<String>,
    pub queue: Vec<String>,
    pub position_seconds: f64,
    pub is_playing: bool,
    pub volume: f64,
    pub repeat_mode: String,
    pub shuffle: bool,
}

impl Default for PlaybackSnapshot {
    fn default() -> Self {
        Self {
            current_song_id: None,
            current_title: None,
            current_artist: None,
            current_artwork: None,
            queue: Vec::new(),
            position_seconds: 0.0,
            is_playing: false,
            volume: 0.8,
            repeat_mode: "off".to_string(),
            shuffle: false,
        }
    }
}

/// Thread-safe holder for the latest snapshot.
pub struct PlayerSession {
    snapshot: Mutex<PlaybackSnapshot>,
}

impl PlayerSession {
    pub fn new() -> Self {
        Self {
            snapshot: Mutex::new(PlaybackSnapshot::default()),
        }
    }

    /// A poisoned lock is reported as `Err` instead of panicking: the
    /// snapshot commands return `Result`, so the frontend gets a
    /// rejection it can ignore (snapshot sync is best-effort).
    pub fn snapshot(&self) -> Result<PlaybackSnapshot, String> {
        self.snapshot
            .lock()
            .map_err(|_| "player lock poisoned".to_string())
            .map(|snap| snap.clone())
    }

    pub fn update(&self, snapshot: PlaybackSnapshot) -> Result<(), String> {
        self.snapshot
            .lock()
            .map_err(|_| "player lock poisoned".to_string())
            .map(|mut snap| *snap = snapshot)
    }
}

impl Default for PlayerSession {
    fn default() -> Self {
        Self::new()
    }
}
