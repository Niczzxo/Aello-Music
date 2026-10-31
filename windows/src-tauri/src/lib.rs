//! Aello for Windows — Tauri application library (v6.1.45 milestone).
//!
//! Status: the Rust core covers the full 6.1.45 contract
//! (RUST_API_CONTRACT.md): InnerTube browsing (artist/charts/explore/
//! related), library store, lyrics, downloads, update check, Listen
//! Together room creation, Google sign-in (cookie in the Rust store
//! only), and Windows SMTC. The Windows build runs on GitHub Actions;
//! nothing here has run on a real Windows PC yet — the user's PC test
//! is the acceptance check. See README.md and BUILD_PLAN.md.
//!
//! Architecture: the Rust core owns settings persistence, the music-service
//! abstraction, the library store, and the playback snapshot. The web
//! frontend owns rendering and audio output (the `<audio>` element inside
//! WebView2). The frontend never talks to music backends directly; every
//! backend call goes through a Tauri command below so the Rust side can
//! enforce the request path (see `music_service.rs` for the InnerTube
//! port notes).

mod innertube;
mod library;
mod local_files;
mod lyrics;
mod models;
mod music_service;
mod player;
mod settings;
mod signin_capture;
#[cfg(windows)]
mod smtc;
mod update;

use innertube::InnerTubeSession;
use library::{now_iso, LibraryStore};
use models::{
    AccountChannel, Artist, ArtistDetail, DownloadedSong, ExploreData, HomeFeed, HomeItem,
    HomeSection, LibraryData, LibraryPlaylist, LyricsResult, PersonalSections, SearchPage, Song,
    SyncReport, UpdateInfo,
};
use music_service::MusicService;
use player::{PlaybackSnapshot, PlayerSession};
use serde::Serialize;
use settings::{AppSettings, SettingsStore};
use signin_capture::CaptureOutcome;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

/// Shared state managed by Tauri and handed to commands.
pub struct AppState {
    music: InnerTubeSession,
    player: PlayerSession,
    settings: Mutex<SettingsStore>,
    library: Mutex<LibraryStore>,
    #[cfg(windows)]
    smtc: Mutex<Option<smtc::SmtcManager>>,
}

