//! Aello for Windows — Tauri application library (v0.1.0 milestone).
//!
//! Status: the Rust core is complete for the core player (real InnerTube
//! backend + local-file playback). The Windows build runs on GitHub
//! Actions; nothing here has run on a real Windows PC yet — the user's
//! PC test is the acceptance check. See README.md and BUILD_PLAN.md.
//!
//! Architecture: the Rust core owns settings persistence, the music-service
//! abstraction, and the playback snapshot. The web frontend owns rendering
//! and audio output (the `<audio>` element inside WebView2). The frontend
//! never talks to music backends directly; every backend call goes through
//! a Tauri command below so the Rust side can enforce the request path
//! (see `music_service.rs` for the InnerTube port notes).

mod innertube;
mod local_files;
mod models;
mod music_service;
mod player;
mod settings;

use innertube::InnerTubeSession;
use models::{AlbumDetail, HomeItem, HomeSection, PlaylistDetail};
use music_service::MusicService;
use player::{PlaybackSnapshot, PlayerSession};
use settings::{AppSettings, SettingsStore};
use std::sync::Mutex;
use tauri::{Manager, State};

/// Shared state managed by Tauri and handed to commands.
pub struct AppState {
    music: InnerTubeSession,
    player: PlayerSession,
    settings: Mutex<SettingsStore>,
}

#[tauri::command]
async fn get_home_feed(state: State<'_, AppState>) -> Result<Vec<HomeSection>, String> {
    MusicService::home_feed(&state.music).await
}

#[tauri::command]
async fn search_music(state: State<'_, AppState>, query: String) -> Result<Vec<HomeItem>, String> {
    MusicService::search(&state.music, &query).await
}

#[tauri::command]
async fn get_playlist(state: State<'_, AppState>, id: String) -> Result<PlaylistDetail, String> {
    MusicService::playlist(&state.music, &id).await
}

#[tauri::command]
async fn get_album(state: State<'_, AppState>, id: String) -> Result<AlbumDetail, String> {
    MusicService::album(&state.music, &id).await
}

#[tauri::command]
async fn resolve_stream(
    state: State<'_, AppState>,
    song_id: String,
) -> Result<Option<String>, String> {
    // Call the MusicService trait method explicitly: the inherent
    // `InnerTubeSession::resolve_stream` returns `StreamUrl`, while the
    // trait contract (and this command) return `Option<String>`.
    MusicService::resolve_stream(&state.music, &song_id).await
}

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
    state.player.update(snapshot)
}

/// Directory where downloads will live once the download feature lands.
/// Today this only resolves the path; nothing downloads yet.
#[tauri::command]
fn download_dir(app: tauri::AppHandle) -> Result<String, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("downloads").to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Settings load here because the app-data directory is only
            // known once the app is running. A missing or corrupt file
            // falls back to defaults; nothing crashes on first run.
            let dir = app.path().app_data_dir()?;
            app.manage(AppState {
                music: InnerTubeSession::new(),
                player: PlayerSession::new(),
                settings: Mutex::new(SettingsStore::load(&dir)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_home_feed,
            search_music,
            get_playlist,
            get_album,
            resolve_stream,
            get_settings,
            save_settings,
            get_player_snapshot,
            update_player_snapshot,
            download_dir,
            local_files::pick_local_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running Aello");
}
