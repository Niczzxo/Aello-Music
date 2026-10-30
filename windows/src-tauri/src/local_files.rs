//! Local audio files picked from disk (Phase 2).
//!
//! The Rust side only opens the native file picker and returns the chosen
//! paths. No file content is read here; the frontend plays the files
//! through the `<audio>` element via the Tauri asset protocol
//! (see `AelloApi.localFileUrl` in src/js/api.js).
//!
//! Wiring (done by the parent in lib.rs / capabilities / tauri.conf.json):
//!   - `mod local_files;` and `local_files::pick_local_files` in
//!     `generate_handler!`
//!   - `.plugin(tauri_plugin_dialog::init())` on the builder
//!   - capability permission `dialog:allow-open`
//!   - `app.withGlobalTauri: true`, `app.security.assetProtocol` enabled,
//!     and `asset:` / `http://asset.localhost` in the CSP `media-src`
//!     (otherwise `convertFileSrc` has no global and the audio element
//!     cannot load the asset URL).

use serde::Serialize;
use std::path::PathBuf;
use tauri_plugin_dialog::{DialogExt, FilePath};

/// One audio file the user picked in the native open dialog.
#[derive(Debug, Clone, Serialize)]
pub struct LocalFile {
    /// Absolute filesystem path, e.g. `C:\Music\song.mp3`.
    pub path: String,
    /// File stem without the extension, e.g. `song`.
    pub name: String,
}

fn file_stem(path: &PathBuf) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned())
        })
}

fn to_local_file(file_path: FilePath) -> Option<LocalFile> {
    match file_path {
        FilePath::Path(path) => Some(LocalFile {
            name: file_stem(&path),
            path: path.to_string_lossy().into_owned(),
        }),
        // A `file://` URL (or a non-filesystem URI on other platforms):
        // only usable when it maps back to a real path.
        FilePath::Url(url) => url.to_file_path().ok().map(|path| LocalFile {
            name: file_stem(&path),
            path: path.to_string_lossy().into_owned(),
        }),
    }
}

/// Opens the native file picker for audio files (multiple selection).
///
/// Returns an empty vec when the user cancels the dialog — never an
/// error. The blocking call is safe here: Tauri commands run off the
/// main thread, and the dialog plugin documents this exact pattern
/// (`blocking_pick_files` inside an `async #[tauri::command]`).
#[tauri::command]
pub async fn pick_local_files(app: tauri::AppHandle) -> Result<Vec<LocalFile>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter(
            "Audio",
            &["mp3", "m4a", "flac", "ogg", "opus", "wav", "aac"],
        )
        .set_title("Open audio files")
        .blocking_pick_files();
    match picked {
        None => Ok(Vec::new()),
        Some(paths) => Ok(paths.into_iter().filter_map(to_local_file).collect()),
    }
}