/// Fisher–Yates shuffle with a xorshift seed from the wall clock
/// (no `rand` dependency; shuffling is cosmetic, not cryptographic).
fn shuffle<T>(items: &mut [T]) {
    if items.len() < 2 {
        return;
    }
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    if seed == 0 {
        seed = 0x9E37_79B9_7F4A_7C15;
    }
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for i in (1..items.len()).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

// ── catalogue ───────────────────────────────────────────────────────────

#[tauri::command]
async fn get_home_feed(
    state: State<'_, AppState>,
    params: Option<String>,
) -> Result<HomeFeed, String> {
    MusicService::home_feed(&state.music, params.as_deref()).await
}

/// Next home page from a continuation token (audit #1 load-more).
#[tauri::command]
async fn get_home_feed_more(
    state: State<'_, AppState>,
    continuation: String,
) -> Result<HomeFeed, String> {
    MusicService::home_feed_more(&state.music, &continuation).await
}

#[tauri::command]
async fn search_music(
    state: State<'_, AppState>,
    query: String,
    filter: Option<String>,
) -> Result<SearchPage, String> {
    MusicService::search(&state.music, &query, filter.as_deref()).await
}

/// Next search page from a continuation token (audit #5).
#[tauri::command]
async fn search_music_more(
    state: State<'_, AppState>,
    continuation: String,
) -> Result<SearchPage, String> {
    MusicService::search_more(&state.music, &continuation).await
}

/// Related songs for one video (radio / autoplay, audit #28). The
/// frontend calls this when the queue runs out.
#[tauri::command]
async fn get_related(state: State<'_, AppState>, video_id: String) -> Result<Vec<Song>, String> {
    MusicService::related_for_video(&state.music, &video_id).await
}

/// Home personal sections from the local library store (audit #3):
/// Quick Picks / Keep Listening / Forgotten Favorites / Speed Dial.
#[tauri::command]
fn get_home_personal(state: State<'_, AppState>) -> Result<PersonalSections, String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())
        .map(|store| store.personal_sections())
}

#[tauri::command]
async fn get_playlist(state: State<'_, AppState>, id: String) -> Result<models::PlaylistDetail, String> {
    MusicService::playlist(&state.music, &id).await
}

#[tauri::command]
async fn get_album(state: State<'_, AppState>, id: String) -> Result<models::AlbumDetail, String> {
    MusicService::album(&state.music, &id).await
}

#[tauri::command]
async fn get_artist(state: State<'_, AppState>, id: String) -> Result<ArtistDetail, String> {
    MusicService::artist(&state.music, &id).await
}

#[tauri::command]
async fn get_trending(state: State<'_, AppState>) -> Result<Vec<HomeSection>, String> {
    MusicService::charts(&state.music).await
}

#[tauri::command]
async fn get_explore(state: State<'_, AppState>) -> Result<ExploreData, String> {
    MusicService::explore(&state.music).await
}

#[tauri::command]
async fn browse_mood(state: State<'_, AppState>, params: String) -> Result<Vec<HomeItem>, String> {
    MusicService::browse_mood(&state.music, &params).await
}

#[tauri::command]
async fn resolve_stream(
    state: State<'_, AppState>,
    song_id: String,
) -> Result<Option<String>, String> {
    // Call the MusicService trait method explicitly: the inherent
    // `InnerTubeSession::resolve_stream` returns `StreamUrl`, while the
    // trait contract (and this command) return `Option<String>`.
    // The stored audio-quality setting drives format selection
    // (itag 140 for auto/high, 139 for low).
    let audio_quality = state
        .settings
        .lock()
        .map(|store| store.current().audio_quality.clone())
        .unwrap_or_else(|_| "auto".to_string());
    MusicService::resolve_stream(&state.music, &song_id, &audio_quality).await
}

/// Related songs for one seed, or empty when there is no seed / the
/// fetch fails. Daily Discover runs up to 3 of these in parallel
/// (R1: the old sequential `for seed` loop was 6 round trips in
/// series on the Home load path).
async fn related_or_empty(music: &InnerTubeSession, seed: Option<String>) -> Vec<Song> {
    match seed {
        Some(id) => MusicService::related_for_video(music, &id)
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Daily Discover (Android `HomeViewModel.getDailyDiscover`): 3 random
/// seeds from liked ∪ recent ∪ most-played → `next`/`related`, 6 songs
/// per seed. Keep-last-good: an empty fetch returns the previous list;
/// with no previous list at all, fall back to a "top songs" search (25).
#[tauri::command]
async fn get_daily_discover(state: State<'_, AppState>) -> Result<Vec<Song>, String> {
    let mut seeds = state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())
        .map(|store| store.seed_ids())
        .unwrap_or_default();
    shuffle(&mut seeds);
    seeds.truncate(3);
    // Per-seed fetches run in parallel (R1), not in series.
    let (r0, r1, r2) = tokio::join!(
        related_or_empty(&state.music, seeds.get(0).cloned()),
        related_or_empty(&state.music, seeds.get(1).cloned()),
        related_or_empty(&state.music, seeds.get(2).cloned()),
    );
    let mut songs: Vec<Song> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (i, mut related) in [r0, r1, r2].into_iter().enumerate() {
        let seed = seeds.get(i);
        shuffle(&mut related);
        for song in related.into_iter().take(6) {
            if Some(&song.id) != seed && seen.insert(song.id.clone()) {
                songs.push(song);
            }
        }
    }
    if !songs.is_empty() {
        if let Ok(mut store) = state.library.lock() {
            let _ = store.set_last_daily_discover(songs.clone());
        }
        return Ok(songs);
    }
    // Keep-last-good.
    if let Ok(store) = state.library.lock() {
        let last = store.last_daily_discover();
        if !last.is_empty() {
            return Ok(last);
        }
    }
    // No-history fallback: top songs search, shuffled, 25.
    match MusicService::search(&state.music, "top songs", Some("song")).await {
        Ok(page) => {
            let mut top: Vec<Song> = page
                .items
                .into_iter()
                .filter_map(|i| match i {
                    HomeItem::Song(s) => Some(s),
                    _ => None,
                })
                .collect();
            shuffle(&mut top);
            top.truncate(25);
            if !top.is_empty() {
                if let Ok(mut store) = state.library.lock() {
                    let _ = store.set_last_daily_discover(top.clone());
                }
            }
            Ok(top)
        }
        Err(e) => Err(format!("Daily Discover unavailable: {e}")),
    }
}

// ── lyrics ──────────────────────────────────────────────────────────────

#[tauri::command]
async fn get_lyrics(
    state: State<'_, AppState>,
    song_id: String,
    title: String,
    artist: String,
    album: Option<String>,
    duration_secs: Option<u64>,
) -> Result<LyricsResult, String> {
    lyrics::get_lyrics(
        &state.music,
        &song_id,
        &title,
        &artist,
        album.as_deref(),
        duration_secs,
    )
    .await
}

// ── downloads ───────────────────────────────────────────────────────────

/// Resolve + stream a song to `downloads/<songId>.m4a` and record it.
/// Shared by the `download_song` command and the auto-download-on-like
/// background task (best-effort there; errors are swallowed by the
/// caller, never by the like action).
async fn download_song_inner(
    app: &tauri::AppHandle,
    state: &AppState,
    song: &Song,
) -> Result<DownloadedSong, String> {
    if let Ok(store) = state.library.lock() {
        if let Some(row) = store.find_download(&song.id) {
            if std::path::Path::new(&row.path).exists() {
                return Ok(row);
            }
        }
    }
    let audio_quality = state
        .settings
        .lock()
        .map(|store| store.current().audio_quality.clone())
        .unwrap_or_else(|_| "auto".to_string());
    let url = MusicService::resolve_stream(&state.music, &song.id, &audio_quality)
        .await?
        .ok_or_else(|| format!("No playable stream for {}", song.id))?;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("downloads");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.m4a", song.id));
    let size = state
        .music
        .download_to_file(&url, &path)
        .await
        .map_err(|e| e.to_string())?;
    if size == 0 {
        let _ = std::fs::remove_file(&path);
        return Err(format!("Downloaded empty file for {}", song.id));
    }
    let row = DownloadedSong {
        song_id: song.id.clone(),
        title: song.title.clone(),
        artists: song.artists.clone(),
        path: path.to_string_lossy().into_owned(),
        size_bytes: size,
        downloaded_at: now_iso(),
    };
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .add_download(row.clone())?;
    Ok(row)
}

#[tauri::command]
async fn download_song(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    song_id: String,
    title: String,
    artists: Vec<Artist>,
) -> Result<DownloadedSong, String> {
    let song = Song {
        id: song_id,
        title,
        artists,
        album: None,
        duration_seconds: None,
        artwork: Vec::new(),
        stream_url: None,
    };
    download_song_inner(&app, &state, &song).await
}

#[tauri::command]
fn get_downloads(state: State<'_, AppState>) -> Result<Vec<DownloadedSong>, String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .downloads_pruned()
}

#[tauri::command]
fn remove_download(state: State<'_, AppState>, song_id: String) -> Result<(), String> {
    let removed = state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .remove_download(&song_id)?;
    if let Some(row) = removed {
        let _ = std::fs::remove_file(&row.path);
    }
    Ok(())
}

// ── library ─────────────────────────────────────────────────────────────

#[tauri::command]
fn get_library(state: State<'_, AppState>) -> Result<LibraryData, String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())
        .map(|store| store.library_data())
}

#[tauri::command]
async fn like_song(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    song: Song,
    liked: bool,
) -> Result<(), String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .set_liked(&song, liked)?;
    // Auto-download on like + YouTube like push: best-effort
    // background tasks — they must never fail the like itself. The
    // remote push (Android `SyncUtils.likeSong` → `YouTube.likeVideo`)
    // only runs when signed in; a failed push keeps the local like
    // and is reported by the next sync, never by failing this call.
    if liked {
        let auto = state
            .settings
            .lock()
            .map(|s| s.current().auto_download_on_like)
            .unwrap_or(false);
        if auto {
            let app2 = app.clone();
            let song2 = song.clone();
            tauri::async_runtime::spawn(async move {
                let state2 = app2.state::<AppState>();
                let _ = download_song_inner(&app2, &state2, &song2).await;
            });
        }
    }
    {
        let app2 = app.clone();
        let song2 = song.clone();
        tauri::async_runtime::spawn(async move {
            let state2 = app2.state::<AppState>();
            let signed_in = state2
                .settings
                .lock()
                .map(|s| s.auth().cookie.is_some())
                .unwrap_or(false);
            if signed_in {
                let _ = MusicService::like_video(&state2.music, &song2.id, liked).await;
            }
        });
    }
    Ok(())
}

/// Pin / unpin a song on the Speed Dial (audit #18, max 27).
/// Returns true when the song is pinned afterwards.
#[tauri::command]
fn toggle_speed_dial(state: State<'_, AppState>, song: Song) -> Result<bool, String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .toggle_speed_dial(&song)
}

/// Full library backup as JSON (audit #17).
#[tauri::command]
fn export_library(state: State<'_, AppState>) -> Result<String, String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .export_library_json()
}

/// Restore a library backup (audit #17). Replaces the whole store.
#[tauri::command]
fn import_library(state: State<'_, AppState>, backup_json: String) -> Result<(), String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .import_library_json(&backup_json)
}

/// Import a CSV playlist (audit #17): lines of
/// `videoId,title,artist,…`.
#[tauri::command]
fn import_csv_playlist(
    state: State<'_, AppState>,
    name: String,
    csv: String,
) -> Result<LibraryPlaylist, String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .import_csv_playlist(&name, &csv)
}

