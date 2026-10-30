"use strict";

/* Rendering for the Music, Library, Settings, and full-player views.
 * All text from the backend is escaped before it reaches the page.
 * Empty states are honest about which state they are: a genuine empty
 * result ("No results…") vs. the plain-browser preview, where the
 * desktop bridge is unavailable (see `AelloApi.outsideApp`). Transport
 * or backend failures never reach these renderers — the catch in
 * app.js shows the real error instead. */

window.AelloViews = (() => {
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

  function renderHome(sections, statusEl) {
    const host = document.getElementById("home-sections");
    // Prune the registry: ids from earlier renders must not resolve
    // via findItem after the list is replaced.
    registry.clear();
    if (!sections || sections.length === 0) {
      if (statusEl) {
        statusEl.textContent = isPreview()
          ? "No music to show yet — the desktop bridge is not connected in this preview."
          : "No music to show yet.";
      }
      host.innerHTML = "";
      return;
    }
    if (statusEl) statusEl.textContent = "";
    sections.forEach(registerSection);
    host.innerHTML = sections
      .map((section) => {
        const cards = section.items.map(cardHtml).join("");
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
      })
      .join("");
  }

  function renderSearchResults(items, query, statusEl) {
    if (!items || items.length === 0) {
      const host = document.getElementById("home-sections");
      host.innerHTML = "";
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

  function renderDetail(detail, isAlbum, statusEl) {
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
  }

  const LIBRARY_EMPTY = {
    playlists: "No playlists yet. Saved playlists are not available in v0.1.0.",
    albums: "No albums yet. A saved-albums view is not available in v0.1.0.",
    artists: "No artists yet. Artist pages are not available in v0.1.0.",
    songs: "No songs yet. Use “Open audio files…” above to pick audio files from this computer.",
    downloads:
      "No downloads yet. Offline downloads are planned for a later phase and are not available in v0.1.0.",
  };

  function renderLibrary(segment, localSongs) {
    const host = document.getElementById("library-list");
    const songs = Array.isArray(localSongs) ? localSongs : [];
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
    host.innerHTML =
      '<p class="empty">' + escapeHtml(LIBRARY_EMPTY[segment]) + "</p>";
  }

  function applyTheme(settings) {
    const root = document.documentElement;
    root.dataset.accent = settings.accentTheme || "aello-red";
    root.dataset.appearance = settings.appearance || "system";
  }

  function fillSettingsForm(settings) {
    const form = document.getElementById("settings-form");
    const accent = form.querySelector(
      'input[name="accent"][value="' + settings.accentTheme + '"]',
    );
    if (accent) accent.checked = true;
    document.getElementById("setting-appearance").value =
      settings.appearance || "system";
    document.getElementById("setting-volume").value = settings.volume;
    document.getElementById("setting-updates").checked =
      settings.checkForUpdatesOnStartup !== false;
  }

  function readSettingsForm() {
    const form = document.getElementById("settings-form");
    const accent = form.querySelector('input[name="accent"]:checked');
    return {
      accentTheme: accent ? accent.value : "aello-red",
      appearance: document.getElementById("setting-appearance").value,
      volume: parseFloat(document.getElementById("setting-volume").value),
      checkForUpdatesOnStartup: document.getElementById("setting-updates")
        .checked,
    };
  }

  // Signature of the last rendered queue list (queue ids + current
  // track id). The queue DOM is rebuilt only when this changes — not
  // on every timeupdate (~4x/sec), which used to drop queue clicks
  // mid-click and destroy focus.
  let lastQueueSignature = null;

  function updateNowPlaying(song, audio) {
    const title = song ? song.title : "Nothing playing";
    const artist = song ? artistNames(song) : "—";
    const url = song ? artworkUrl(song) : null;
    const initial = song
      ? (song.title || "?").slice(0, 1).toUpperCase()
      : "A";

    document.getElementById("pb-title").textContent = title;
    document.getElementById("pb-artist").textContent = artist;
    document.getElementById("pb-artwork").innerHTML = url
      ? '<img src="' + escapeHtml(url) + '" alt="">'
      : escapeHtml(initial);

    document.getElementById("fp-title").textContent = title;
    document.getElementById("fp-artist").textContent = song
      ? artist
      : "Pick a song to start listening.";
    document.getElementById("fp-artwork").innerHTML = url
      ? '<img src="' + escapeHtml(url) + '" alt="">'
      : escapeHtml(initial);

    const duration = Number.isFinite(audio.duration) ? audio.duration : 0;
    const position = Number.isFinite(audio.currentTime)
      ? audio.currentTime
      : 0;
    document.getElementById("pb-duration").textContent =
      formatTime(duration);
    document.getElementById("fp-duration").textContent =
      formatTime(duration);
    document.getElementById("pb-current-time").textContent =
      formatTime(position);
    document.getElementById("fp-current-time").textContent =
      formatTime(position);
    const progress = duration > 0 ? (position / duration) * 100 : 0;
    document.getElementById("pb-progress").value = progress;
    document.getElementById("fp-progress").value = progress;

    const playing = song && !audio.paused && !audio.ended;
    document.getElementById("pb-play").textContent = playing ? "⏸" : "▶";
    document.getElementById("fp-play").textContent = playing
      ? "Pause"
      : "Play";

    const muteBar = document.getElementById("pb-mute");
    if (muteBar) muteBar.textContent = audio.muted ? "🔇" : "🔊";
    const muteFull = document.getElementById("fp-mute");
    if (muteFull) muteFull.textContent = audio.muted ? "Unmute" : "Mute";

    const queueEl = document.getElementById("fp-queue");
    const queue = window.AelloPlayer.queue;
    const queueSignature =
      queue.map((item) => item.id).join("\n") +
      "|" +
      (song ? song.id : "");
    if (queueSignature !== lastQueueSignature) {
      lastQueueSignature = queueSignature;
      queueEl.innerHTML = queue
      .map((item, index) => {
        const current = song && item.id === song.id;
        const sub =
          artistNames(item) || (item.localPath ? "Local file" : "");
        return (
          "<li" +
          (current ? ' aria-current="true"' : "") +
          ">" +
          '<span class="queue-title">' +
          (current ? "▶ " : "") +
          escapeHtml(item.title || "Untitled") +
          (sub ? " — " + escapeHtml(sub) : "") +
          "</span>" +
          '<span class="queue-actions">' +
          '<button type="button" data-queue-action="up" data-queue-index="' +
          index +
          '" aria-label="Move up"' +
          (index === 0 ? " disabled" : "") +
          ">↑</button>" +
          '<button type="button" data-queue-action="down" data-queue-index="' +
          index +
          '" aria-label="Move down"' +
          (index === queue.length - 1 ? " disabled" : "") +
          ">↓</button>" +
          '<button type="button" data-queue-action="remove" data-queue-index="' +
          index +
          '" aria-label="Remove from queue">✕</button>' +
          "</span>" +
          "</li>"
        );
      })
      .join("");
    }
  }

  return {
    escapeHtml,
    formatTime,
    findItem,
    renderHome,
    renderSearchResults,
    renderDetail,
    renderLibrary,
    applyTheme,
    fillSettingsForm,
    readSettingsForm,
    updateNowPlaying,
  };
})();
