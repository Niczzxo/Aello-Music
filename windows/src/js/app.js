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

  let activeHomeChipParams = null;

  async function loadHome(chipParams) {
    const statusEl = $("home-status");
    activeHomeChipParams = chipParams != null ? chipParams : null;
    // R4: show the skeleton while the feed loads so a slow network
    // shows structure instead of a blank page.
    if (statusEl) statusEl.textContent = "Loading…";
    views.renderHomeSkeleton();
    try {
      const [feed, daily, personal] = await Promise.all([
        api.getHomeFeed(activeHomeChipParams || undefined),
        api.getDailyDiscover(),
        api.getHomePersonal(),
      ]);
      views.renderHome(feed, statusEl, daily, personal, activeHomeChipParams);
    } catch (err) {
      if (statusEl) {
        statusEl.textContent =
          "Couldn't load the home feed: " + (err && err.message ? err.message : err);
      }
    }
  }

  // Home chips (audit #1): a tap re-fetches the feed WITH that chip's
  // params and the feed visibly changes; All returns to the unfiltered
  // feed. (The Android chips bug — chips that don't filter — is the
  // exact failure this avoids: the refetch is the filter.)
  const homeChipsHost = $("home-chips");
  if (homeChipsHost) {
    homeChipsHost.addEventListener("click", (event) => {
      const chip = event.target.closest("[data-chip-params]");
      if (!chip) return;
      const params = chip.dataset.chipParams || "";
      loadHome(params === "" ? null : params);
    });
  }

  // Home load-more (audit #1) + per-section Play-all.
  const homeSectionsHost = $("home-sections");
  if (homeSectionsHost) {
    homeSectionsHost.addEventListener("click", async (event) => {
      const more = event.target.closest("[data-home-more]");
      if (more) {
        const continuation = views.getHomeContinuation();
        if (!continuation) return;
        more.disabled = true;
        more.textContent = "Loading…";
        try {
          const feed = await api.getHomeFeedMore(continuation);
          views.appendHomeSections(feed);
        } catch (err) {
          player.setStatus(
            "Couldn't load more: " + (err && err.message ? err.message : err),
          );
          more.disabled = false;
          more.textContent = "Load more";
        }
        return;
      }
      const playAll = event.target.closest("[data-play-section]");
      if (playAll) {
        const songs = views.getSectionSongs(Number(playAll.dataset.playSection));
        if (songs.length > 0) await player.playSong(songs[0], songs);
        return;
      }
      const detailAction = event.target.closest("[data-detail-action]");
      if (detailAction) {
        await handleDetailAction(detailAction.dataset.detailAction);
        return;
      }
    });
    // Speed-dial pinning (audit #18): right-click a card to pin /
    // unpin its song. The Speed Dial row on Home updates on the next
    // Home load.
    homeSectionsHost.addEventListener("contextmenu", async (event) => {
      const card = event.target.closest(".card");
      if (!card || card.dataset.kind !== "song") return;
      event.preventDefault();
      const entry = views.findItem(card.dataset.id);
      if (!entry) return;
      const pinned = await api.toggleSpeedDial(entry.item);
      player.setStatus(
        pinned
          ? 'Pinned "' + (entry.item.title || "Untitled") + '" to Speed Dial.'
          : 'Removed "' + (entry.item.title || "Untitled") + '" from Speed Dial.',
      );
      loadHome(activeHomeChipParams);
    });
  }

  // Detail header actions (audit #8/#9): play-all, shuffle, like-all,
  // download-all, copy link.
  async function handleDetailAction(action) {
    const songs = views.getDetailSongs();
    if (songs.length === 0) return;
    if (action === "play-all") {
      await player.playSong(songs[0], songs);
    } else if (action === "shuffle") {
      const shuffled = songs.slice();
      for (let i = shuffled.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
      }
      await player.playSong(shuffled[0], shuffled);
    } else if (action === "like-all") {
      // Like every song that is not liked yet; when all are liked,
      // unlike them all (toggle semantics, like a header heart).
      const allLiked = songs.every((song) => player.isLiked(song.id));
      let done = 0;
      for (const song of songs) {
        const ok = await api.likeSong(song, !allLiked);
        if (ok) done += 1;
      }
      await refreshLibraryData();
      player.setStatus(
        done === 0
          ? "Couldn't save those likes yet."
          : allLiked
            ? "Removed " + done + " song(s) from Liked."
            : "Liked " + done + " song(s).",
      );
    } else if (action === "download-all") {
      let done = 0;
      for (const song of songs) {
        player.setStatus(
          "Downloading " + (done + 1) + " of " + songs.length + "…",
        );
        const row = await api.downloadSong(song);
        if (row) done += 1;
      }
      await refreshLibraryData();
      player.setStatus(
        done === songs.length
          ? "Downloaded all " + done + " songs."
          : "Downloaded " + done + " of " + songs.length + " songs.",
      );
    } else if (action === "copy-link") {
      const info = views.getDetailInfo();
      const url = info
        ? info.isAlbum
          ? "https://music.youtube.com/browse/" + info.id
          : "https://music.youtube.com/playlist?list=" + info.id
        : "";
      if (url && navigator.clipboard) {
        try {
          await navigator.clipboard.writeText(url);
          player.setStatus("Link copied.");
        } catch {
          player.setStatus(url);
        }
      } else if (url) {
        player.setStatus(url);
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

  // Playlist / album URLs carry list= (PL… / OLAK… / RD…); channel
  // URLs carry no playable id and fall through to a text search.
  function youtubeListIdFromUrl(text) {
    const match = String(text || "").match(/[?&]list=([A-Za-z0-9_-]+)/);
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
    // URL paste: a playlist link opens the playlist; a single-video
    // link plays the video directly. The title is unknown until it
    // plays, so it is labelled honestly.
    const listId = youtubeListIdFromUrl(q);
    if (listId) {
      rememberSearch(q);
      try {
        const detail = await api.getPlaylist(listId);
        views.renderDetail(detail, false, $("home-status"), () => loadHome());
        showView("home");
      } catch (err) {
        player.setStatus(
          "Couldn't open that playlist: " + (err && err.message ? err.message : err),
        );
      }
      return;
    }
    const videoId = youtubeIdFromUrl(q);
    if (videoId) {
      rememberSearch(q);
      await player.playSong({ id: videoId, title: "YouTube video", artists: [] });
      return;
    }
    views.renderSearchView([], q, tab, searchHistory);
    try {
      const page = await api.search(q, tab === "all" ? undefined : tab);
      rememberSearch(q);
      views.renderSearchView(page.items, q, tab, searchHistory, page.continuation);
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

  // Search load-more (audit #5).
  const searchResultsHost = $("search-results");
  if (searchResultsHost) {
    searchResultsHost.addEventListener("click", async (event) => {
      const more = event.target.closest("[data-search-more]");
      if (!more) return;
      const continuation = views.getSearchContinuation();
      if (!continuation) return;
      more.disabled = true;
      more.textContent = "Loading…";
      try {
        const page = await api.searchMore(continuation);
        views.appendSearchResults(page.items, page.continuation);
      } catch (err) {
        player.setStatus(
          "Couldn't load more: " + (err && err.message ? err.message : err),
        );
        more.disabled = false;
        more.textContent = "Load more";
      }
    });
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
    const host = $("trending-sections");
    if (statusEl) statusEl.textContent = "Loading…";
    if (host) {
      host.innerHTML =
        '<div class="skeleton-block"><div class="skeleton-line skeleton-title"></div>' +
        '<div class="skeleton-row"><span class="skeleton-card"></span><span class="skeleton-card"></span><span class="skeleton-card"></span><span class="skeleton-card"></span></div></div>';
    }
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
  // Subscribe toggles on YouTube (audit #7; Android
  // `YouTube.subscribeChannel`). Needs a signed-in account; the Rust
  // command says so honestly when there is none.
  const subscribedArtists = new Set();
  on("artist-subscribe", "click", async (event) => {
    const channelId = views.getArtistId();
    if (!channelId) return;
    const button = event.target.closest("button");
    const subscribing = !subscribedArtists.has(channelId);
    const ok = await api.subscribeArtist(channelId, subscribing);
    if (!ok) {
      player.setStatus(
        "Subscribing needs a signed-in account — sign in with Google in Settings first.",
      );
      return;
    }
    if (subscribing) {
      subscribedArtists.add(channelId);
      if (button) button.textContent = "Subscribed";
      player.setStatus("Subscribed to this artist on YouTube.");
    } else {
      subscribedArtists.delete(channelId);
      if (button) button.textContent = "Subscribe";
      player.setStatus("Unsubscribed from this artist on YouTube.");
    }
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
      const librarySongButton = event.target.closest("[data-library-song-id]");
      if (librarySongButton) {
        const song = ((libraryCache && libraryCache.librarySongs) || []).find(
          (entry) => entry.id === librarySongButton.dataset.librarySongId,
        );
        if (song) {
          await player.playSong(
            song,
            (libraryCache.librarySongs || []).filter(
              (entry) => entry && entry.id,
            ),
          );
        }
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
      const exportDownload = event.target.closest('[data-download-action="export"]');
      if (exportDownload) {
        const dest = await api.exportSong(exportDownload.dataset.downloadId);
        player.setStatus(
          dest ? "Exported to " + dest + "." : "Export cancelled.",
        );
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

  // Local files picked from disk. The picked paths persist across
  // restarts (audit #14, persisted index): the files themselves stay
  // on disk, so a stored path still plays — a file deleted or moved
  // since then fails honestly at play time, like any missing file.
  const LOCAL_FILES_KEY = "aello.localFiles.v1";
  function saveLocalFiles() {
    try {
      localStorage.setItem(
        LOCAL_FILES_KEY,
        JSON.stringify(
          localSongs.map((song) => ({ path: song.localPath, name: song.title })),
        ),
      );
    } catch {
      /* Paths stay in memory for this session. */
    }
  }
  function restoreLocalFiles() {
    try {
      const stored = JSON.parse(localStorage.getItem(LOCAL_FILES_KEY) || "[]");
      if (!Array.isArray(stored)) return;
      stored.forEach((file) => {
        if (!file || !file.path) return;
        const song = {
          id: "local:" + file.path,
          title: file.name || file.path,
          artists: [],
          localPath: file.path,
          duration: null,
        };
        if (!localSongs.some((existing) => existing.id === song.id)) {
          localSongs.push(song);
        }
      });
    } catch {
      /* No stored local files. */
    }
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
    saveLocalFiles();
    renderLibrarySegment();
    await player.playSong(fresh[0], player.queue.concat(fresh));
  }

  on("library-open-files", "click", openLocalFiles);
  on("pb-open-files", "click", openLocalFiles);

  // ---- library backup / restore + CSV import (audit #17) ----------------
  function downloadTextFile(filename, text) {
    const blob = new Blob([text], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  on("library-export", "click", async () => {
    const text = await api.exportLibrary();
    if (!text) {
      player.setStatus("Couldn't export the library yet.");
      return;
    }
    const date = new Date().toISOString().slice(0, 10);
    downloadTextFile("aello-library-backup-" + date + ".json", text);
    player.setStatus("Library backup downloaded.");
  });
  function readPickedFile(input, done) {
    const file = input && input.files ? input.files[0] : null;
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => done(String(reader.result || ""));
    reader.onerror = () => player.setStatus("Couldn't read that file.");
    reader.readAsText(file);
    input.value = "";
  }
  on("library-import", "click", () => {
    const input = $("library-import-file");
    if (input) input.click();
  });
  on("library-import-file", "change", (event) => {
    readPickedFile(event.target, async (text) => {
      const ok = await api.importLibrary(text);
      player.setStatus(
        ok ? "Library restored from backup." : "That file is not an Aello library backup.",
      );
      if (ok) await refreshLibraryData();
    });
  });
  on("library-import-csv", "click", () => {
    const input = $("library-csv-file");
    if (input) input.click();
  });
  on("library-csv-file", "change", (event) => {
    readPickedFile(event.target, async (text) => {
      const name = window.prompt("Name for the imported playlist", "Imported playlist");
      if (!name || !name.trim()) return;
      const created = await api.importCsvPlaylist(name.trim(), text);
      player.setStatus(
        created
          ? 'Playlist "' + created.name + '" imported (' + (created.songIds || []).length + " songs)."
          : "No songs found — CSV lines must start with an 11-character YouTube video id.",
      );
      if (created) await refreshLibraryData();
    });
  });

  // ---- player bar + full player controls ---------------------------------------
  function refreshNowPlaying() {
    const song = player.currentSong();
    views.updateNowPlaying(song, audioEl());
    // Radio + Speed Dial pin buttons mirror the current state.
    const radioOn = player.isRadio ? player.isRadio() : true;
    const fpRadio = $("fp-radio");
    if (fpRadio) {
      fpRadio.setAttribute("aria-pressed", radioOn ? "true" : "false");
    }
    const queueRadio = $("queue-radio");
    if (queueRadio) {
      queueRadio.setAttribute("aria-pressed", radioOn ? "true" : "false");
      queueRadio.textContent = "Radio: " + (radioOn ? "on" : "off");
    }
    const fpPin = $("fp-pin");
    if (fpPin) {
      const pinned = !!(song && libraryCache && (libraryCache.speedDialIds || []).includes(song.id));
      fpPin.setAttribute("aria-pressed", pinned ? "true" : "false");
    }
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

  // Radio (autoplay) toggle (audit #28): full player + queue panel.
  function toggleRadio() {
    const next = !player.isRadio();
    player.setRadio(next);
    player.setStatus(next ? "Radio on — similar songs will keep playing." : "Radio off — playback stops at the end of the queue.");
    refreshNowPlaying();
  }
  on("fp-radio", "click", toggleRadio);
  on("queue-radio", "click", toggleRadio);

  // Speed-dial pin for the playing song (audit #18).
  on("fp-pin", "click", async () => {
    const song = player.currentSong();
    if (!song) return;
    const pinned = await api.toggleSpeedDial(song);
    player.setStatus(
      pinned
        ? 'Pinned "' + (song.title || "Untitled") + '" to Speed Dial.'
        : 'Removed "' + (song.title || "Untitled") + '" from Speed Dial.',
    );
    await refreshLibraryData();
    refreshNowPlaying();
  });

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
  // Dialog exit is animated (dialog-out, 140ms — faster than the
  // 220ms enter, per quick-reference §7 exit-faster-than-enter):
  // add .closing, wait out the animation, then really close. With
  // reduced motion (or a second close while one is running) the
  // dialog closes immediately instead.
  const DIALOG_CLOSE_MS = 150;
  function openDialog(id) {
    const dialog = $(id);
    if (!dialog) return null;
    dialog.hidden = false;
    dialog.classList.remove("closing");
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
    if (dialog.open) {
      const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      if (!reduced && !dialog.classList.contains("closing")) {
        dialog.classList.add("closing");
        setTimeout(() => {
          dialog.classList.remove("closing");
          if (dialog.open) dialog.close();
          dialog.hidden = true;
        }, DIALOG_CLOSE_MS);
        return dialog;
      }
      dialog.close();
    }
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
  // Lyrics style (audit #35, styles only).
  on("lyrics-style", "change", (event) => {
    views.applyLyricsStyle(event.target.value);
  });

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
  // 8D audio + normalization (audit #30/#32). The 8D effect orbits
  // the sound with a slow pan LFO; normalization is a gentle
  // compressor, NOT per-track loudness — the UI says exactly that.
  on("eq-8d", "change", (event) => {
    player.set8d(event.target.checked);
    player.setStatus(
      event.target.checked ? "8D audio on." : "8D audio off.",
    );
  });
  on("eq-normalize", "change", (event) => {
    player.setNormalization(event.target.checked);
    player.setStatus(
      event.target.checked
        ? "Volume normalization on (gentle compressor)."
        : "Volume normalization off.",
    );
  });
  // APO preset import (audit #37): reads a Peace/APO text file and
  // applies its filters to the panel (max 5 bands; extras reported).
  on("eq-import-apo", "click", () => {
    const input = $("eq-apo-file");
    if (input) input.click();
  });
  on("eq-apo-file", "change", (event) => {
    const input = event.target;
    const file = input && input.files ? input.files[0] : null;
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const parsed = views.parseApo(String(reader.result || ""));
      if (!parsed) {
        player.setStatus("That file has no APO filters I can read (Preamp + Filter lines).");
        return;
      }
      views.applyEqCurveToPanel(parsed.bands, parsed.preampDb);
      const panel = views.readEqPanel();
      player.setLiveEqCurve(panel.bands, panel.preampDb);
      views.updateEqLiveNote();
      player.setStatus(
        "APO preset applied" +
          (parsed.dropped > 0
            ? " — " + parsed.dropped + " filter(s) skipped (5-band limit)."
            : "."),
      );
    };
    reader.onerror = () => player.setStatus("Couldn't read that file.");
    reader.readAsText(file);
    input.value = "";
  });

  // ---- YouTube sync (audit #45) -----------------------------------------------------
  // Additive only: remote likes / library songs / saved playlists are
  // merged into the local library; nothing local is deleted and nothing
  // is uploaded except likes tapped in this app.
  let syncRunning = false;
  async function runYouTubeSync(auto) {
    if (syncRunning) return;
    syncRunning = true;
    const statusEl = $("account-sync-status");
    if (statusEl) statusEl.textContent = "Syncing with YouTube…";
    try {
      const report = await api.syncYouTube();
      const parts = [];
      if (report.likedAdded > 0) parts.push(report.likedAdded + " liked");
      if (report.libraryAdded > 0) parts.push(report.libraryAdded + " library");
      if (report.playlistsAdded > 0) parts.push(report.playlistsAdded + " playlist(s)");
      const summary =
        parts.length > 0
          ? "Sync done — added " + parts.join(", ") + "."
          : "Sync done — everything was already here.";
      const errors = (report.errors || []).join(" ");
      if (statusEl) {
        statusEl.textContent = summary + (errors ? " " + errors : "");
      }
      if (!auto) player.setStatus(summary);
      await refreshLibraryData();
    } catch (err) {
      const message = err && err.message ? err.message : err;
      if (statusEl) statusEl.textContent = "Sync failed: " + message;
      if (!auto) player.setStatus("Sync failed: " + message);
    } finally {
      syncRunning = false;
    }
  }
  on("account-sync", "click", () => runYouTubeSync(false));

  // Clear downloads (audit #50): removes every downloaded file.
  on("settings-clear-downloads", "click", async () => {
    if (!window.confirm("Delete all downloaded songs? Your likes, playlists, and history stay.")) {
      return;
    }
    const removed = await api.clearDownloads();
    const statusEl = $("settings-storage-status");
    if (statusEl) {
      statusEl.textContent =
        removed == null
          ? "Couldn't clear downloads right now."
          : "Removed " + removed + " downloaded song(s).";
    }
    await refreshLibraryData();
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
      // NEVER "latest" on a failed check. The failure is persisted
      // (audit #51) so Settings still shows it on the next launch.
      if (statusEl) {
        statusEl.textContent =
          "Couldn't check for updates: " + info.error +
          " The last update check failed — check your connection and try again.";
      }
      if (settings) settings.lastUpdateCheckFailed = true;
      return;
    }
    if (settings) settings.lastUpdateCheckFailed = false;
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
  // Automatic, APK-style: the login window opens at the Google
  // ServiceLogin URL and Rust polls its WebView2 cookies (including the
  // HttpOnly SAPISID `document.cookie` can never see), validating the
  // moment SAPISID appears. There is no paste form: the cookie never
  // enters this page at all — it goes from the WebView2 cookie store
  // straight to the Rust validation path.
  const SIGNIN_POLL_MS = 2000;
  const SIGNIN_MAX_ERRORS = 5;
  // A plain Chrome UA so Google doesn't flag the embedded WebView2
  // view (the APK strips the `wv` token for the same reason). If this
  // Tauri version ignores the option, the default Edge WebView2 UA is
  // used instead — sign-in still proceeds.
  const SIGNIN_USER_AGENT =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";

  let signinPollTimer = null;
  let signinLoginWindow = null;

  function stopSigninPoll() {
    if (signinPollTimer !== null) {
      clearTimeout(signinPollTimer);
      signinPollTimer = null;
    }
    signinLoginWindow = null;
  }

  // Sign-in flow state for the Settings → Account surface
  // (#account-signin-status): waiting while the login window is open,
  // capturing while cookies validate, success / needs-channel /
  // cancelled / error at the end. The player toast still carries the
  // same message; this is the in-place state next to the account.
  function setSigninFlowState(state, message) {
    const el = $("account-signin-status");
    if (!el) return;
    el.dataset.state = state;
    if (message) el.textContent = message;
  }

  async function closeSigninWindow() {
    const win = signinLoginWindow;
    signinLoginWindow = null;
    if (win) {
      try {
        await win.close();
      } catch {
        /* Already closed by the user. */
      }
    }
  }

  async function pollSigninCapture(errorCount) {
    if (!signinLoginWindow) return;
    const result = await api.signinPollCapture();
    if (!signinLoginWindow) return; // stopped while awaiting
    if (!result || !result.status) {
      if (errorCount + 1 >= SIGNIN_MAX_ERRORS) {
        await closeSigninWindow();
        stopSigninPoll();
        setSigninFlowState("error", "Sign-in check failed — try signing in again.");
        player.setStatus("Sign-in check failed — try signing in again.");
        api.logDiagnostic(
          "signin",
          "sign-in check failed: no poll result after " +
            SIGNIN_MAX_ERRORS +
            " tries",
        );
        return;
      }
      signinPollTimer = setTimeout(
        () => pollSigninCapture(errorCount + 1),
        SIGNIN_POLL_MS,
      );
      return;
    }
    switch (result.status) {
      case "not-signed-in":
        signinPollTimer = setTimeout(() => pollSigninCapture(0), SIGNIN_POLL_MS);
        return;
      case "capture-error":
        // The Rust capture itself failed (COM / timeout) — the real
        // reason is in result.message. Keep polling a few times,
        // then stop with the reason instead of a generic line.
        if (errorCount + 1 >= SIGNIN_MAX_ERRORS) {
          await closeSigninWindow();
          stopSigninPoll();
          const reason = result.message || "couldn't read the login window";
          setSigninFlowState("error", "Sign-in check failed: " + reason);
          player.setStatus("Sign-in check failed: " + reason);
          api.logDiagnostic("signin", "sign-in check failed: " + reason);
          return;
        }
        signinPollTimer = setTimeout(
          () => pollSigninCapture(errorCount + 1),
          SIGNIN_POLL_MS,
        );
        return;
      case "window-gone":
        stopSigninPoll();
        setSigninFlowState("cancelled", "Sign-in cancelled — nothing changed.");
        player.setStatus("Sign-in cancelled.");
        api.logDiagnostic("signin", "login window closed before sign-in completed");
        return;
      case "signed-in":
        await closeSigninWindow();
        stopSigninPoll();
        await refreshSigninStatus();
        setSigninFlowState(
          "success",
          "Signed in as " + (result.accountName || "your account") + ".",
        );
        player.setStatus(
          "Signed in as " + (result.accountName || "your account") + ".",
        );
        api.logDiagnostic(
          "signin",
          "signed in as " + (result.accountName || "your account"),
        );
        // Auto-sync once after a fresh sign-in (audit #45): pull the
        // YouTube library into the local library, best-effort.
        await runYouTubeSync(true);
        return;
      case "needs-channel":
        await closeSigninWindow();
        stopSigninPoll();
        await refreshSigninStatus();
        views.setAccountChannels(result.channels || [], null);
        setSigninFlowState("needs-channel", "Signed in — pick a channel below to finish.");
        player.setStatus("Signed in — pick a channel below to finish.");
        return;
      case "validation-failed":
      default:
        await closeSigninWindow();
        stopSigninPoll();
        setSigninFlowState(
          "error",
          result.message || "That sign-in didn't work — try signing in again.",
        );
        player.setStatus(
          result.message || "That sign-in didn't work — try signing in again.",
        );
        api.logDiagnostic(
          "signin",
          "sign-in validation failed: " + (result.message || "unknown"),
        );
        return;
    }
  }

  on("account-signin", "click", async () => {
    if (signinLoginWindow) {
      try {
        await signinLoginWindow.setFocus();
      } catch {
        // The window is gone (user closed it): drop the stale
        // handle so we fall through and open a fresh window instead
        // of returning silently with nothing happening — the
        // "click does nothing, no message" failure.
        signinLoginWindow = null;
      }
      if (signinLoginWindow) return;
    }
    const started = await api.signinStart();
    if (!started || !started.authUrl) {
      setSigninFlowState("error", "Sign-in is not available right now.");
      player.setStatus("Sign-in is not available right now.");
      return;
    }
    let loginWindow = null;
    try {
      const tauriWindow =
        window.__TAURI__ && window.__TAURI__.window
          ? window.__TAURI__.window
          : null;
      if (tauriWindow && tauriWindow.WebviewWindow) {
        loginWindow = new tauriWindow.WebviewWindow("aello-signin", {
          url: started.authUrl,
          title: "Sign in to Aello",
          width: 480,
          height: 720,
          userAgent: SIGNIN_USER_AGENT,
        });
      }
    } catch {
      loginWindow = null;
    }
    if (!loginWindow) {
      setSigninFlowState("error", "Couldn't open the sign-in window — try again.");
      player.setStatus("Couldn't open the sign-in window — try again.");
      api.logDiagnostic("signin", "login window failed to open (no WebviewWindow)");
      return;
    }
    signinLoginWindow = loginWindow;
    api.logDiagnostic("signin", "login window opened");
    setSigninFlowState("waiting", "Sign in with Google in the login window…");
    player.setStatus("Sign in with Google in the login window…");
    signinPollTimer = setTimeout(() => pollSigninCapture(0), SIGNIN_POLL_MS);
  });

  on("account-signout", "click", async () => {
    const ok = await api.signOut();
    if (ok) {
      await closeSigninWindow();
      stopSigninPoll();
      views.setAccountChannels([], null);
      await refreshSigninStatus();
      setSigninFlowState("idle", "Sign in with Google to use your YouTube account in Aello.");
      player.setStatus("Signed out.");
    } else {
      setSigninFlowState("error", "Couldn't sign out right now.");
      player.setStatus("Couldn't sign out right now.");
    }
  });

  // Channel picker (Settings → Account): shown only when the account
  // has more than one channel. Picking one applies that channel's
  // identity to the session and re-validates the account name.
  const channelSelect = $("account-channel");
  if (channelSelect) {
    channelSelect.addEventListener("change", async () => {
      const id = channelSelect.value;
      if (!id) return;
      const result = await api.signinChooseChannel(id);
      if (result && result.accountName) {
        await refreshSigninStatus();
        setSigninFlowState("success", "Signed in as " + result.accountName + ".");
        player.setStatus("Signed in as " + result.accountName + ".");
      } else {
        setSigninFlowState("error", "Couldn't switch to that channel — try again.");
        player.setStatus("Couldn't switch to that channel — try again.");
        await refreshSigninStatus();
      }
    });
  }

  async function refreshSigninStatus() {
    const status = await api.signinStatus();
    views.setAccountState(
      !!status.signedIn,
      status.accountName || null,
      status.avatarUrl || null,
    );
    views.setAccountChannels(
      status.signedIn ? status.channels || [] : [],
      status.dataSyncId || null,
    );
    return status;
  }

  // Diagnostics (Settings → About): show the log file path and copy
  // its contents so the user can send the actual error when sign-in
  // or playback fails on their PC.
  async function refreshDiagnosticsPath() {
    const el = $("diagnostics-path");
    if (!el) return;
    const path = await api.diagnosticsPath();
    el.textContent =
      path || "Diagnostics log is not available in this build.";
  }
  on("diagnostics-copy", "click", async () => {
    const text = await api.readDiagnosticsLog();
    const body = text || "No diagnostics logged yet.";
    if (navigator.clipboard) {
      try {
        await navigator.clipboard.writeText(body);
        player.setStatus("Diagnostics copied — paste them in chat.");
        return;
      } catch {
        /* Fall through to the status fallback below. */
      }
    }
    player.setStatus(
      "Couldn't copy diagnostics — open the log file at the path above.",
    );
  });
  refreshDiagnosticsPath();

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
    // Blocked usernames persist locally (Android 6.1.45
    // ListenTogetherBlockedUsersKey). Auto-join rejects them.
    blocked: new Set(
      (() => {
        try {
          return JSON.parse(localStorage.getItem("aello.lt.blocked.v1") || "[]");
        } catch {
          return [];
        }
      })(),
    ),
    isBlocked(username) {
      return !!username && this.blocked.has(String(username).toLowerCase());
    },
    block(username) {
      if (!username) return;
      this.blocked.add(String(username).toLowerCase());
      try {
        localStorage.setItem(
          "aello.lt.blocked.v1",
          JSON.stringify([...this.blocked]),
        );
      } catch {
        /* Block list stays in memory for this session. */
      }
    },

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
          // Android 6.1.45 control-mode preference: the host's last
          // control mode is restored on the next room.
          if (settings && settings.ltControlMode === "everyone") {
            this.controlMode = "everyone";
            this.send("set_control_mode", { control_mode: "everyone" });
            views.showLtRoom(this.roomCode, true, "everyone");
          }
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
          // Android 6.1.45 auto-join: every join request is approved
          // immediately; no host approval needed. Blocked users are
          // still rejected (silently, with the reason). The pending
          // list is kept only for requests that arrive while this
          // client is not the host (it never approves those).
          if (this.isHost) {
            if (this.isBlocked(p.username)) {
              this.send("reject_join", {
                user_id: p.user_id,
                reason: "You are blocked",
              });
              views.setLtStatus(
                "Blocked user " + (p.username || "Guest") + " tried to join.",
              );
            } else {
              this.send("approve_join", { user_id: p.user_id });
              views.setLtStatus(
                (p.username || "Someone") + " joined the room.",
              );
            }
          } else {
            if (!this.pending.some((item) => item.user_id === p.user_id)) {
              this.pending.push({ user_id: p.user_id, username: p.username });
            }
            this.renderMembers();
            views.setLtStatus((p.username || "Someone") + " wants to join.");
          }
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
          // Persist the host's choice (Android 6.1.45
          // ListenTogetherControlModeKey).
          if (this.isHost && settings) {
            settings.ltControlMode = this.controlMode;
            saveSettingsQuietly();
          }
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
          views.setLtStatus(
            this.isHost
              ? "This room expires soon — extend it to keep listening."
              : "This room expires soon — ask the host to extend it.",
          );
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
        this.isHost,
      );
    },

    extendSession() {
      if (!this.joined || !this.isHost) return;
      this.send("extend_session", {});
      views.setLtStatus("Session extended.");
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
  on("lt-extend", "click", () => lt.extendSession());
  on("lt-control-mode", "change", (event) => {
    if (!lt.isHost) return;
    lt.send("set_control_mode", { control_mode: event.target.value });
    // Persist the host's choice so the next room restores it
    // (Android 6.1.45 ListenTogetherControlModeKey).
    if (settings) {
      settings.ltControlMode = event.target.value === "everyone" ? "everyone" : "owner";
      saveSettingsQuietly();
    }
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
        return;
      }
      // Host member controls (audit #42): kick, make host, block.
      const kick = event.target.closest("[data-lt-kick]");
      if (kick) {
        lt.send("kick_user", { user_id: kick.dataset.ltKick });
        views.setLtStatus("Removed that member.");
        return;
      }
      const transfer = event.target.closest("[data-lt-transfer]");
      if (transfer) {
        const name = transfer.dataset.ltTransferName || "that member";
        if (window.confirm("Make " + name + " the host of this room?")) {
          lt.send("transfer_host", { user_id: transfer.dataset.ltTransfer });
          views.setLtStatus("Host transfer requested.");
        }
        return;
      }
      const block = event.target.closest("[data-lt-block]");
      if (block) {
        lt.block(block.dataset.ltBlockName);
        lt.send("kick_user", { user_id: block.dataset.ltBlock });
        views.setLtStatus(
          "Blocked " + (block.dataset.ltBlockName || "that user") + " — they can no longer join.",
        );
        return;
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
    } else if (settings.lastUpdateCheckFailed) {
      const statusEl = $("settings-update-status");
      if (statusEl) {
        statusEl.textContent =
          "The last update check failed — check your connection and try again.";
      }
    }
    // Restore the lyrics style select + LT control-mode select from
    // their persisted values.
    const lyricsStyleSelect = $("lyrics-style");
    if (lyricsStyleSelect) {
      lyricsStyleSelect.value = views.getLyricsStyle();
    }
    const ltControlModeSelect = $("lt-control-mode");
    if (ltControlModeSelect) {
      ltControlModeSelect.value =
        settings.ltControlMode === "everyone" ? "everyone" : "owner";
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
  restoreLocalFiles();
  renderLibrarySegment();
  updateHomeGreeting();
  loadSettings().catch(() => {
    /* Defaults already applied in the markup; settings load is best-effort. */
  });
  loadHome();

  // Boot loader: the shell is ready — fade the circular-logo loader
  // out (CSS .done) and remove it once the fade finishes.
  const bootLoader = document.getElementById("boot-loader");
  if (bootLoader) {
    bootLoader.classList.add("done");
    setTimeout(() => bootLoader.remove(), 500);
  }

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