/// Remove every download row AND its file (audit #50 storage).
#[tauri::command]
fn clear_downloads(state: State<'_, AppState>) -> Result<usize, String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .clear_downloads()
}

/// Copy a downloaded song to a folder the user picks (audit #41 song
/// export). Returns the destination path, or None when the user
/// cancels the folder picker.
#[tauri::command]
fn export_song(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    song_id: String,
) -> Result<Option<String>, String> {
    let row = state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .find_download(&song_id)
        .ok_or_else(|| "That song is not downloaded yet.".to_string())?;
    let source = std::path::PathBuf::from(&row.path);
    let file_name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("{song_id}.m4a"));
    let dest_dir = app
        .dialog()
        .file()
        .set_title("Choose a folder to export the song to")
        .blocking_pick_folder();
    let Some(dest_dir) = dest_dir else {
        return Ok(None);
    };
    let dest_dir = match dest_dir {
        tauri_plugin_dialog::FilePath::Path(path) => path,
        tauri_plugin_dialog::FilePath::Url(url) => url
            .to_file_path()
            .map_err(|_| "That folder could not be used.".to_string())?,
    };
    let dest = dest_dir.join(&file_name);
    std::fs::copy(&source, &dest).map_err(|e| e.to_string())?;
    Ok(Some(dest.to_string_lossy().into_owned()))
}

