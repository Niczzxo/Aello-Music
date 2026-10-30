"use strict";

/* Frontend playback.
 *
 * Audio renders through the <audio> element (WebView2 on Windows). The
 * queue lives here; stream URLs come from the Rust music service via
 * AelloApi.resolveStream. When there is no stream URL, playback does not
 * start and the UI says why — nothing is faked with placeholder audio.
 *
 * A snapshot of the queue/position is pushed to the Rust core so later
 * work (media keys, System Media Transport Controls) can read it. */

window.AelloPlayer = (() => {
  const audio = document.getElementById("audio");
  let queue = [];
  let currentIndex = -1;
  let statusTimer = null;
  let onChange = () => {};

  function currentSong() {
    if (currentIndex < 0 || currentIndex >= queue.length) return null;
    return queue[currentIndex];
  }

  function setStatus(message) {
    const el = document.getElementById("pb-status");
    if (!el) return;
    el.textContent = message || "";
    if (statusTimer) clearTimeout(statusTimer);
    if (message) {
      statusTimer = setTimeout(() => {
        el.textContent = "";
      }, 5000);
    }
  }

  function snapshot() {
    return {
      currentSongId: currentSong() ? currentSong().id : null,
      queue: queue.map((song) => song.id),
      positionSeconds: Number.isFinite(audio.currentTime)
        ? audio.currentTime
        : 0,
      isPlaying: !audio.paused && !audio.ended,
      volume: audio.volume,
      repeatMode: "off",
      shuffle: false,
    };
  }

  function syncSnapshot() {
    window.AelloApi.updatePlayerSnapshot(snapshot()).catch(() => {
      /* Snapshot sync is best-effort; playback must never fail for it. */
    });
  }

  function mediaErrorText(err) {
    if (!err) return "unknown error";
    const detail = err.message ? " (" + err.message + ")" : "";
    switch (err.code) {
      case 1:
        return "playback was aborted" + detail;
      case 2:
        return "a network error stopped the download" + detail;
      case 3:
        return "the file could not be decoded" + detail;
      case 4:
        return "this file format is not supported" + detail;
      default:
        return err.message || "unknown error";
    }
  }

  // Give up on the track at `index`: clear the audio element (same
  // pattern as removeAt) so Space/Play cannot resume the PREVIOUS
  // track's URL while the UI names the failed one, and restore the
  // previously playing index (or -1 when there was none).
  function failPlay(previousIndex, message) {
    audio.pause();
    audio.removeAttribute("src");
    audio.load();
    currentIndex =
      previousIndex >= 0 && previousIndex < queue.length
        ? previousIndex
        : -1;
    setStatus(message);
    onChange();
    return false;
  }

  async function playAt(index) {
    const song = queue[index];
    if (!song) return false;
    const previousIndex = currentIndex;
    currentIndex = index;
    let url;
    if (song.localPath) {
      // Local file: play through the Tauri asset protocol, no stream call.
      url = window.AelloApi.localFileUrl(song.localPath);
      if (!url) {
        return failPlay(
          previousIndex,
          "Couldn't play this local file: the desktop app's asset protocol is not available.",
        );
      }
    } else {
      try {
        url = await window.AelloApi.resolveStream(song.id);
      } catch (err) {
        // resolve_stream rejects with the real YouTube reason (e.g.
        // "This video is unavailable") — show it instead of a generic line.
        const reason =
          typeof err === "string"
            ? err
            : (err && err.message) || "No playable stream is available for this song yet.";
        return failPlay(previousIndex, reason);
      }
      if (!url) {
        return failPlay(
          previousIndex,
          "No playable stream is available for this song yet.",
        );
      }
    }
    audio.src = url;
    try {
      await audio.play();
    } catch (err) {
      return failPlay(
        previousIndex,
        "Couldn't play \"" +
          (song.title || "Untitled") +
          "\": " +
          (err && err.message ? err.message : "playback failed to start."),
      );
    }
    onChange();
    syncSnapshot();
    return true;
  }

  async function playSong(song, newQueue) {
    if (Array.isArray(newQueue) && newQueue.length > 0) {
      queue = newQueue.slice();
    }
    let index = queue.findIndex((item) => item.id === song.id);
    if (index === -1) {
      queue.push(song);
      index = queue.length - 1;
    }
    return playAt(index);
  }

  async function toggle() {
    const song = currentSong();
    if (!song) {
      setStatus("Pick a song first.");
      return;
    }
    if (audio.paused) {
      try {
        await audio.play();
      } catch (err) {
        setStatus(
          "Playback failed to start" +
            (err && err.message ? ": " + err.message : "."),
        );
      }
    } else {
      audio.pause();
    }
    onChange();
    syncSnapshot();
  }

  function next() {
    if (queue.length === 0) return;
    // Repeat is off (see snapshot()): stop at the end of the queue
    // instead of wrapping to the start. This also makes `ended`
    // stop playback at the last track.
    if (currentIndex >= queue.length - 1) {
      audio.pause();
      onChange();
      syncSnapshot();
      return;
    }
    playAt(currentIndex + 1);
  }

  function previous() {
    if (queue.length === 0) return;
    if (audio.currentTime > 3 && currentIndex >= 0) {
      audio.currentTime = 0;
      onChange();
      return;
    }
    const prevIndex =
      (currentIndex - 1 + queue.length) % queue.length;
    playAt(prevIndex);
  }

  function seekTo(seconds) {
    if (!Number.isFinite(seconds)) return;
    let target = Math.max(0, seconds);
    if (Number.isFinite(audio.duration) && audio.duration > 0) {
      target = Math.min(target, audio.duration);
    }
    audio.currentTime = target;
    onChange();
  }

  function setVolume(value) {
    audio.volume = Math.min(1, Math.max(0, value));
    onChange();
  }

  function setMuted(value) {
    audio.muted = !!value;
    onChange();
  }

  function toggleMute() {
    setMuted(!audio.muted);
    return audio.muted;
  }

  function isMuted() {
    return audio.muted;
  }

  // Queue management (used by the full-player queue list). Removing the
  // currently playing item stops playback; moving an item keeps the
  // current track pointing at the same song.
  function removeAt(index) {
    if (index < 0 || index >= queue.length) return;
    const removingCurrent = index === currentIndex;
    queue.splice(index, 1);
    if (removingCurrent) {
      audio.pause();
      audio.removeAttribute("src");
      audio.load();
      currentIndex = -1;
    } else if (index < currentIndex) {
      currentIndex -= 1;
    }
    onChange();
    syncSnapshot();
  }

  function moveUp(index) {
    if (index <= 0 || index >= queue.length) return;
    const [item] = queue.splice(index, 1);
    queue.splice(index - 1, 0, item);
    if (currentIndex === index) currentIndex = index - 1;
    else if (currentIndex === index - 1) currentIndex = index;
    onChange();
    syncSnapshot();
  }

  function moveDown(index) {
    if (index < 0 || index >= queue.length - 1) return;
    const [item] = queue.splice(index, 1);
    queue.splice(index + 1, 0, item);
    if (currentIndex === index) currentIndex = index + 1;
    else if (currentIndex === index + 1) currentIndex = index;
    onChange();
    syncSnapshot();
  }

  audio.addEventListener("timeupdate", () => {
    onChange();
  });

  // Duration becomes known once metadata loads (local files report it
  // here); refresh the bar so the time shows up.
  audio.addEventListener("loadedmetadata", () => {
    onChange();
  });

  audio.addEventListener("ended", () => {
    next();
  });

  audio.addEventListener("error", () => {
    if (!audio.src) return;
    setStatus("Couldn't play this track: " + mediaErrorText(audio.error) + ".");
    onChange();
  });

  return {
    get queue() {
      return queue.slice();
    },
    currentSong,
    playSong,
    playAt,
    toggle,
    next,
    previous,
    seekTo,
    setVolume,
    setMuted,
    toggleMute,
    isMuted,
    removeAt,
    moveUp,
    moveDown,
    setStatus,
    onStateChange(handler) {
      onChange = handler;
    },
    audio,
  };
})();
