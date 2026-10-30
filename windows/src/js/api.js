"use strict";

/* Bridge to the Tauri Rust core.
 *
 * When this page runs inside the Aello desktop app, calls go through
 * `window.__TAURI__.core.invoke` to the commands in src-tauri/src/lib.rs.
 *
 * When the page is opened in a plain browser (previewing the interface),
 * there is no bridge. Every call then resolves to the same honest empty
 * result the stub backend would return, and `outsideApp` stays true so
 * the UI can say so instead of pretending to be connected. */

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
  };

  async function call(command, args) {
    if (!invoke) return null;
    return invoke(command, args || {});
  }

  return {
    outsideApp,

    async getHomeFeed() {
      const sections = await call("get_home_feed");
      return Array.isArray(sections) ? sections : [];
    },

    async search(query) {
      const items = await call("search_music", { query });
      return Array.isArray(items) ? items : [];
    },

    async getPlaylist(id) {
      return call("get_playlist", { id });
    },

    async getAlbum(id) {
      return call("get_album", { id });
    },

    // Returns a playable URL string, or null when there is none.
    async resolveStream(songId) {
      const url = await call("resolve_stream", { songId });
      return typeof url === "string" && url.length > 0 ? url : null;
    },

    async getSettings() {
      const settings = await call("get_settings");
      if (settings && typeof settings === "object") return settings;
      return { ...DEFAULT_SETTINGS };
    },

    async saveSettings(settings) {
      if (!invoke) return false;
      await call("save_settings", { settings });
      return true;
    },

    // Best-effort: the Rust side returns Err on a poisoned lock, and
    // no caller depends on the snapshot — null means "unknown".
    async getPlayerSnapshot() {
      try {
        return await call("get_player_snapshot");
      } catch {
        return null;
      }
    },

    // Best-effort: a rejection (e.g. poisoned lock) must never break
    // playback — the snapshot is only a hint for later SMTC work.
    async updatePlayerSnapshot(snapshot) {
      if (!invoke) return;
      try {
        await call("update_player_snapshot", { snapshot });
      } catch {
        /* Snapshot sync is best-effort; playback must never fail for it. */
      }
    },

    async downloadDir() {
      const dir = await call("download_dir");
      return typeof dir === "string" ? dir : null;
    },

    // Opens the native file picker; returns [{ path, name }], or [] when
    // the user cancels. Needs the desktop app (dialog plugin).
    async pickLocalFiles() {
      const files = await call("pick_local_files");
      return Array.isArray(files) ? files : [];
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
  };
})();