/// Subscribe / unsubscribe an artist on YouTube (audit #7; Android
/// `YouTube.subscribeChannel` → `subscription/subscribe`). Requires a
/// signed-in account — otherwise the honest error below.
#[tauri::command]
async fn subscribe_artist(
    state: State<'_, AppState>,
    channel_id: String,
    subscribe: bool,
) -> Result<(), String> {
    let signed_in = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())
        .map(|s| s.auth().cookie.is_some())
        .unwrap_or(false);
    if !signed_in {
        return Err(
            "Subscribing needs a signed-in account — sign in with Google first.".to_string(),
        );
    }
    MusicService::subscribe_channel(&state.music, &channel_id, subscribe).await
}

#[tauri::command]
fn create_playlist(state: State<'_, AppState>, name: String) -> Result<LibraryPlaylist, String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .create_playlist(&name)
}

#[tauri::command]
fn rename_playlist(state: State<'_, AppState>, id: String, name: String) -> Result<(), String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .rename_playlist(&id, &name)
}

#[tauri::command]
fn delete_playlist(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .delete_playlist(&id)
}

#[tauri::command]
fn playlist_add_song(state: State<'_, AppState>, id: String, song: Song) -> Result<(), String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .playlist_add_song(&id, &song)
}

#[tauri::command]
fn playlist_remove_song(
    state: State<'_, AppState>,
    id: String,
    song_id: String,
) -> Result<(), String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .playlist_remove_song(&id, &song_id)
}

#[tauri::command]
fn add_to_history(state: State<'_, AppState>, song: Song) -> Result<(), String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .add_to_history(&song)
}

#[tauri::command]
fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())?
        .clear_history()
}

#[tauri::command]
fn get_stats(state: State<'_, AppState>) -> Result<Vec<models::HistoryEntry>, String> {
    state
        .library
        .lock()
        .map_err(|_| "library lock poisoned".to_string())
        .map(|store| store.most_played(25))
}

