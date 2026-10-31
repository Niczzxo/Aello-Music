"use strict";

/* Frontend playback.
 *
 * Audio renders through <audio> elements (WebView2 on Windows). The
 * queue lives here; stream URLs come from the Rust music service via
 * AelloApi.resolveStream. When there is no stream URL, playback does not
 * start and the UI says why — nothing is faked with placeholder audio.
 *
 * A snapshot of the queue/position is pushed to the Rust core so later
 * work (media keys, System Media Transport Controls) can read it.
 *
 * Crossfade uses TWO <audio> elements (the #audio element + one created
 * here) with a Web Audio gain ramp between them. Sound Enhance / custom
 * EQ runs through the same Web Audio graph (MediaElementSource -> 5x
 * BiquadFilter -> preamp GainNode -> master GainNode -> destination).
 * If Web Audio is unavailable, playback falls back to the plain element
 * (hard cuts, no EQ) — never silence.
 *
 * Queue persistence: the Rust snapshot only carries song ids, which is
 * not enough to rebuild the queue after a restart (no metadata comes
 * back). The full queue is therefore saved to localStorage as well, and
 * restored from there. The Rust snapshot is still sent for SMTC. */

window.AelloPlayer = (() => {
  // The #audio element is in index.html; fall back to a created element
  // if it is ever missing so the module loads instead of crashing.
  const audioA =
    document.getElementById("audio") || document.createElement("audio");
  // Second element for crossfades. Created in JS (no id — the DOM
  // contract only lists #audio); it never needs to be in the document.
  const audioB = document.createElement("audio");
  audioB.preload = "auto";

  let queue = [];
  let currentIndex = -1;
  let statusTimer = null;
  let onChange = () => {};

  // ---- persisted player prefs (localStorage; Rust settings do not
  // carry shuffle/repeat/speed, so this is the durable store) --------
  const PREFS_KEY = "aello.player.prefs.v1";
  const QUEUE_KEY = "aello.queue.v1";
  let prefs = { shuffle: false, repeat: "off", speed: 1, radio: true, eightD: false, normalize: false };
  try {
    const stored = JSON.parse(localStorage.getItem(PREFS_KEY) || "null");
    if (stored && typeof stored === "object") prefs = { ...prefs, ...stored };
  } catch {
    /* Corrupt prefs: start from defaults. */
  }
  function savePrefs() {
    try {
      localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));
    } catch {
      /* Storage unavailable: prefs stay in memory for this session. */
    }
  }

  // ---- settings-driven audio options (app.js feeds these in) --------
  let crossfadeEnabled = true;
  let crossfadeSecs = 5;
  let soundEnhanceEnabled = true;
  let activeEqProfileId = null;
  let eqProfiles = [];
  let persistentQueue = true;
  let userVolume = 0.8;

  // ---- Web Audio graph ------------------------------------------------
  // Lazily built on the first user-gesture play (AudioContext needs a
  // gesture). Null until built, or when Web Audio is unavailable.
  let actx = null;
  let srcA = null;
  let srcB = null;
  let gainA = null;
  let gainB = null;
  let eqIn = null;
  let eqBands = []; // 5 BiquadFilterNodes
  let preamp = null;
  let master = null;
  let graphReady = false;
  // 8D audio (audit #30): a StereoPannerNode after the master gain,
  // driven by a slow LFO (Android 8D pans the stereo image). Off by
  // default; the oscillator is created on enable (an OscillatorNode
  // cannot restart after stop()).
  let panner = null;
  let lfo = null;
  let lfoDepth = null;
  // Normalization (audit #32): a DynamicsCompressorNode between the
  // preamp and the master gain — an approximation of Android's
  // per-track LoudnessEnhancer (no loudness metadata is fetched, so
  // this is a fixed gentle compressor, not per-track gain). Off by
  // default; ratio 1:1 + 0 dB threshold is a pass-through when off.
  let compressor = null;

  const ENHANCE_BANDS = [
    { freqHz: 80, gainDb: 3.0, q: 0.7, kind: "lowshelf" },
    { freqHz: 300, gainDb: 1.0, q: 1.0, kind: "peaking" },
    { freqHz: 3000, gainDb: 2.0, q: 1.0, kind: "peaking" },
    { freqHz: 8000, gainDb: 1.5, q: 0.7, kind: "highshelf" },
  ];
  const ENHANCE_PREAMP_DB = -3.0;

  function dbToGain(db) {
    return Math.pow(10, db / 20);
  }

  function ensureGraph() {
    if (graphReady) return true;
    if (actx) return false; // Tried already and failed.
    try {
      const Ctor =
        window.AudioContext ||
        window.webkitAudioContext;
      if (!Ctor) return false;
      actx = new Ctor();
      srcA = actx.createMediaElementSource(audioA);
      srcB = actx.createMediaElementSource(audioB);
      gainA = actx.createGain();
      gainB = actx.createGain();
      gainA.gain.value = 1;
      gainB.gain.value = 0;
      eqIn = actx.createGain();
      eqBands = [];
      for (let i = 0; i < 5; i++) {
        const node = actx.createBiquadFilter();
        node.type = "peaking";
        node.frequency.value = 1000;
        node.gain.value = 0;
        node.Q.value = 1;
        eqBands.push(node);
      }
      preamp = actx.createGain();
      preamp.gain.value = 1;
      compressor = actx.createDynamicsCompressor();
      master = actx.createGain();
      master.gain.value = 1;
      panner = actx.createStereoPanner();
      panner.pan.value = 0;
      srcA.connect(gainA);
      srcB.connect(gainB);
      gainA.connect(eqIn);
      gainB.connect(eqIn);
      let node = eqIn;
      eqBands.forEach((band) => {
        node.connect(band);
        node = band;
      });
      node.connect(preamp);
      preamp.connect(compressor);
      compressor.connect(master);
      master.connect(panner);
      panner.connect(actx.destination);
      graphReady = true;
      applyEq();
      applyNormalization();
      apply8d();
      applyVolumeToGraph();
      return true;
    } catch {
      actx = null;
      return false;
    }
  }

  function resumeGraph() {
    if (actx && actx.state === "suspended") {
      actx.resume().catch(() => {
        /* Resume is best-effort; the element still plays directly. */
      });
    }
  }

  // Precedence (Android MusicService, never stacked): custom profile >
  // Sound Enhance > flat. Flat = all band gains 0 + preamp 0 dB, i.e. a
  // pass-through bypass with no rewiring.
  // A live panel curve (from the EQ panel's sliders, not yet saved as a
  // profile) wins over all three while it is set; pass null to clear it.
  let liveEqCurve = null;

  function setLiveEqCurve(bands, preampDb) {
    if (!bands) {
      liveEqCurve = null;
    } else {
      liveEqCurve = {
        bands: Array.isArray(bands) ? bands : [],
        preampDb: typeof preampDb === "number" ? preampDb : 0,
      };
    }
    applyEq();
  }

  function activeEqCurve() {
    if (liveEqCurve) return liveEqCurve;
    const custom = eqProfiles.find((p) => p && p.id === activeEqProfileId);
    if (custom) {
      return {
        bands: Array.isArray(custom.bands) ? custom.bands : [],
        preampDb:
          typeof custom.preampDb === "number" ? custom.preampDb : 0,
      };
    }
    if (soundEnhanceEnabled) {
      return { bands: ENHANCE_BANDS, preampDb: ENHANCE_PREAMP_DB };
    }
    return { bands: [], preampDb: 0 };
  }

  function applyEq() {
    if (!graphReady) return;
    const curve = activeEqCurve();
    for (let i = 0; i < 5; i++) {
      const band = curve.bands[i];
      const node = eqBands[i];
      if (band) {
        node.type =
          band.kind === "lowshelf" || band.kind === "highshelf"
            ? band.kind
            : "peaking";
        node.frequency.value = band.freqHz || 1000;
        node.gain.value = band.gainDb || 0;
        node.Q.value = band.q || 1;
      } else {
        node.type = "peaking";
        node.frequency.value = 1000;
        node.gain.value = 0;
        node.Q.value = 1;
      }
    }
    preamp.gain.value = dbToGain(curve.preampDb);
  }

  function setEqSettings(settings) {
    if (typeof settings.soundEnhanceEnabled === "boolean") {
      soundEnhanceEnabled = settings.soundEnhanceEnabled;
    }
    if (typeof settings.activeEqProfileId !== "undefined") {
      activeEqProfileId = settings.activeEqProfileId || null;
    }
    if (Array.isArray(settings.eqProfiles)) {
      eqProfiles = settings.eqProfiles;
    }
    applyEq();
  }

  // ---- 8D audio + normalization (audit #30 / #32) ----------------------
  function applyNormalization() {
    if (!graphReady || !compressor) return;
    if (prefs.normalize) {
      compressor.threshold.value = -18;
      compressor.knee.value = 12;
      compressor.ratio.value = 4;
      compressor.attack.value = 0.003;
      compressor.release.value = 0.25;
    } else {
      // Pass-through: no gain reduction at any level.
      compressor.threshold.value = 0;
      compressor.knee.value = 0;
      compressor.ratio.value = 1;
      compressor.attack.value = 0.003;
      compressor.release.value = 0.25;
    }
  }

  function setNormalization(enabled) {
    prefs.normalize = !!enabled;
    savePrefs();
    applyNormalization();
    onChange();
  }

  function isNormalization() {
    return !!prefs.normalize;
  }

  function apply8d() {
    if (!graphReady || !panner) return;
    if (prefs.eightD) {
      if (!lfo) {
        lfo = actx.createOscillator();
        lfo.frequency.value = 0.07; // one full L↔R sweep ≈ 14s
        lfoDepth = actx.createGain();
        lfoDepth.gain.value = 0.85;
        lfo.connect(lfoDepth);
        lfoDepth.connect(panner.pan);
        lfo.start();
      }
    } else if (lfo) {
      try {
        lfo.stop();
      } catch {
        /* Already stopped. */
      }
      try {
        lfo.disconnect();
      } catch {
        /* Already disconnected. */
      }
      if (lfoDepth) {
        try {
          lfoDepth.disconnect();
        } catch {
          /* Already disconnected. */
        }
      }
      lfo = null;
      lfoDepth = null;
      panner.pan.value = 0;
    }
  }

  function set8d(enabled) {
    prefs.eightD = !!enabled;
    savePrefs();
    apply8d();
    onChange();
  }

  function is8d() {
    return !!prefs.eightD;
  }

  // ---- volume / mute / sleep-fade multiplier ---------------------------
  let muted = false;
  let sleepMultiplier = 1; // Sleep-timer fade scales the master gain.

  function effectiveVolume() {
    return muted ? 0 : userVolume * sleepMultiplier;
  }

  function applyVolumeToGraph() {
    if (!graphReady) return;
    master.gain.setTargetAtTime(effectiveVolume(), actx.currentTime, 0.03);
  }

  function applyVolumeToElements() {
    // When the graph is not in use the elements play directly — the
    // sleep fade still applies here (effectiveVolume carries the
    // sleepMultiplier), otherwise the fade would be a no-op.
    audioA.volume = effectiveVolume();
    audioB.volume = effectiveVolume();
    audioA.muted = false;
    audioB.muted = false;
  }

  function applyVolume() {
    if (graphReady) {
      // Element volume stays at 1 once routed through the graph; the
      // master gain owns volume (and the sleep fade).
      audioA.volume = 1;
      audioB.volume = 1;
      applyVolumeToGraph();
    } else {
      applyVolumeToElements();
    }
  }

  // ---- active / standby elements ---------------------------------------
  let activeEl = audioA;
  let standbyEl = audioB;
  let crossfadeTimer = null;

  function activeGainNode() {
    return activeEl === audioA ? gainA : gainB;
  }
  function standbyGainNode() {
    return standbyEl === audioA ? gainA : gainB;
  }

  function cancelCrossfade() {
    if (crossfadeTimer) {
      clearTimeout(crossfadeTimer);
      crossfadeTimer = null;
    }
  }

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
    // SMTC reads current_title / current_artist / current_artwork from
    // this snapshot (player.rs already declares them, all
    // #[serde(default)]); without them it falls back to the song id.
    const song = currentSong();
    const artwork =
      song && Array.isArray(song.artwork) && song.artwork.length > 0
        ? song.artwork[song.artwork.length - 1].url || null
        : null;
    return {
      currentSongId: song ? song.id : null,
      currentTitle: song ? song.title || null : null,
      currentArtist:
        song && Array.isArray(song.artists) && song.artists.length > 0
          ? song.artists.map((artist) => artist.name).join(", ")
          : null,
      currentArtwork: artwork,
      queue: queue.map((song) => song.id),
      positionSeconds: Number.isFinite(activeEl.currentTime)
        ? activeEl.currentTime
        : 0,
      isPlaying: !activeEl.paused && !activeEl.ended,
      volume: userVolume,
      repeatMode: prefs.repeat,
      shuffle: prefs.shuffle,
    };
  }

  function syncSnapshot() {
    window.AelloApi.updatePlayerSnapshot(snapshot()).catch(() => {
      /* Snapshot sync is best-effort; playback must never fail for it. */
    });
    saveQueueToStorage();
  }

  function saveQueueToStorage() {
    if (!persistentQueue) return;
    try {
      localStorage.setItem(
        QUEUE_KEY,
        JSON.stringify({
          queue,
          index: currentIndex,
          position: Number.isFinite(activeEl.currentTime)
            ? activeEl.currentTime
            : 0,
        }),
      );
    } catch {
      /* Storage unavailable: queue stays in memory for this session. */
    }
  }

  // Restores the queue saved by saveQueueToStorage. Returns the saved
  // position in seconds (0 when nothing was saved). The caller decides
  // whether to resume playback; we never auto-play on startup.
  function restoreQueue() {
    if (!persistentQueue) return 0;
    try {
      const stored = JSON.parse(localStorage.getItem(QUEUE_KEY) || "null");
      if (!stored || !Array.isArray(stored.queue) || stored.queue.length === 0) {
        return 0;
      }
      queue = stored.queue;
      currentIndex =
        typeof stored.index === "number" &&
        stored.index >= 0 &&
        stored.index < queue.length
          ? stored.index
          : -1;
      onChange();
      return typeof stored.position === "number" ? stored.position : 0;
    } catch {
      return 0;
    }
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

  // Give up on the track at `index`: clear both audio elements (same
  // pattern as removeAt) so Space/Play cannot resume the PREVIOUS
  // track's URL while the UI names the failed one, and restore the
  // previously playing index (or -1 when there was none).
  function failPlay(previousIndex, message) {
    cancelCrossfade();
    audioA.pause();
    audioB.pause();
    audioA.removeAttribute("src");
    audioB.removeAttribute("src");
    audioA.load();
    audioB.load();
    currentIndex =
      previousIndex >= 0 && previousIndex < queue.length
        ? previousIndex
        : -1;
    setStatus(message);
    onChange();
    return false;
  }

  function setSpeedOnElements() {
    audioA.playbackRate = prefs.speed;
    audioB.playbackRate = prefs.speed;
  }

  // Starts `url` on the standby element and crossfades from the active
  // one over crossfadeSecs. When crossfade is off (or Web Audio is
  // unavailable) the new track simply starts on the active element.
  async function startWithCrossfade(url) {
    const wasPlaying = !activeEl.paused && !activeEl.ended && !!activeEl.src;
    if (
      !wasPlaying ||
      !crossfadeEnabled ||
      crossfadeSecs <= 0 ||
      !graphReady
    ) {
      // Hard switch on the active element.
      cancelCrossfade();
      if (standbyEl.src) {
        standbyEl.pause();
        standbyEl.removeAttribute("src");
        standbyEl.load();
      }
      activeEl.src = url;
      setSpeedOnElements();
      await activeEl.play();
      return;
    }
    const fromEl = activeEl;
    const fromGain = activeGainNode();
    const toEl = standbyEl;
    const toGain = standbyGainNode();
    const secs = Math.min(Math.max(crossfadeSecs, 1), 10);
    toEl.src = url;
    setSpeedOnElements();
    const now = actx.currentTime;
    toGain.gain.cancelScheduledValues(now);
    toGain.gain.setValueAtTime(0, now);
    toGain.gain.linearRampToValueAtTime(1, now + secs);
    fromGain.gain.cancelScheduledValues(now);
    fromGain.gain.setValueAtTime(fromGain.gain.value, now);
    fromGain.gain.linearRampToValueAtTime(0, now + secs);
    try {
      await toEl.play();
    } catch (err) {
      // Roll the gains back so the old track keeps playing.
      toGain.gain.cancelScheduledValues(actx.currentTime);
      toGain.gain.setValueAtTime(0, actx.currentTime);
      fromGain.gain.cancelScheduledValues(actx.currentTime);
      fromGain.gain.setValueAtTime(1, actx.currentTime);
      throw err;
    }
    crossfadeTimer = setTimeout(() => {
      crossfadeTimer = null;
      fromEl.pause();
      fromEl.removeAttribute("src");
      fromEl.load();
      fromGain.gain.setTargetAtTime(0, actx.currentTime, 0.05);
      activeEl = toEl;
      standbyEl = fromEl;
      onChange();
      syncSnapshot();
    }, secs * 1000);
    // Swap the logical active element immediately so time/progress
    // read from the incoming track; the old element is only audio now.
    activeEl = toEl;
    standbyEl = fromEl;
  }

  async function playAt(index) {
    const song = queue[index];
    if (!song) return false;
    ensureGraph();
    resumeGraph();
    const previousIndex = currentIndex;
    currentIndex = index;
    resetHistoryTimer();
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
    } else if (song.downloadedPath) {
      // Offline download: same asset protocol, local file.
      url = window.AelloApi.localFileUrl(song.downloadedPath);
      if (!url) {
        return failPlay(
          previousIndex,
          "Couldn't play this download: the desktop app's asset protocol is not available.",
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
    try {
      await startWithCrossfade(url);
    } catch (err) {
      return failPlay(
        previousIndex,
        "Couldn't play \"" +
          (song.title || "Untitled") +
          "\": " +
          (err && err.message ? err.message : "playback failed to start."),
      );
    }
    radioPrefetchFor = null;
    onChange();
    syncSnapshot();
    maybePrefetchRadio();
    return true;
  }

  async function playSong(song, newQueue) {
    if (Array.isArray(newQueue) && newQueue.length > 0) {
      queue = newQueue.slice();
      // The queue was replaced: the shuffle walk-back history refers
      // to the old queue and must not survive it.
      shuffleHistory.length = 0;
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
    ensureGraph();
    resumeGraph();
    if (activeEl.paused) {
      try {
        await activeEl.play();
      } catch (err) {
        setStatus(
          "Playback failed to start" +
            (err && err.message ? ": " + err.message : "."),
        );
      }
    } else {
      cancelCrossfade();
      activeEl.pause();
      if (!standbyEl.paused) standbyEl.pause();
    }
    onChange();
    syncSnapshot();
  }

  // Shuffle history so Previous walks back through what was actually
  // played while shuffling (Android keeps the same behaviour).
  const shuffleHistory = [];

  function pickRandomIndex(except) {
    if (queue.length <= 1) return 0;
    let next = except;
    while (next === except) {
      next = Math.floor(Math.random() * queue.length);
    }
    return next;
  }

  // ---- radio / autoplay (audit #28) --------------------------------------
  // When the queue ends and radio is on, extend it from related songs
  // (Android `PlayerConnection.startRadioSeamlessly`); the same
  // primitive Daily Discover uses (`related_for_video`, via
  // `get_related`). Off by the user's toggle; when it is off, or when
  // no related songs come back, playback stops exactly as before.
  let radioExtending = false;
  let radioPrefetchFor = null;

  function setRadio(enabled) {
    prefs.radio = !!enabled;
    savePrefs();
    onChange();
  }

  function isRadio() {
    return prefs.radio !== false;
  }

  function appendRelated(songs, limit) {
    let added = 0;
    const seen = new Set(queue.map((item) => item.id));
    for (const song of songs || []) {
      if (!song || !song.id || seen.has(song.id)) continue;
      seen.add(song.id);
      queue.push(song);
      added += 1;
      if (added >= (limit || 8)) break;
    }
    if (added > 0) {
      onChange();
      syncSnapshot();
    }
    return added;
  }

  // Proactive prefetch: when ≤2 tracks remain, fetch related songs
  // for the current track in the background so the queue rarely runs
  // dry. Fire-and-forget; a failed fetch just means no prefetch.
  function maybePrefetchRadio() {
    if (!isRadio() || radioExtending) return;
    const song = currentSong();
    if (!song || song.localPath || song.downloadedPath) return;
    if (queue.length - 1 - currentIndex > 2) return;
    if (radioPrefetchFor === song.id) return;
    radioPrefetchFor = song.id;
    radioExtending = true;
    window.AelloApi.getRelated(song.id)
      .then((related) => {
        appendRelated(related, 8);
      })
      .catch(() => {
        /* No prefetch — playback continues; next() retries at the end. */
      })
      .finally(() => {
        radioExtending = false;
      });
  }

  // End-of-queue extension: fetch related songs for the last track and
  // keep playing. Returns true when playback continues.
  async function extendRadio() {
    if (radioExtending) return false;
    const song = currentSong();
    if (!song || song.localPath || song.downloadedPath) return false;
    radioExtending = true;
    try {
      const related = await window.AelloApi.getRelated(song.id);
      const added = appendRelated(related, 8);
      if (added === 0) return false;
      await playAt(currentIndex + 1);
      return true;
    } catch {
      return false;
    } finally {
      radioExtending = false;
    }
  }

  function next() {
    if (queue.length === 0) return;
    if (prefs.repeat === "one") {
      playAt(currentIndex);
      return;
    }
    if (prefs.shuffle) {
      shuffleHistory.push(currentIndex);
      playAt(pickRandomIndex(currentIndex));
      return;
    }
    if (currentIndex >= queue.length - 1) {
      if (prefs.repeat === "all") {
        playAt(0);
        return;
      }
      // Radio: extend the queue from related songs instead of
      // stopping (Android autoplay). extendRadio pauses below when
      // it cannot extend. While an extension is already in flight,
      // do nothing — the in-flight one will continue or pause.
      if (radioExtending) return;
      if (isRadio()) {
        extendRadio().then((continued) => {
          if (!continued) {
            activeEl.pause();
            setStatus("Radio: no more related songs.");
            onChange();
            syncSnapshot();
          }
        });
        return;
      }
      activeEl.pause();
      onChange();
      syncSnapshot();
      return;
    }
    playAt(currentIndex + 1);
  }

  function previous() {
    if (queue.length === 0) return;
    if (activeEl.currentTime > 3 && currentIndex >= 0) {
      activeEl.currentTime = 0;
      onChange();
      return;
    }
    if (prefs.shuffle && shuffleHistory.length > 0) {
      playAt(shuffleHistory.pop());
      return;
    }
    if (prefs.repeat === "all" && currentIndex <= 0) {
      playAt(queue.length - 1);
      return;
    }
    // No wrap when repeat is off: stop at track 0 (mirrors next(),
    // which stops at the last track instead of wrapping).
    if (currentIndex <= 0) {
      if (currentIndex === 0) {
        activeEl.currentTime = 0;
        onChange();
        return;
      }
      playAt(0);
      return;
    }
    playAt(currentIndex - 1);
  }

  function seekTo(seconds) {
    if (!Number.isFinite(seconds)) return;
    let target = Math.max(0, seconds);
    if (Number.isFinite(activeEl.duration) && activeEl.duration > 0) {
      target = Math.min(target, activeEl.duration);
    }
    activeEl.currentTime = target;
    onChange();
  }

  function setVolume(value) {
    userVolume = Math.min(1, Math.max(0, value));
    applyVolume();
    onChange();
  }

  function getVolume() {
    return userVolume;
  }

  function setMuted(value) {
    muted = !!value;
    applyVolume();
    onChange();
  }

  function toggleMute() {
    setMuted(!muted);
    return muted;
  }

  function isMuted() {
    return muted;
  }

  // ---- shuffle / repeat --------------------------------------------------
  function setShuffle(value) {
    prefs.shuffle = !!value;
    savePrefs();
    onChange();
    syncSnapshot();
  }

  function isShuffle() {
    return prefs.shuffle;
  }

  function cycleRepeat() {
    prefs.repeat =
      prefs.repeat === "off"
        ? "all"
        : prefs.repeat === "all"
          ? "one"
          : "off";
    savePrefs();
    onChange();
    syncSnapshot();
    return prefs.repeat;
  }

  function setRepeat(mode) {
    if (mode === "off" || mode === "all" || mode === "one") {
      prefs.repeat = mode;
      savePrefs();
      onChange();
      syncSnapshot();
    }
  }

  function getRepeat() {
    return prefs.repeat;
  }

  // ---- playback speed ------------------------------------------------------
  const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2];

  function setSpeed(speed) {
    if (!SPEEDS.includes(speed)) return;
    prefs.speed = speed;
    setSpeedOnElements();
    savePrefs();
    onChange();
  }

  function cycleSpeed() {
    const index = SPEEDS.indexOf(prefs.speed);
    const nextSpeed = SPEEDS[(index + 1) % SPEEDS.length];
    setSpeed(nextSpeed);
    return nextSpeed;
  }

  function getSpeed() {
    return prefs.speed;
  }

  // ---- crossfade settings --------------------------------------------------
  function setCrossfade(enabled, secs) {
    crossfadeEnabled = !!enabled;
    if (typeof secs === "number" && Number.isFinite(secs)) {
      crossfadeSecs = Math.min(10, Math.max(1, secs));
    }
  }

  // ---- history reporting (after 30s of playback) ---------------------------
  let playedSeconds = 0;
  let historyReportedFor = null;

  function resetHistoryTimer() {
    playedSeconds = 0;
    historyReportedFor = null;
  }

  function noteTimeUpdate() {
    const song = currentSong();
    if (!song || activeEl.paused) return;
    // timeupdate fires ~4x/sec; accumulate wall-clock playback time.
    playedSeconds += 0.25;
    if (playedSeconds >= 30 && historyReportedFor !== song.id) {
      historyReportedFor = song.id;
      window.AelloApi.addToHistory(song);
    }
  }

  // ---- likes / downloads -----------------------------------------------------
  let likedIds = new Set();

  function setLikedIds(ids) {
    likedIds = new Set(Array.isArray(ids) ? ids : []);
    onChange();
  }

  function isLiked(songId) {
    return likedIds.has(songId);
  }

  async function toggleLike() {
    const song = currentSong();
    if (!song) {
      setStatus("Nothing playing to like.");
      return false;
    }
    const nextLiked = !likedIds.has(song.id);
    const ok = await window.AelloApi.likeSong(song, nextLiked);
    if (!ok) {
      setStatus(
        "Couldn't save that like — the library store is not available yet.",
      );
      return false;
    }
    if (nextLiked) likedIds.add(song.id);
    else likedIds.delete(song.id);
    onChange();
    return nextLiked;
  }

  async function downloadCurrent() {
    const song = currentSong();
    if (!song) {
      setStatus("Nothing playing to download.");
      return;
    }
    if (song.localPath) {
      setStatus("That song is already a local file.");
      return;
    }
    setStatus('Downloading "' + (song.title || "Untitled") + '"…');
    const row = await window.AelloApi.downloadSong(song);
    if (row) {
      setStatus('Downloaded "' + (song.title || "Untitled") + '".');
    } else {
      setStatus(
        "Couldn't download that song — the download command is not available yet.",
      );
    }
  }

  // ---- sleep timer (Android SleepTimer.kt) -----------------------------------
  // Fade: final 60s (or half the timer when shorter) in 30 linear steps,
  // then pause. Volume restores to the user's intended volume — the fade
  // only ever scales the master gain multiplier, so restoring is setting
  // it back to 1. Mute-aware: a muted player stays muted.
  let sleepTimer = null; // { deadlineMs, totalMs, endOfSong, intervalId }

  function clearSleepTimer(restoreVolume) {
    if (sleepTimer && sleepTimer.intervalId) {
      clearInterval(sleepTimer.intervalId);
    }
    sleepTimer = null;
    if (restoreVolume !== false) {
      sleepMultiplier = 1;
      applyVolume();
    }
    onChange();
  }

  function sleepRemainingMs() {
    if (!sleepTimer || sleepTimer.endOfSong) return null;
    return Math.max(0, sleepTimer.deadlineMs - Date.now());
  }

  function isSleepEndOfSong() {
    return !!(sleepTimer && sleepTimer.endOfSong);
  }

  function isSleepActive() {
    return !!sleepTimer;
  }

  function startSleepTimer(minutes) {
    clearSleepTimer();
    if (minutes === -1) {
      // End-of-song mode: fires on the natural `ended` event.
      sleepTimer = { endOfSong: true, intervalId: null };
      setStatus("Sleep timer: pausing when this song ends.");
      onChange();
      return;
    }
    const totalMs = minutes * 60 * 1000;
    const fadeMs = Math.min(60000, totalMs / 2);
    sleepTimer = {
      deadlineMs: Date.now() + totalMs,
      totalMs,
      endOfSong: false,
      intervalId: setInterval(() => {
        if (!sleepTimer || sleepTimer.endOfSong) return;
        const remaining = sleepTimer.deadlineMs - Date.now();
        if (remaining <= 0) {
          clearSleepTimer(false);
          sleepMultiplier = 1;
          applyVolume();
          activeEl.pause();
          if (!standbyEl.paused) standbyEl.pause();
          setStatus("Sleep timer finished.");
          onChange();
          syncSnapshot();
          return;
        }
        if (remaining <= fadeMs) {
          const progress = 1 - remaining / fadeMs;
          const step = Math.min(30, Math.floor(progress * 30));
          sleepMultiplier = 1 - step / 30;
          applyVolume();
        }
      }, 500),
    };
    setStatus("Sleep timer set for " + minutes + " minutes.");
    onChange();
  }

  // ---- queue management (used by the full-player queue list) ------------------
  // Removing the currently playing item stops playback; moving an item
  // keeps the current track pointing at the same song.
  function removeAt(index) {
    if (index < 0 || index >= queue.length) return;
    const removingCurrent = index === currentIndex;
    queue.splice(index, 1);
    if (removingCurrent) {
      cancelCrossfade();
      audioA.pause();
      audioB.pause();
      audioA.removeAttribute("src");
      audioB.removeAttribute("src");
      audioA.load();
      audioB.load();
      currentIndex = -1;
      resetHistoryTimer();
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

  function clearQueue() {
    cancelCrossfade();
    audioA.pause();
    audioB.pause();
    audioA.removeAttribute("src");
    audioB.removeAttribute("src");
    audioA.load();
    audioB.load();
    queue = [];
    shuffleHistory.length = 0;
    currentIndex = -1;
    resetHistoryTimer();
    onChange();
    syncSnapshot();
  }

  // Appends a song to the end of the queue without playing it (used by
  // Listen Together suggestions and remote queue_add).
  function addToQueue(song) {
    if (!song || !song.id) return;
    if (queue.some((item) => item.id === song.id)) return;
    queue.push(song);
    onChange();
    syncSnapshot();
  }

  // Replaces the whole queue (remote sync_queue). The current index is
  // kept when the playing song survives, else -1.
  function setQueue(songs) {
    const current = currentSong();
    queue = Array.isArray(songs) ? songs.slice() : [];
    shuffleHistory.length = 0;
    currentIndex = current
      ? queue.findIndex((item) => item.id === current.id)
      : -1;
    onChange();
    syncSnapshot();
  }

  function shufflePlayQueue() {
    if (queue.length === 0) return;
    const current = currentSong();
    const rest = queue.filter((item) => item !== current);
    for (let i = rest.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [rest[i], rest[j]] = [rest[j], rest[i]];
    }
    queue = current ? [current, ...rest] : rest;
    shuffleHistory.length = 0;
    currentIndex = current ? 0 : -1;
    onChange();
    syncSnapshot();
  }

  // ---- element events ---------------------------------------------------------
  function attachAudioEvents(el) {
    el.addEventListener("timeupdate", () => {
      if (el !== activeEl) return;
      noteTimeUpdate();
      onChange();
    });

    // Duration becomes known once metadata loads (local files report it
    // here); refresh the bar so the time shows up.
    el.addEventListener("loadedmetadata", () => {
      if (el === activeEl) onChange();
    });

    el.addEventListener("ended", () => {
      if (el !== activeEl) return;
      if (isSleepEndOfSong()) {
        clearSleepTimer();
        setStatus("Sleep timer finished — paused at the end of the song.");
        onChange();
        syncSnapshot();
        return;
      }
      if (prefs.repeat === "one") {
        playAt(currentIndex);
        return;
      }
      next();
    });

    el.addEventListener("error", () => {
      if (!el.src || el !== activeEl) return;
      setStatus("Couldn't play this track: " + mediaErrorText(el.error) + ".");
      onChange();
    });
  }
  attachAudioEvents(audioA);
  attachAudioEvents(audioB);

  // Apply persisted prefs to the elements immediately.
  setSpeedOnElements();
  applyVolume();

  return {
    get queue() {
      return queue.slice();
    },
    get currentIndex() {
      return currentIndex;
    },
    // The active element — the one whose currentTime/duration the UI
    // reads. Swaps during a crossfade.
    get audio() {
      return activeEl;
    },
    currentSong,
    playSong,
    playAt,
    toggle,
    next,
    previous,
    seekTo,
    setVolume,
    getVolume,
    setMuted,
    toggleMute,
    isMuted,
    removeAt,
    moveUp,
    moveDown,
    clearQueue,
    shufflePlayQueue,
    addToQueue,
    setQueue,
    setStatus,
    onStateChange(handler) {
      onChange = handler;
    },
    // Shuffle / repeat (persisted in localStorage prefs).
    setShuffle,
    isShuffle,
    cycleRepeat,
    setRepeat,
    getRepeat,
    // Playback speed.
    setSpeed,
    cycleSpeed,
    getSpeed,
    // Crossfade + EQ, fed from settings by app.js.
    setCrossfade,
    setEqSettings,
    setLiveEqCurve,
    // The EFFECTIVE curve (live > custom profile > Sound Enhance >
    // flat) — what the graph actually plays, and what the EQ panel
    // must render so it never shows a flat curve while Enhance runs.
    activeEqCurve,
    isLiveEqCurve() {
      return liveEqCurve != null;
    },
    isPlaying() {
      return !activeEl.paused && !activeEl.ended;
    },
    applySettings(settings) {
      if (!settings || typeof settings !== "object") return;
      if (typeof settings.volume === "number") {
        userVolume = Math.min(1, Math.max(0, settings.volume));
      }
      if (typeof settings.crossfadeEnabled === "boolean") {
        crossfadeEnabled = settings.crossfadeEnabled;
      }
      if (typeof settings.crossfadeSecs === "number") {
        crossfadeSecs = Math.min(10, Math.max(1, settings.crossfadeSecs));
      }
      if (typeof settings.persistentQueue === "boolean") {
        persistentQueue = settings.persistentQueue;
      }
      setEqSettings(settings);
      applyVolume();
    },
    // Queue persistence (localStorage; see the header note).
    restoreQueue,
    saveQueueToStorage,
    // Likes / downloads.
    setLikedIds,
    isLiked,
    toggleLike,
    downloadCurrent,
    // Radio / autoplay.
    setRadio,
    isRadio,
    // 8D audio + normalization.
    set8d,
    is8d,
    setNormalization,
    isNormalization,
    // Sleep timer.
    startSleepTimer,
    clearSleepTimer,
    sleepRemainingMs,
    isSleepEndOfSong,
    isSleepActive,
  };
})();
