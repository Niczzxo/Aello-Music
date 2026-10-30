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
#[cfg(windows)]
mod smtc;
mod update;

use innertube::InnerTubeSession;
use library::{now_iso, LibraryStore};
use models::{
    Artist, ArtistDetail, DownloadedSong, ExploreData, HomeItem, HomeSection, LibraryData,
    LibraryPlaylist, LyricsResult, Song, UpdateInfo,
};
use music_service::MusicService;
use player::{PlaybackSnapshot, PlayerSession};
use serde::Serialize;
use settings::{AppSettings, SettingsStore};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Manager, State};

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
async fn get_home_feed(state: State<'_, AppState>) -> Result<Vec<HomeSection>, String> {
    MusicService::home_feed(&state.music).await
}

#[tauri::command]
async fn search_music(
    state: State<'_, AppState>,
    query: String,
    filter: Option<String>,
) -> Result<Vec<HomeItem>, String> {
    MusicService::search(&state.music, &query, filter.as_deref()).await
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

    let mut songs: Vec<Song> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for seed in &seeds {
        let Ok(mut related) = MusicService::related_for_video(&state.music, seed).await else {
            continue;
        };
        shuffle(&mut related);
        for song in related.into_iter().take(6) {
            if song.id != *seed && seen.insert(song.id.clone()) {
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
        Ok(items) => {
            let mut top: Vec<Song> = items
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
    // Auto-download on like: best-effort background task — it must never
    // fail the like itself.
    if liked {
        let auto = state
            .settings
            .lock()
            .map(|s| s.current().auto_download_on_like)
            .unwrap_or(false);
        if auto {
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                let state2 = app2.state::<AppState>();
                let _ = download_song_inner(&app2, &state2, &song).await;
            });
        }
    }
    Ok(())
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
    Ok(update::check_for_update(&state.music).await)
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

// ── Google sign-in (cookie harvest, no OAuth) ───────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthUrl {
    auth_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountName {
    account_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SigninStatus {
    signed_in: bool,
    account_name: Option<String>,
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
/// it, then store cookie + visitorData in the Rust settings store
/// (NEVER in JS storage). Rejects a blank or invalid cookie.
#[tauri::command]
async fn signin_finish(
    state: State<'_, AppState>,
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
    match MusicService::account_name(&state.music).await {
        Ok(name) => {
            state
                .settings
                .lock()
                .map_err(|_| "settings lock poisoned".to_string())?
                .set_auth(cookie, visitor_data, name.clone())?;
            Ok(AccountName { account_name: name })
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
    })
}

#[tauri::command]
async fn sign_out(state: State<'_, AppState>) -> Result<(), String> {
    state.music.set_cookie(None);
    // Clear the session visitorData too: signing out must leave no
    // signed-in session state behind in the music client.
    state.music.set_visitor_data(None).await;
    state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned".to_string())?
        .clear_auth()
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
            search_music,
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
            signin_status,
            sign_out,
            local_files::pick_local_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running Aello");
}