/// YouTube two-way sync, pull side (audit #45; Android
/// `SyncUtils.executeFullSync`, same endpoints, same order).
///
/// Additive only, by design: remote liked songs and library songs are
/// merged INTO the local store, and remote saved playlists are
/// imported as local playlists. Nothing local is deleted, and nothing
/// local is uploaded by a sync run (likes are pushed at tap time, in
/// `like_song`). A step that fails is recorded in `errors` and the
/// other steps still run — a partial sync reports what did land.
///
/// NOT synced (no local store exists for them yet — see the audit
/// update): subscriptions (no subscribed-artists store), liked /
/// uploaded albums (no albums store), uploaded songs (uploads have no
/// store; the `FEmusic_library_privately_owned_tracks` tab path exists
/// in `library_page` but is untested live).
#[tauri::command]
async fn sync_youtube(state: State<'_, AppState>) -> Result<SyncReport, String> {
    let signed_in = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())
        .map(|s| s.auth().cookie.is_some())
        .unwrap_or(false);
    if !signed_in {
        return Err("Sign in with Google first — sync needs your YouTube account.".to_string());
    }
    let mut report = SyncReport {
        liked_added: 0,
        library_added: 0,
        playlists_added: 0,
        errors: Vec::new(),
    };
    // 1. Liked songs: the "LM" playlist (Android
    // `executeSyncLikedSongs` → `YouTube.playlist("LM")`).
    match MusicService::playlist(&state.music, "LM").await {
        Ok(detail) => {
            let added = state
                .library
                .lock()
                .map_err(|_| "library lock poisoned".to_string())?
                .add_liked_songs(&detail.songs)?;
            report.liked_added = added;
        }
        Err(e) => report.errors.push(format!("Liked songs: {e}")),
    }
    // 2. Library songs: `FEmusic_liked_videos` (Android
    // `executeSyncLibrarySongs`), following continuations (cap 3 extra
    // pages — a huge library still syncs its first pages).
    match MusicService::library_page(&state.music, "FEmusic_liked_videos", 0).await {
        Ok(mut page) => {
            let mut songs: Vec<Song> = Vec::new();
            let mut pages = 0;
            loop {
                for item in page.items {
                    if let HomeItem::Song(song) = item {
                        songs.push(song);
                    }
                }
                let Some(token) = page.continuation else {
                    break;
                };
                if pages >= 3 {
                    break;
                }
                pages += 1;
                match MusicService::library_page_more(&state.music, &token).await {
                    Ok(next) => page = next,
                    Err(e) => {
                        report.errors.push(format!("Library songs (page {}): {e}", pages + 1));
                        break;
                    }
                }
            }
            let added = state
                .library
                .lock()
                .map_err(|_| "library lock poisoned".to_string())?
                .add_library_songs(&songs)?;
            report.library_added = added;
        }
        Err(e) => report.errors.push(format!("Library songs: {e}")),
    }
    // 3. Saved playlists: `FEmusic_liked_playlists` (Android
    // `executeSyncSavedPlaylists`). "LM" (Liked Music) and "SE" are
    // skipped, exactly as Android does. Each new playlist's songs are
    // fetched (cap 10 playlists per run, to bound sync time).
    match MusicService::library_page(&state.music, "FEmusic_liked_playlists", 0).await {
        Ok(page) => {
            let mut fetched = 0;
            for item in page.items {
                let HomeItem::Playlist(playlist) = item else {
                    continue;
                };
                if playlist.id == "LM" || playlist.id == "SE" {
                    continue;
                }
                // Skip playlists already imported (by browse id).
                let already = state
                    .library
                    .lock()
                    .map_err(|_| "library lock poisoned".to_string())
                    .map(|store| {
                        store
                            .library_data()
                            .playlists
                            .iter()
                            .any(|p| p.browse_id.as_deref() == Some(playlist.id.as_str()))
                    })
                    .unwrap_or(false);
                if already {
                    continue;
                }
                if fetched >= 10 {
                    report
                        .errors
                        .push("Saved playlists: stopped after 10 — run sync again for the rest.".to_string());
                    break;
                }
                fetched += 1;
                match MusicService::playlist(&state.music, &playlist.id).await {
                    Ok(detail) => {
                        let song_ids: Vec<String> =
                            detail.songs.iter().map(|s| s.id.clone()).collect();
                        let created = state
                            .library
                            .lock()
                            .map_err(|_| "library lock poisoned".to_string())?
                            .import_synced_playlist(&detail.playlist.title, &playlist.id, &song_ids)?;
                        if created.is_some() {
                            report.playlists_added += 1;
                        }
                    }
                    Err(e) => report.errors.push(format!(
                        "Playlist “{}”: {e}",
                        playlist.title
                    )),
                }
            }
        }
        Err(e) => report.errors.push(format!("Saved playlists: {e}")),
    }
    Ok(report)
}

