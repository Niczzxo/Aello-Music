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
    DownloadedSong, HistoryEntry, LibraryData, LibraryPlaylist, PersonalSections, Song,
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
    /// Full songs for pinned ids (a pinned song may be neither liked
    /// nor in history — the id alone cannot render its card).
    speed_dial_songs: Vec<Song>,
    /// Songs in the YouTube library (sync `FEmusic_liked_videos`) that
    /// are not necessarily liked. Empty until a sync runs.
    library_songs: Vec<Song>,
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
            library_songs: self.library.library_songs.clone(),
        }
    }

    /// Add songs to the liked store that are not there yet. Returns
    /// how many were added. Used by YouTube sync (additive only — it
    /// never removes a local like).
    pub fn add_liked_songs(&mut self, songs: &[Song]) -> Result<usize, String> {
        let mut added = 0;
        for song in songs {
            if !self.is_liked(&song.id) {
                self.library.liked_songs.push(song.clone());
                added += 1;
            }
        }
        if added > 0 {
            self.save_library()?;
        }
        Ok(added)
    }

    /// Add songs to the YouTube-library store that are not there yet.
    /// Returns how many were added (additive only).
    pub fn add_library_songs(&mut self, songs: &[Song]) -> Result<usize, String> {
        let mut added = 0;
        for song in songs {
            if !self
                .library
                .library_songs
                .iter()
                .any(|s| s.id == song.id)
            {
                self.library.library_songs.push(song.clone());
                added += 1;
            }
        }
        if added > 0 {
            self.save_library()?;
        }
        Ok(added)
    }

    /// Import a YouTube playlist as a local playlist, unless a
    /// playlist with the same YouTube browse id was already imported.
    /// Returns the created playlist, or None when it already exists.
    pub fn import_synced_playlist(
        &mut self,
        name: &str,
        browse_id: &str,
        song_ids: &[String],
    ) -> Result<Option<LibraryPlaylist>, String> {
        if self
            .library
            .playlists
            .iter()
            .any(|p| p.browse_id.as_deref() == Some(browse_id))
        {
            return Ok(None);
        }
        let mut playlist = LibraryPlaylist {
            id: unique_id("pl"),
            name: name.to_string(),
            song_ids: song_ids.to_vec(),
            created_at: now_iso(),
            browse_id: Some(browse_id.to_string()),
        };
        // Dedup song ids, preserving order.
        let mut seen = HashSet::new();
        playlist.song_ids.retain(|id| seen.insert(id.clone()));
        self.library.playlists.push(playlist.clone());
        self.save_library()?;
        Ok(Some(playlist))
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
            browse_id: None,
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

    // ── Home personal sections (audit #3) ─────────────────────────────
    //
    // Built only from the local store — liked songs, history, play
    // counts. No network, no invented items: a section the store
    // cannot fill comes back empty and the frontend hides it.

    /// Quick Picks: liked songs interleaved with recent history,
    /// shuffled, 12 max. Keep Listening: history in recency order, 12
    /// max. Forgotten Favorites: liked songs whose last play is the
    /// oldest (never-played first), 12 max. Speed Dial: pinned songs
    /// in pin order, 27 max (the Android cap).
    pub fn personal_sections(&self) -> PersonalSections {
        let history_song = |entry: &HistoryEntry| Song {
            id: entry.song_id.clone(),
            title: entry.title.clone(),
            artists: entry.artists.clone(),
            album: None,
            duration_seconds: None,
            artwork: Vec::new(),
            stream_url: None,
        };
        // Quick Picks: liked (up to 8) + recent (up to 8), interleaved.
        let mut quick: Vec<Song> = Vec::new();
        let mut seen = HashSet::new();
        let liked = &self.library.liked_songs;
        let recent: Vec<Song> = self.history.iter().take(8).map(history_song).collect();
        for i in 0..8 {
            for song in [liked.get(i), recent.get(i)] {
                if let Some(song) = song {
                    if seen.insert(song.id.clone()) {
                        quick.push(song.clone());
                    }
                }
            }
        }
        quick.truncate(12);
        // Keep Listening: most recent first.
        let keep: Vec<Song> = self
            .history
            .iter()
            .take(12)
            .map(history_song)
            .collect();
        // Forgotten Favorites: liked songs sorted by last play,
        // oldest first; never played sorts before any played date
        // (ISO timestamps compare lexicographically).
        let last_played = |id: &str| {
            self.history
                .iter()
                .find(|e| e.song_id == id)
                .map(|e| e.played_at.clone())
                .unwrap_or_default()
        };
        let mut forgotten: Vec<Song> = liked.clone();
        forgotten.sort_by(|a, b| last_played(&a.id).cmp(&last_played(&b.id)));
        forgotten.truncate(12);
        // Speed Dial: pinned songs in pin order; fall back to liked /
        // history when a pinned id has no stored song (older pins).
        let speed: Vec<Song> = self
            .library
            .speed_dial_ids
            .iter()
            .filter_map(|id| {
                self.library
                    .speed_dial_songs
                    .iter()
                    .find(|s| &s.id == id)
                    .cloned()
                    .or_else(|| liked.iter().find(|s| &s.id == id).cloned())
                    .or_else(|| {
                        self.history
                            .iter()
                            .find(|e| &e.song_id == id)
                            .map(history_song)
                    })
            })
            .collect();
        PersonalSections {
            quick_picks: quick,
            keep_listening: keep,
            forgotten_favorites: forgotten,
            speed_dial: speed,
        }
    }

    // ── Speed Dial pinning (audit #18, max 27 — the Android cap) ──────

    /// Pin or unpin a song. Returns true when the song is pinned
    /// afterwards. Pinning stores the full song (an id alone cannot
    /// render its card); unpinning drops the id and the stored song.
    /// The 27-cap is enforced: pinning a 28th song is refused.
    pub fn toggle_speed_dial(&mut self, song: &Song) -> Result<bool, String> {
        if self
            .library
            .speed_dial_ids
            .iter()
            .any(|id| id == &song.id)
        {
            self.library.speed_dial_ids.retain(|id| id != &song.id);
            self.library.speed_dial_songs.retain(|s| s.id != song.id);
            self.save_library()?;
            return Ok(false);
        }
        if self.library.speed_dial_ids.len() >= 27 {
            return Err("Speed Dial is full (27 songs max) — unpin one first.".to_string());
        }
        self.library.speed_dial_ids.push(song.id.clone());
        self.library.speed_dial_songs.push(song.clone());
        self.save_library()?;
        Ok(true)
    }

    // ── Backup / restore + CSV import (audit #17) ──────────────────────

    /// Full library backup as JSON: liked songs, playlists, speed-dial
    /// ids + songs, library songs, history. Downloads are NOT included
    /// (the audio files stay on disk; their rows live in
    /// downloads.json and are re-found by path).
    pub fn export_library_json(&self) -> Result<String, String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Backup<'a> {
            format: &'a str,
            liked_songs: &'a [Song],
            playlists: &'a [LibraryPlaylist],
            speed_dial_ids: &'a [String],
            speed_dial_songs: &'a [Song],
            library_songs: &'a [Song],
            history: &'a [HistoryEntry],
        }
        let backup = Backup {
            format: "aello-library-backup-v1",
            liked_songs: &self.library.liked_songs,
            playlists: &self.library.playlists,
            speed_dial_ids: &self.library.speed_dial_ids,
            speed_dial_songs: &self.library.speed_dial_songs,
            library_songs: &self.library.library_songs,
            history: &self.history,
        };
        serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())
    }

    /// Restore a backup produced by `export_library_json` (or an older
    /// backup missing the newer fields — `#[serde(default)]` on
    /// `LibraryFile` covers that). Replaces the whole library store.
    pub fn import_library_json(&mut self, text: &str) -> Result<(), String> {
        #[derive(Default, Deserialize)]
        #[serde(rename_all = "camelCase", default)]
        struct Backup {
            format: String,
            liked_songs: Vec<Song>,
            playlists: Vec<LibraryPlaylist>,
            speed_dial_ids: Vec<String>,
            speed_dial_songs: Vec<Song>,
            library_songs: Vec<Song>,
            history: Vec<HistoryEntry>,
        }
        let backup: Backup =
            serde_json::from_str(text).map_err(|e| format!("Not an Aello library backup: {e}"))?;
        if !backup.format.is_empty() && backup.format != "aello-library-backup-v1" {
            return Err(format!("Unsupported backup format: {}", backup.format));
        }
        // A backup must carry at least one section to be plausible —
        // an empty file restores nothing and wipes the library.
        if backup.liked_songs.is_empty()
            && backup.playlists.is_empty()
            && backup.history.is_empty()
        {
            return Err("Backup is empty — nothing to restore.".to_string());
        }
        self.library.liked_songs = backup.liked_songs;
        self.library.playlists = backup.playlists;
        self.library.speed_dial_ids = backup.speed_dial_ids;
        self.library.speed_dial_songs = backup.speed_dial_songs;
        self.library.library_songs = backup.library_songs;
        self.history = backup.history;
        self.history.truncate(500);
        self.save_library()?;
        self.save_history()
    }

    /// Import a CSV playlist. Format (one song per line, documented in
    /// the Library UI): `videoId,title,artist1,artist2,…`. A header
    /// line (`videoId,…` or `id,…`) is skipped; lines without a valid
    /// 11-character video id are skipped, never fatal.
    pub fn import_csv_playlist(
        &mut self,
        name: &str,
        csv: &str,
    ) -> Result<LibraryPlaylist, String> {
        let mut song_ids: Vec<String> = Vec::new();
        for raw_line in csv.lines() {
            let line = raw_line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split(',');
            let Some(id) = parts.next().map(str::trim) else {
                continue;
            };
            if id.eq_ignore_ascii_case("videoid") || id.eq_ignore_ascii_case("id") {
                continue; // header row
            }
            if id.len() != 11 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                continue;
            }
            if !song_ids.iter().any(|s| s == id) {
                song_ids.push(id.to_string());
            }
        }
        if song_ids.is_empty() {
            return Err(
                "No songs found in that CSV — lines must start with an 11-character YouTube video id."
                    .to_string(),
            );
        }
        let mut playlist = LibraryPlaylist {
            id: unique_id("pl"),
            name: name.to_string(),
            song_ids,
            created_at: now_iso(),
            browse_id: None,
        };
        let mut seen = HashSet::new();
        playlist.song_ids.retain(|id| seen.insert(id.clone()));
        self.library.playlists.push(playlist.clone());
        self.save_library()?;
        Ok(playlist)
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

    /// Remove every download row AND its file (storage management,
    /// audit #50). Returns how many files were removed.
    pub fn clear_downloads(&mut self) -> Result<usize, String> {
        let removed = self.downloads.len();
        for row in &self.downloads {
            let _ = std::fs::remove_file(&row.path);
        }
        self.downloads.clear();
        self.save_downloads()?;
        Ok(removed)
    }
}
