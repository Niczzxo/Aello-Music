//! Data models shared between the Rust core and the web frontend.
//!
//! Shapes deliberately mirror the Android app's concepts (songs, albums,
//! playlists, home sections) so a future port can reuse the same mental
//! model. Field names serialize as camelCase for the JavaScript side.

use serde::{Deserialize, Serialize};

/// A YouTube channel the signed-in Google account can act as (Android
/// `AccountInfo` from `YouTube.getAccountChannels`). `data_sync_id` is
/// the identity token: when set, InnerTube requests carry it as
/// `context.user.onBehalfOfUser` (Android `toContext`), so the account
/// acts as that channel. `None` means the primary identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountChannel {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub channel_handle: Option<String>,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub data_sync_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artwork {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumRef {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Song {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artists: Vec<Artist>,
    #[serde(default)]
    pub album: Option<AlbumRef>,
    #[serde(default)]
    pub duration_seconds: Option<u64>,
    #[serde(default)]
    pub artwork: Vec<Artwork>,
    /// Resolved playable URL. `None` until `resolve_stream` runs; the
    /// frontend must never guess a stream URL.
    #[serde(default)]
    pub stream_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artists: Vec<Artist>,
    pub year: Option<i32>,
    pub artwork: Vec<Artwork>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub artwork: Vec<Artwork>,
    pub song_count: Option<usize>,
}

/// One feed card. The `kind` tag tells the frontend which renderer to use.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HomeItem {
    Song(Song),
    Album(Album),
    Playlist(Playlist),
    /// Artist row (search / related). Added in 6.1.45 with the artist page.
    Artist(Artist),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeSection {
    pub title: String,
    pub subtitle: Option<String>,
    pub items: Vec<HomeItem>,
}

/// One Home chip (Android `HomePage.Chip`): the title shown on the chip
/// and the browse `params` a tap re-fetches the feed with. `params`
/// None = the chip carries no params (treated as "All").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeChip {
    pub title: String,
    pub params: Option<String>,
}

/// Home feed page (Android `HomePage`): chips + sections + the
/// continuation token for "load more" home sections, if YouTube served
/// one. `continuation` None means there is no next page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeFeed {
    pub chips: Vec<HomeChip>,
    pub sections: Vec<HomeSection>,
    pub continuation: Option<String>,
}

/// One page of search results (Android `SearchResult`): items + the
/// continuation token for the next page, if YouTube served one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub items: Vec<HomeItem>,
    pub continuation: Option<String>,
}

/// Personal Home sections (audit #3), built entirely from the local
/// library store — liked ∪ recent ∪ most-played + history. A section
/// is omitted by the frontend when its list is empty; nothing here is
/// invented.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalSections {
    pub quick_picks: Vec<Song>,
    pub keep_listening: Vec<Song>,
    pub forgotten_favorites: Vec<Song>,
    pub speed_dial: Vec<Song>,
}

/// Result of one YouTube sync run (audit #45). Additive only: remote
/// items are merged INTO the local library; nothing local is deleted
/// and nothing local is uploaded except likes the user taps (see
/// `like_video`). `errors` carries per-step failures — a partial sync
/// still reports what did land.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub liked_added: usize,
    pub library_added: usize,
    pub playlists_added: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistDetail {
    pub playlist: Playlist,
    pub songs: Vec<Song>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumDetail {
    pub album: Album,
    pub songs: Vec<Song>,
}

// ── 6.1.45 additions (RUST_API_CONTRACT.md) ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistDetail {
    pub artist: Artist,
    pub top_songs: Vec<Song>,
    pub albums: Vec<Album>,
    pub singles: Vec<Album>,
    pub related: Vec<Artist>,
    pub monthly_listeners: Option<String>,
    pub subscriber_count: Option<String>,
    pub description: Option<String>,
}

/// One Mood & Genres chip from the explore page. `params` is the browse
/// params string the frontend passes back to browse that mood.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoodItem {
    pub id: String,
    pub title: String,
    pub params: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreData {
    pub sections: Vec<HomeSection>,
    pub moods: Vec<MoodItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    pub time_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsResult {
    pub lines: Vec<LyricLine>,
    pub provider: String,
    pub synced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedSong {
    pub song_id: String,
    pub title: String,
    pub artists: Vec<Artist>,
    pub path: String,
    pub size_bytes: u64,
    pub downloaded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryPlaylist {
    pub id: String,
    pub name: String,
    pub song_ids: Vec<String>,
    pub created_at: String,
    /// YouTube browse id of a playlist imported by sync (`None` for
    /// local-only playlists). Used to skip re-importing it on the next
    /// sync run.
    #[serde(default)]
    pub browse_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub song_id: String,
    pub title: String,
    pub artists: Vec<Artist>,
    pub played_at: String,
    pub play_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryData {
    pub liked_ids: Vec<String>,
    pub playlists: Vec<LibraryPlaylist>,
    pub history: Vec<HistoryEntry>,
    pub speed_dial_ids: Vec<String>,
    /// Songs in the YouTube library (synced `FEmusic_liked_videos`)
    /// that are not necessarily liked. Empty until a sync runs.
    #[serde(default)]
    pub library_songs: Vec<Song>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub version: String,
    pub notes: String,
    pub exe_url: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqBand {
    pub freq_hz: f64,
    pub gain_db: f64,
    pub q: f64,
    /// "lowshelf" | "peaking" | "highshelf"
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqProfile {
    pub id: String,
    pub name: String,
    pub bands: Vec<EqBand>,
    pub preamp_db: f64,
}
