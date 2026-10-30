"use strict";

/* Application wiring: navigation, search, settings, player controls.
 * No backend data is invented here — every list comes from AelloApi,
 * and empty results render as empty states. */

document.addEventListener("DOMContentLoaded", () => {
  const api = window.AelloApi;
  const player = window.AelloPlayer;
  const views = window.AelloViews;
  const audio = player.audio;

  if (api.outsideApp) {
    document.getElementById("bridge-notice").hidden = false;
  }

  function showView(name) {
    const viewsByName = {
      home: "view-home",
      library: "view-library",
      settings: "view-settings",
      player: "view-player",
      about: "view-about",
    };
    Object.values(viewsByName).forEach((id) => {
      document.getElementById(id).hidden = true;
    });
    const target = document.getElementById(viewsByName[name]);
    if (target) target.hidden = false;
    document.querySelectorAll(".nav-item[data-view]").forEach((button) => {
      button.classList.toggle("active", button.dataset.view === name);
    });
  }

  document.querySelectorAll("[data-view]").forEach((el) => {
    el.addEventListener("click", () => showView(el.dataset.view));
  });

  async function loadHome() {
    const statusEl = document.getElementById("home-status");
    try {
      const sections = await api.getHomeFeed();
      views.renderHome(sections, statusEl);
    } catch (err) {
      statusEl.textContent = "Couldn't load the home feed: " + err.message;
    }
  }

  async function runSearch() {
    const query = document.getElementById("search-input").value.trim();
    const statusEl = document.getElementById("home-status");
    showView("home");
    if (!query) {
      await loadHome();
      return;
    }
    statusEl.textContent = "Searching…";
    try {
      const items = await api.search(query);
      views.renderSearchResults(items, query, statusEl);
    } catch (err) {
      statusEl.textContent = "Search failed: " + err.message;
    }
  }

  document
    .getElementById("search-btn")
    .addEventListener("click", runSearch);
  document
    .getElementById("search-input")
    .addEventListener("keydown", (event) => {
      if (event.key === "Enter") runSearch();
    });

  document
    .getElementById("home-sections")
    .addEventListener("click", async (event) => {
      const card = event.target.closest(".card");
      if (!card) return;
      const kind = card.dataset.kind;
      const id = card.dataset.id;
      const statusEl = document.getElementById("home-status");
      try {
        if (kind === "song") {
          const entry = views.findItem(id);
          if (entry) {
            await player.playSong(
              entry.item,
              entry.queue.length > 0 ? entry.queue : undefined,
            );
          } else {
            player.setStatus("Couldn't find that song.");
          }
          return;
        }
        if (kind === "playlist") {
          const detail = await api.getPlaylist(id);
          views.renderDetail(detail, false, statusEl);
        } else if (kind === "album") {
          const detail = await api.getAlbum(id);
          views.renderDetail(detail, true, statusEl);
        }
      } catch (err) {
        statusEl.textContent = "Couldn't open this item: " + err.message;
      }
    });

  // Local files picked from disk. Session-only: never written to disk,
  // never persisted; gone when the app closes.
  let localSongs = [];
  let activeSegment = "playlists";

  function renderLibrarySegment() {
    views.renderLibrary(activeSegment, localSongs);
  }

  async function openLocalFiles() {
    if (api.outsideApp) {
      player.setStatus(
        "Local files need the desktop app — the file picker is not available in preview mode.",
      );
      return;
    }
    let files;
    try {
      files = await api.pickLocalFiles();
    } catch (err) {
      player.setStatus("Couldn't open the file picker: " + err.message);
      return;
    }
    if (!files || files.length === 0) return; // User cancelled the dialog.
    const fresh = [];
    files.forEach((file) => {
      const song = {
        id: "local:" + file.path,
        title: file.name,
        artists: [],
        localPath: file.path,
        duration: null,
      };
      if (!localSongs.some((existing) => existing.id === song.id)) {
        localSongs.push(song);
        fresh.push(song);
      }
    });
    if (fresh.length === 0) {
      player.setStatus("Those files are already in your local library.");
      return;
    }
    renderLibrarySegment();
    await player.playSong(fresh[0], player.queue.concat(fresh));
  }

  document
    .getElementById("library-open-files")
    .addEventListener("click", openLocalFiles);
  document
    .getElementById("pb-open-files")
    .addEventListener("click", openLocalFiles);

  // Clicking a local song in the Library Songs list plays it (queue keeps
  // what was already there and gains the local songs).
  document
    .getElementById("library-list")
    .addEventListener("click", async (event) => {
      const track = event.target.closest("[data-local-index]");
      if (!track) return;
      const song = localSongs[Number(track.dataset.localIndex)];
      if (!song) return;
      await player.playSong(song, player.queue.concat(localSongs));
    });

  // Queue management in the full player: remove / move up / move down.
  document
    .getElementById("fp-queue")
    .addEventListener("click", (event) => {
      const button = event.target.closest("[data-queue-action]");
      if (!button) return;
      const index = Number(button.dataset.queueIndex);
      const action = button.dataset.queueAction;
      if (action === "remove") player.removeAt(index);
      else if (action === "up") player.moveUp(index);
      else if (action === "down") player.moveDown(index);
    });

  document.querySelectorAll(".segment").forEach((button) => {
    button.addEventListener("click", () => {
      document
        .querySelectorAll(".segment")
        .forEach((item) => item.classList.remove("active"));
      button.classList.add("active");
      activeSegment = button.dataset.segment;
      renderLibrarySegment();
    });
  });
  renderLibrarySegment();

  // Player bar + full player controls.
  function refreshNowPlaying() {
    views.updateNowPlaying(player.currentSong(), audio);
  }
  player.onStateChange(refreshNowPlaying);

  document
    .getElementById("pb-play")
    .addEventListener("click", () => player.toggle());
  document
    .getElementById("fp-play")
    .addEventListener("click", () => player.toggle());
  document
    .getElementById("pb-next")
    .addEventListener("click", () => player.next());
  document
    .getElementById("fp-next")
    .addEventListener("click", () => player.next());
  document
    .getElementById("pb-prev")
    .addEventListener("click", () => player.previous());
  document
    .getElementById("fp-prev")
    .addEventListener("click", () => player.previous());

  document
    .getElementById("pb-mute")
    .addEventListener("click", () => player.toggleMute());
  document
    .getElementById("fp-mute")
    .addEventListener("click", () => player.toggleMute());

  function bindSeek(el) {
    el.addEventListener("input", () => {
      if (Number.isFinite(audio.duration) && audio.duration > 0) {
        player.seekTo((parseFloat(el.value) / 100) * audio.duration);
      }
    });
  }
  bindSeek(document.getElementById("pb-progress"));
  bindSeek(document.getElementById("fp-progress"));

  function bindVolume(el) {
    el.addEventListener("input", () => {
      player.setVolume(parseFloat(el.value));
      document.getElementById("pb-volume").value = el.value;
      document.getElementById("setting-volume").value = el.value;
    });
  }
  bindVolume(document.getElementById("pb-volume"));
  bindVolume(document.getElementById("setting-volume"));

  // Keyboard: Space toggles playback, Left/Right seek ±10s, M mutes.
  // None of them fire while typing in a field (that also keeps the
  // arrow keys working natively on the volume/seek sliders).
  document.addEventListener("keydown", (event) => {
    const target = event.target;
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLSelectElement ||
      target instanceof HTMLTextAreaElement
    ) {
      return;
    }
    // A focused button may also fire its click on keyup (e.g. Space on
    // the Play button) — skip so playback does not double-toggle.
    if (target instanceof Element && target.closest("button")) {
      return;
    }
    if (event.code === "Space") {
      event.preventDefault();
      player.toggle();
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      player.seekTo(audio.currentTime + 10);
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      player.seekTo(audio.currentTime - 10);
    } else if (event.key === "m" || event.key === "M") {
      player.toggleMute();
    }
  });

  // Settings: load, apply, save.
  async function loadSettings() {
    const settings = await api.getSettings();
    views.applyTheme(settings);
    views.fillSettingsForm(settings);
    player.setVolume(settings.volume);
    document.getElementById("pb-volume").value = settings.volume;
    // The download folder is only a planned location in v0.1.0 — there
    // is no download feature yet, so show the plan, not a live path.
    document.getElementById("download-dir-display").textContent =
      "Downloads are planned for a later phase.";
  }

  document
    .getElementById("settings-form")
    .addEventListener("submit", async (event) => {
      event.preventDefault();
      const settings = views.readSettingsForm();
      views.applyTheme(settings);
      player.setVolume(settings.volume);
      document.getElementById("pb-volume").value = settings.volume;
      const statusEl = document.getElementById("settings-status");
      try {
        const saved = await api.saveSettings(settings);
        statusEl.textContent = saved
          ? "Settings saved."
          : "Settings apply for this session only — saving needs the desktop app.";
      } catch (err) {
        statusEl.textContent =
          "Couldn't save settings: " +
          (err && err.message ? err.message : err);
      }
    });

  refreshNowPlaying();
  loadSettings().catch(() => {
    /* Defaults already applied in the markup; settings load is best-effort. */
  });
  loadHome();
});
