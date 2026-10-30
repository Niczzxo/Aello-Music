//! Local library store: liked songs, manual playlists, play history,
//! downloads, and the keep-last-good Daily Discover list.
//!
//! Plain `std::fs` JSON I/O in the app-data dir, like `settings.rs`:
//! `library.json` (liked songs, playlists, speed-dial ids, last Daily
//! Discover), `history.json` (play history, cap 500, newest first),
//! `downloads.json` (downloaded rows). A missing or corrupt file falls
//! back to empty defaults — it never fails startup.
//!
//! All access goes through one `Mutex<LibraryStore>` in `AppState`.

use crate::models::{
    DownloadedSong, HistoryEntry, LibraryData, LibraryPlaylist, Song,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

/// Current UTC time as `YYYY-MM-DDTHH:MM:SSZ` (no chrono dependency;
/// civil-from-days algorithm).
pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let secs_of_day = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Monotonic-ish unique id from the wall clock (`pl-<nanos>` /
/// `dl-<nanos>`); collisions need two calls in the same nanosecond.
fn unique_id(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{nanos}")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct LibraryFile {
    /// Full liked songs (not just ids): Daily Discover seeds and a
    /// future Liked auto-playlist need the song, not only its id.
    liked_songs: Vec<Song>,
    playlists: Vec<LibraryPlaylist>,
    speed_dial_ids: Vec<String>,
    last_daily_discover: Vec<Song>,
}

fn load_json<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> T {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

pub struct LibraryStore {
    library_path: PathBuf,
    history_path: PathBuf,
    downloads_path: PathBuf,
    library: LibraryFile,
    history: Vec<HistoryEntry>,
    downloads: Vec<DownloadedSong>,
}

impl LibraryStore {
    pub fn load(app_data_dir: &Path) -> Self {
        let library_path = app_data_dir.join("library.json");
        let history_path = app_data_dir.join("history.json");
        let downloads_path = app_data_dir.join("downloads.json");
        Self {
            library: load_json(&library_path),
            history: load_json(&history_path),
            downloads: load_json(&downloads_path),
            library_path,
            history_path,
            downloads_path,
        }
    }

    fn save_library(&self) -> Result<(), String> {
        save_json(&self.library_path, &self.library)
    }

    fn save_history(&self) -> Result<(), String> {
        save_json(&self.history_path, &self.history)
    }

    fn save_downloads(&self) -> Result<(), String> {
        save_json(&self.downloads_path, &self.downloads)
    }

    // ── library snapshot ────────────────────────────────────────────────

    pub fn library_data(&self) -> LibraryData {
        LibraryData {
            liked_ids: self
                .library
                .liked_songs
                .iter()
                .map(|s| s.id.clone())
                .collect(),
            playlists: self.library.playlists.clone(),
            history: self.history.clone(),
            speed_dial_ids: self.library.speed_dial_ids.clone(),
        }
    }

    // ── likes ───────────────────────────────────────────────────────────

    pub fn is_liked(&self, song_id: &str) -> bool {
        self.library.liked_songs.iter().any(|s| s.id == song_id)
    }

    pub fn set_liked(&mut self, song: &Song, liked: bool) -> Result<(), String> {
        if liked {
            if !self.is_liked(&song.id) {
                self.library.liked_songs.push(song.clone());
            }
        } else {
            self.library.liked_songs.retain(|s| s.id != song.id);
        }
        self.save_library()
    }

    // ── playlists ───────────────────────────────────────────────────────

    pub fn create_playlist(&mut self, name: &str) -> Result<LibraryPlaylist, String> {
        let playlist = LibraryPlaylist {
            id: unique_id("pl"),
            name: name.to_string(),
            song_ids: Vec::new(),
            created_at: now_iso(),
        };
        self.library.playlists.push(playlist.clone());
        self.save_library()?;
        Ok(playlist)
    }

    pub fn rename_playlist(&mut self, id: &str, name: &str) -> Result<(), String> {
        let Some(p) = self.library.playlists.iter_mut().find(|p| p.id == id) else {
            return Err(format!("playlist not found: {id}"));
        };
        p.name = name.to_string();
        self.save_library()
    }

    pub fn delete_playlist(&mut self, id: &str) -> Result<(), String> {
        let before = self.library.playlists.len();
        self.library.playlists.retain(|p| p.id != id);
        if self.library.playlists.len() == before {
            return Err(format!("playlist not found: {id}"));
        }
        self.save_library()
    }

    pub fn playlist_add_song(&mut self, id: &str, song: &Song) -> Result<(), String> {
        let Some(p) = self.library.playlists.iter_mut().find(|p| p.id == id) else {
            return Err(format!("playlist not found: {id}"));
        };
        if !p.song_ids.contains(&song.id) {
            p.song_ids.push(song.id.clone());
        }
        self.save_library()
    }

    pub fn playlist_remove_song(&mut self, id: &str, song_id: &str) -> Result<(), String> {
        let Some(p) = self.library.playlists.iter_mut().find(|p| p.id == id) else {
            return Err(format!("playlist not found: {id}"));
        };
        p.song_ids.retain(|s| s != song_id);
        self.save_library()
    }

    // ── history ─────────────────────────────────────────────────────────

    /// Record a play (called after 30s of playback). An existing entry
    /// moves to the front with `playCount + 1` and a fresh timestamp.
    pub fn add_to_history(&mut self, song: &Song) -> Result<(), String> {
        let now = now_iso();
        if let Some(pos) = self.history.iter().position(|e| e.song_id == song.id) {
            let mut entry = self.history.remove(pos);
            entry.play_count += 1;
            entry.played_at = now;
            self.history.insert(0, entry);
        } else {
            self.history.insert(
                0,
                HistoryEntry {
                    song_id: song.id.clone(),
                    title: song.title.clone(),
                    artists: song.artists.clone(),
                    played_at: now,
                    play_count: 1,
                },
            );
        }
        self.history.truncate(500);
        self.save_history()
    }

    pub fn clear_history(&mut self) -> Result<(), String> {
        self.history.clear();
        self.save_history()
    }

    /// Most-played entries, highest `playCount` first.
    pub fn most_played(&self, limit: usize) -> Vec<HistoryEntry> {
        let mut entries = self.history.clone();
        entries.sort_by(|a, b| b.play_count.cmp(&a.play_count));
        entries.truncate(limit);
        entries
    }

    /// Seed ids for Daily Discover: liked ∪ recent ∪ most-played,
    /// deduped, most-recent / most-played first.
    pub fn seed_ids(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for id in self
            .library
            .liked_songs
            .iter()
            .map(|s| s.id.clone())
            .chain(self.history.iter().map(|e| e.song_id.clone()))
            .chain(
                self.most_played(20)
                    .into_iter()
                    .map(|e| e.song_id),
            )
        {
            if seen.insert(id.clone()) {
                out.push(id);
            }
        }
        out
    }

    // ── Daily Discover keep-last-good ───────────────────────────────────

    pub fn last_daily_discover(&self) -> Vec<Song> {
        self.library.last_daily_discover.clone()
    }

    pub fn set_last_daily_discover(&mut self, songs: Vec<Song>) -> Result<(), String> {
        self.library.last_daily_discover = songs;
        self.save_library()
    }

    // ── downloads ───────────────────────────────────────────────────────

    pub fn find_download(&self, song_id: &str) -> Option<DownloadedSong> {
        self.downloads
            .iter()
            .find(|d| d.song_id == song_id)
            .cloned()
    }

    pub fn add_download(&mut self, row: DownloadedSong) -> Result<(), String> {
        self.downloads.retain(|d| d.song_id != row.song_id);
        self.downloads.push(row);
        self.save_downloads()
    }

    pub fn remove_download(&mut self, song_id: &str) -> Result<Option<DownloadedSong>, String> {
        let removed = self
            .downloads
            .iter()
            .find(|d| d.song_id == song_id)
            .cloned();
        self.downloads.retain(|d| d.song_id != song_id);
        self.save_downloads()?;
        Ok(removed)
    }

    /// Downloads whose file still exists; prunes (and persists) rows
    /// whose file is gone.
    pub fn downloads_pruned(&mut self) -> Result<Vec<DownloadedSong>, String> {
        let before = self.downloads.len();
        self.downloads
            .retain(|d| Path::new(&d.path).exists());
        if self.downloads.len() != before {
            self.save_downloads()?;
        }
        Ok(self.downloads.clone())
    }
}