// ── settings / player ───────────────────────────────────────────────────

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())
        .map(|store| store.current().clone())
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    let mut store = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())?;
    store.save(settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_player_snapshot(state: State<'_, AppState>) -> Result<PlaybackSnapshot, String> {
    state.player.snapshot()
}

#[tauri::command]
fn update_player_snapshot(
    state: State<'_, AppState>,
    snapshot: PlaybackSnapshot,
) -> Result<(), String> {
    state.player.update(snapshot.clone())?;
    #[cfg(windows)]
    {
        if let Ok(smtc) = state.smtc.lock() {
            if let Some(mgr) = smtc.as_ref() {
                mgr.update(&snapshot);
            }
        }
    }
    Ok(())
}

/// Directory where downloads live. Today this only resolves the path;
/// `download_song` does the actual downloading.
#[tauri::command]
fn download_dir(app: tauri::AppHandle) -> Result<String, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("downloads").to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

// ── update check ────────────────────────────────────────────────────────

#[tauri::command]
async fn check_for_update(state: State<'_, AppState>) -> Result<UpdateInfo, String> {
    let info = update::check_for_update(&state.music).await;
    // Android 6.1.45 persisted failure flag: a failed check sets it,
    // a conclusive one clears it, so Settings can show "Can't check
    // for updates" instead of looking up-to-date.
    if let Ok(mut store) = state.settings.lock() {
        let _ = store.set_last_update_check_failed(info.error.is_some());
    }
    Ok(info)
}

/// Open a URL in the system browser (the update download link). Uses
/// the OS opener directly — no extra crate. Only https URLs are opened.
#[tauri::command]
fn open_update_download(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("refusing to open a non-https URL".to_string());
    }
    #[cfg(windows)]
    let result = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn();
    #[cfg(not(windows))]
    let result = std::process::Command::new("xdg-open").arg(&url).spawn();
    result.map(|_| ()).map_err(|e| e.to_string())
}

// ── Listen Together ─────────────────────────────────────────────────────

/// `ltServer` setting value → HTTP base URL. "global" / "usa" are the
/// two shipped servers (audit §10); anything else is a custom URL
/// (wss:// is rewritten to https:// for the room-create POST).
fn lt_http_base(setting: &str) -> String {
    let base = match setting {
        "global" => "https://devilmi-vivi-music-listen-together.hf.space".to_string(),
        "usa" => "https://vivimusic-listen-together.onrender.com".to_string(),
        other => other.trim_end_matches('/').to_string(),
    };
    base.replace("wss://", "https://")
        .replace("ws://", "http://")
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LtRoom {
    room_code: String,
}

/// Create a Listen Together room: POST `{server}/api/rooms` (empty
/// body) → `{room_code}`. The WebSocket itself runs in frontend JS.
#[tauri::command]
async fn lt_create_room(state: State<'_, AppState>, username: String) -> Result<LtRoom, String> {
    let username = username.trim();
    if username.is_empty() {
        return Err("username must not be empty".to_string());
    }
    let server = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())
        .map(|s| s.current().lt_server.clone())
        .unwrap_or_else(|_| "global".to_string());
    let url = format!("{}/api/rooms", lt_http_base(&server));
    let value = state
        .music
        .http_post_empty(&url)
        .await
        .map_err(|e| e.to_string())?;
    let room_code = value
        .get("room_code")
        .and_then(|c| c.as_str())
        .filter(|c| !c.is_empty())
        .ok_or_else(|| "Listen Together server did not return a room_code".to_string())?
        .to_string();
    Ok(LtRoom { room_code })
}

// ── Google sign-in (automatic cookie capture, no OAuth) ─────────────
// Flow (Android `LoginScreen.kt` parity): the main window opens a
// second Tauri window ("aello-signin") at the Google ServiceLogin URL;
// the frontend polls `signin_poll_capture`, which reads the login
// WebView2's cookies via `ICoreWebView2CookieManager` — including the
// HttpOnly SAPISID `document.cookie` can never see — and, the moment
// SAPISID appears, validates the cookie via `account_menu` and stores
// it in the Rust settings store. No manual paste anywhere: the cookie
// never crosses into JS at all.

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthUrl {
    auth_url: String,
}

/// A completed (or channel-pending) sign-in: the validated account
/// name, the avatar URL from the `account_menu` response (if any), and
/// the account's channels. `needs_channel_choice` is true only when
/// the account has more than one channel — the frontend then shows the
/// channel picker and calls `signin_choose_channel`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountName {
    account_name: String,
    avatar_url: Option<String>,
    channels: Vec<AccountChannel>,
    needs_channel_choice: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SigninStatus {
    signed_in: bool,
    account_name: Option<String>,
    avatar_url: Option<String>,
    channel_name: Option<String>,
    data_sync_id: Option<String>,
    channels: Vec<AccountChannel>,
}

/// One poll of the login window, for the frontend's ~2s loop.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SigninPoll {
    /// `not-signed-in` | `window-gone` | `signed-in` |
    /// `needs-channel` | `validation-failed`
    status: &'static str,
    account_name: Option<String>,
    avatar_url: Option<String>,
    channels: Vec<AccountChannel>,
    message: Option<String>,
}

/// The Google ServiceLogin URL for the login WebView window — the exact
/// URL Android loads (`LoginScreen.kt:267`).
#[tauri::command]
fn signin_start() -> Result<AuthUrl, String> {
    Ok(AuthUrl {
        auth_url: "https://accounts.google.com/ServiceLogin?continue=https%3A%2F%2Fmusic.youtube.com"
            .to_string(),
    })
}

