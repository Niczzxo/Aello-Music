# Rust API Contract — Aello Windows 6.1.45

New Tauri commands the Rust builder implements in `src-tauri/src/lib.rs`
(and the JS builder calls via `AelloApi`). Existing commands stay
unchanged: `get_home_feed`, `search_music`, `get_playlist`, `get_album`,
`resolve_stream`, `get_settings`, `save_settings`, `get_player_snapshot`,
`update_player_snapshot`, `download_dir`, `pick_local_files`.

All commands return `Result<T, String>`; errors carry the real reason.
The frontend must treat a rejected command as "feature unavailable",
never invent data.

## Models (add to `models.rs`, camelCase serde)

- `ArtistDetail { artist: Artist, topSongs: Vec<Song>, albums: Vec<Album>, singles: Vec<Album>, related: Vec<Artist>, monthlyListeners: Option<String>, subscriberCount: Option<String>, description: Option<String> }`
- `MoodItem { id: String, title: String, params: String }` (browse params for mood detail)
- `ExploreData { sections: Vec<HomeSection>, moods: Vec<MoodItem> }`
- `LyricLine { timeMs: i64, text: String }`
- `LyricsResult { lines: Vec<LyricLine>, provider: String, synced: bool }`
- `DownloadedSong { songId: String, title: String, artists: Vec<Artist>, path: String, sizeBytes: u64, downloadedAt: String }`
- `LibraryPlaylist { id: String, name: String, songIds: Vec<String>, createdAt: String }`
- `HistoryEntry { songId: String, title: String, artists: Vec<Artist>, playedAt: String, playCount: u64 }`
- `LibraryData { likedIds: Vec<String>, playlists: Vec<LibraryPlaylist>, history: Vec<HistoryEntry>, speedDialIds: Vec<String> }`
- `UpdateInfo { available: bool, version: String, notes: String, exeUrl: Option<String>, error: Option<String> }`
  - `available=false, error=None` = genuinely latest. `error=Some(..)` =
    check failed — the UI must show "couldn't check", NEVER "latest".
- `EqBand { freqHz: f64, gainDb: f64, q: f64, kind: String }` ("lowshelf" | "peaking" | "highshelf")
- `EqProfile { id: String, name: String, bands: Vec<EqBand>, preampDb: f64 }`

## New commands

