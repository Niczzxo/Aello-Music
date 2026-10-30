"use strict";

/* Rendering for the Music, Library, Settings, and full-player views.
 * All text from the backend is escaped before it reaches the page.
 * Empty states are honest about which state they are: a genuine empty
 * result ("No results…") vs. the plain-browser preview, where the
 * desktop bridge is unavailable (see `AelloApi.outsideApp`). Transport
 * or backend failures never reach these renderers — the catch in
 * app.js shows the real error instead.
 *
 * Per-entity color (APK_LOOK.md / Android PlayerColorExtractor.kt):
 * dominant swatch first, then vibrant variants, vividness enhanced,
 * achromatic artwork falls back to #1C1C1E; text on a tinted page keeps
 * the hue and pushes lightness to a readable extreme (AppleTokens
 * onColor / onColorSecondary / onColorHeading). The page background is
 * ONE uniform color from the artwork — set as --entity-color on the
 * content area, no second color intruding. */

window.AelloViews = (() => {
  const AELLO_RED = "#FB2831";
  // 1px transparent GIF (same placeholder src as index.html) — fallback
  // artwork for <img> elements with nothing to show.
  const TRANSPARENT_PIXEL =
    "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
  const NEUTRAL_FALLBACK = "#1C1C1E";

  // Auto-playlist card colors, taken from the APK (APK_LOOK.md).
  // Rendered from data only — never invented for other lists.
  const AUTO_PLAYLIST_COLORS = {
    liked: "#7C3AED",
    downloaded: "#22C55E",
    top50: "#F43F5E",
    cached: "#3B82F6",
    uploaded: "#8B5CF6",
    local: "#F59E0B",
  };
  const AUTO_PLAYLIST_PLAY_GREEN = "#22C55E";

  function escapeHtml(value) {
    return String(value == null ? "" : value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function formatTime(seconds) {
    if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
    const total = Math.floor(seconds);
    const minutes = Math.floor(total / 60);
    const rest = total % 60;
    return minutes + ":" + String(rest).padStart(2, "0");
  }

  function artistNames(song) {
    if (!song || !Array.isArray(song.artists)) return "";
    return song.artists.map((artist) => artist.name).join(", ");
  }

  function artworkUrl(item) {
    if (!item || !Array.isArray(item.artwork) || item.artwork.length === 0) {
      return null;
    }
    const best = item.artwork[item.artwork.length - 1];
    return best && best.url ? best.url : null;
  }

  function cardHtml(item) {
    const url = artworkUrl(item);
    const art = url
      ? '<img src="' + escapeHtml(url) + '" alt="">'
      : escapeHtml((item.title || "?").slice(0, 1).toUpperCase());
    const sub =
      item.kind === "song"
        ? artistNames(item)
        : item.kind === "album"
          ? artistNames(item)
          : item.description || "";
    return (
      '<button type="button" class="card" data-kind="' +
      escapeHtml(item.kind) +
      '" data-id="' +
      escapeHtml(item.id) +
      '">' +
      '<span class="card-art">' +
      art +
      "</span>" +
      "<b>" +
      escapeHtml(item.title || "Untitled") +
      "</b>" +
      "<small>" +
      escapeHtml(sub) +
      "</small>" +
      "</button>"
    );
  }

  // Registry of rendered items, so a clicked card can be resolved back
  // to the full item (cards only carry id + kind in the markup).
  const registry = new Map();

  function registerSection(section) {
    if (!section || !Array.isArray(section.items)) return;
    const songs = section.items.filter((item) => item.kind === "song");
    section.items.forEach((item) => {
      registry.set(item.id, { item, queue: songs });
    });
  }

  function findItem(id) {
    return registry.get(id) || null;
  }

  // True when the page runs in a plain browser (preview): the Tauri
  // bridge — and with it the music backend — is unavailable.
  function isPreview() {
    return !!(window.AelloApi && window.AelloApi.outsideApp);
  }

  function sectionHtml(section) {
    const cards = (section.items || []).map(cardHtml).join("");
    return (
      '<div class="section-block"><h2>' +
      escapeHtml(section.title) +
      "</h2>" +
      (section.subtitle
        ? '<p class="section-sub">' + escapeHtml(section.subtitle) + "</p>"
        : "") +
      '<div class="card-row">' +
      cards +
      "</div></div>"
    );
  }

  function renderHome(sections, statusEl, dailySongs) {
    const host = document.getElementById("home-sections");
    // Prune the registry: ids from earlier renders must not resolve
    // via findItem after the list is replaced.
    registry.clear();
    const all = [];
    if (Array.isArray(dailySongs) && dailySongs.length > 0) {
      // Daily Discover is pinned first (Android: first section).
      const daily = {
        title: "Daily Discover",
        subtitle: "Made for you",
        items: dailySongs.map((song) => ({ ...song, kind: "song" })),
      };
      all.push(daily);
    }
    if (Array.isArray(sections)) all.push(...sections);
    if (all.length === 0) {
      if (statusEl) {
        statusEl.textContent = isPreview()
          ? "No music to show yet — the desktop bridge is not connected in this preview."
          : "No music to show yet.";
      }
      if (host) host.innerHTML = "";
      return;
    }
    if (statusEl) statusEl.textContent = "";
    all.forEach(registerSection);
    if (host) host.innerHTML = all.map(sectionHtml).join("");
  }

  function renderSearchResults(items, query, statusEl) {
    if (!items || items.length === 0) {
      const host = document.getElementById("home-sections");
      if (host) host.innerHTML = "";
      if (statusEl) {
        statusEl.textContent = isPreview()
          ? "No results — the desktop bridge is not connected in this preview."
          : 'No results for "' + (query || "") + '".';
      }
      return;
    }
    renderHome(
      [{ title: "Search results", subtitle: null, items }],
      statusEl,
    );
  }

  // ---- Search view (tabs + history) --------------------------------------
  let activeSearchTab = "all";
  let searchHistoryList = [];

  function normalizeSearchItem(item) {
    if (!item) return null;
    const kind = item.kind || "song";
    return {
      ...item,
      kind,
      title: item.title || item.name || "Untitled",
    };
  }

  function renderSearchView(items, query, filter, history) {
    const host = document.getElementById("search-results");
    const historyHost = document.getElementById("search-history");
    activeSearchTab = filter || "all";
    searchHistoryList = Array.isArray(history) ? history : [];
    document.querySelectorAll("[data-search-tab]").forEach((button) => {
      button.classList.toggle("active", button.dataset.searchTab === activeSearchTab);
      button.setAttribute(
        "aria-pressed",
        button.dataset.searchTab === activeSearchTab ? "true" : "false",
      );
    });
    if (historyHost) {
      historyHost.innerHTML =
        searchHistoryList.length === 0
          ? '<p class="empty">No recent searches yet.</p>'
          : '<ul class="history-list">' +
            searchHistoryList
              .map(
                (entry) =>
                  '<li><button type="button" class="history-item" data-history-query="' +
                  escapeHtml(entry) +
                  '">' +
                  escapeHtml(entry) +
                  "</button></li>",
              )
              .join("") +
            "</ul>";
    }
    if (!host) return;
    if (!items || items.length === 0) {
      host.innerHTML =
        '<p class="empty">' +
        (query
          ? 'No results for "' + escapeHtml(query) + '".'
          : "Search for songs, albums, artists, or playlists.") +
        "</p>";
      return;
    }
    const normalized = items.map(normalizeSearchItem).filter(Boolean);
    normalized.forEach((item) => {
      const songs = normalized.filter((entry) => entry.kind === "song");
      registry.set(item.id, { item, queue: songs });
    });
    host.innerHTML =
      '<div class="card-row">' +
      normalized.map(cardHtml).join("") +
      "</div>";
  }

  function getActiveSearchTab() {
    return activeSearchTab;
  }

  // ---- Moods & moments (from get_explore) ---------------------------------
  function renderMoods(moods) {
    const host = document.getElementById("search-moods");
    if (!host) return;
    if (!moods || moods.length === 0) {
      host.innerHTML = "";
      return;
    }
    host.innerHTML = moods
      .map(
        (mood) =>
          '<button type="button" class="mood-card" data-mood-params="' +
          escapeHtml(mood.params || "") +
          '" data-mood-title="' +
          escapeHtml(mood.title || "") +
          '">' +
          escapeHtml(mood.title || "Mood") +
          "</button>",
      )
      .join("");
  }

  // ---- Trending -------------------------------------------------------------
  function renderTrending(sections, statusEl) {
    const host = document.getElementById("trending-sections");
    const status = document.getElementById("trending-status") || statusEl;
    if (!sections || sections.length === 0) {
      if (status) {
        status.textContent = isPreview()
          ? "Trending needs the desktop bridge — not connected in this preview."
          : "No trending music right now.";
      }
      if (host) host.innerHTML = "";
      return;
    }
    if (status) status.textContent = "";
    sections.forEach(registerSection);
    if (host) host.innerHTML = sections.map(sectionHtml).join("");
  }

  // ---- Artist page ------------------------------------------------------------
  let artistSongs = [];

  function getArtistSongs() {
    return artistSongs.slice();
  }

  function renderArtist(detail, statusEl) {
    const view = document.getElementById("view-artist");
    if (!detail || !detail.artist) {
      if (statusEl) statusEl.textContent = "Couldn't open this artist.";
      return;
    }
    if (statusEl) statusEl.textContent = "";
    const artist = detail.artist;
    const art = document.getElementById("artist-art");
    const artUrl = artworkUrl(artist);
    if (art) {
      // #artist-art is an <img> (a void element) — innerHTML is a no-op
      // on it, so set src/alt directly.
      art.src = artUrl || TRANSPARENT_PIXEL;
      art.alt = artist.name || "Artist";
    }
    const nameEl = document.getElementById("artist-name");
    if (nameEl) nameEl.textContent = artist.name || "Artist";
    const metaEl = document.getElementById("artist-meta");
    if (metaEl) {
      const parts = [];
      if (detail.monthlyListeners) parts.push(detail.monthlyListeners + " monthly listeners");
      if (detail.subscriberCount) parts.push(detail.subscriberCount + " subscribers");
      if (detail.description) parts.push(detail.description);
      metaEl.textContent = parts.join(" · ");
    }
    artistSongs = Array.isArray(detail.topSongs) ? detail.topSongs : [];
    const songsHost = document.getElementById("artist-songs");
    if (songsHost) {
      songsHost.innerHTML =
        artistSongs.length === 0
          ? '<p class="empty">No top songs listed for this artist.</p>'
          : '<ul class="track-list">' +
            artistSongs
              .map(
                (song, index) =>
                  '<li><button type="button" class="track" data-artist-song-index="' +
                  index +
                  '">' +
                  escapeHtml(song.title || "Untitled") +
                  ' <span class="muted">— ' +
                  escapeHtml(artistNames(song) || "") +
                  "</span></button></li>",
              )
              .join("") +
            "</ul>";
    }
    const albumsHost = document.getElementById("artist-albums");
    if (albumsHost) {
      const albums = (detail.albums || []).concat(detail.singles || []);
      albumsHost.innerHTML =
        albums.length === 0
          ? ""
          : '<h3>Albums &amp; singles</h3><div class="card-row">' +
            albums
              .map((album) => cardHtml({ ...album, kind: "album" }))
              .join("") +
            "</div>";
      albums.forEach((album) => {
        registry.set(album.id, { item: { ...album, kind: "album" }, queue: [] });
      });
    }
    const relatedHost = document.getElementById("artist-related");
    if (relatedHost) {
      const related = Array.isArray(detail.related) ? detail.related : [];
      relatedHost.innerHTML =
        related.length === 0
          ? ""
          : '<h3>Related artists</h3><div class="card-row">' +
            related
              .map((rel) =>
                cardHtml({
                  ...rel,
                  kind: "artist",
                  title: rel.name || rel.title,
                }),
              )
              .join("") +
            "</div>";
      related.forEach((rel) => {
        registry.set(rel.id, {
          item: { ...rel, kind: "artist", title: rel.name || rel.title },
          queue: [],
        });
      });
    }
    if (view && artUrl) applyEntityColor(artUrl, view);
  }

  function renderDetail(detail, isAlbum, statusEl, onBack) {
    const songs = detail.songs;
    const info = isAlbum ? detail.album : detail.playlist;
    renderHome(
      [
        {
          title: info.title,
          subtitle: info.description || null,
          items: songs.map((song) => ({ ...song, kind: "song" })),
        },
      ],
      statusEl,
    );
    // renderHome replaces the home feed with this detail, so add a
    // Back button (same style as the artist page's) that restores it.
    const host = document.getElementById("home-sections");
    if (host) {
      host.insertAdjacentHTML(
        "afterbegin",
        '<button type="button" class="back-btn" id="detail-back">' +
          '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m12 19-7-7 7-7"/><path d="M19 12H5"/></svg>' +
          "<span>Back</span></button>",
      );
      const back = document.getElementById("detail-back");
      if (back && typeof onBack === "function") {
        back.addEventListener("click", onBack);
      }
    }
  }

  // ---- Library ---------------------------------------------------------------
  const LIBRARY_EMPTY = {
    playlists: "No playlists yet.",
    albums: "No saved albums yet.",
    artists: "No saved artists yet.",
    songs: "No songs yet. Use “Open audio files…” above to pick audio files from this computer.",
    downloads: "No downloads yet. Download a song from the player bar to listen offline.",
    liked: "No liked songs yet. Tap the heart on a song you like.",
    history: "No listening history yet. Songs you play for over 30 seconds show up here.",
  };

  function autoPlaylistCardHtml(key, title, countLabel, iconSvg) {
    // Uses the existing .auto-grid / .auto-card set from styles.css
    // (same structure as the static placeholder in index.html):
    // .auto-card.auto-<key> for the color, .auto-icon, .auto-play,
    // <b> title, <small> subtitle. No new classes.
    return (
      '<button type="button" class="auto-card auto-' +
      escapeHtml(key) +
      '" data-auto-playlist="' +
      escapeHtml(key) +
      '">' +
      '<span class="auto-icon" aria-hidden="true">' +
      iconSvg +
      "</span>" +
      '<span class="auto-play" aria-hidden="true"><svg viewBox="0 0 24 24" width="16" height="16" fill="#fff" aria-hidden="true"><path d="M8 5v14l11-7z"/></svg></span>' +
      "<b>" +
      escapeHtml(title) +
      "</b>" +
      "<small>" +
      escapeHtml(countLabel) +
      "</small>" +
      "</button>"
    );
  }

  function renderLibrary(segment, localSongs, extra) {
    const host = document.getElementById("library-list");
    if (!host) return;
    const data = extra || {};
    const library = data.library || {
      likedIds: [],
      playlists: [],
      history: [],
      speedDialIds: [],
    };
    const downloads = Array.isArray(data.downloads) ? data.downloads : [];
    const stats = Array.isArray(data.stats) ? data.stats : [];
    const songs = Array.isArray(localSongs) ? localSongs : [];

    if (segment === "playlists") {
      const likedCount = (library.likedIds || []).length;
      const userPlaylists = Array.isArray(library.playlists)
        ? library.playlists
        : [];
      host.innerHTML =
        '<div class="auto-grid">' +
        autoPlaylistCardHtml(
          "liked",
          "Liked",
          likedCount + (likedCount === 1 ? " song" : " songs"),
          '<svg viewBox="0 0 24 24" width="40" height="40" fill="none" stroke="#fff" stroke-width="2" aria-hidden="true"><path d="M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.6l-1-1a5.5 5.5 0 0 0-7.8 7.8l1 1L12 21.2l7.8-7.8 1-1a5.5 5.5 0 0 0 0-7.8z"/></svg>',
        ) +
        autoPlaylistCardHtml(
          "downloaded",
          "Downloaded",
          downloads.length + (downloads.length === 1 ? " song" : " songs"),
          '<svg viewBox="0 0 24 24" width="40" height="40" fill="none" stroke="#fff" stroke-width="2" aria-hidden="true"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="M7 10l5 5 5-5"/><path d="M12 15V3"/></svg>',
        ) +
        autoPlaylistCardHtml(
          "top50",
          "My top 50",
          stats.length > 0 ? "Your most played" : "No plays yet",
          '<svg viewBox="0 0 24 24" width="40" height="40" fill="none" stroke="#fff" stroke-width="2" aria-hidden="true"><path d="M3 3v18h18"/><path d="M7 15l4-6 4 3 5-8"/></svg>',
        ) +
        autoPlaylistCardHtml(
          "local",
          "Local",
          songs.length + (songs.length === 1 ? " file" : " files"),
          '<svg viewBox="0 0 24 24" width="40" height="40" fill="none" stroke="#fff" stroke-width="2" aria-hidden="true"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>',
        ) +
        "</div>" +
        '<div class="playlist-create"><input type="text" class="playlist-name-input" placeholder="New playlist name" aria-label="New playlist name"><button type="button" data-playlist-action="create">Create playlist</button></div>' +
        (userPlaylists.length === 0
          ? '<p class="empty">No custom playlists yet.</p>'
          : '<ul class="track-list">' +
            userPlaylists
              .map(
                (playlist) =>
                  '<li><span class="track">' +
                  escapeHtml(playlist.name || "Untitled playlist") +
                  ' <span class="muted">— ' +
                  (playlist.songIds || []).length +
                  " songs</span></span>" +
                  '<span class="queue-actions">' +
                  '<button type="button" data-playlist-action="rename" data-playlist-id="' +
                  escapeHtml(playlist.id) +
                  '" aria-label="Rename playlist">Rename</button>' +
                  '<button type="button" data-playlist-action="delete" data-playlist-id="' +
                  escapeHtml(playlist.id) +
                  '" aria-label="Delete playlist">Delete</button>' +
                  "</span></li>",
              )
              .join("") +
            "</ul>");
      return;
    }

    if (segment === "songs") {
      if (songs.length === 0) {
        host.innerHTML =
          '<p class="empty">No local songs yet. Use “Open audio files…” above to pick audio files from this computer. Local files are kept for this session only and are not saved to disk.</p>';
        return;
      }
      host.innerHTML =
        '<ul class="track-list">' +
        songs
          .map(
            (song, index) =>
              '<li><button type="button" class="track" data-local-index="' +
              index +
              '">' +
              escapeHtml(song.title || "Untitled") +
              ' <span class="muted">— Local file</span></button></li>',
          )
          .join("") +
        "</ul>" +
        '<p class="muted">Local files are kept for this session only and are not saved to disk.</p>';
      return;
    }

    if (segment === "downloads") {
      if (downloads.length === 0) {
        host.innerHTML = '<p class="empty">' + escapeHtml(LIBRARY_EMPTY.downloads) + "</p>";
        return;
      }
      host.innerHTML =
        '<ul class="track-list">' +
        downloads
          .map(
            (row) =>
              '<li><button type="button" class="track" data-download-id="' +
              escapeHtml(row.songId) +
              '">' +
              escapeHtml(row.title || "Untitled") +
              ' <span class="muted">— ' +
              escapeHtml(artistNames(row)) +
              (row.sizeBytes
                ? " · " + Math.round(row.sizeBytes / 1024) + " KB"
                : "") +
              "</span></button>" +
              '<span class="queue-actions"><button type="button" data-download-action="remove" data-download-id="' +
              escapeHtml(row.songId) +
              '" aria-label="Remove download">Remove</button></span></li>',
          )
          .join("") +
        "</ul>";
      return;
    }

    if (segment === "liked") {
      const likedIds = library.likedIds || [];
      const history = library.history || [];
      const matched = history.filter((entry) => likedIds.includes(entry.songId));
      if (likedIds.length === 0) {
        host.innerHTML = '<p class="empty">' + escapeHtml(LIBRARY_EMPTY.liked) + "</p>";
        return;
      }
      host.innerHTML =
        '<ul class="track-list">' +
        matched
          .map(
            (entry) =>
              '<li><button type="button" class="track" data-history-song-id="' +
              escapeHtml(entry.songId) +
              '">' +
              escapeHtml(entry.title || "Untitled") +
              ' <span class="muted">— ' +
              escapeHtml(artistNames(entry)) +
              "</span></button></li>",
          )
          .join("") +
        "</ul>" +
        (matched.length < likedIds.length
          ? '<p class="muted">' +
            (likedIds.length - matched.length) +
            " more liked song(s) — details load when played again.</p>"
          : "");
      return;
    }

    if (segment === "history") {
      const history = library.history || [];
      let html = "";
      if (history.length === 0) {
        html = '<p class="empty">' + escapeHtml(LIBRARY_EMPTY.history) + "</p>";
      } else {
        html =
          '<ul class="track-list">' +
          history
            .map(
              (entry) =>
                '<li><button type="button" class="track" data-history-song-id="' +
                escapeHtml(entry.songId) +
                '">' +
                escapeHtml(entry.title || "Untitled") +
                ' <span class="muted">— ' +
                escapeHtml(artistNames(entry)) +
                (entry.playCount > 1 ? " · played " + entry.playCount + "×" : "") +
                "</span></button></li>",
            )
            .join("") +
          "</ul>" +
          '<button type="button" data-history-action="clear">Clear history</button>';
      }
      if (stats.length > 0) {
        html +=
          '<h3>Most played</h3><ol class="stats-list">' +
          stats
            .slice(0, 10)
            .map(
              (entry) =>
                "<li>" +
                escapeHtml(entry.title || "Untitled") +
                ' <span class="muted">— ' +
                escapeHtml(artistNames(entry)) +
                " · " +
                (entry.playCount || 1) +
                " plays</span></li>",
            )
            .join("") +
          "</ol>";
      }
      host.innerHTML = html;
      return;
    }

    if (segment === "albums" || segment === "artists") {
      // No saved-albums/artists store exists in this pass; derive what
      // we honestly can from history, otherwise say so.
      const history = library.history || [];
      if (segment === "artists") {
        const names = [];
        history.forEach((entry) => {
          (entry.artists || []).forEach((artist) => {
            if (artist && artist.name && !names.includes(artist.name)) {
              names.push(artist.name);
            }
          });
        });
        host.innerHTML =
          names.length === 0
            ? '<p class="empty">' + escapeHtml(LIBRARY_EMPTY.artists) + "</p>"
            : '<ul class="track-list">' +
              names
                .map(
                  (name) =>
                    '<li><button type="button" class="track" data-artist-name="' +
                    escapeHtml(name) +
                    '">' +
                    escapeHtml(name) +
                    "</button></li>",
                )
                .join("") +
              "</ul>";
        return;
      }
      host.innerHTML =
        '<p class="empty">' + escapeHtml(LIBRARY_EMPTY.albums) + "</p>";
      return;
    }

    host.innerHTML =
      '<p class="empty">' + escapeHtml(LIBRARY_EMPTY[segment] || "Nothing here yet.") + "</p>";
  }

  // ---- Settings ---------------------------------------------------------------
  function applyTheme(settings) {
    const root = document.documentElement;
    root.dataset.accent = settings.accentTheme || "aello-red";
    root.dataset.appearance = settings.appearance || "system";
  }

  function setValue(id, value) {
    const el = document.getElementById(id);
    if (el && typeof value !== "undefined") el.value = value;
  }

  function setChecked(id, value) {
    const el = document.getElementById(id);
    if (el) el.checked = !!value;
  }

  function fillSettingsForm(settings) {
    const form = document.getElementById("settings-form");
    if (!form) return;
    const accent = form.querySelector(
      'input[name="accent"][value="' + settings.accentTheme + '"]',
    );
    if (accent) accent.checked = true;
    setValue("setting-appearance", settings.appearance || "system");
    setValue("setting-volume", settings.volume);
    setChecked("setting-updates", settings.checkForUpdatesOnStartup !== false);
    setValue("setting-quality", settings.audioQuality || "auto");
    setChecked("setting-crossfade", settings.crossfadeEnabled !== false);
    setValue(
      "setting-crossfade-duration",
      typeof settings.crossfadeSecs === "number" ? settings.crossfadeSecs : 5,
    );
    setChecked("setting-persistent-queue", settings.persistentQueue !== false);
    setChecked("setting-auto-download-like", !!settings.autoDownloadOnLike);
    // A stored custom URL is neither "global" nor "usa": show the
    // select as "custom" and unhide the custom input, so saving
    // Settings can no longer silently wipe it to "".
    const storedLtServer = settings.ltServer || "global";
    const isCustomLtServer =
      storedLtServer !== "global" && storedLtServer !== "usa";
    setValue(
      "setting-listen-together-server",
      isCustomLtServer ? "custom" : storedLtServer,
    );
    const customServer = document.getElementById("setting-lt-custom-server");
    if (customServer) {
      customServer.value = isCustomLtServer ? storedLtServer : "";
      customServer.hidden = !isCustomLtServer;
    }
  }

  function readSettingsForm(current) {
    const base = current || {};
    const form = document.getElementById("settings-form");
    const accent = form
      ? form.querySelector('input[name="accent"]:checked')
      : null;
    const readValue = (id, fallback) => {
      const el = document.getElementById(id);
      return el ? el.value : fallback;
    };
    const readChecked = (id, fallback) => {
      const el = document.getElementById(id);
      return el ? el.checked : fallback;
    };
    let ltServer = readValue("setting-listen-together-server", base.ltServer || "global");
    if (ltServer === "custom") {
      const custom = readValue("setting-lt-custom-server", "");
      // Keep the stored custom URL when the field is left blank, so a
      // save never wipes a custom server the user did not touch.
      ltServer = custom.trim() || base.ltServer || "global";
    } else if (
      ltServer !== "global" &&
      ltServer !== "usa" &&
      ltServer.trim() !== ""
    ) {
      // A raw URL stored directly (older saves) is a custom server.
      ltServer = ltServer.trim();
    }
    return {
      accentTheme: accent ? accent.value : base.accentTheme || "aello-red",
      appearance: readValue("setting-appearance", base.appearance || "system"),
      volume: parseFloat(readValue("setting-volume", base.volume ?? 0.8)),
      checkForUpdatesOnStartup: readChecked(
        "setting-updates",
        base.checkForUpdatesOnStartup !== false,
      ),
      audioQuality: readValue("setting-quality", base.audioQuality || "auto"),
      crossfadeEnabled: readChecked(
        "setting-crossfade",
        base.crossfadeEnabled !== false,
      ),
      crossfadeSecs: parseFloat(
        readValue("setting-crossfade-duration", base.crossfadeSecs ?? 5),
      ),
      soundEnhanceEnabled:
        typeof base.soundEnhanceEnabled === "boolean"
          ? base.soundEnhanceEnabled
          : true,
      activeEqProfileId: base.activeEqProfileId ?? null,
      eqProfiles: Array.isArray(base.eqProfiles) ? base.eqProfiles : [],
      persistentQueue: readChecked(
        "setting-persistent-queue",
        base.persistentQueue !== false,
      ),
      autoDownloadOnLike: readChecked(
        "setting-auto-download-like",
        !!base.autoDownloadOnLike,
      ),
      ltServer,
      ltUsername: base.ltUsername || "",
    };
  }

  function setAccountState(signedIn, accountName) {
    const el = document.getElementById("account-state");
    if (el) {
      el.textContent = signedIn
        ? "Signed in as " + (accountName || "your account") + "."
        : "Not signed in — guest mode.";
    }
    const signin = document.getElementById("account-signin");
    const signout = document.getElementById("account-signout");
    if (signin) signin.hidden = !!signedIn;
    if (signout) signout.hidden = !signedIn;
  }

  // ---- Lyrics (Android LyricsUtils / Lyrics.kt §7) ------------------------------
  // Rust returns parsed lines with [offset:] already applied to every
  // timestamp. parseLrc below is kept for raw LRC text (plain-text
  // fallback) and applies the same rule in JS.
  const LRC_LINE_REGEX = /((?:\[\d\d:\d\d\.\d{2,3}\] ?)+)(.+)/;
  const LRC_TIME_REGEX = /\[(\d\d):(\d\d)\.(\d{2,3})\]/g;
  const LRC_OFFSET_REGEX = /\[offset:([+-]?\d+)\]/;

  function parseLrc(rawText) {
    const lines = [];
    if (typeof rawText !== "string" || rawText.length === 0) return lines;
    let offsetMs = 0;
    const offsetMatch = rawText.match(LRC_OFFSET_REGEX);
    if (offsetMatch) offsetMs = parseInt(offsetMatch[1], 10) || 0;
    rawText.split(/\r?\n/).forEach((rawLine) => {
      const line = rawLine.trim();
      if (!line || LRC_OFFSET_REGEX.test(line)) return;
      const match = line.match(LRC_LINE_REGEX);
      if (!match) return; // Unsynced plain text is not parsed.
      const text = match[2].trim();
      if (!text) return;
      const times = [...match[1].matchAll(LRC_TIME_REGEX)];
      times.forEach((timeMatch) => {
        const minutes = parseInt(timeMatch[1], 10);
        const seconds = parseInt(timeMatch[2], 10);
        let ms = parseInt(timeMatch[3], 10);
        if (timeMatch[3].length === 2) ms *= 10;
        const timeMs = Math.max(
          0,
          minutes * 60000 + seconds * 1000 + ms + offsetMs,
        );
        lines.push({ timeMs, text });
      });
    });
    lines.sort((a, b) => a.timeMs - b.timeMs);
    return lines;
  }

  // Binary search: first line with time >= position + 300ms; the
  // current line is the one before it. -1 before the first line.
  function findCurrentLineIndex(lines, positionMs) {
    if (!lines || lines.length === 0) return -1;
    const target = positionMs + 300;
    let low = 0;
    let high = lines.length;
    while (low < high) {
      const mid = (low + high) >> 1;
      if (lines[mid].timeMs < target) low = mid + 1;
      else high = mid;
    }
    return low - 1;
  }

  // The provider header ("Lyrics from <provider>") is row 0 of the list
  // when shown, so every line's list index is its line index + 1
  // (Android listIndexForLine fix — without it every scroll/highlight
  // is one row off).
  let lyricsLines = [];
  let lyricsProvider = null;
  let lyricsSynced = false;
  let lyricsSongId = null;
  let lyricsOffsetMs = 0;
  let lyricsAutoScroll = true;
  let lyricsCurrentIndex = -1;
  const LYRICS_OFFSET_KEY = "aello.lyricsOffset.v1";
  let lyricsOffsets = {};
  try {
    lyricsOffsets = JSON.parse(localStorage.getItem(LYRICS_OFFSET_KEY) || "{}") || {};
  } catch {
    lyricsOffsets = {};
  }

  function getLyricsOffset(songId) {
    return typeof lyricsOffsets[songId] === "number" ? lyricsOffsets[songId] : 0;
  }

  function setLyricsOffset(songId, ms) {
    lyricsOffsets[songId] = ms;
    try {
      localStorage.setItem(LYRICS_OFFSET_KEY, JSON.stringify(lyricsOffsets));
    } catch {
      /* Offset stays in memory for this session. */
    }
  }

  function renderLyrics(result, song) {
    const linesHost = document.getElementById("lyrics-lines");
    const providerEl = document.getElementById("lyrics-provider");
    const statusEl = document.getElementById("lyrics-status");
    const offsetEl = document.getElementById("lyrics-offset");
    const offsetLabel = document.getElementById("lyrics-offset-label");
    lyricsSongId = song ? song.id : null;
    lyricsOffsetMs = lyricsSongId ? getLyricsOffset(lyricsSongId) : 0;
    lyricsAutoScroll = true;
    lyricsCurrentIndex = -1;
    if (offsetEl) offsetEl.value = lyricsOffsetMs;
    if (offsetLabel) {
      offsetLabel.textContent =
        (lyricsOffsetMs >= 0 ? "+" : "") + lyricsOffsetMs + " ms";
    }
    if (!result || !Array.isArray(result.lines) || result.lines.length === 0) {
      lyricsLines = [];
      lyricsProvider = null;
      lyricsSynced = false;
      if (providerEl) providerEl.textContent = "";
      if (linesHost) linesHost.innerHTML = "";
      if (statusEl) {
        statusEl.textContent = result
          ? "No lyrics found for this song."
          : "Lyrics are not available right now.";
      }
      return;
    }
    lyricsProvider = result.provider || null;
    lyricsSynced = !!result.synced;
    lyricsLines = result.lines
      .filter((line) => line && typeof line.text === "string")
      .map((line) => ({ timeMs: line.timeMs, text: line.text }));
    attachLyricsScrollListener();
    if (providerEl) {
      providerEl.textContent =
        lyricsProvider && lyricsProvider !== "Unknown"
          ? "Lyrics from " + lyricsProvider
          : "";
    }
    if (statusEl) statusEl.textContent = "";
    if (!linesHost) return;
    const headerShown = !!(lyricsProvider && lyricsProvider !== "Unknown");
    let html = "";
    if (headerShown) {
      html +=
        '<p class="lyrics-provider-header">Lyrics from ' +
        escapeHtml(lyricsProvider) +
        "</p>";
    }
    if (!lyricsSynced) {
      html +=
        '<div class="lyrics-plain">' +
        lyricsLines
          .map((line) => "<p>" + escapeHtml(line.text) + "</p>")
          .join("") +
        "</div>";
    } else {
      html += lyricsLines
        .map(
          (line, index) =>
            '<button type="button" class="lyric-line" data-line-index="' +
            index +
            '" data-time-ms="' +
            line.timeMs +
            '">' +
            escapeHtml(line.text) +
            "</button>",
        )
        .join("");
    }
    linesHost.innerHTML = html;
    // Initial scroll centers line 0 (only when synced lines exist).
    if (lyricsSynced && lyricsLines.length > 0) {
      requestAnimationFrame(() => scrollToLyricLine(0, true));
    }
  }

  function listIndexForLine(lineIndex) {
    const headerShown = !!(lyricsProvider && lyricsProvider !== "Unknown");
    return headerShown ? lineIndex + 1 : lineIndex;
  }

  function scrollToLyricLine(lineIndex, instant) {
    const host = document.getElementById("lyrics-lines");
    if (!host) return;
    const child = host.children[listIndexForLine(lineIndex)];
    if (!child) return;
    // Programmatic centering fires scroll events too — suppress the
    // manual-scroll detector briefly so it does not switch auto-scroll
    // off the moment a line change centers itself.
    suppressScrollUntil = Date.now() + 800;
    child.scrollIntoView({
      behavior: instant ? "auto" : "smooth",
      block: "center",
    });
  }

  // Any manual scroll/drag disables auto-scroll (Android Lyrics.kt);
  // the "scroll to current" button re-enables it. The listener is
  // attached once, the first time lyrics render.
  let suppressScrollUntil = 0;
  let lyricsScrollListenerAttached = false;

  function attachLyricsScrollListener() {
    if (lyricsScrollListenerAttached) return;
    const host = document.getElementById("lyrics-lines");
    if (!host) return;
    lyricsScrollListenerAttached = true;
    host.addEventListener("scroll", () => {
      if (Date.now() < suppressScrollUntil) return;
      if (lyricsAutoScroll) lyricsAutoScroll = false;
    });
  }

  // Called every 100ms from app.js while the lyrics panel is visible.
  // positionMs already includes the per-song user offset (Android:
  // position + lyricsOffset).
  function updateLyricsHighlight(audio) {
    if (!lyricsSynced || lyricsLines.length === 0 || !audio) return;
    const positionMs =
      (Number.isFinite(audio.currentTime) ? audio.currentTime : 0) * 1000 +
      lyricsOffsetMs;
    const index = findCurrentLineIndex(lyricsLines, positionMs);
    if (index === lyricsCurrentIndex) return;
    lyricsCurrentIndex = index;
    const host = document.getElementById("lyrics-lines");
    if (!host) return;
    host.querySelectorAll(".lyric-line").forEach((el) => {
      const isCurrent = Number(el.dataset.lineIndex) === index;
      el.classList.toggle("current", isCurrent);
      el.setAttribute("aria-current", isCurrent ? "true" : "false");
    });
    if (lyricsAutoScroll && index >= 0) scrollToLyricLine(index, false);
  }

  function setLyricsAutoScroll(enabled) {
    lyricsAutoScroll = !!enabled;
  }

  function isLyricsAutoScroll() {
    return lyricsAutoScroll;
  }

  function scrollLyricsToCurrent() {
    lyricsAutoScroll = true;
    if (lyricsCurrentIndex >= 0) scrollToLyricLine(lyricsCurrentIndex, false);
  }

  function getLyricsOffsetMs() {
    return lyricsOffsetMs;
  }

  function setLyricsOffsetMs(ms) {
    lyricsOffsetMs = ms;
    if (lyricsSongId) setLyricsOffset(lyricsSongId, ms);
    const label = document.getElementById("lyrics-offset-label");
    if (label) {
      label.textContent = (ms >= 0 ? "+" : "") + ms + " ms";
    }
  }

  function getLyricsLineTime(index) {
    const line = lyricsLines[index];
    return line ? line.timeMs : null;
  }

  // ---- EQ panel ---------------------------------------------------------------
  const EQ_PRESETS = [
    { name: "Flat", bands: [], preampDb: 0 },
    {
      name: "Bass Boost",
      bands: [
        { freqHz: 60, gainDb: 5.0, q: 0.7, kind: "lowshelf" },
        { freqHz: 250, gainDb: 2.0, q: 1.0, kind: "peaking" },
      ],
      preampDb: -3.0,
    },
    {
      name: "Treble Boost",
      bands: [
        { freqHz: 3000, gainDb: 3.0, q: 1.0, kind: "peaking" },
        { freqHz: 10000, gainDb: 4.0, q: 0.7, kind: "highshelf" },
      ],
      preampDb: -2.0,
    },
    {
      name: "Vocal Boost",
      bands: [
        { freqHz: 300, gainDb: -2.0, q: 1.0, kind: "peaking" },
        { freqHz: 1000, gainDb: 3.0, q: 1.0, kind: "peaking" },
        { freqHz: 4000, gainDb: 2.0, q: 1.0, kind: "peaking" },
      ],
      preampDb: -1.0,
    },
    {
      name: "Sound Enhance",
      bands: [
        { freqHz: 80, gainDb: 3.0, q: 0.7, kind: "lowshelf" },
        { freqHz: 300, gainDb: 1.0, q: 1.0, kind: "peaking" },
        { freqHz: 3000, gainDb: 2.0, q: 1.0, kind: "peaking" },
        { freqHz: 8000, gainDb: 1.5, q: 0.7, kind: "highshelf" },
      ],
      preampDb: -3.0,
    },
  ];

  // Q values of the bands last rendered by renderEqPanel /
  // applyEqPreset. The panel has no Q input, so readEqPanel preserves
  // these instead of hard-coding q: 1 (which would silently flatten
  // the Sound Enhance shelves' 0.7 Q when a curve is saved).
  let lastEqQs = [1, 1, 1, 1, 1];

  // The EFFECTIVE curve — the one the player graph actually applies
  // (live panel curve > custom profile > Sound Enhance > flat). The
  // player owns that precedence; read it from there so the panel never
  // shows flat bands + preamp 0 while Enhance is actually running.
  function effectiveEqCurve(settings) {
    const playerApi = window.AelloPlayer;
    if (playerApi && typeof playerApi.activeEqCurve === "function") {
      try {
        return playerApi.activeEqCurve();
      } catch {
        /* Fall through to the settings-only curve below. */
      }
    }
    const active = (settings.eqProfiles || []).find(
      (p) => p && p.id === settings.activeEqProfileId,
    );
    if (active) {
      return {
        bands: Array.isArray(active.bands) ? active.bands : [],
        preampDb: typeof active.preampDb === "number" ? active.preampDb : 0,
      };
    }
    if (settings.soundEnhanceEnabled !== false) {
      const enhance = EQ_PRESETS.find((p) => p.name === "Sound Enhance");
      return {
        bands: enhance ? enhance.bands : [],
        preampDb: enhance ? enhance.preampDb : 0,
      };
    }
    return { bands: [], preampDb: 0 };
  }

  // Shows the "Live (unsaved)" note while a live (unsaved) panel curve
  // is active in the player; hidden once a profile is saved/applied.
  function updateEqLiveNote() {
    const note = document.getElementById("eq-live-note");
    if (!note) return;
    const playerApi = window.AelloPlayer;
    const live =
      playerApi &&
      typeof playerApi.isLiveEqCurve === "function" &&
      playerApi.isLiveEqCurve();
    note.hidden = !live;
  }

  function renderEqPanel(settings) {
    const enhanceEl = document.getElementById("eq-enhance");
    if (enhanceEl) {
      enhanceEl.checked = settings.soundEnhanceEnabled !== false;
    }
    const presetsEl = document.getElementById("eq-presets");
    if (presetsEl) {
      const custom = Array.isArray(settings.eqProfiles)
        ? settings.eqProfiles
        : [];
      presetsEl.innerHTML =
        EQ_PRESETS.map(
          (preset) =>
            '<option value="builtin:' +
            escapeHtml(preset.name) +
            '">' +
            escapeHtml(preset.name) +
            "</option>",
        ).join("") +
        custom
          .map(
            (profile) =>
              '<option value="custom:' +
              escapeHtml(profile.id) +
              '"' +
              (settings.activeEqProfileId === profile.id ? " selected" : "") +
              ">" +
              escapeHtml(profile.name) +
              "</option>",
          )
          .join("");
    }
    const preampEl = document.getElementById("eq-preamp");
    if (preampEl) {
      preampEl.value = effectiveEqCurve(settings).preampDb;
    }
    const bandsHost = document.getElementById("eq-bands");
    if (bandsHost) {
      const bands = effectiveEqCurve(settings).bands;
      lastEqQs = [0, 1, 2, 3, 4].map((i) =>
        bands[i] && typeof bands[i].q === "number" ? bands[i].q : 1,
      );
      let html = "";
      for (let i = 0; i < 5; i++) {
        const band = bands[i] || {
          freqHz: 1000,
          gainDb: 0,
          q: 1,
          kind: "peaking",
        };
        html +=
          '<div class="eq-band-row"><label>Band ' +
          (i + 1) +
          '</label>' +
          '<input type="number" class="eq-freq" data-band="' +
          i +
          '" value="' +
          band.freqHz +
          '" min="20" max="20000" step="10" aria-label="Band ' +
          (i + 1) +
          ' frequency Hz">' +
          '<input type="range" class="eq-gain" data-band="' +
          i +
          '" value="' +
          band.gainDb +
          '" min="-30" max="30" step="0.5" aria-label="Band ' +
          (i + 1) +
          ' gain dB">' +
          '<select class="eq-type" data-band="' +
          i +
          '" aria-label="Band ' +
          (i + 1) +
          ' type">' +
          ["lowshelf", "peaking", "highshelf"]
            .map(
              (kind) =>
                '<option value="' +
                kind +
                '"' +
                (band.kind === kind ? " selected" : "") +
                ">" +
                kind +
                "</option>",
            )
            .join("") +
          "</select></div>";
      }
      bandsHost.innerHTML = html;
    }
    updateEqLiveNote();
  }

  function readEqPanel() {
    const bands = [];
    document.querySelectorAll("#eq-bands .eq-band-row").forEach((row, i) => {
      const freq = row.querySelector(".eq-freq");
      const gain = row.querySelector(".eq-gain");
      const type = row.querySelector(".eq-type");
      bands.push({
        freqHz: freq ? parseFloat(freq.value) || 1000 : 1000,
        gainDb: gain ? parseFloat(gain.value) || 0 : 0,
        // Preserve the Q of the rendered curve (e.g. Sound Enhance's
        // 0.7-Q shelves) — the panel has no Q input.
        q: typeof lastEqQs[i] === "number" ? lastEqQs[i] : 1,
        kind: type ? type.value : "peaking",
      });
    });
    const preampEl = document.getElementById("eq-preamp");
    return {
      bands,
      preampDb: preampEl ? parseFloat(preampEl.value) || 0 : 0,
      enhance:
        document.getElementById("eq-enhance") &&
        document.getElementById("eq-enhance").checked,
    };
  }

  function applyEqPreset(name) {
    const preset = EQ_PRESETS.find((p) => p.name === name);
    if (!preset) return false;
    const bandsHost = document.getElementById("eq-bands");
    if (!bandsHost) return false;
    for (let i = 0; i < 5; i++) {
      const band = preset.bands[i];
      const freq = bandsHost.querySelector('.eq-freq[data-band="' + i + '"]');
      const gain = bandsHost.querySelector('.eq-gain[data-band="' + i + '"]');
      const type = bandsHost.querySelector('.eq-type[data-band="' + i + '"]');
      if (freq) freq.value = band ? band.freqHz : 1000;
      if (gain) gain.value = band ? band.gainDb : 0;
      if (type) type.value = band ? band.kind : "peaking";
      lastEqQs[i] = band && typeof band.q === "number" ? band.q : 1;
    }
    const preampEl = document.getElementById("eq-preamp");
    if (preampEl) preampEl.value = preset.preampDb;
    return true;
  }

  // ---- Update dialog ------------------------------------------------------------
  function showUpdateDialog(info) {
    const dialog = document.getElementById("update-dialog");
    if (!dialog) return;
    const title = document.getElementById("update-title");
    if (title) title.textContent = "Update available";
    const version = document.getElementById("update-version");
    if (version) version.textContent = info.version || "";
    const notes = document.getElementById("update-notes");
    if (notes) notes.textContent = info.notes || "";
    const download = document.getElementById("update-download");
    if (download) {
      if (info.exeUrl) {
        download.hidden = false;
        download.dataset.url = info.exeUrl;
      } else {
        download.hidden = true;
      }
    }
    const status = document.getElementById("update-status");
    if (status) status.textContent = "";
    // A <dialog> needs showModal() to appear — flipping `hidden` alone
    // never shows it. Clear `hidden` first (a hidden dialog stays
    // hidden even with showModal in some browsers).
    dialog.hidden = false;
    if (!dialog.open) {
      try {
        dialog.showModal();
      } catch {
        /* Already open, or no modal support — hidden=false shows it. */
      }
    }
  }

  function hideUpdateDialog() {
    const dialog = document.getElementById("update-dialog");
    if (!dialog) return;
    if (dialog.open) dialog.close();
    dialog.hidden = true;
  }

  // ---- Sleep dialog ---------------------------------------------------------------
  function updateSleepStatus(text) {
    const status = document.getElementById("sleep-status");
    if (status) status.textContent = text || "";
    const button = document.getElementById("pb-sleep");
    if (button) button.title = text || "Sleep timer";
    const fullButton = document.getElementById("fp-sleep");
    if (fullButton) fullButton.title = text || "Sleep timer";
  }

  // ---- Listen Together view ---------------------------------------------------------
  function setLtStatus(text) {
    const el = document.getElementById("lt-status");
    if (el) el.textContent = text || "";
  }

  function showLtRoom(code, isHost, controlMode) {
    const room = document.getElementById("lt-room");
    if (room) room.hidden = false;
    const codeEl = document.getElementById("lt-room-code");
    if (codeEl) codeEl.textContent = code || "";
    const modeEl = document.getElementById("lt-control-mode");
    if (modeEl) {
      modeEl.value = controlMode || "owner";
      modeEl.disabled = !isHost;
    }
  }

  function hideLtRoom() {
    const room = document.getElementById("lt-room");
    if (room) room.hidden = true;
    const log = document.getElementById("lt-chat-log");
    if (log) log.innerHTML = "";
    const members = document.getElementById("lt-members");
    if (members) members.innerHTML = "";
  }

  function renderLtMembers(users, hostId, myUserId, pending) {
    const host = document.getElementById("lt-members");
    if (!host) return;
    let html = "";
    (pending || []).forEach((request) => {
      html +=
        '<li class="lt-pending"><span>' +
        escapeHtml(request.username || "Guest") +
        " wants to join</span>" +
        '<span class="queue-actions">' +
        '<button type="button" data-lt-approve="' +
        escapeHtml(request.user_id) +
        '">Approve</button>' +
        '<button type="button" data-lt-reject="' +
        escapeHtml(request.user_id) +
        '">Reject</button>' +
        "</span></li>";
    });
    (users || []).forEach((user) => {
      html +=
        "<li>" +
        escapeHtml(user.username || "Guest") +
        (user.user_id === hostId ? ' <span class="muted">(host)</span>' : "") +
        (user.user_id === myUserId ? ' <span class="muted">(you)</span>' : "") +
        (user.is_connected === false ? ' <span class="muted">(away)</span>' : "") +
        "</li>";
    });
    host.innerHTML = html || '<li class="empty">No members yet.</li>';
  }

  function appendLtChat(username, text) {
    const log = document.getElementById("lt-chat-log");
    if (!log) return;
    const line = document.createElement("p");
    line.innerHTML =
      "<b>" + escapeHtml(username || "Guest") + ":</b> " + escapeHtml(text);
    log.appendChild(line);
    log.scrollTop = log.scrollHeight;
  }

  // ---- Per-entity color (Android PlayerColorExtractor + AppleTokens) ------------------
  function rgbToHsv(r, g, b) {
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    const delta = max - min;
    let h = 0;
    if (delta !== 0) {
      if (max === r) h = ((g - b) / delta) % 6;
      else if (max === g) h = (b - r) / delta + 2;
      else h = (r - g) / delta + 4;
      h *= 60;
      if (h < 0) h += 360;
    }
    const s = max === 0 ? 0 : delta / max;
    return { h, s, v: max / 255 };
  }

  function hsvToRgb(h, s, v) {
    const c = v * s;
    const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
    const m = v - c;
    let r = 0;
    let g = 0;
    let b = 0;
    if (h < 60) [r, g, b] = [c, x, 0];
    else if (h < 120) [r, g, b] = [x, c, 0];
    else if (h < 180) [r, g, b] = [0, c, x];
    else if (h < 240) [r, g, b] = [0, x, c];
    else if (h < 300) [r, g, b] = [x, 0, c];
    else [r, g, b] = [c, 0, x];
    return {
      r: Math.round((r + m) * 255),
      g: Math.round((g + m) * 255),
      b: Math.round((b + m) * 255),
    };
  }

  function rgbToHsl(r, g, b) {
    const rn = r / 255;
    const gn = g / 255;
    const bn = b / 255;
    const max = Math.max(rn, gn, bn);
    const min = Math.min(rn, gn, bn);
    const l = (max + min) / 2;
    if (max === min) return { h: 0, s: 0, l };
    const d = max - min;
    const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    let h = 0;
    if (max === rn) h = (gn - bn) / d + (gn < bn ? 6 : 0);
    else if (max === gn) h = (bn - rn) / d + 2;
    else h = (rn - gn) / d + 4;
    return { h: h * 60, s, l };
  }

  function hslToRgb(h, s, l) {
    const c = (1 - Math.abs(2 * l - 1)) * s;
    const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
    const m = l - c / 2;
    let r = 0;
    let g = 0;
    let b = 0;
    if (h < 60) [r, g, b] = [c, x, 0];
    else if (h < 120) [r, g, b] = [x, c, 0];
    else if (h < 180) [r, g, b] = [0, c, x];
    else if (h < 240) [r, g, b] = [0, x, c];
    else if (h < 300) [r, g, b] = [x, 0, c];
    else [r, g, b] = [c, 0, x];
    return {
      r: Math.round((r + m) * 255),
      g: Math.round((g + m) * 255),
      b: Math.round((b + m) * 255),
    };
  }

  function rgbToHex(r, g, b) {
    return (
      "#" +
      [r, g, b]
        .map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0"))
        .join("")
        .toUpperCase()
    );
  }

  function hexToRgb(hex) {
    const match = /^#?([0-9a-f]{6})$/i.exec(hex || "");
    if (!match) return null;
    const n = parseInt(match[1], 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
  }

  function luminance(r, g, b) {
    const linear = (v) => {
      const n = v / 255;
      return n <= 0.03928 ? n / 12.92 : Math.pow((n + 0.055) / 1.055, 2.4);
    };
    return (
      0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    );
  }

  // AppleTokens.shiftedForContrast: keep the hue, cap the saturation,
  // push lightness to a readable extreme (on-dark / on-light targets).
  // Hueless artwork has no hue to carry — plain white / near-black.
  function shiftedForContrast(hex, satCap, lightDark, lightLight) {
    const rgb = hexToRgb(hex);
    if (!rgb) return "#FFFFFF";
    const hsl = rgbToHsl(rgb.r, rgb.g, rgb.b);
    const onDark = luminance(rgb.r, rgb.g, rgb.b) <= 0.5;
    const target = onDark ? lightDark : lightLight;
    if (hsl.s < 0.08) {
      hsl.s = 0;
      hsl.l = target > 0.9 ? 1 : target < 0.12 ? 0.04 : target;
    } else {
      hsl.s = Math.min(hsl.s, satCap);
      hsl.l = target;
    }
    const out = hslToRgb(hsl.h, hsl.s, hsl.l);
    return rgbToHex(out.r, out.g, out.b);
  }

  function onColor(hex) {
    return shiftedForContrast(hex, 0.16, 0.96, 0.1);
  }
  function onColorSecondary(hex) {
    return shiftedForContrast(hex, 0.42, 0.76, 0.34);
  }
  function onColorHeading(hex) {
    return shiftedForContrast(hex, 0.55, 0.88, 0.18);
  }

  // Port of PlayerColorExtractor.extractGradientColors over a canvas
  // histogram (Palette's max-color-count-8 quantisation, approximated
  // with 5-bit buckets). Candidates in priority order: dominant,
  // vibrant, dark vibrant, light vibrant, muted, dark muted, light
  // muted; best by population-weighted vibrancy. Achromatic artwork
  // (population-weighted mean saturation < 0.10, near-black/white
  // pixels weighted out) stays grey instead of amplifying a JPEG cast.
  // Vividness: saturation x1.12 when vibrant, x1.0 otherwise (cap
  // 0.82); value x0.92 clamped 0.12-0.70.
  const entityColorCache = new Map();

  function extractEntityColor(url, callback) {
    if (!url) {
      callback(NEUTRAL_FALLBACK);
      return;
    }
    if (entityColorCache.has(url)) {
      callback(entityColorCache.get(url));
      return;
    }
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      try {
        const size = 100;
        const canvas = document.createElement("canvas");
        canvas.width = size;
        canvas.height = size;
        const ctx = canvas.getContext("2d", { willReadFrequently: true });
        if (!ctx) {
          callback(NEUTRAL_FALLBACK);
          return;
        }
        ctx.drawImage(img, 0, 0, size, size);
        const data = ctx.getImageData(0, 0, size, size).data;
        const buckets = new Map();
        for (let i = 0; i < data.length; i += 4) {
          if (data[i + 3] < 128) continue; // transparent pixel
          const key =
            ((data[i] >> 3) << 10) | ((data[i + 1] >> 3) << 5) | (data[i + 2] >> 3);
          const existing = buckets.get(key);
          if (existing) existing.population += 1;
          else {
            buckets.set(key, {
              r: data[i],
              g: data[i + 1],
              b: data[i + 2],
              population: 1,
            });
          }
        }
        const swatches = [...buckets.values()];
        if (swatches.length === 0) {
          callback(NEUTRAL_FALLBACK);
          return;
        }
        swatches.forEach((swatch) => {
          const hsv = rgbToHsv(swatch.r, swatch.g, swatch.b);
          swatch.h = hsv.h;
          swatch.s = hsv.s;
          swatch.v = hsv.v;
        });
        const byPopulation = [...swatches].sort(
          (a, b) => b.population - a.population,
        );
        const dominant = byPopulation[0];
        const pick = (test) => {
          const matches = swatches.filter(test);
          if (matches.length === 0) return null;
          return matches.sort((a, b) => b.population - a.population)[0];
        };
        const candidates = [
          dominant,
          pick((s) => s.s > 0.3 && s.v >= 0.3 && s.v <= 0.9), // vibrant
          pick((s) => s.s > 0.3 && s.v < 0.35), // dark vibrant
          pick((s) => s.s > 0.3 && s.v > 0.7), // light vibrant
          pick((s) => s.s <= 0.3 && s.v >= 0.3 && s.v <= 0.7), // muted
          pick((s) => s.s <= 0.3 && s.v < 0.35), // dark muted
          pick((s) => s.s <= 0.3 && s.v > 0.7), // light muted
        ].filter(Boolean);
        const weight = (s) => {
          const bonus = s.s > 0.3 && s.v > 0.3 ? 1.5 : 1;
          return s.population * 2 * bonus * ((s.s + s.v) / 2);
        };
        let best = null;
        let bestWeight = -1;
        candidates.forEach((s) => {
          const w = weight(s);
          if (w > bestWeight) {
            bestWeight = w;
            best = s;
          }
        });
        // Achromatic decision across the whole palette, weighted by
        // population; near-black/white pixels weighted out.
        let weighted = 0;
        let total = 0;
        swatches.forEach((s) => {
          const valueWeight = Math.min(
            1,
            Math.max(0, s.v * (1 - s.v) * 4),
          );
          const w = s.population * valueWeight;
          weighted += s.s * w;
          total += w;
        });
        const achromatic = total <= 0 || weighted / total < 0.1;
        let color;
        if (achromatic) {
          const base = best || dominant;
          const hsv = rgbToHsv(base.r, base.g, base.b);
          const v = Math.min(0.72, Math.max(0.14, hsv.v));
          const rgb = hsvToRgb(hsv.h, 0, v);
          color = rgbToHex(rgb.r, rgb.g, rgb.b);
        } else {
          const base = best || dominant;
          const vibrant =
            base.s > 0.25 && base.v > 0.2 && base.v < 0.9;
          const factor = vibrant ? 1.12 : 1.0;
          const hsv = rgbToHsv(base.r, base.g, base.b);
          hsv.s = Math.min(0.82, hsv.s * factor);
          hsv.v = Math.min(0.7, Math.max(0.12, hsv.v * 0.92));
          const rgb = hsvToRgb(hsv.h, hsv.s, hsv.v);
          color = rgbToHex(rgb.r, rgb.g, rgb.b);
        }
        if (entityColorCache.size > 128) entityColorCache.clear();
        entityColorCache.set(url, color);
        callback(color);
      } catch {
        callback(NEUTRAL_FALLBACK);
      }
    };
    img.onerror = () => callback(NEUTRAL_FALLBACK);
    img.src = url;
  }

  // Sets --entity-color (plus the on-color text variants) on the given
  // content element. One uniform color per page; the CSS uses the var
  // for the page background. Fallback is Aello Red when there is no
  // artwork, neutral #1C1C1E when extraction yields nothing.
  // Sets --entity-color (and its derived on-colors) on :root, so every
  // rule that reads it — including the view-level gradient — resolves
  // to the artwork color instead of the :root default. Also mirrored
  // onto the target element for the --accent mapping there.
  function setEntityVars(el, color) {
    el.style.setProperty("--entity-color", color);
    el.style.setProperty("--entity-on-color", onColor(color));
    el.style.setProperty(
      "--entity-on-color-secondary",
      onColorSecondary(color),
    );
    el.style.setProperty(
      "--entity-on-color-heading",
      onColorHeading(color),
    );
  }

  function applyEntityColor(url, targetEl) {
    if (!targetEl) return;
    if (!url) {
      setEntityVars(document.documentElement, AELLO_RED);
      setEntityVars(targetEl, AELLO_RED);
      return;
    }
    extractEntityColor(url, (color) => {
      setEntityVars(document.documentElement, color);
      setEntityVars(targetEl, color);
    });
  }

  // Signature of the last rendered queue list (queue ids + current
  // track id). The queue DOM is rebuilt only when this changes — not
  // on every timeupdate (~4x/sec), which used to drop queue clicks
  // mid-click and destroy focus.
  let lastQueueSignature = null;

  // Signature of the last artwork / play / mute innerHTML written by
  // updateNowPlaying (artwork URL + play state + mute state). Those
  // nodes are rewritten only when this changes — not on every
  // timeupdate (~4x/sec) — mirroring the queue-signature guard above.
  let lastNowPlayingArtSignature = null;

  // Inline SVG icons (exact copies of the icons the UI builder put in
  // index.html). updateNowPlaying must swap these via innerHTML — never
  // set textContent on an icon button, which would wipe its SVG and put
  // a text glyph back (the v0.1.0 emoji bug).
  const ICON_PLAY =
    '<svg class="icon" viewBox="0 0 24 24" fill="currentColor" stroke="none" aria-hidden="true"><polygon points="6 3 20 12 6 21 6 3"/></svg>';
  const ICON_PAUSE =
    '<svg class="icon" viewBox="0 0 24 24" fill="currentColor" stroke="none" aria-hidden="true"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>';
  const ICON_VOLUME =
    '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/></svg>';
  const ICON_MUTED =
    '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>';
  const ICON_CHEVRON_UP =
    '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="18 15 12 9 6 15"/></svg>';
  const ICON_CHEVRON_DOWN =
    '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="6 9 12 15 18 9"/></svg>';
  const ICON_X =
    '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>';

  function queueListHtml(queue, song) {
    return queue
      .map((item, index) => {
        const current = song && item.id === song.id;
        const sub =
          artistNames(item) || (item.localPath ? "Local file" : "");
        return (
          "<li" +
          (current ? ' aria-current="true"' : "") +
          ">" +
          '<span class="queue-title">' +
          (current ? ICON_PLAY + " " : "") +
          escapeHtml(item.title || "Untitled") +
          (sub ? " — " + escapeHtml(sub) : "") +
          "</span>" +
          '<span class="queue-actions">' +
          '<button type="button" data-queue-action="up" data-queue-index="' +
          index +
          '" aria-label="Move up"' +
          (index === 0 ? " disabled" : "") +
          ">" +
          ICON_CHEVRON_UP +
          "</button>" +
          '<button type="button" data-queue-action="down" data-queue-index="' +
          index +
          '" aria-label="Move down"' +
          (index === queue.length - 1 ? " disabled" : "") +
          ">" +
          ICON_CHEVRON_DOWN +
          "</button>" +
          '<button type="button" data-queue-action="remove" data-queue-index="' +
          index +
          '" aria-label="Remove from queue">' +
          ICON_X +
          "</button>" +
          "</span>" +
          "</li>"
        );
      })
      .join("");
  }

  function updateNowPlaying(song, audio) {
    const title = song ? song.title : "Nothing playing";
    const artist = song ? artistNames(song) : "—";
    const url = song ? artworkUrl(song) : null;
    const initial = song
      ? (song.title || "?").slice(0, 1).toUpperCase()
      : "A";

    const setText = (id, value) => {
      const el = document.getElementById(id);
      if (el) el.textContent = value;
    };
    const setHtml = (id, value) => {
      const el = document.getElementById(id);
      if (el) el.innerHTML = value;
    };

    setText("pb-title", title);
    setText("pb-artist", artist);

    setText("fp-title", title);
    setText("fp-artist", song ? artist : "Pick a song to start listening.");

    const duration = audio && Number.isFinite(audio.duration) ? audio.duration : 0;
    const position =
      audio && Number.isFinite(audio.currentTime) ? audio.currentTime : 0;
    setText("pb-duration", formatTime(duration));
    setText("fp-duration", formatTime(duration));
    setText("pb-current-time", formatTime(position));
    setText("fp-current-time", formatTime(position));
    const progress = duration > 0 ? (position / duration) * 100 : 0;
    const pbProgress = document.getElementById("pb-progress");
    if (pbProgress) pbProgress.value = progress;
    const fpProgress = document.getElementById("fp-progress");
    if (fpProgress) fpProgress.value = progress;

    const playing = song && audio && !audio.paused && !audio.ended;
    const player = window.AelloPlayer;
    const muted = player ? player.isMuted() : false;
    // Artwork + play/mute icons are innerHTML rewrites: only touch
    // them when the artwork URL, play state, or mute state changed.
    const artSignature = (url || initial) + "|" + playing + "|" + muted;
    if (artSignature !== lastNowPlayingArtSignature) {
      lastNowPlayingArtSignature = artSignature;
      setHtml(
        "pb-artwork",
        url
          ? '<img src="' + escapeHtml(url) + '" alt="">'
          : escapeHtml(initial),
      );
      setHtml(
        "fp-artwork",
        url
          ? '<img src="' + escapeHtml(url) + '" alt="">'
          : escapeHtml(initial),
      );
      setHtml("pb-play", playing ? ICON_PAUSE : ICON_PLAY);
      setHtml("fp-play", playing ? ICON_PAUSE : ICON_PLAY);
      setHtml("pb-mute", muted ? ICON_MUTED : ICON_VOLUME);
      setHtml("fp-mute", muted ? ICON_MUTED : ICON_VOLUME);
    }

    // Shuffle / repeat / like / speed states (guarded: the UI builder
    // creates these controls in parallel).
    if (player) {
      const shuffleBar = document.getElementById("pb-shuffle");
      if (shuffleBar) {
        shuffleBar.setAttribute(
          "aria-pressed",
          player.isShuffle() ? "true" : "false",
        );
      }
      const shuffleFull = document.getElementById("fp-shuffle");
      if (shuffleFull) {
        shuffleFull.setAttribute(
          "aria-pressed",
          player.isShuffle() ? "true" : "false",
        );
      }
      const repeatMode = player.getRepeat();
      ["pb-repeat", "fp-repeat"].forEach((id) => {
        const el = document.getElementById(id);
        if (!el) return;
        el.dataset.repeat = repeatMode;
        // Update only the label span — the button also holds the repeat SVG.
        const label = el.querySelector(".repeat-label");
        if (label) {
          label.textContent =
            repeatMode === "all" ? "All" : repeatMode === "one" ? "One" : "Off";
        }
      });
      const liked = song ? player.isLiked(song.id) : false;
      ["pb-like", "fp-like"].forEach((id) => {
        const el = document.getElementById(id);
        if (!el) return;
        el.setAttribute("aria-pressed", liked ? "true" : "false");
        // Fill the heart SVG when liked; never replace the button content.
        const svg = el.querySelector("svg");
        if (svg) svg.setAttribute("fill", liked ? "currentColor" : "none");
      });
      const speed = player.getSpeed();
      ["pb-speed", "fp-speed"].forEach((id) => {
        const el = document.getElementById(id);
        if (!el) return;
        // Update only the label span — the button also holds the gauge SVG.
        const label = el.querySelector(".speed-label");
        if (label) label.textContent = speed + "×";
      });
    }

    // Per-entity color for the full player page (one uniform color
    // from the current artwork; cached, so this is cheap per tick).
    const playerView = document.getElementById("view-player");
    if (playerView) applyEntityColor(url, playerView);

    const queue = player ? player.queue : [];
    const queueSignature =
      queue.map((item) => item.id).join("\n") +
      "|" +
      (song ? song.id : "");
    if (queueSignature !== lastQueueSignature) {
      lastQueueSignature = queueSignature;
      const html = queueListHtml(queue, song);
      const queueEl = document.getElementById("fp-queue");
      if (queueEl) queueEl.innerHTML = html;
      const panelEl = document.getElementById("queue-list");
      if (panelEl) panelEl.innerHTML = html;
    }
  }

  return {
    escapeHtml,
    formatTime,
    findItem,
    renderHome,
    renderSearchResults,
    renderSearchView,
    renderMoods,
    getActiveSearchTab,
    renderTrending,
    renderArtist,
    getArtistSongs,
    renderDetail,
    renderLibrary,
    applyTheme,
    fillSettingsForm,
    readSettingsForm,
    setAccountState,
    updateNowPlaying,
    // Lyrics
    parseLrc,
    findCurrentLineIndex,
    renderLyrics,
    updateLyricsHighlight,
    setLyricsAutoScroll,
    isLyricsAutoScroll,
    scrollLyricsToCurrent,
    getLyricsOffsetMs,
    setLyricsOffsetMs,
    getLyricsLineTime,
    // EQ
    renderEqPanel,
    readEqPanel,
    applyEqPreset,
    updateEqLiveNote,
    EQ_PRESETS,
    // Update / sleep dialogs
    showUpdateDialog,
    hideUpdateDialog,
    updateSleepStatus,
    // Listen Together
    setLtStatus,
    showLtRoom,
    hideLtRoom,
    renderLtMembers,
    appendLtChat,
    // Per-entity color
    extractEntityColor,
    applyEntityColor,
    onColor,
    onColorSecondary,
    onColorHeading,
    AUTO_PLAYLIST_COLORS,
    AELLO_RED,
    NEUTRAL_FALLBACK,
  };
})();