/// Validate a harvested cookie by calling InnerTube `account_menu` with
/// it, then store cookie + visitorData + account info in the Rust
/// settings store (NEVER in JS storage). Rejects a blank or invalid
/// cookie. The channel list is best-effort: a failed lookup means an
/// empty list (single-channel account), never a failed sign-in.
async fn finish_signin(
    state: &State<'_, AppState>,
    cookie: String,
    visitor_data: Option<String>,
) -> Result<AccountName, String> {
    let cookie = cookie.trim().to_string();
    if cookie.is_empty() {
        return Err("Empty cookie: sign-in did not produce a session".to_string());
    }
    state.music.set_cookie(Some(cookie.clone()));
    if let Some(vd) = &visitor_data {
        if !vd.trim().is_empty() {
            state.music.set_visitor_data(Some(vd.clone())).await;
        }
    }
    match state.music.account_info().await {
        Ok((name, avatar_url)) => {
            // Channel list: best-effort (Android treats an empty list
            // as legitimate). A lookup failure must not fail sign-in.
            let channels = state.music.account_channels().await.unwrap_or_default();
            let needs_channel_choice = channels.len() > 1;
            state
                .settings
                .lock()
                .map_err(|_| "settings lock poisoned".to_string())?
                .set_auth(
                    cookie,
                    visitor_data,
                    name.clone(),
                    avatar_url.clone(),
                    channels.clone(),
                )?;
            Ok(AccountName {
                account_name: name,
                avatar_url,
                channels,
                needs_channel_choice,
            })
        }
        Err(e) => {
            // Invalid cookie: do not keep it anywhere — clear the
            // session cookie AND the stored auth.json cookie, or the
            // next restart restores the stale cookie and signin_status
            // reports signed-in. Also clear the session visitorData
            // this attempt may have set.
            state.music.set_cookie(None);
            state.music.set_visitor_data(None).await;
            let _ = state
                .settings
                .lock()
                .map(|mut store| store.clear_auth());
            Err(format!("Sign-in validation failed: {e}"))
        }
    }
}

/// Validate a harvested cookie (same path as `signin_poll_capture`
/// uses after auto-capture). Kept as a command for completeness; the
/// frontend no longer passes cookies through JS.
#[tauri::command]
async fn signin_finish(
    state: State<'_, AppState>,
    cookie: String,
    visitor_data: Option<String>,
) -> Result<AccountName, String> {
    finish_signin(&state, cookie, visitor_data).await
}

/// One auto-capture poll of the login window's WebView2 cookies.
/// Returns `not-signed-in` while the user is still on the Google pages,
/// `window-gone` when the login window was closed (frontend stops
/// polling), and `signed-in` / `needs-channel` once the SAPISID cookie
/// was captured AND validated. `validation-failed` means SAPISID was
/// captured but `account_menu` rejected it — the frontend stops polling
/// and asks the user to try again.
#[tauri::command]
async fn signin_poll_capture(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SigninPoll, String> {
    let none = |status: &'static str| SigninPoll {
        status,
        account_name: None,
        avatar_url: None,
        channels: Vec::new(),
        message: None,
    };
    match signin_capture::capture_signin_cookie(&app, "aello-signin").await? {
        CaptureOutcome::NotSignedIn => Ok(none("not-signed-in")),
        CaptureOutcome::WindowGone => Ok(none("window-gone")),
        CaptureOutcome::SignedIn(cookie) => match finish_signin(&state, cookie, None).await {
            Ok(done) => Ok(SigninPoll {
                status: if done.needs_channel_choice {
                    "needs-channel"
                } else {
                    "signed-in"
                },
                account_name: Some(done.account_name),
                avatar_url: done.avatar_url,
                channels: done.channels,
                message: None,
            }),
            Err(e) => Ok(SigninPoll {
                status: "validation-failed",
                account_name: None,
                avatar_url: None,
                channels: Vec::new(),
                message: Some(e),
            }),
        },
    }
}

