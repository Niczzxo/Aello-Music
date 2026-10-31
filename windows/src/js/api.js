"use strict";

/* Bridge to the Tauri Rust core.
 *
 * When this page runs inside the Aello desktop app, calls go through
 * `window.__TAURI__.core.invoke` to the commands in src-tauri/src/lib.rs.
 *
 * When the page is opened in a plain browser (previewing the interface),
 * there is no bridge. Every call then resolves to the same honest empty
 * result the stub backend would return, and `outsideApp` stays true so
 * the UI can say so instead of pretending to be connected.
 *
 * Every new command is wrapped in try/catch: the Rust side is built in
 * parallel, so a command that rejects or does not exist yet degrades to
 * an honest empty result — never invented data, never a crash. */

window.AelloApi = (() => {
  const tauriCore =
    window.__TAURI__ && window.__TAURI__.core ? window.__TAURI__.core : null;
  const invoke = tauriCore ? tauriCore.invoke : null;
  // Tauri 2 exposes convertFileSrc on the same `window.__TAURI__.core`
  // object as invoke (v2.tauri.app/reference/javascript/api/namespacecore).
  // It turns a device path into an asset-protocol URL the <audio> element
  // can load. Needs `app.withGlobalTauri: true` plus an enabled
  // assetProtocol with scope and CSP coverage in tauri.conf.json.
  const convertFileSrc =
    tauriCore && typeof tauriCore.convertFileSrc === "function"
      ? tauriCore.convertFileSrc
      : null;
  const outsideApp = !invoke;

  const DEFAULT_SETTINGS = {
    accentTheme: "aello-red",
    appearance: "system",
    volume: 0.8,
    checkForUpdatesOnStartup: true,
    audioQuality: "auto",
    crossfadeEnabled: true,
    crossfadeSecs: 5.0,
    soundEnhanceEnabled: true,
    activeEqProfileId: null,
    eqProfiles: [],
    persistentQueue: true,
    autoDownloadOnLike: false,
    ltServer: "global",
    ltUsername: "",
    // Android 6.1.45: persisted update-check failure flag + the host's
    // last Listen Together control mode. Defaults keep old saves valid.
    lastUpdateCheckFailed: false,
    ltControlMode: "owner",
  };

  async function call(command, args) {
    if (!invoke) return null;
    return invoke(command, args || {});
  }

  // Safe wrapper: null when there is no bridge, null when the command
  // rejects or does not exist yet. Callers turn null into an honest
  // empty result.
  async function safeCall(command, args) {
    if (!invoke) return null;
    try {
      return await invoke(command, args || {});
    } catch {
      return null;
    }
  }

  // Timed call (R4): long Rust commands (home/search/artist/playlist)
  // must not hang the UI forever on a slow network — 30s, then the
  // caller shows a timeout error instead of a blank page. Throws on
  // timeout (unlike safeCall) so the UI can tell "timed out" apart
  // from "empty".
  async function timedCall(command, args, timeoutMs) {
    if (!invoke) return null;
    let timer = null;
    try {
      return await Promise.race([
        invoke(command, args || {}),
        new Promise((_, reject) => {
          timer = setTimeout(
            () => reject(new Error("Timed out after " + Math.round((timeoutMs || 30000) / 1000) + "s")),
            timeoutMs || 30000,
          );
        }),
      ]);
    } finally {
      if (timer) clearTimeout(timer);
    }
  }

  function asArray(value) {
    return Array.isArray(value) ? value : [];
  }

  return {
    outsideApp,

    // Home feed page { chips, sections, continuation }. `params` is a
    // chip's browse params (undefined = unfiltered feed); a chip tap
    // re-fetches with that chip's params, "All" re-fetches unfiltered.
    async getHomeFeed(params) {
      const args = {};
      if (params) args.params = params;
      const feed = await timedCall("get_home_feed", args);
      if (feed && typeof feed === "object" && Array.isArray(feed.sections)) return feed;
      // Older shape (plain section array) — wrap it.
      if (Array.isArray(feed)) return { chips: [], sections: feed, continuation: null };
      return { chips: [], sections: [], continuation: null };
    },

    async getHomeFeedMore(continuation) {
      const feed = await timedCall("get_home_feed_more", { continuation });
      if (feed && typeof feed === "object" && Array.isArray(feed.sections)) return feed;
      return { chips: [], sections: [], continuation: null };
    },

    // Personal Home sections from the local library store — only
    // sections with real items are rendered by the caller.
    async getHomePersonal() {
      const data = await safeCall("get_home_personal");
      if (data && typeof data === "object") return data;
      return { quickPicks: [], keepListening: [], forgottenFavorites: [], speedDial: [] };
    },

    // Related songs for one video (radio / autoplay).
    async getRelated(videoId) {
      const songs = await safeCall("get_related", { videoId });
      return asArray(songs);
    },

    async search(query, filter) {
      // Search tabs send plurals ("songs" | "albums" | "artists" |
      // "playlists"); the Rust filter only matches singulars, so map
      // here too (Rust also accepts plurals — defense in depth).
      const FILTER_MAP = {
        songs: "song",
        albums: "album",
        artists: "artist",
        playlists: "playlist",
      };
      const args = { query };
      if (filter) args.filter = FILTER_MAP[filter] || filter;
      const page = await timedCall("search_music", args);
      if (page && typeof page === "object" && Array.isArray(page.items)) return page;
      if (Array.isArray(page)) return { items: page, continuation: null };
      return { items: [], continuation: null };
    },

    async searchMore(continuation) {
      const page = await timedCall("search_music_more", { continuation });
      if (page && typeof page === "object" && Array.isArray(page.items)) return page;
      return { items: [], continuation: null };
    },

    async getPlaylist(id) {
      return timedCall("get_playlist", { id });
    },

    async getAlbum(id) {
      return timedCall("get_album", { id });
    },

    // Returns a playable URL string, or null when there is none.
    async resolveStream(songId) {
      const url = await safeCall("resolve_stream", { songId });
      return typeof url === "string" && url.length > 0 ? url : null;
    },

    async getSettings() {
      const settings = await safeCall("get_settings");
      if (settings && typeof settings === "object") {
        return { ...DEFAULT_SETTINGS, ...settings };
      }
      return { ...DEFAULT_SETTINGS };
    },

    async saveSettings(settings) {
      if (!invoke) return false;
      try {
        await invoke("save_settings", { settings });
        return true;
      } catch {
        return false;
      }
    },

    // Best-effort: the Rust side returns Err on a poisoned lock, and
    // no caller depends on the snapshot — null means "unknown".
    async getPlayerSnapshot() {
      return safeCall("get_player_snapshot");
    },

    // Best-effort: a rejection (e.g. poisoned lock) must never break
    // playback — the snapshot is only a hint for later SMTC work.
    async updatePlayerSnapshot(snapshot) {
      if (!invoke) return;
      try {
        await invoke("update_player_snapshot", { snapshot });
      } catch {
        /* Snapshot sync is best-effort; playback must never fail for it. */
      }
    },

    async downloadDir() {
      const dir = await safeCall("download_dir");
      return typeof dir === "string" ? dir : null;
    },

    // Opens the native file picker; returns [{ path, name }], or [] when
    // the user cancels. Needs the desktop app (dialog plugin).
    async pickLocalFiles() {
      const files = await safeCall("pick_local_files");
      return asArray(files);
    },

    // Converts a local filesystem path to a URL the <audio> element can
    // load (Tauri 2 asset protocol). Returns null outside the desktop app
    // or when the Tauri global is unavailable.
    localFileUrl(path) {
      if (!convertFileSrc || typeof path !== "string" || path.length === 0) {
        return null;
      }
      return convertFileSrc(path);
    },

    // ---- 6.1.45: catalogue & browsing ---------------------------------

    async getArtist(id) {
      return timedCall("get_artist", { id });
    },

    async getTrending() {
      const sections = await timedCall("get_trending");
      return asArray(sections);
    },

    async getExplore() {
      const data = await safeCall("get_explore");
      if (data && typeof data === "object") return data;
      return { sections: [], moods: [] };
    },

    // Browse a mood/genre by its InnerTube params (from get_explore).
    async browseMood(params) {
      const items = await timedCall("browse_mood", { params });
      return asArray(items);
    },

    // Rust keeps the last-good list in memory + store, so an empty
    // result here genuinely means "no Daily Discover yet".
    async getDailyDiscover() {
      const songs = await safeCall("get_daily_discover");
      return asArray(songs);
    },

    // ---- 6.1.45: lyrics -------------------------------------------------

    // Returns { lines: [{ timeMs, text }], provider, synced } or null.
    // Rust applies [offset:] to every timestamp before returning.
    async getLyrics(songId, title, artist, album, durationSecs) {
      const args = { songId, title, artist };
      if (album != null) args.album = album;
      if (durationSecs != null) args.durationSecs = durationSecs;
      const result = await safeCall("get_lyrics", args);
      if (result && typeof result === "object") return result;
      return null;
    },

    // ---- 6.1.45: downloads ----------------------------------------------

    async downloadSong(song) {
      if (!song) return null;
      return safeCall("download_song", {
        songId: song.id,
        title: song.title || "Untitled",
        artists: Array.isArray(song.artists) ? song.artists : [],
      });
    },

    async getDownloads() {
      const rows = await safeCall("get_downloads");
      return asArray(rows);
    },

    // Best-effort; returns false when the command is unavailable.
    async removeDownload(songId) {
      if (!invoke) return false;
      try {
        await invoke("remove_download", { songId });
        return true;
      } catch {
        return false;
      }
    },

    // ---- 6.1.45: library -------------------------------------------------

    // Returns { likedIds, playlists, history, speedDialIds,
    // librarySongs } or the empty shape when the store/command is
    // unavailable.
    async getLibrary() {
      const data = await safeCall("get_library");
      if (data && typeof data === "object") return data;
      return { likedIds: [], playlists: [], history: [], speedDialIds: [], librarySongs: [] };
    },

    // Best-effort; returns false when the command is unavailable.
    // Auto-download-on-like is handled Rust-side when the setting is on.
    async likeSong(song, liked) {
      if (!invoke || !song) return false;
      try {
        await invoke("like_song", { song, liked: !!liked });
        return true;
      } catch {
        return false;
      }
    },

    async createPlaylist(name) {
      return safeCall("create_playlist", { name });
    },

    async renamePlaylist(id, name) {
      if (!invoke) return false;
      try {
        await invoke("rename_playlist", { id, name });
        return true;
      } catch {
        return false;
      }
    },

    async deletePlaylist(id) {
      if (!invoke) return false;
      try {
        await invoke("delete_playlist", { id });
        return true;
      } catch {
        return false;
      }
    },

    async playlistAddSong(id, song) {
      if (!invoke || !song) return false;
      try {
        await invoke("playlist_add_song", { id, song });
        return true;
      } catch {
        return false;
      }
    },

    async playlistRemoveSong(id, songId) {
      if (!invoke) return false;
      try {
        await invoke("playlist_remove_song", { id, songId });
        return true;
      } catch {
        return false;
      }
    },

    // Called by the player after 30s of playback. Best-effort.
    async addToHistory(song) {
      if (!invoke || !song) return;
      try {
        await invoke("add_to_history", { song });
      } catch {
        /* History reporting must never break playback. */
      }
    },

    async clearHistory() {
      if (!invoke) return false;
      try {
        await invoke("clear_history");
        return true;
      } catch {
        return false;
      }
    },

    // Pin / unpin a song on the Speed Dial (max 27, enforced Rust-side).
    // Returns true when pinned afterwards, false when unpinned or when
    // the command failed / the dial is full.
    async toggleSpeedDial(song) {
      if (!invoke || !song) return false;
      try {
        return await invoke("toggle_speed_dial", { song });
      } catch {
        return false;
      }
    },

    // ---- 6.1.45: backup / restore / CSV / storage ------------------------

    async exportLibrary() {
      const text = await safeCall("export_library");
      return typeof text === "string" ? text : null;
    },

    async importLibrary(backupJson) {
      if (!invoke) return false;
      try {
        await invoke("import_library", { backupJson });
        return true;
      } catch {
        return false;
      }
    },

    async importCsvPlaylist(name, csv) {
      if (!invoke) return null;
      try {
        return await invoke("import_csv_playlist", { name, csv });
      } catch {
        return null;
      }
    },

    async clearDownloads() {
      if (!invoke) return null;
      try {
        return await invoke("clear_downloads");
      } catch {
        return null;
      }
    },

    // Copy a downloaded song to a folder the user picks. Returns the
    // destination path, null when cancelled/unavailable.
    async exportSong(songId) {
      if (!invoke) return null;
      try {
        return await invoke("export_song", { songId });
      } catch {
        return null;
      }
    },

    // ---- 6.1.45: YouTube sync + subscribe --------------------------------

    // Subscribe / unsubscribe an artist channel. Returns false when
    // not signed in or the call failed (the UI shows the real reason
    // via the thrown message where available).
    async subscribeArtist(channelId, subscribe) {
      if (!invoke) return false;
      try {
        await invoke("subscribe_artist", { channelId, subscribe: !!subscribe });
        return true;
      } catch {
        return false;
      }
    },

    // One YouTube sync run. Returns { likedAdded, libraryAdded,
    // playlistsAdded, errors } or null when unavailable / not signed
    // in (the command rejects then — callers show the sign-in prompt).
    async syncYouTube() {
      if (!invoke) return null;
      try {
        const report = await invoke("sync_youtube");
        if (report && typeof report === "object") return report;
        return null;
      } catch {
        return null;
      }
    },

    // Most-played entries (top by playCount) for the Stats view.
    async getStats() {
      const entries = await safeCall("get_stats");
      return asArray(entries);
    },

    // ---- 6.1.45: updates -------------------------------------------------

    // Returns { available, version, notes, exeUrl, error } or null when
    // the command is unavailable. available=false + error set means the
    // check FAILED — the UI must show "couldn't check", never "latest".
    async checkForUpdate() {
      return safeCall("check_for_update");
    },

    // Opens a URL in the system browser. Returns false when unavailable;
    // the UI then shows the link as text instead.
    async openUpdateDownload(url) {
      if (!invoke || !url) return false;
      try {
        await invoke("open_update_download", { url });
        return true;
      } catch {
        return false;
      }
    },

    // ---- 6.1.45: Listen Together -----------------------------------------

    // POST {server}/api/rooms; returns { roomCode } or null.
    async ltCreateRoom(username) {
      const result = await safeCall("lt_create_room", {
        username: username || "Guest",
      });
      if (result && typeof result === "object" && result.roomCode) {
        return result;
      }
      return null;
    },

    // ---- 6.1.45: account -------------------------------------------------

    // Returns { authUrl } or null.
    async signinStart() {
      return safeCall("signin_start");
    },

    // One auto-capture poll of the login window's cookies. Returns
    // { status, accountName, avatarUrl, channels, message } or null on
    // a transport failure (the caller keeps polling; the cookie never
    // crosses into JS — Rust captures and validates it).
    async signinPollCapture() {
      const result = await safeCall("signin_poll_capture");
      if (result && typeof result === "object") return result;
      return null;
    },

    // Finish a multi-channel sign-in with the picked channel's
    // dataSyncId. Returns { accountName, ... } or null on failure.
    async signinChooseChannel(dataSyncId) {
      if (!dataSyncId) return null;
      const result = await safeCall("signin_choose_channel", { dataSyncId });
      if (result && typeof result === "object") return result;
      return null;
    },

    async signinStatus() {
      const status = await safeCall("signin_status");
      if (status && typeof status === "object") return status;
      return { signedIn: false, accountName: null };
    },

    async signOut() {
      if (!invoke) return false;
      try {
        await invoke("sign_out");
        return true;
      } catch {
        return false;
      }
    },

    // ---- diagnostics (6.1.45 exe fix) --------------------------------

    // Append a line to the app's diagnostics log file
    // (aello-diagnostics.log in the app data dir). Fire-and-forget;
    // null when there is no bridge.
    async logDiagnostic(kind, message) {
      return safeCall("log_diagnostic", {
        kind,
        message: String(message || ""),
      });
    },

    // Absolute path of the diagnostics log file (shown in About).
    async diagnosticsPath() {
      const path = await safeCall("diagnostics_path");
      return typeof path === "string" ? path : "";
    },

    // Last lines of the diagnostics log (Copy diagnostics button).
    async readDiagnosticsLog() {
      const text = await safeCall("read_diagnostics_log");
      return typeof text === "string" ? text : "";
    },
  };
})();
