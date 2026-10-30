"use strict";

/* Application wiring: navigation, search, settings, player controls,
 * Listen Together, sign-in, updates.
 *
 * No backend data is invented here — every list comes from AelloApi,
 * and empty results render as empty states. Every getElementById is
 * guarded: the UI builder creates the new views in parallel, so a
 * missing element degrades gracefully instead of crashing.
 *
 * Listen Together implements the JSON protocol from
 * listen-together-server/src/protocol.ts (read, not guessed):
 * create/join/approve, playback_action, sync_playback, buffer_ready
 * gate, control_mode owner/everyone with per-track suggested_by
 * ownership, chat, suggestions, reconnect (15 attempts, exp backoff
 * capped at 120s), session token in localStorage. */

document.addEventListener("DOMContentLoaded", () => {
  const api = window.AelloApi;
  const player = window.AelloPlayer;
  const views = window.AelloViews;

  // ---- small DOM helpers (null-safe) -----------------------------------
  function $(id) {
    return document.getElementById(id);
  }
  function on(id, event, handler) {
    const el = $(id);
    if (el) el.addEventListener(event, handler);
    return el;
  }
  function audioEl() {
    return player.audio;
  }

  let settings = null;
  let localSongs = [];
  let activeSegment = "playlists";
  let libraryCache = null;
  let downloadsCache = null;
  let statsCache = null;
  let lastSongId = null;

  if (api.outsideApp) {
    const notice = $("bridge-notice");
    if (notice) notice.hidden = false;
  }

  // ---- navigation ---------------------------------------------------------
  const VIEW_IDS = {
    home: "view-home",
    search: "view-search",
    library: "view-library",
    trending: "view-trending",
    together: "view-together",
    artist: "view-artist",
    settings: "view-settings",
    player: "view-player",
    about: "view-about",
  };

  function showView(name) {
    Object.values(VIEW_IDS).forEach((id) => {
      const el = $(id);
      if (el) el.hidden = true;
    });
    const target = $(VIEW_IDS[name]);
    if (target) target.hidden = false;
    document.querySelectorAll(".nav-item[data-view]").forEach((button) => {
      button.classList.toggle("active", button.dataset.view === name);
    });
    if (name === "trending") loadTrending();
    if (name === "search") {
      const input = $("search-input");
      if (input) input.focus();
      loadMoods();
    }
  }

  document.querySelectorAll("[data-view]").forEach((el) => {
    el.addEventListener("click", () => {
      const name = el.dataset.view;
      if (VIEW_IDS[name]) showView(name);
    });
  });

  // ---- home (Daily Discover first) ----------------------------------------
  // Time-aware greeting (the markup default is the evening variant).
  function updateHomeGreeting() {
    const hour = new Date().getHours();
    const greeting =
      hour >= 5 && hour < 12
        ? { eyebrow: "Morning vibes", title: "Good morning" }
        : hour >= 12 && hour < 17
          ? { eyebrow: "Afternoon grooves", title: "Good afternoon" }
          : hour >= 17 && hour < 21
            ? { eyebrow: "Evening chill", title: "Good evening" }
            : { eyebrow: "Late night", title: "Good night" };
    const eyebrowEl = $("home-greeting-eyebrow");
    if (eyebrowEl) eyebrowEl.textContent = greeting.eyebrow;
    const titleEl = $("home-greeting");
    if (titleEl) titleEl.textContent = greeting.title;
  }

  async function loadHome() {
    const statusEl = $("home-status");
    try {
      const [sections, daily] = await Promise.all([
        api.getHomeFeed(),
        api.getDailyDiscover(),
      ]);
      views.renderHome(sections, statusEl, daily);
    } catch (err) {
      if (statusEl) {
        statusEl.textContent =
          "Couldn't load the home feed: " + (err && err.message ? err.message : err);
      }
    }
  }

  // ---- search ---------------------------------------------------------------
  const SEARCH_HISTORY_KEY = "aello.searchHistory.v1";
  let searchHistory = [];
  try {
    searchHistory = JSON.parse(localStorage.getItem(SEARCH_HISTORY_KEY) || "[]") || [];
    if (!Array.isArray(searchHistory)) searchHistory = [];
  } catch {
    searchHistory = [];
  }

  function rememberSearch(query) {
    searchHistory = [query, ...searchHistory.filter((item) => item !== query)].slice(0, 20);
    try {
      localStorage.setItem(SEARCH_HISTORY_KEY, JSON.stringify(searchHistory));
    } catch {
      /* History stays in memory for this session. */
    }
  }

  function clearSearchHistory() {
    searchHistory = [];
    try {
      localStorage.removeItem(SEARCH_HISTORY_KEY);
    } catch {
      /* Already gone. */
    }
    views.renderSearchView([], "", views.getActiveSearchTab(), searchHistory);
  }

  function youtubeIdFromUrl(text) {
    const match = String(text || "").match(
      /(?:youtube\.com\/(?:watch\?[^#]*v=|embed\/|shorts\/)|youtu\.be\/)([A-Za-z0-9_-]{11})/,
    );
    return match ? match[1] : null;
  }

  async function runSearch(query, filter) {
    const q = (query != null ? query : $("search-input") ? $("search-input").value : "").trim();
    const tab = filter || views.getActiveSearchTab() || "all";
    showView("search");
    if (!q) {
      views.renderSearchView([], "", tab, searchHistory);
      return;
    }
    // URL paste: a YouTube link resolves straight to the video. The
    // title is unknown until it plays, so it is labelled honestly.
    const videoId = youtubeIdFromUrl(q);
    if (videoId) {
      rememberSearch(q);
      await player.playSong({ id: videoId, title: "YouTube video", artists: [] });
      return;
    }
    views.renderSearchView([], q, tab, searchHistory);
    try {
      const items = await api.search(q, tab === "all" ? undefined : tab);
      rememberSearch(q);
      views.renderSearchView(items, q, tab, searchHistory);
    } catch (err) {
      const host = $("search-results");
      if (host) {
        host.innerHTML =
          '<p class="empty">Search failed: ' +
          views.escapeHtml(err && err.message ? err.message : err) +
          "</p>";
      }
    }
  }

  on("search-btn", "click", () => runSearch());
  on("search-input", "keydown", (event) => {
    if (event.key === "Enter") runSearch();
  });
  const searchTabs = $("search-tabs");
  if (searchTabs) {
    searchTabs.addEventListener("click", (event) => {
      const button = event.target.closest("[data-search-tab]");
      if (!button) return;
      const query = $("search-input") ? $("search-input").value.trim() : "";
      runSearch(query, button.dataset.searchTab);
    });
  }
  const searchHistoryHost = $("search-history");
  if (searchHistoryHost) {
    searchHistoryHost.addEventListener("click", (event) => {
      const item = event.target.closest("[data-history-query]");
      if (!item) return;
      const input = $("search-input");
      if (input) input.value = item.dataset.historyQuery;
      runSearch(item.dataset.historyQuery);
    });
  }
  on("search-history-clear", "click", clearSearchHistory);

  // ---- moods & moments (lazy, once per session) -------------------------
  let moodsLoaded = false;
  async function loadMoods() {
    if (moodsLoaded) return;
    moodsLoaded = true;
    try {
      const explore = await api.getExplore();
      views.renderMoods(explore && explore.moods ? explore.moods : []);
    } catch {
      views.renderMoods([]);
    }
  }
  const moodsHost = $("search-moods");
  if (moodsHost) {
    moodsHost.addEventListener("click", async (event) => {
      const card = event.target.closest("[data-mood-params]");
      if (!card) return;
      const title = card.dataset.moodTitle || "Mood";
      const statusEl = $("home-status");
      try {
        const items = await api.browseMood(card.dataset.moodParams || "");
        views.renderSearchView(items, title, views.getActiveSearchTab(), searchHistory);
      } catch (err) {
        if (statusEl) {
          statusEl.textContent =
            "Couldn't open this mood: " + (err && err.message ? err.message : err);
        }
      }
    });
  }

  // ---- trending ---------------------------------------------------------------
  async function loadTrending() {
    const statusEl = $("trending-status");
    try {
      const sections = await api.getTrending();
      views.renderTrending(sections, statusEl);
    } catch (err) {
      if (statusEl) {
        statusEl.textContent =
          "Couldn't load trending: " + (err && err.message ? err.message : err);
      }
    }
  }

  // ---- artist page --------------------------------------------------------------
  async function openArtist(id) {
    showView("artist");
    const statusEl = $("home-status");
    try {
      const detail = await api.getArtist(id);
      views.renderArtist(detail, statusEl);
    } catch (err) {
      if (statusEl) {
        statusEl.textContent =
          "Couldn't open this artist: " + (err && err.message ? err.message : err);
      }
    }
  }

  on("artist-play", "click", async () => {
    const songs = views.getArtistSongs();
    if (songs.length === 0) return;
    await player.playSong(songs[0], songs);
  });
  on("artist-shuffle", "click", async () => {
    const songs = views.getArtistSongs();
    if (songs.length === 0) return;
    const shuffled = songs.slice();
    for (let i = shuffled.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
    }
    await player.playSong(shuffled[0], shuffled);
  });
  on("artist-subscribe", "click", () => {
    player.setStatus(
      "Subscribing to artists needs a signed-in account and is not available in this version yet.",
    );
  });
  const artistSongsHost = $("artist-songs");
  if (artistSongsHost) {
    artistSongsHost.addEventListener("click", async (event) => {
      const track = event.target.closest("[data-artist-song-index]");
      if (!track) return;
      const songs = views.getArtistSongs();
      const song = songs[Number(track.dataset.artistSongIndex)];
      if (song) await player.playSong(song, songs);
    });
  }

  // Clicking cards in any card container (home, search, trending,
  // artist albums/related) resolves through the views registry.
  async function handleCardClick(event, statusEl) {
    const card = event.target.closest(".card");
    if (!card) return;
    const kind = card.dataset.kind;
    const id = card.dataset.id;
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
        views.renderDetail(detail, false, statusEl, () => loadHome());
        showView("home");
      } else if (kind === "album") {
        const detail = await api.getAlbum(id);
        views.renderDetail(detail, true, statusEl, () => loadHome());
        showView("home");
      } else if (kind === "artist") {
        await openArtist(id);
      }
    } catch (err) {
      if (statusEl) {
        statusEl.textContent =
          "Couldn't open this item: " + (err && err.message ? err.message : err);
      }
    }
  }

  ["home-sections", "search-results", "trending-sections", "artist-albums", "artist-related"].forEach(
    (id) => {
      const host = $(id);
      if (host) {
        host.addEventListener("click", (event) => handleCardClick(event, $("home-status")));
      }
    },
  );

  // ---- library ---------------------------------------------------------------
  async function loadLibraryData(force) {
    if (!force && libraryCache && downloadsCache && statsCache) return;
    const [library, downloads, stats] = await Promise.all([
      api.getLibrary(),
      api.getDownloads(),
      api.getStats(),
    ]);
    libraryCache = library;
    downloadsCache = downloads;
    statsCache = stats;
    player.setLikedIds(library.likedIds || []);
  }

  async function renderLibrarySegment() {
    try {
      await loadLibraryData(false);
    } catch {
      /* Library data is best-effort; render with what we have. */
    }
    views.renderLibrary(activeSegment, localSongs, {
      library: libraryCache,
      downloads: downloadsCache,
      stats: statsCache,
    });
  }

  function refreshLibraryData() {
    libraryCache = null;
    downloadsCache = null;
    statsCache = null;
    return renderLibrarySegment();
  }

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

  const libraryList = $("library-list");
  if (libraryList) {
    libraryList.addEventListener("click", async (event) => {
      const track = event.target.closest("[data-local-index]");
      if (track) {
        const song = localSongs[Number(track.dataset.localIndex)];
        if (song) await player.playSong(song, player.queue.concat(localSongs));
        return;
      }
      const downloadButton = event.target.closest("[data-download-id]");
      if (downloadButton && !event.target.closest("[data-download-action]")) {
        const row = (downloadsCache || []).find(
          (entry) => entry.songId === downloadButton.dataset.downloadId,
        );
        if (row) {
          await player.playSong({
            id: row.songId,
            title: row.title,
            artists: row.artists || [],
            downloadedPath: row.path,
          });
        }
        return;
      }
      const removeDownload = event.target.closest('[data-download-action="remove"]');
      if (removeDownload) {
        const ok = await api.removeDownload(removeDownload.dataset.downloadId);
        player.setStatus(
          ok ? "Download removed." : "Couldn't remove that download yet.",
        );
        await refreshLibraryData();
        return;
      }
      const historyButton = event.target.closest("[data-history-song-id]");
      if (historyButton) {
        const entry = ((libraryCache && libraryCache.history) || []).find(
          (item) => item.songId === historyButton.dataset.historySongId,
        );
        if (entry) {
          await player.playSong({
            id: entry.songId,
            title: entry.title,
            artists: entry.artists || [],
          });
        }
        return;
      }
      const artistButton = event.target.closest("[data-artist-name]");
      if (artistButton) {
        const input = $("search-input");
        if (input) input.value = artistButton.dataset.artistName;
        runSearch(artistButton.dataset.artistName, "artists");
        return;
      }
      const clearHistory = event.target.closest('[data-history-action="clear"]');
      if (clearHistory) {
        const ok = await api.clearHistory();
        player.setStatus(ok ? "History cleared." : "Couldn't clear history yet.");
        await refreshLibraryData();
        return;
      }
      const autoCard = event.target.closest("[data-auto-playlist]");
      if (autoCard) {
        const key = autoCard.dataset.autoPlaylist;
        const segmentByKey = {
          liked: "liked",
          downloaded: "downloads",
          top50: "history",
          local: "songs",
        };
        const segment = segmentByKey[key];
        if (segment) {
          activeSegment = segment;
          document.querySelectorAll(".segment").forEach((item) => {
            item.classList.toggle("active", item.dataset.segment === segment);
          });
          renderLibrarySegment();
        }
        return;
      }
      const playlistAction = event.target.closest("[data-playlist-action]");
      if (playlistAction) {
        const action = playlistAction.dataset.playlistAction;
        if (action === "create") {
          const input = libraryList.querySelector(".playlist-name-input");
          const name = input ? input.value.trim() : "";
          if (!name) {
            player.setStatus("Type a name for the new playlist first.");
            return;
          }
          const created = await api.createPlaylist(name);
          player.setStatus(
            created ? 'Playlist "' + name + '" created.' : "Couldn't create that playlist yet.",
          );
          await refreshLibraryData();
        } else if (action === "rename") {
          const id = playlistAction.dataset.playlistId;
          const playlist = ((libraryCache && libraryCache.playlists) || []).find(
            (item) => item.id === id,
          );
          const name = window.prompt(
            "Rename playlist",
            playlist ? playlist.name : "",
          );
          if (name && name.trim()) {
            const ok = await api.renamePlaylist(id, name.trim());
            player.setStatus(ok ? "Playlist renamed." : "Couldn't rename that playlist yet.");
            await refreshLibraryData();
          }
        } else if (action === "delete") {
          const id = playlistAction.dataset.playlistId;
          if (window.confirm("Delete this playlist? The songs stay in your library.")) {
            const ok = await api.deletePlaylist(id);
            player.setStatus(ok ? "Playlist deleted." : "Couldn't delete that playlist yet.");
            await refreshLibraryData();
          }
        }
      }
    });
  }

  // Local files picked from disk. Session-only: never written to disk,
  // never persisted; gone when the app closes.
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
      player.setStatus("Couldn't open the file picker: " + (err && err.message ? err.message : err));
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

  on("library-open-files", "click", openLocalFiles);
  on("pb-open-files", "click", openLocalFiles);

  // ---- player bar + full player controls ---------------------------------------
  function refreshNowPlaying() {
    const song = player.currentSong();
    views.updateNowPlaying(song, audioEl());
    // Track change: reload lyrics when the panel is open, and tell the
    // Listen Together room (host / allowed member only).
    const songId = song ? song.id : null;
    if (songId !== lastSongId) {
      lastSongId = songId;
      const lyricsPanel = $("lyrics-panel");
      if (lyricsPanel && !lyricsPanel.hidden && song) loadLyricsForCurrent();
      if (song && lt.joined && lt.canControlPlayback() && !lt.applyingRemote) {
        lt.sendPlaybackAction("change_track", {
          track_info: lt.trackInfo(song),
          position: 0,
        });
      }
    }
    // Sleep countdown text.
    updateSleepCountdown();
  }
  player.onStateChange(refreshNowPlaying);

  function togglePlayback() {
    player.toggle();
    // Broadcast the intent immediately (toggle() is async; the state
    // check below reads the pre-toggle state, which is the intent).
    if (lt.joined && lt.canControlPlayback() && player.currentSong()) {
      lt.sendPlaybackAction(audioEl().paused ? "play" : "pause", {});
    }
  }

  on("pb-play", "click", togglePlayback);
  on("fp-play", "click", togglePlayback);

  function ltGuardedNext() {
    if (lt.joined && !lt.canSkipCurrent()) return;
    player.next();
  }
  function ltGuardedPrev() {
    if (lt.joined && !lt.canSkipCurrent()) return;
    player.previous();
  }

  on("pb-next", "click", ltGuardedNext);
  on("fp-next", "click", ltGuardedNext);
  on("pb-prev", "click", ltGuardedPrev);
  on("fp-prev", "click", ltGuardedPrev);

  on("pb-mute", "click", () => player.toggleMute());
  on("fp-mute", "click", () => player.toggleMute());

  function bindSeek(el) {
    if (!el) return;
    el.addEventListener("input", () => {
      const audio = audioEl();
      if (Number.isFinite(audio.duration) && audio.duration > 0) {
        player.seekTo((parseFloat(el.value) / 100) * audio.duration);
        ltDebouncedSeek();
      }
    });
  }
  bindSeek($("pb-progress"));
  bindSeek($("fp-progress"));

  let seekDebounce = null;
  function ltDebouncedSeek() {
    if (!lt.joined || !lt.canControlPlayback()) return;
    if (seekDebounce) clearTimeout(seekDebounce);
    seekDebounce = setTimeout(() => {
      lt.sendPlaybackAction("seek", {
        position: Math.round(audioEl().currentTime * 1000),
      });
    }, 300);
  }

  // Volume-slider changes persist (debounced): settings.volume is a
  // real field with a save path, so this needs no new plumbing.
  let volumeSaveTimer = null;
  function bindVolume(el) {
    if (!el) return;
    el.addEventListener("input", () => {
      player.setVolume(parseFloat(el.value));
      const pbVolume = $("pb-volume");
      if (pbVolume) pbVolume.value = el.value;
      const settingVolume = $("setting-volume");
      if (settingVolume) settingVolume.value = el.value;
      if (settings) {
        settings.volume = parseFloat(el.value);
        if (volumeSaveTimer) clearTimeout(volumeSaveTimer);
        volumeSaveTimer = setTimeout(() => saveSettingsQuietly(), 500);
      }
    });
  }
  bindVolume($("pb-volume"));
  bindVolume($("setting-volume"));

  function toggleHidden(id) {
    const el = $(id);
    if (el) el.hidden = !el.hidden;
    return el;
  }

  // <dialog> elements stay display:none without the `open` attribute,
  // no matter what `hidden` says — so dialogs go through showModal() /
  // close(), with `hidden` kept in sync (a `hidden` dialog stays hidden
  // even after showModal() in some browsers, so clear it first).
  function openDialog(id) {
    const dialog = $(id);
    if (!dialog) return null;
    dialog.hidden = false;
    if (!dialog.open) {
      try {
        dialog.showModal();
      } catch {
        /* Already open, or no modal support — hidden=false shows it. */
      }
    }
    return dialog;
  }

  function closeDialog(id) {
    const dialog = $(id);
    if (!dialog) return null;
    if (dialog.open) dialog.close();
    dialog.hidden = true;
    return dialog;
  }

  function toggleDialog(id) {
    const dialog = $(id);
    if (!dialog) return null;
    return dialog.open ? closeDialog(id) : openDialog(id);
  }

  on("pb-queue-toggle", "click", () => toggleHidden("queue-panel"));
  on("pb-shuffle", "click", () => player.setShuffle(!player.isShuffle()));
  on("fp-shuffle", "click", () => player.setShuffle(!player.isShuffle()));
  on("pb-repeat", "click", () => player.cycleRepeat());
  on("fp-repeat", "click", () => player.cycleRepeat());
  on("pb-like", "click", () => player.toggleLike().then(() => refreshLibraryData()));
  on("fp-like", "click", () => player.toggleLike().then(() => refreshLibraryData()));
  on("pb-download", "click", () => player.downloadCurrent().then(() => refreshLibraryData()));
  on("fp-download", "click", () => player.downloadCurrent().then(() => refreshLibraryData()));
  on("pb-speed", "click", () => player.setStatus("Speed: " + player.cycleSpeed() + "×"));
  on("fp-speed", "click", () => player.setStatus("Speed: " + player.cycleSpeed() + "×"));

  // Lyrics + EQ panels live inside the full player view.
  on("pb-lyrics-toggle", "click", () => {
    const panel = toggleHidden("lyrics-panel");
    if (panel && !panel.hidden) loadLyricsForCurrent();
  });
  on("fp-lyrics-toggle", "click", () => {
    const panel = toggleHidden("lyrics-panel");
    if (panel && !panel.hidden) loadLyricsForCurrent();
  });
  on("pb-eq-toggle", "click", () => {
    const panel = toggleHidden("eq-panel");
    if (panel && !panel.hidden && settings) views.renderEqPanel(settings);
  });
  on("fp-eq-toggle", "click", () => {
    const panel = toggleHidden("eq-panel");
    if (panel && !panel.hidden && settings) views.renderEqPanel(settings);
  });

  // Queue management: full-player list + queue panel share actions.
  function handleQueueAction(event) {
    const button = event.target.closest("[data-queue-action]");
    if (!button) return;
    const index = Number(button.dataset.queueIndex);
    const action = button.dataset.queueAction;
    if (action === "remove") player.removeAt(index);
    else if (action === "up") player.moveUp(index);
    else if (action === "down") player.moveDown(index);
  }
  const fpQueue = $("fp-queue");
  if (fpQueue) fpQueue.addEventListener("click", handleQueueAction);
  const queueList = $("queue-list");
  if (queueList) queueList.addEventListener("click", handleQueueAction);
  on("queue-clear", "click", () => player.clearQueue());
  on("queue-shuffle-play", "click", () => player.shufflePlayQueue());

  // ---- lyrics -------------------------------------------------------------------
  async function loadLyricsForCurrent() {
    const song = player.currentSong();
    if (!song) return;
    const statusEl = $("lyrics-status");
    if (statusEl) statusEl.textContent = "Loading lyrics…";
    const result = await api.getLyrics(
      song.id,
      song.title,
      (song.artists && song.artists[0] && song.artists[0].name) || "",
      // Song.album is an AlbumRef object { id, title }, not a string —
      // pass the title or the Rust command's serde rejects the call.
      song.album ? song.album.title || null : null,
      song.duration_seconds != null
        ? song.duration_seconds
        : song.duration || null,
    );
    // The song may have changed while fetching — only render for the
    // song that is still current.
    const now = player.currentSong();
    if (!now || now.id !== song.id) return;
    views.renderLyrics(result, song);
  }

  const lyricsLinesHost = $("lyrics-lines");
  if (lyricsLinesHost) {
    lyricsLinesHost.addEventListener("click", (event) => {
      const line = event.target.closest(".lyric-line");
      if (!line) return;
      const timeMs = Number(line.dataset.timeMs);
      if (Number.isFinite(timeMs) && timeMs >= 0) {
        player.seekTo(timeMs / 1000);
      }
    });
  }
  on("lyrics-offset", "input", (event) => {
    views.setLyricsOffsetMs(parseInt(event.target.value, 10) || 0);
  });
  on("lyrics-scroll-current", "click", () => views.scrollLyricsToCurrent());

  // 100ms highlight poll (Android: LaunchedEffect delay(100)).
  setInterval(() => {
    const panel = $("lyrics-panel");
    if (panel && !panel.hidden && player.currentSong()) {
      views.updateLyricsHighlight(audioEl());
    }
  }, 100);

  // ---- EQ panel -------------------------------------------------------------------
  const eqBandsHost = $("eq-bands");
  if (eqBandsHost) {
    eqBandsHost.addEventListener("input", () => {
      const panel = views.readEqPanel();
      player.setLiveEqCurve(panel.bands, panel.preampDb);
      views.updateEqLiveNote();
    });
  }
  on("eq-enhance", "change", (event) => {
    if (!settings) return;
    settings.soundEnhanceEnabled = event.target.checked;
    if (event.target.checked) {
      // "No Equalization" stores null profile AND switches Enhance off;
      // re-enabling Enhance clears any live panel curve.
      player.setLiveEqCurve(null);
      player.setEqSettings(settings);
    } else {
      player.setLiveEqCurve(null);
      player.setEqSettings(settings);
    }
    saveSettingsQuietly();
  });
  on("eq-presets", "change", (event) => {
    const value = event.target.value;
    if (value.startsWith("builtin:")) {
      views.applyEqPreset(value.slice("builtin:".length));
      const panel = views.readEqPanel();
      player.setLiveEqCurve(panel.bands, panel.preampDb);
      views.updateEqLiveNote();
    } else if (value.startsWith("custom:")) {
      const id = value.slice("custom:".length);
      if (settings) {
        settings.activeEqProfileId = id;
        player.setLiveEqCurve(null);
        player.setEqSettings(settings);
        views.renderEqPanel(settings);
        saveSettingsQuietly();
      }
    }
  });
  on("eq-save-preset", "click", () => {
    if (!settings) return;
    const name = window.prompt("Name for this EQ preset");
    if (!name || !name.trim()) return;
    const panel = views.readEqPanel();
    const profile = {
      id: "eq-" + Date.now().toString(36),
      name: name.trim(),
      bands: panel.bands.filter((band) => band.gainDb !== 0),
      preampDb: panel.preampDb,
    };
    settings.eqProfiles = [...(settings.eqProfiles || []), profile];
    settings.activeEqProfileId = profile.id;
    settings.soundEnhanceEnabled = false;
    player.setLiveEqCurve(null);
    player.setEqSettings(settings);
    views.renderEqPanel(settings);
    saveSettingsQuietly();
    player.setStatus('EQ preset "' + profile.name + '" saved.');
  });
  on("eq-delete-preset", "click", () => {
    if (!settings) return;
    const id = settings.activeEqProfileId;
    if (!id) {
      player.setStatus("No custom preset is active.");
      return;
    }
    settings.eqProfiles = (settings.eqProfiles || []).filter(
      (profile) => profile.id !== id,
    );
    settings.activeEqProfileId = null;
    player.setLiveEqCurve(null);
    player.setEqSettings(settings);
    views.renderEqPanel(settings);
    saveSettingsQuietly();
    player.setStatus("EQ preset deleted.");
  });
  on("eq-reset", "click", () => {
    if (!settings) return;
    settings.activeEqProfileId = null;
    settings.soundEnhanceEnabled = true;
    player.setLiveEqCurve(null);
    player.setEqSettings(settings);
    views.renderEqPanel(settings);
    saveSettingsQuietly();
  });
  on("eq-preamp", "input", () => {
    const panel = views.readEqPanel();
    player.setLiveEqCurve(panel.bands, panel.preampDb);
    views.updateEqLiveNote();
  });

  // ---- sleep timer ------------------------------------------------------------------
  function updateSleepCountdown() {
    if (!player.isSleepActive()) {
      views.updateSleepStatus("");
      return;
    }
    if (player.isSleepEndOfSong()) {
      views.updateSleepStatus("Sleep timer: pausing when this song ends.");
      return;
    }
    const remaining = player.sleepRemainingMs();
    if (remaining == null) return;
    const totalSeconds = Math.ceil(remaining / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    views.updateSleepStatus(
      "Sleep timer: " + minutes + ":" + String(seconds).padStart(2, "0") + " left",
    );
  }
  setInterval(updateSleepCountdown, 1000);

  on("pb-sleep", "click", () => toggleDialog("sleep-dialog"));
  on("fp-sleep", "click", () => toggleDialog("sleep-dialog"));
  const sleepOptions = $("sleep-options");
  if (sleepOptions) {
    sleepOptions.addEventListener("click", (event) => {
      const button = event.target.closest("[data-minutes]");
      if (!button) return;
      const minutes = Number(button.dataset.minutes);
      player.startSleepTimer(minutes);
      closeDialog("sleep-dialog");
      updateSleepCountdown();
    });
  }
  on("sleep-cancel", "click", () => {
    player.clearSleepTimer();
    closeDialog("sleep-dialog");
    updateSleepCountdown();
  });

  // ---- updates -------------------------------------------------------------------------
  async function runUpdateCheck(fromStartup) {
    const info = await api.checkForUpdate();
    const statusEl = $("settings-update-status");
    if (!info) {
      // Command unavailable (preview / Rust not built yet).
      if (!fromStartup && statusEl) {
        statusEl.textContent = "Couldn't check for updates right now.";
      }
      return;
    }
    if (info.error) {
      // NEVER "latest" on a failed check.
      if (statusEl) {
        statusEl.textContent = "Couldn't check for updates: " + info.error;
      }
      return;
    }
    if (info.available) {
      views.showUpdateDialog(info);
      if (statusEl) statusEl.textContent = "An update is available.";
    } else if (!fromStartup && statusEl) {
      statusEl.textContent = "You're up to date.";
    }
  }
  on("settings-check-updates", "click", () => runUpdateCheck(false));
  on("update-later", "click", () => views.hideUpdateDialog());
  on("update-download", "click", async (event) => {
    const url = event.target.dataset.url;
    if (!url) return;
    const opened = await api.openUpdateDownload(url);
    const statusEl = $("update-status");
    if (!opened && statusEl) {
      statusEl.textContent = "Couldn't open the download — get it here: " + url;
    } else {
      views.hideUpdateDialog();
    }
  });

  // ---- sign-in ----------------------------------------------------------------------------
  // The cookie is never stored in JS: it goes straight to signin_finish
  // and the input is cleared. The login window path opens a second
  // Tauri window at the Google ServiceLogin URL; the cookie hand-off
  // still uses the paste box below, because the Tauri JS API has no
  // cross-webview JS eval to harvest it from that window. (A Rust
  // eval command would close that gap.)
  let signinPasteForm = null;

  function showSigninPasteForm(authUrl) {
    const block = $("account-block");
    if (!block) return null;
    if (signinPasteForm) return signinPasteForm;
    const form = document.createElement("div");
    form.className = "signin-paste";
    form.innerHTML =
      '<p>Sign in in the login window (or your browser), then paste the cookie here to finish. The cookie is sent to the app core immediately and never stored in this page.</p>' +
      '<input type="password" class="signin-cookie-input" placeholder="Paste the cookie…" autocomplete="off" aria-label="Cookie for sign-in">' +
      '<button type="button" class="signin-finish">Finish sign-in</button>' +
      '<button type="button" class="signin-browser">Open in browser instead</button>';
    block.appendChild(form);
    const finishButton = form.querySelector(".signin-finish");
    finishButton.addEventListener("click", async () => {
      const input = form.querySelector(".signin-cookie-input");
      const cookie = input ? input.value.trim() : "";
      if (!cookie) {
        player.setStatus("Paste the cookie first.");
        return;
      }
      if (input) input.value = "";
      const result = await api.signinFinish(cookie);
      if (result && result.accountName) {
        form.remove();
        signinPasteForm = null;
        await refreshSigninStatus();
        player.setStatus("Signed in as " + result.accountName + ".");
      } else {
        player.setStatus("That cookie didn't work — try signing in again.");
      }
    });
    const browserButton = form.querySelector(".signin-browser");
    browserButton.addEventListener("click", async () => {
      if (authUrl) await api.openUpdateDownload(authUrl);
    });
    signinPasteForm = form;
    return form;
  }

  on("account-signin", "click", async () => {
    const started = await api.signinStart();
    if (!started || !started.authUrl) {
      player.setStatus("Sign-in is not available right now.");
      return;
    }
    let openedWindow = false;
    try {
      const tauriWindow =
        window.__TAURI__ && window.__TAURI__.window
          ? window.__TAURI__.window
          : null;
      if (tauriWindow && tauriWindow.WebviewWindow) {
        const loginWindow = new tauriWindow.WebviewWindow("aello-signin", {
          url: started.authUrl,
          title: "Sign in to Aello",
          width: 480,
          height: 720,
        });
        loginWindow.once("tauri://error", () => {
          /* Window failed to open; the paste fallback below covers it. */
        });
        openedWindow = true;
      }
    } catch {
      openedWindow = false;
    }
    player.setStatus(
      openedWindow
        ? "Sign-in window opened — sign in there, then paste the cookie below to finish."
        : "Couldn't open the sign-in window — sign in in your browser instead, then paste the cookie below.",
    );
    showSigninPasteForm(started.authUrl);
  });

  on("account-signout", "click", async () => {
    const ok = await api.signOut();
    if (ok) {
      await refreshSigninStatus();
      player.setStatus("Signed out.");
    } else {
      player.setStatus("Couldn't sign out right now.");
    }
  });

  async function refreshSigninStatus() {
    const status = await api.signinStatus();
    views.setAccountState(!!status.signedIn, status.accountName || null);
    return status;
  }

  // ---- Listen Together -----------------------------------------------------------------------
  const LT_SESSION_KEY = "aello.lt.session.v1";
  const LT_SERVERS = {
    global: "wss://devilmi-vivi-music-listen-together.hf.space",
    usa: "wss://vivimusic-listen-together.onrender.com",
  };
  const MAX_RECONNECT_ATTEMPTS = 15;
  const MAX_RECONNECT_DELAY_MS = 120000;

  const lt = {
    socket: null,
    roomCode: null,
    username: "",
    myUserId: null,
    isHost: false,
    controlMode: "owner",
    joined: false,
    members: new Map(),
    pending: [],
    sessionToken: null,
    reconnectAttempts: 0,
    reconnectTimer: null,
    intentionalLeave: false,
    pendingAction: null, // { type: "create"|"join", roomCode? }
    applyingRemote: false,
    pendingRemotePlay: false,
    savedMuted: false,
    pingTimer: null,

    serverWsUrl() {
      // The Listen Together view's free-text server field (#lt-server)
      // wins when filled; otherwise fall back to the saved setting.
      const input = document.getElementById("lt-server");
      const typed = input && input.value ? input.value.trim() : "";
      if (typed && /^(wss?:\/\/|https?:\/\/)/i.test(typed)) {
        return typed.replace(/^http/i, "ws");
      }
      const value = (settings && settings.ltServer) || "global";
      if (LT_SERVERS[value]) return LT_SERVERS[value];
      if (/^(wss?:\/\/|https?:\/\/)/i.test(value)) {
        return value.replace(/^http/i, "ws");
      }
      return LT_SERVERS.global;
    },

    httpBase() {
      return this.serverWsUrl().replace(/^wss:\/\//i, "https://").replace(/^ws:\/\//i, "http://");
    },

    send(type, payload) {
      if (!this.socket || this.socket.readyState !== WebSocket.OPEN) return false;
      try {
        this.socket.send(
          JSON.stringify(payload === undefined ? { type } : { type, payload }),
        );
        return true;
      } catch {
        return false;
      }
    },

    canControlPlayback() {
      if (!this.joined) return false;
      return this.controlMode === "everyone" || this.isHost;
    },

    // Per-track ownership (Android RoomPermissions): in EVERYONE mode a
    // track belongs to whoever queued it (suggested_by); only that user
    // or the host may skip/remove it. A track with no suggested_by came
    // from the host's queue — guests get "request" instead of skip.
    canSkipCurrent() {
      if (!this.joined) return true;
      if (!this.canControlPlayback()) {
        player.setStatus("Only the host can control playback in this room.");
        return false;
      }
      if (this.controlMode === "everyone" && !this.isHost) {
        const track = this.roomTrack;
        if (track && track.suggested_by && track.suggested_by !== this.username) {
          player.setStatus(
            "That track was added by " + track.suggested_by + " — only they or the host can skip it.",
          );
          return false;
        }
      }
      return true;
    },

    trackInfo(song) {
      // album is an AlbumRef object on API songs — the wire protocol
      // carries a plain title string (or null), never the object.
      const albumTitle =
        song.album == null
          ? null
          : typeof song.album === "string"
            ? song.album
            : song.album.title || null;
      const durationSecs =
        song.duration_seconds != null
          ? song.duration_seconds
          : song.duration || 0;
      return {
        id: song.id,
        title: song.title || "Untitled",
        artist: (song.artists || []).map((artist) => artist.name).join(", "),
        album: albumTitle,
        duration: Math.round(durationSecs * 1000),
        thumbnail: null,
        suggested_by: null,
      };
    },

    songFromTrack(track) {
      return {
        id: track.id,
        title: track.title || "Untitled",
        artists: track.artist ? [{ name: track.artist }] : [],
        album: track.album || null,
        duration: track.duration ? track.duration / 1000 : null,
      };
    },

    sendPlaybackAction(action, extra) {
      if (!this.joined || !this.canControlPlayback()) return;
      this.send("playback_action", { action, ...(extra || {}) });
    },

    async createRoom(username) {
      this.username = username;
      this.clearSession();
      let code = null;
      const created = await api.ltCreateRoom(username);
      if (created && created.roomCode) {
        code = created.roomCode;
      } else {
        // Fallback: direct POST (may fail on CORS — the Rust command
        // is the supported path).
        try {
          const response = await fetch(this.httpBase() + "/api/rooms", {
            method: "POST",
            body: "",
          });
          const data = await response.json();
          code = data.room_code || data.roomCode || null;
        } catch {
          code = null;
        }
      }
      if (code) {
        this.pendingAction = { type: "create", roomCode: code };
        this.connect(code);
      } else {
        // Legacy servers mint the code after connect.
        this.pendingAction = { type: "create", roomCode: null };
        this.connect(null);
      }
    },

    async joinRoom(code, username) {
      const clean = String(code || "").trim().toUpperCase();
      if (!/^[A-Z0-9]{4,12}$/.test(clean)) {
        views.setLtStatus("Room codes are 4–12 letters or numbers.");
        return;
      }
      this.username = username;
      if (
        this.socket &&
        this.socket.readyState === WebSocket.OPEN &&
        this.roomCode === clean
      ) {
        this.send("join_room", { room_code: clean, username });
        return;
      }
      this.pendingAction = { type: "join", roomCode: clean };
      this.connect(clean);
    },

    connect(roomCode) {
      this.closeSocket(true);
      this.roomCode = roomCode;
      const base = this.serverWsUrl().replace(/\/$/, "");
      const url = roomCode ? base + "/room/" + roomCode : base + "/room/";
      views.setLtStatus(
        roomCode ? "Connecting to room " + roomCode + "…" : "Connecting…",
      );
      let socket;
      try {
        socket = new WebSocket(url);
      } catch {
        views.setLtStatus("Couldn't reach the Listen Together server.");
        return;
      }
      this.socket = socket;
      socket.addEventListener("open", () => this.onOpen());
      socket.addEventListener("message", (event) => this.onMessage(event));
      socket.addEventListener("close", () => this.onClose());
      socket.addEventListener("error", () => {
        /* Close follows; the reconnect path reports it. */
      });
    },

    onOpen() {
      this.reconnectAttempts = 0;
      if (this.pingTimer) clearInterval(this.pingTimer);
      this.pingTimer = setInterval(() => this.send("ping"), 25000);
      // Session first (Android: reconnect with the token on open);
      // a pending create/join wins for a fresh room.
      if (this.pendingAction) {
        const action = this.pendingAction;
        this.pendingAction = null;
        if (action.type === "create") {
          this.send("create_room", { username: this.username });
        } else {
          this.send("join_room", {
            room_code: action.roomCode,
            username: this.username,
          });
        }
      } else if (this.sessionToken) {
        this.send("reconnect", { session_token: this.sessionToken });
      } else {
        this.send("request_sync");
      }
    },

    onClose() {
      if (this.pingTimer) {
        clearInterval(this.pingTimer);
        this.pingTimer = null;
      }
      this.socket = null;
      if (this.intentionalLeave) {
        this.intentionalLeave = false;
        return;
      }
      if (!this.roomCode && !this.sessionToken) return;
      if (this.reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
        views.setLtStatus(
          "Connection lost. Your session is saved — rejoin the room to continue.",
        );
        return;
      }
      const delay = Math.min(
        MAX_RECONNECT_DELAY_MS,
        1000 * Math.pow(2, this.reconnectAttempts),
      );
      this.reconnectAttempts += 1;
      views.setLtStatus(
        "Connection lost — reconnecting (attempt " +
          this.reconnectAttempts +
          " of " +
          MAX_RECONNECT_ATTEMPTS +
          ")…",
      );
      this.reconnectTimer = setTimeout(() => {
        this.connect(this.roomCode);
      }, delay);
    },

    onMessage(event) {
      let msg;
      try {
        msg = JSON.parse(event.data);
      } catch {
        return; // Binary/protobuf frames are not spoken by this server.
      }
      if (!msg || typeof msg.type !== "string") return;
      const p = msg.payload || {};
      switch (msg.type) {
        case "room_created": {
          this.myUserId = p.user_id || null;
          this.isHost = true;
          this.joined = true;
          this.roomCode = p.room_code || this.roomCode;
          if (p.session_token) this.saveSession(p.session_token);
          views.showLtRoom(this.roomCode, true, "owner");
          views.setLtStatus("Room created — share the code to invite people.");
          if (p.state) this.applyRoomState(p.state);
          break;
        }
        case "join_approved": {
          this.myUserId = p.user_id || null;
          this.joined = true;
          this.roomCode = p.room_code || this.roomCode;
          if (p.session_token) this.saveSession(p.session_token);
          views.showLtRoom(this.roomCode, this.isHost, this.controlMode);
          views.setLtStatus("Joined room " + this.roomCode + ".");
          if (p.state) this.applyRoomState(p.state);
          // Guests are muted on join; the previous mute state is
          // restored on leave.
          if (!this.isHost) {
            this.savedMuted = player.isMuted();
            player.setMuted(true);
          }
          this.send("request_sync");
          break;
        }
        case "join_rejected": {
          views.setLtStatus("Couldn't join: " + (p.reason || "rejected by host."));
          this.clearSession();
          break;
        }
        case "join_request": {
          if (!this.pending.some((item) => item.user_id === p.user_id)) {
            this.pending.push({ user_id: p.user_id, username: p.username });
          }
          this.renderMembers();
          views.setLtStatus((p.username || "Someone") + " wants to join.");
          break;
        }
        case "user_joined": {
          if (p.user_id) {
            this.members.set(p.user_id, {
              user_id: p.user_id,
              username: p.username || "Guest",
              is_host: p.user_id === this.hostId,
              is_connected: true,
            });
          }
          this.renderMembers();
          break;
        }
        case "user_left": {
          if (p.user_id) this.members.delete(p.user_id);
          this.renderMembers();
          break;
        }
        case "user_reconnected":
        case "user_disconnected": {
          const member = this.members.get(p.user_id);
          if (member) {
            member.is_connected = msg.type === "user_reconnected";
            this.renderMembers();
          }
          break;
        }
        case "host_changed": {
          this.hostId = p.new_host_id || null;
          this.isHost = this.hostId === this.myUserId;
          views.showLtRoom(this.roomCode, this.isHost, this.controlMode);
          this.renderMembers();
          views.setLtStatus(
            (p.new_host_name || "Someone") + " is now the host.",
          );
          break;
        }
        case "control_mode_changed": {
          this.controlMode = p.control_mode === "everyone" ? "everyone" : "owner";
          views.showLtRoom(this.roomCode, this.isHost, this.controlMode);
          views.setLtStatus(
            this.controlMode === "everyone"
              ? "Everyone can control playback now."
              : "Only the host can control playback now.",
          );
          break;
        }
        case "sync_playback": {
          this.applySyncPlayback(p);
          break;
        }
        case "sync_state": {
          this.applySyncState(p);
          break;
        }
        case "buffer_wait": {
          views.setLtStatus("Waiting for everyone to buffer…");
          break;
        }
        case "buffer_complete": {
          views.setLtStatus("");
          if (this.pendingRemotePlay) {
            this.pendingRemotePlay = false;
            if (audioEl().paused) player.toggle();
          }
          break;
        }
        case "chat": {
          views.appendLtChat(p.username, p.message || "");
          break;
        }
        case "suggestion_received": {
          const track = p.track_info || {};
          const line =
            (p.from_username || "Someone") +
            " suggested “" +
            (track.title || "a song") +
            "”.";
          views.appendLtChat("Listen Together", line);
          if (this.isHost) {
            const log = $("lt-chat-log");
            if (log) {
              const row = document.createElement("p");
              row.innerHTML =
                '<button type="button" data-lt-approve-suggestion="' +
                views.escapeHtml(p.suggestion_id || "") +
                '">Add to queue</button> ' +
                '<button type="button" data-lt-reject-suggestion="' +
                views.escapeHtml(p.suggestion_id || "") +
                '">Dismiss</button>';
              log.appendChild(row);
            }
          }
          break;
        }
        case "suggestion_approved": {
          const track = p.track_info || {};
          views.appendLtChat(
            "Listen Together",
            "“" + (track.title || "A song") + "” was added to the queue.",
          );
          if (track.id) player.addToQueue(this.songFromTrack(track));
          break;
        }
        case "suggestion_rejected": {
          views.appendLtChat(
            "Listen Together",
            "A suggestion was dismissed." +
              (p.reason ? " (" + p.reason + ")" : ""),
          );
          break;
        }
        case "kicked": {
          views.setLtStatus("You were removed from the room.");
          this.leave(true);
          break;
        }
        case "room_expiring": {
          views.setLtStatus("This room expires soon — ask the host to extend it.");
          break;
        }
        case "room_closed": {
          views.setLtStatus("The room was closed.");
          this.leave(true);
          break;
        }
        case "reconnected": {
          this.joined = true;
          views.showLtRoom(this.roomCode, this.isHost, this.controlMode);
          if (p.state) this.applyRoomState(p.state);
          views.setLtStatus("Reconnected to room " + (this.roomCode || "") + ".");
          break;
        }
        case "error": {
          views.setLtStatus(
            "Listen Together: " + (p.message || p.code || "unknown error"),
          );
          break;
        }
        case "pong":
          break;
        default:
          break;
      }
    },

    applyRoomState(state) {
      if (!state || typeof state !== "object") return;
      if (state.host_id) this.hostId = state.host_id;
      this.isHost = this.hostId === this.myUserId;
      if (state.control_mode) {
        this.controlMode =
          state.control_mode === "everyone" ? "everyone" : "owner";
      }
      if (Array.isArray(state.users)) {
        this.members.clear();
        state.users.forEach((user) => {
          this.members.set(user.user_id, {
            user_id: user.user_id,
            username: user.username || "Guest",
            is_host: !!user.is_host,
            is_connected: user.is_connected !== false,
          });
        });
      }
      this.roomTrack = state.current_track || null;
      this.renderMembers();
      if (state.current_track) {
        this.applySyncState({
          current_track: state.current_track,
          is_playing: !!state.is_playing,
          position: state.position || 0,
        });
      }
    },

    applySyncState(state) {
      if (state.control_mode) {
        this.controlMode =
          state.control_mode === "everyone" ? "everyone" : "owner";
        views.showLtRoom(this.roomCode, this.isHost, this.controlMode);
      }
      if (state.current_track) {
        this.roomTrack = state.current_track;
        this.applySyncStateTrack(state);
      }
    },

    applySyncStateTrack(state) {
      const track = state.current_track;
      const current = player.currentSong();
      if (!current || current.id !== track.id) {
        this.applyRemoteTrack(track, state);
        return;
      }
      this.applyRemotePlayState(state);
    },

    applySyncPlayback(p) {
      if (p.from_user_id && p.from_user_id === this.myUserId) return;
      switch (p.action) {
        case "play":
          if (audioEl().paused) player.toggle();
          break;
        case "pause":
          if (!audioEl().paused) player.toggle();
          break;
        case "seek":
          if (typeof p.position === "number") {
            const target = p.position / 1000;
            if (Math.abs(audioEl().currentTime - target) > 2) {
              player.seekTo(target);
            }
          }
          break;
        case "change_track":
        case "skip_next":
        case "skip_prev":
          if (p.track_info) {
            this.roomTrack = p.track_info;
            this.applyRemoteTrack(p.track_info, p);
          }
          break;
        case "queue_add":
          if (p.track_info) player.addToQueue(this.songFromTrack(p.track_info));
          break;
        case "queue_remove":
          if (p.track_id) {
            const index = player.queue.findIndex((song) => song.id === p.track_id);
            if (index >= 0) player.removeAt(index);
          }
          break;
        case "queue_clear":
          player.clearQueue();
          break;
        case "sync_queue":
          if (Array.isArray(p.queue)) {
            player.setQueue(p.queue.map((track) => this.songFromTrack(track)));
          }
          break;
        default:
          break;
      }
    },

    applyRemotePlayState(state) {
      const wantPlaying = !!state.is_playing;
      const paused = audioEl().paused;
      if (wantPlaying && paused) player.toggle();
      else if (!wantPlaying && !paused) player.toggle();
      if (typeof state.position === "number") {
        const target = state.position / 1000;
        if (Math.abs(audioEl().currentTime - target) > 2) {
          player.seekTo(target);
        }
      }
    },

    // Buffer gate: load the track paused, report buffer_ready when the
    // stream is buffered, and only play on buffer_complete (the server
    // releases the room after 10s if someone never reports).
    async applyRemoteTrack(track, state) {
      const song = this.songFromTrack(track);
      this.applyingRemote = true;
      try {
        await player.playSong(song);
      } finally {
        setTimeout(() => {
          this.applyingRemote = false;
        }, 1000);
      }
      const audio = audioEl();
      audio.pause();
      this.pendingRemotePlay = state.is_playing !== false;
      if (typeof state.position === "number" && state.position > 0) {
        player.seekTo(state.position / 1000);
      }
      const reportReady = () => {
        this.send("buffer_ready", { track_id: track.id });
      };
      if (audio.readyState >= 3) {
        reportReady();
      } else {
        const onCanPlay = () => {
          audio.removeEventListener("canplay", onCanPlay);
          reportReady();
        };
        audio.addEventListener("canplay", onCanPlay);
        // Never wait forever: the server's 10s timeout releases the
        // room anyway, but report once so a slow stream doesn't stall it.
        setTimeout(() => {
          audio.removeEventListener("canplay", onCanPlay);
          reportReady();
        }, 8000);
      }
    },

    renderMembers() {
      views.renderLtMembers(
        [...this.members.values()],
        this.hostId,
        this.myUserId,
        this.pending,
      );
    },

    saveSession(token) {
      this.sessionToken = token;
      try {
        localStorage.setItem(
          LT_SESSION_KEY,
          JSON.stringify({
            token,
            roomCode: this.roomCode,
            username: this.username,
          }),
        );
      } catch {
        /* Session stays in memory for this run. */
      }
    },

    loadSession() {
      try {
        const stored = JSON.parse(localStorage.getItem(LT_SESSION_KEY) || "null");
        if (stored && stored.token) {
          this.sessionToken = stored.token;
          this.roomCode = stored.roomCode || null;
          this.username = stored.username || "";
          return true;
        }
      } catch {
        /* No saved session. */
      }
      return false;
    },

    clearSession() {
      this.sessionToken = null;
      try {
        localStorage.removeItem(LT_SESSION_KEY);
      } catch {
        /* Already gone. */
      }
    },

    closeSocket(silent) {
      if (this.reconnectTimer) {
        clearTimeout(this.reconnectTimer);
        this.reconnectTimer = null;
      }
      if (this.socket) {
        try {
          this.socket.close();
        } catch {
          /* Already closed. */
        }
        this.socket = null;
      }
      if (!silent) {
        /* Leave path handles the UI. */
      }
    },

    leave(silent) {
      this.intentionalLeave = true;
      this.send("leave_room");
      this.closeSocket();
      this.joined = false;
      this.isHost = false;
      this.roomCode = null;
      this.roomTrack = null;
      this.members.clear();
      this.pending = [];
      this.clearSession();
      if (!this.isHost) player.setMuted(this.savedMuted);
      views.hideLtRoom();
      if (!silent) views.setLtStatus("You left the room.");
    },
  };

  function ltUsername() {
    const input = $("lt-username");
    const name = input ? input.value.trim() : "";
    return name || (settings && settings.ltUsername) || "Guest";
  }

  on("lt-create", "click", () => {
    const username = ltUsername();
    if (settings) {
      settings.ltUsername = username;
      saveSettingsQuietly();
    }
    lt.createRoom(username);
  });
  on("lt-join", "click", () => {
    const username = ltUsername();
    const codeInput = $("lt-join-code");
    const code = codeInput ? codeInput.value : "";
    if (settings) {
      settings.ltUsername = username;
      saveSettingsQuietly();
    }
    lt.joinRoom(code, username);
  });
  on("lt-join-code", "keydown", (event) => {
    if (event.key === "Enter") {
      const codeInput = $("lt-join-code");
      lt.joinRoom(codeInput ? codeInput.value : "", ltUsername());
    }
  });
  on("lt-leave", "click", () => lt.leave(false));
  on("lt-control-mode", "change", (event) => {
    if (!lt.isHost) return;
    lt.send("set_control_mode", { control_mode: event.target.value });
  });
  on("lt-chat-send", "click", () => {
    const input = $("lt-chat-input");
    if (!input) return;
    const text = input.value.trim();
    if (!text) return;
    input.value = "";
    lt.send("chat", { message: text });
  });
  on("lt-chat-input", "keydown", (event) => {
    if (event.key === "Enter") {
      const send = $("lt-chat-send");
      if (send) send.click();
    }
  });
  on("lt-suggest", "click", () => {
    const song = player.currentSong();
    if (!song) {
      views.setLtStatus("Play a song first to suggest it.");
      return;
    }
    lt.send("suggest_track", { track_info: lt.trackInfo(song) });
    views.setLtStatus('Suggested "' + (song.title || "Untitled") + '" to the host.');
  });
  const ltMembersHost = $("lt-members");
  if (ltMembersHost) {
    ltMembersHost.addEventListener("click", (event) => {
      const approve = event.target.closest("[data-lt-approve]");
      if (approve) {
        lt.send("approve_join", { user_id: approve.dataset.ltApprove });
        lt.pending = lt.pending.filter(
          (item) => item.user_id !== approve.dataset.ltApprove,
        );
        lt.renderMembers();
        return;
      }
      const reject = event.target.closest("[data-lt-reject]");
      if (reject) {
        lt.send("reject_join", { user_id: reject.dataset.ltReject });
        lt.pending = lt.pending.filter(
          (item) => item.user_id !== reject.dataset.ltReject,
        );
        lt.renderMembers();
      }
    });
  }
  const ltChatLog = $("lt-chat-log");
  if (ltChatLog) {
    ltChatLog.addEventListener("click", (event) => {
      const approve = event.target.closest("[data-lt-approve-suggestion]");
      if (approve) {
        lt.send("approve_suggestion", {
          suggestion_id: approve.dataset.ltApproveSuggestion,
        });
        approve.closest("p").remove();
        return;
      }
      const reject = event.target.closest("[data-lt-reject-suggestion]");
      if (reject) {
        lt.send("reject_suggestion", {
          suggestion_id: reject.dataset.ltRejectSuggestion,
        });
        reject.closest("p").remove();
      }
    });
  }

  // ---- keyboard shortcuts ---------------------------------------------------------
  // Space toggles playback, Left/Right seek ±10s, M mutes. Added: N/P
  // next/prev, S shuffle, R repeat, L like, Up/Down volume. None fire
  // while typing in a field (that also keeps the arrow keys working
  // natively on the volume/seek sliders).
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
      togglePlayback();
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      player.seekTo(audioEl().currentTime + 10);
      ltDebouncedSeek();
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      player.seekTo(audioEl().currentTime - 10);
      ltDebouncedSeek();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      player.setVolume(player.getVolume() + 0.05);
      const pbVolume = $("pb-volume");
      if (pbVolume) pbVolume.value = player.getVolume();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      player.setVolume(player.getVolume() - 0.05);
      const pbVolume = $("pb-volume");
      if (pbVolume) pbVolume.value = player.getVolume();
    } else if (event.key === "m" || event.key === "M") {
      player.toggleMute();
    } else if (event.key === "n" || event.key === "N") {
      ltGuardedNext();
    } else if (event.key === "p" || event.key === "P") {
      ltGuardedPrev();
    } else if (event.key === "s" || event.key === "S") {
      player.setShuffle(!player.isShuffle());
    } else if (event.key === "r" || event.key === "R") {
      player.cycleRepeat();
    } else if (event.key === "l" || event.key === "L") {
      player.toggleLike();
    }
  });

  // ---- settings: load, apply, save -----------------------------------------------
  async function saveSettingsQuietly() {
    if (!settings) return;
    try {
      await api.saveSettings(settings);
    } catch {
      /* Settings save is best-effort outside the desktop app. */
    }
  }

  async function loadSettings() {
    settings = await api.getSettings();
    views.applyTheme(settings);
    views.fillSettingsForm(settings);
    player.applySettings(settings);
    player.setVolume(settings.volume);
    const pbVolume = $("pb-volume");
    if (pbVolume) pbVolume.value = settings.volume;
    const ltServerSelect = $("setting-listen-together-server");
    if (ltServerSelect) {
      ltServerSelect.addEventListener("change", () => {
        const custom = $("setting-lt-custom-server");
        if (custom) custom.hidden = ltServerSelect.value !== "custom";
      });
    }
    // The download folder is real now: show the app-data downloads
    // dir plus how many songs are stored there.
    try {
      const dir = await api.downloadDir();
      const downloads = await api.getDownloads();
      const display = $("download-dir-display");
      if (display) {
        display.textContent = dir
          ? dir +
            " — " +
            downloads.length +
            (downloads.length === 1 ? " song" : " songs") +
            " downloaded"
          : "Downloads are not available in preview mode.";
      }
    } catch {
      const display = $("download-dir-display");
      if (display) display.textContent = "Downloads are not available right now.";
    }
    // Restore the persisted queue (localStorage; the Rust snapshot
    // only carries ids). Never auto-plays on startup.
    try {
      const position = player.restoreQueue();
      const song = player.currentSong();
      if (song && position > 0) {
        const url = song.localPath
          ? api.localFileUrl(song.localPath)
          : song.downloadedPath
            ? api.localFileUrl(song.downloadedPath)
            : await api.resolveStream(song.id);
        if (url) {
          const audio = audioEl();
          const onMeta = () => {
            audio.removeEventListener("loadedmetadata", onMeta);
            if (Number.isFinite(audio.duration) && position < audio.duration) {
              audio.currentTime = position;
            }
          };
          audio.addEventListener("loadedmetadata", onMeta);
          audio.src = url;
        }
      }
    } catch {
      /* Queue restore is best-effort. */
    }
    // Update check on startup (only when the setting is on; the dialog
    // appears only when an update is actually available).
    if (settings.checkForUpdatesOnStartup !== false) {
      runUpdateCheck(true).catch(() => {
        /* A failed startup check stays silent; the settings page says so. */
      });
    }
    await refreshSigninStatus();
    // A saved Listen Together session offers a manual rejoin; it never
    // auto-connects (the user may be on a different network now).
    if (lt.loadSession() && lt.roomCode) {
      views.setLtStatus(
        "You have a saved Listen Together session for room " +
          lt.roomCode +
          " — enter the code again to rejoin.",
      );
      const codeInput = $("lt-join-code");
      if (codeInput) codeInput.value = lt.roomCode;
      const nameInput = $("lt-username");
      if (nameInput && lt.username) nameInput.value = lt.username;
    }
  }

  const settingsForm = $("settings-form");
  if (settingsForm) {
    settingsForm.addEventListener("submit", async (event) => {
      event.preventDefault();
      const next = views.readSettingsForm(settings);
      // Live EQ state (panel edits / Enhance checkbox) flows into the
      // saved settings; the panel's transient curve is cleared so the
      // stored profile is what the player uses from here on.
      if (settings) {
        next.soundEnhanceEnabled = settings.soundEnhanceEnabled;
        next.activeEqProfileId = settings.activeEqProfileId;
        next.eqProfiles = settings.eqProfiles;
        next.ltUsername = settings.ltUsername || "";
      }
      settings = next;
      views.applyTheme(settings);
      player.applySettings(settings);
      player.setVolume(settings.volume);
      const pbVolume = $("pb-volume");
      if (pbVolume) pbVolume.value = settings.volume;
      player.setLiveEqCurve(null);
      const statusEl = $("settings-status");
      try {
        const saved = await api.saveSettings(settings);
        if (statusEl) {
          statusEl.textContent = saved
            ? "Settings saved."
            : "Settings apply for this session only — saving needs the desktop app.";
        }
      } catch (err) {
        if (statusEl) {
          statusEl.textContent =
            "Couldn't save settings: " + (err && err.message ? err.message : err);
        }
      }
    });
  }

  refreshNowPlaying();
  renderLibrarySegment();
  updateHomeGreeting();
  loadSettings().catch(() => {
    /* Defaults already applied in the markup; settings load is best-effort. */
  });
  loadHome();

  // SMTC (Windows media keys / overlay): Rust emits `smtc-play` /
  // `smtc-pause` / `smtc-next` / `smtc-previous` (src-tauri/src/smtc.rs)
  // — wire them to the player. No-ops outside the desktop app.
  const tauriEvent =
    window.__TAURI__ && window.__TAURI__.event ? window.__TAURI__.event : null;
  if (tauriEvent && typeof tauriEvent.listen === "function") {
    const listenSmtc = (event, handler) => {
      try {
        const result = tauriEvent.listen(event, handler);
        if (result && typeof result.catch === "function") {
          result.catch(() => {
            /* SMTC events are best-effort. */
          });
        }
      } catch {
        /* SMTC events are best-effort. */
      }
    };
    listenSmtc("smtc-play", () => {
      if (!player.isPlaying()) player.toggle();
    });
    listenSmtc("smtc-pause", () => {
      if (player.isPlaying()) player.toggle();
    });
    listenSmtc("smtc-next", () => player.next());
    listenSmtc("smtc-previous", () => player.previous());
  }
});
