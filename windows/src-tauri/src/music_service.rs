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

use crate::models::{AlbumDetail, HomeItem, HomeSection, PlaylistDetail};

/// Errors surface to the frontend as plain strings so the UI can show the
/// real reason instead of a silent failure.
pub type ServiceResult<T> = Result<T, String>;

/// The contract every music backend must satisfy. Async functions in
/// traits are stable since Rust 1.75, so no extra async-trait crate is
/// needed.
pub trait MusicService: Send + Sync {
    /// Home feed sections.
    async fn home_feed(&self) -> ServiceResult<Vec<HomeSection>>;
    /// Search across songs, albums, and playlists. Artist rows are
    /// intentionally not returned in v0.1.0 (there is no artist page
    /// to open them from); searching an artist still returns their
    /// songs.
    async fn search(&self, query: &str) -> ServiceResult<Vec<HomeItem>>;
    /// A playlist and its songs.
    async fn playlist(&self, id: &str) -> ServiceResult<PlaylistDetail>;
    /// An album and its songs.
    async fn album(&self, id: &str) -> ServiceResult<AlbumDetail>;
    /// Resolve a playable stream URL for a song. `Ok(None)` means the
    /// backend is reachable but has no playable stream for this song.
    /// `Err` carries the real reason (e.g. YouTube's playabilityStatus
    /// reason) for the UI to show.
    async fn resolve_stream(&self, song_id: &str) -> ServiceResult<Option<String>>;
}