/// Finish a multi-channel sign-in: apply the picked channel's identity
/// (`dataSyncId`) to the session, re-validate the account name with
/// that identity, and persist the choice. The id must be one of the
/// channels stored at sign-in — an arbitrary value is rejected.
/// Returns the account name for the picked channel.
#[tauri::command]
async fn signin_choose_channel(
    state: State<'_, AppState>,
    data_sync_id: String,
) -> Result<AccountName, String> {
    let data_sync_id = data_sync_id.trim().to_string();
    let channel = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())?
        .auth()
        .channels
        .iter()
        .find(|c| c.data_sync_id.as_deref() == Some(data_sync_id.as_str()))
        .cloned()
        .ok_or_else(|| "Unknown channel: sign in again to refresh the channel list".to_string())?;
    state.music.set_data_sync_id(Some(data_sync_id.clone()));
    match state.music.account_info().await {
        Ok((name, avatar_url)) => {
            let channels = state
                .settings
                .lock()
                .map_err(|_| "settings lock poisoned".to_string())?
                .auth()
                .channels
                .clone();
            state
                .settings
                .lock()
                .map_err(|_| "settings lock poisoned".to_string())?
                .set_channel(data_sync_id, channel.name.clone(), name.clone())?;
            Ok(AccountName {
                account_name: name,
                avatar_url,
                channels,
                needs_channel_choice: false,
            })
        }
        Err(e) => {
            state.music.set_data_sync_id(None);
            Err(format!(
                "Could not switch to channel {}: {e}",
                channel.name
            ))
        }
    }
}

#[tauri::command]
fn signin_status(state: State<'_, AppState>) -> Result<SigninStatus, String> {
    let auth = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())
        .map(|store| store.auth().clone())?;
    Ok(SigninStatus {
        signed_in: auth.cookie.is_some(),
        account_name: auth.account_name,
        avatar_url: auth.avatar_url,
        channel_name: auth.channel_name,
        data_sync_id: auth.data_sync_id,
        channels: auth.channels,
    })
}

#[tauri::command]
async fn sign_out(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.music.set_cookie(None);
    state.music.set_data_sync_id(None);
    // Clear the session visitorData too: signing out must leave no
    // signed-in session state behind in the music client.
    state.music.set_visitor_data(None).await;
    state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())?
        .clear_auth()?;
    // Delete the WebView2 cookies too, so a re-sign-in actually prompts
    // again instead of silently reusing the old Google session.
    // Best-effort: it must never fail sign-out.
    signin_capture::clear_webview_cookies(&app);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Settings + library load here because the app-data directory
            // is only known once the app is running. A missing or corrupt
            // file falls back to defaults; nothing crashes on first run.
            let dir = app.path().app_data_dir()?;
            let settings_store = SettingsStore::load(&dir);
            let library_store = LibraryStore::load(&dir);

            // Restore the sign-in cookie into the session (sync setters:
            // setup runs outside the async runtime).
            let music = InnerTubeSession::new();
            if let Some(cookie) = settings_store.auth().cookie.clone() {
                music.set_cookie(Some(cookie));
                music.set_data_sync_id(settings_store.auth().data_sync_id.clone());
                if let Some(vd) = settings_store.auth().visitor_data.clone() {
                    music.set_visitor_data_blocking(Some(vd));
                }
            }

            // SMTC (Windows only): failure degrades to no media
            // controls, never a startup crash. UNTESTED on this VM.
            #[cfg(windows)]
            let smtc_mgr = {
                let mgr = app
                    .get_webview_window("main")
                    .and_then(|w| w.hwnd().ok())
                    .and_then(|hwnd| {
                        smtc::SmtcManager::new(app.handle().clone(), hwnd).ok()
                    });
                Mutex::new(mgr)
            };

            app.manage(AppState {
                music,
                player: PlayerSession::new(),
                settings: Mutex::new(settings_store),
                library: Mutex::new(library_store),
                #[cfg(windows)]
                smtc: smtc_mgr,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_home_feed,
            get_home_feed_more,
            search_music,
            search_music_more,
            get_related,
            get_home_personal,
            get_playlist,
            get_album,
            get_artist,
            get_trending,
            get_explore,
            browse_mood,
            get_daily_discover,
            resolve_stream,
            get_lyrics,
            download_song,
            get_downloads,
            remove_download,
            get_library,
            like_song,
            toggle_speed_dial,
            export_library,
            import_library,
            import_csv_playlist,
            clear_downloads,
            export_song,
            subscribe_artist,
            sync_youtube,
            create_playlist,
            rename_playlist,
            delete_playlist,
            playlist_add_song,
            playlist_remove_song,
            add_to_history,
            clear_history,
            get_stats,
            get_settings,
            save_settings,
            get_player_snapshot,
            update_player_snapshot,
            download_dir,
            check_for_update,
            open_update_download,
            lt_create_room,
            signin_start,
            signin_finish,
            signin_poll_capture,
            signin_choose_channel,
            signin_status,
            sign_out,
            local_files::pick_local_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running Aello");
}