| Command | Args | Returns | Notes |
|---|---|---|---|
| `get_artist` | `id: String` | `ArtistDetail` | InnerTube `artist` browseId. |
| `get_trending` | — | `Vec<HomeSection>` | InnerTube charts/browse page; "Top music videos" kept as its own section. |
| `get_explore` | — | `ExploreData` | InnerTube explore page (sections + moods). |
| `browse_mood` | `params: String` | `Vec<HomeItem>` | InnerTube browse `FEmusic_moods_and_genres` with the mood/genre params from `get_explore` (`MoodItem.params`; same family as Android `MoodAndGenresViewModel.refresh()`); grid + carousel sections flattened, deduped. Empty/unrecognized params → empty list, never an error. |
| `get_daily_discover` | — | `Vec<Song>` | Seeds = 3 random from liked ∪ recent ∪ most-played ∪ local (library store); per seed `next`→`related`, 6 songs; keep-last-good: empty fetch returns the previous list (Rust keeps it in memory + store); no-history fallback = search "top songs", 25. |
| `search_music` (extended) | `query: String, filter: Option<String>` | `Vec<HomeItem>` | filter: "song"\|"album"\|"artist"\|"playlist". Artist rows need `HomeItem::Artist(Artist)` — add the variant. Existing callers pass no filter (= all). |
| `get_lyrics` | `songId, title, artist, album: Option<String>, durationSecs: Option<u64>` | `LyricsResult` | LrcLib `https://lrclib.net/api/search` (4 strategies, title cleaned, primary artist only, best-match by duration); fallback YouTube timed-text subtitles for songId. Overall timeout 6s. Plain (unsynced) text → `synced=false`, lines with `timeMs=-1`. `[offset:]` applied to every timestamp before returning. |
| `download_song` | `songId: String, title: String, artists: Vec<Artist>` | `DownloadedSong` | Resolves stream (itag 140), streams to app-data `downloads/<songId>.m4a`, records in store. Rejects if already downloaded (returns existing row). |
| `get_downloads` | — | `Vec<DownloadedSong>` | From the store; drop rows whose file is gone. |
| `remove_download` | `songId: String` | `()` | Deletes file + store row. |
| `get_library` | — | `LibraryData` | From the JSON store (created empty on first run). |
| `like_song` | `song: Song, liked: bool` | `()` | Updates liked set; if liked && auto-download-on-like setting → queue `download_song` in background (best-effort, never fail the like). |
| `create_playlist` / `rename_playlist` / `delete_playlist` | `name` / `id, name` / `id` | `LibraryPlaylist` / `()` / `()` | Local playlists only. |
| `playlist_add_song` / `playlist_remove_song` | `id, song: Song` / `id, songId` | `()` | |
| `add_to_history` | `song: Song` | `()` | Called by the frontend after 30s of playback; increments playCount. |
| `clear_history` | — | `()` | |
| `get_stats` | — | `Vec<HistoryEntry>` (top by playCount) | Most-played for the Stats view. |
| `check_for_update` | — | `UpdateInfo` | GET GitHub releases list; pick latest tag `^v*` that has an `Aello_*_x64-setup.exe` asset; strictly-newer vs `tauri.conf.json` version. On ANY fetch failure → `available=false, error=Some(reason)`. Never "latest" on failure. |
| `lt_create_room` | `username: String` | `{ roomCode: String }` | POST `{server}/api/rooms` (server from settings, default `https://devilmi-vivi-music-listen-together.hf.space`); empty body; returns `{room_code}`. The WebSocket itself runs in frontend JS. |
| `open_update_download` | `url: String` | `()` | Opens the URL in the system browser (opener plugin or `open` crate — pick what builds; if neither is available, return Err and the UI shows the link). |
| `signin_start` | — | `{ authUrl: String }` | Returns the Google ServiceLogin URL (continue=music.youtube.com) for the login WebView window. |
| `signin_poll_capture` | — | `{ status: "not-signed-in" \| "window-gone" \| "signed-in" \| "needs-channel" \| "validation-failed", accountName?, avatarUrl?, channels?, message? }` | One auto-capture poll: reads the login window's WebView2 cookies via `ICoreWebView2CookieManager` (including HttpOnly SAPISID — `document.cookie` is never used) and, once SAPISID appears, validates via `account_menu` and stores cookie + visitorData + account info in the Rust settings store (NEVER in JS storage). The cookie never crosses into JS. `window-gone` = user closed the login window; `needs-channel` = account has >1 channel, frontend shows the picker. |
| `signin_choose_channel` | `dataSyncId: String` | `{ accountName, avatarUrl, channels, needsChannelChoice: false }` | Applies the picked channel's identity (`context.user.onBehalfOfUser`), re-validates the account name with it, persists the choice. Rejects an id that isn't one of the channels stored at sign-in. |
| `signin_finish` | `cookie: String, visitorData: Option<String>` | `{ accountName, avatarUrl, channels, needsChannelChoice }` | Same validation path as `signin_poll_capture` (shared `finish_signin`); kept as a command for completeness — the frontend no longer passes cookies through JS. Rejects on a blank/invalid cookie. |
| `signin_status` | — | `{ signedIn: bool, accountName?, avatarUrl?, channelName?, dataSyncId?, channels: [] }` | Stored auth state only — no network call. |
| `sign_out` | — | `()` | Clears cookie + visitorData + channel identity locally and deletes the WebView2 cookies (best-effort, never fails sign-out) so a re-sign-in prompts again. |

## Settings additions (extend `AppSettings`, keep old fields)

`audioQuality: String` ("auto"|"high"|"low", default "auto"),
`crossfadeEnabled: bool` (default true), `crossfadeSecs: f64` (default 5.0),
`soundEnhanceEnabled: bool` (default true),
`activeEqProfileId: Option<String>` (None = no custom profile),
`eqProfiles: Vec<EqProfile>` (stored),
`persistentQueue: bool` (default true),
`autoDownloadOnLike: bool` (default false),
`ltServer: String` (default "global" | "usa" | custom URL),
`ltUsername: String` (default "").

## Library store (new file `src-tauri/src/library.rs`)

- JSON files under the app-data dir: `library.json` (liked ids, playlists,
  speed-dial ids), `history.json` (entries, cap 500, newest first),
  `downloads.json` (downloaded rows). Plain `std::fs` I/O like
  `settings.rs`; corrupt file → empty defaults, never fail startup.
- All store access behind one `Mutex<LibraryStore>` in `AppState`.

## InnerTube additions (`innertube.rs` + `parse.rs`)

- `artist(browse_id)`, `charts()` (browse charts page), `explore()`,
  `next(video_id)` → related endpoint, `related(endpoint)` — port the
  JSON navigation from the Android `innertube/.../YouTube.kt` (read it;
  do not guess field names). Reuse the existing `client.rs` session
  (visitorData, ANDROID_VR + fallbacks) and `parse.rs` helpers.
- Search must return artist rows too (`HomeItem::Artist`).

## SMTC (Windows-only, `#[cfg(windows)]`)

- New module `smtc.rs` using the `windows` crate
  (`windows::Media::SystemMediaTransportControls`); drives from the
  playback snapshot (title/artist/artwork, play/pause/next/prev).
- Add `windows = { version = "0.58", features = [...] }` as a
  **target-specific** dependency (`[target.'cfg(windows)'.dependencies]`)
  so Linux `cargo check` still passes. The feature is UNTESTED on the
  VM (no Windows here) — say so in the report.
