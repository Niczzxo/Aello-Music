//! Music backend abstraction.
//!
//! THE REAL BACKEND (v0.1.0) is a pure-Rust port of the Android app's
//! unofficial InnerTube / YouTube Music client — see `innertube.rs`.
//! Guest only: no login, no cookies, no PoToken; only `visitorData`.
//! Format selection prefers AAC/MP4 (itag 140) because WebView2's
//! `<audio>` cannot play Opus-in-WebM; every failure surfaces the real
//! reason via `StreamError` instead of failing silently.
//!
//! PROXY NOTE: browser-style clients (plain web pages, YouTube.js in a
//! browser) must proxy InnerTube requests through their own server,
//! because the browser enforces CORS. This app does NOT need that proxy:
//! the Rust core is a native HTTP client, so it can call the endpoints
//! directly, exactly as the Android app does. The frontend must never
//! call the endpoints itself — every backend call goes through a Tauri
//! command in `lib.rs`, keeping the request path in Rust.

use crate::models::{
    AlbumDetail, ArtistDetail, ExploreData, HomeFeed, HomeItem, HomeSection, LyricLine,
    PlaylistDetail, SearchPage, Song,
};

/// Errors surface to the frontend as plain strings so the UI can show the
/// real reason instead of a silent failure.
pub type ServiceResult<T> = Result<T, String>;

/// The contract every music backend must satisfy. Async functions in
/// traits are stable since Rust 1.75, so no extra async-trait crate is
/// needed.
pub trait MusicService: Send + Sync {
    /// Home feed sections + chips + continuation. `params` is a chip's
    /// browse params (None = unfiltered feed, Android `YouTube.home`).
    async fn home_feed(&self, params: Option<&str>) -> ServiceResult<HomeFeed>;
    /// Next home page from a continuation token (Android
    /// `YouTube.home(continuation)`).
    async fn home_feed_more(&self, continuation: &str) -> ServiceResult<HomeFeed>;
    /// Search across songs, albums, playlists, and artists. `filter` is
    /// "song" | "album" | "artist" | "playlist" (None = all). Returns
    /// the page + continuation token (Android `SearchResult`).
    async fn search(&self, query: &str, filter: Option<&str>) -> ServiceResult<SearchPage>;
    /// Next search page from a continuation token (Android
    /// `YouTube.searchContinuation`).
    async fn search_more(&self, continuation: &str) -> ServiceResult<SearchPage>;
    /// A signed-in library page (Android `YouTube.library`): items +
    /// continuation for `browse_id` at `tab_index`.
    async fn library_page(
        &self,
        browse_id: &str,
        tab_index: usize,
    ) -> ServiceResult<SearchPage>;
    /// Next library page from a continuation token (Android
    /// `YouTube.libraryContinuation`).
    async fn library_page_more(&self, continuation: &str) -> ServiceResult<SearchPage>;
    /// Like / unlike a video on YouTube (Android `YouTube.likeVideo`).
    async fn like_video(&self, video_id: &str, liked: bool) -> ServiceResult<()>;
    /// Subscribe / unsubscribe a channel (Android
    /// `YouTube.subscribeChannel`).
    async fn subscribe_channel(&self, channel_id: &str, subscribe: bool) -> ServiceResult<()>;
    /// A playlist and its songs.
    async fn playlist(&self, id: &str) -> ServiceResult<PlaylistDetail>;
    /// An album and its songs.
    async fn album(&self, id: &str) -> ServiceResult<AlbumDetail>;
    /// An artist page: top songs, albums, singles, related artists.
    async fn artist(&self, id: &str) -> ServiceResult<ArtistDetail>;
    /// Charts / trending sections ("Top music videos" kept as its own).
    async fn charts(&self) -> ServiceResult<Vec<HomeSection>>;
    /// Explore page: sections + Mood & Genres chips.
    async fn explore(&self) -> ServiceResult<ExploreData>;
    /// Mood & Genres detail page for a mood's browse params.
    async fn browse_mood(&self, params: &str) -> ServiceResult<Vec<HomeItem>>;
    /// Related songs for a video (`next` → related endpoint → `related`).
    async fn related_for_video(&self, video_id: &str) -> ServiceResult<Vec<Song>>;
    /// Timed-text transcript lines for a video (lyrics fallback).
    async fn transcript(&self, video_id: &str) -> ServiceResult<Vec<LyricLine>>;
    /// Resolve a playable stream URL for a song. `Ok(None)` means the
    /// backend is reachable but has no playable stream for this song.
    /// `Err` carries the real reason (e.g. YouTube's playabilityStatus
    /// reason) for the UI to show. `audio_quality` is the stored
    /// setting ("auto" | "high" | "low"): low prefers itag 139,
    /// auto/high prefer itag 140.
    async fn resolve_stream(
        &self,
        song_id: &str,
        audio_quality: &str,
    ) -> ServiceResult<Option<String>>;
}
