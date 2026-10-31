# Aello for Windows 6.1.45 — Parity Audit vs Android APK 6.1.45

- **Auditor:** parity-audit subagent (read-only; no code changed by this audit)
- **Date:** 2026-10-31
- **Exe tree audited:** `~/workspace/aello-windows-v0.2.0`, git `main` @ `612c046`
  ("fix(windows): use shipped ISystemMediaTransportControlsInterop, drop hand-defined interface")
- **APK reference:** `~/workspace/averon-6.1.45` (`version.properties` VERSION 6.1.45),
  plus the 6.1.44 inventory `~/workspace/aello-6.1.44-feature-audit.md`
  and the Phase-0 gap table `GAP_TABLE.md` (written against Android 6.1.44).
- **Method:** every GAP_TABLE row re-verified against the CURRENT exe code
  (`src/`, `src/js/`, `src-tauri/src/`), not against the table's claims.
  Where the table said DONE/TO-BUILD, this audit checked the file + function.
  Android 6.1.45 was diffed against `~/workspace/averon-6.1.44`
  (`app/src/main` only — build dirs excluded) for changes the 6.1.44-era
  table could not know about.
- **User order this audit is measured against (2026-10-31 05:50–05:52 +06):**
  "apk te sob section version ei thkbe exe file e 100% same hbe" — every
  section in the APK 100% the same in the exe; every APK feature in the exe;
  from the Home page onward, everything present; the exe must respond properly
  (fast, no lag/freezing); full motion glass UI.

## Status vocabulary

- **DONE** — implemented in the current exe tree, with file evidence cited.
- **PARTIAL** — core implemented, named sub-parts missing (those sub-parts are
  listed as MISSING with effort).
- **MISSING** — implementable on Windows, not present. Rough effort:
  S = hours–1 day, M = 1–3 days, L = 3+ days / new subsystem.
- **BLOCKED** — cannot be done on Windows/Tauri/WebView2, with the exact reason.
- **DEFERRED** — buildable but cut in 6.1.45 (carried from GAP_TABLE, re-confirmed
  absent); the "what it would take" note is included.
- **UNVERIFIED** — could not be proven from code alone (usually needs a Windows
  run / real account / live server); what is needed to verify is stated.
- **UNTESTED** — code exists but cannot be proven working on this Linux VM
  (Windows-only path, real Google account, live Listen Together room).

## Verdict up front

The 6.1.45 coordinator build is real and large: **~45 of 67 rows are DONE or
PARTIAL with working code**, including the hard parts (Daily Discover with
keep-last-good, LrcLib synced lyrics with `[offset:]`, exact Sound Enhance
curve, two-element crossfade, JSON library store, downloads, Listen Together
remote rooms, SMTC wiring, update check with the 6.1.41 error rule). The
parity pass (2026-10-31, uncommitted at the time of this audit update)
closed most of the user-visible gaps: Home now has Quick Picks / Keep
Listening / Forgotten Favorites / Speed Dial, radio/autoplay extends the
queue from related songs, Listen Together auto-joins like Android 6.1.45
(with kick/transfer/block/extend + persisted control-mode preference),
detail pages have header actions, chips actually filter, search and home
paginate, the update check has the `releases/latest` fallback + persisted
failure flag, and YouTube sync pulls liked songs, library songs, and saved
playlists additively (plus a like push at tap time). What is still missing:
the Suggestions tab (Apple Music scraper), sync of subscriptions / liked and
uploaded albums / uploaded songs (no local stores for them), lyrics
providers beyond LrcLib + YouTube subtitles, romanization/translation,
notifications, comments, and full-motion glass (polish pass). Nothing in
the audit found a fake feature — empty states are honest — but several
GAP_TABLE claims were overstated or stale (see "Corrections to GAP_TABLE"
below). **Everything built in the parity pass is code-verified only —
unproven on Windows until his PC test** (see §16).

## Top 10 gaps by user impact (what the user notices first)

1. **Home personal sections — DONE (parity pass 2026-10-31).** Quick Picks /
   Keep Listening / Forgotten Favorites / Speed Dial now render on Home from
   the JSON library store (`personal_sections`, `library.rs`; `renderHome`,
   `views.js`). A section renders only when it has real items. (Was: MISSING, M.)
2. **Radio / autoplay — DONE (parity pass).** When the queue ends, the player
   extends it from related songs (`related_for_video` via `get_related`) and
   keeps playing; a Radio toggle (full player + queue panel, default ON)
   controls it (`player.js`). (Was: MISSING, S–M.)
3. **Listen Together auto-join — DONE (parity pass).** The host now approves
   every join request immediately, exactly like Android 6.1.45; blocked
   users (persisted local list, `aello.lt.blocked.v1`) are rejected with the
   reason. Kick / transfer-host / block UI, host Extend button, and the
   persisted control-mode preference are also in. (Was: MISSING, S.)
4. **YouTube sync — PARTIAL (parity pass).** Liked songs, library songs, and
   saved playlists now sync additively on demand and once after sign-in;
   likes also push up at tap time. Subscriptions, liked/uploaded albums,
   and uploaded songs do NOT sync yet — no local stores exist for them
   (see #45). (Was: MISSING, M–L.)
5. **No Suggestions tab (#11).** Search has tabs + history but no Apple-Music
   trending Suggestions tab; nothing in the exe scrapes Apple Music.
   DEFERRED, M (new scraper).
6. **Playlist / album detail pages — DONE (parity pass).** `renderDetail`
   now has Play all / Shuffle / Like all / Download all / Copy link header
   actions. (Was: MISSING, S.)
7. **Speed-dial pinning — DONE (parity pass).** Right-click a song card (or
   the pin button in the full player) pins/unpins it; 27-cap enforced with
   an honest error; Speed Dial row renders on Home. (Was: MISSING, S.)
8. **Sign-in: channel picker + auto cookie capture — DONE (sign-in agent,
   commit 2a1454d).** `select#account-channel` is populated by
   `refreshSigninStatus` / `setAccountChannels` (`app.js`, `views.js`);
   channel switch via `signinChooseChannel`; automatic cookie capture from
   the WebView2 cookie store (no paste form). PARTIAL → DONE.
9. **Update check 6.1.45 parity — DONE (parity pass).** `releases/latest`
   redirect fallback added (`update.rs`, `check_via_latest_page`) plus the
   persisted failure flag (`last_update_check_failed`) so Settings shows
   the failed-check state instead of looking up-to-date. (Was: MISSING, S.)
10. **"Full motion glass UI" is only half met (#49).** Static glass IS present
    (backdrop blur, see #49) but there is no animated/motion glass — no
    animated gradients, blobs, or motion treatment; CSS has only 8
    `transition`/`animation` occurrences. The APK-side glass kill-switch does
    not apply, but the user's "full motion glass" order asks for more than
    static blur. PARTIAL, M — polish agent's pass, not this one.

---

## 1. Core catalogue & browsing (#1–11)

### #1 Home feed — DONE (core + chips / Play-all / load-more, parity pass)
- Rust `get_home_feed` (`src-tauri/src/lib.rs:83`) → `MusicService::home`;
  rendered by `renderHome` (`src/js/views.js:136-164`).
- **DONE in the parity pass (2026-10-31):** home chips row — chips come
  from the server (`HomeFeed.chips`, parsed from the section-list header in
  `home_feed`, `innertube.rs`) and a tap re-fetches the feed WITH that
  chip's params; "All" re-fetches unfiltered (the Android chips bug — chips
  that don't filter — is avoided by construction: the refetch IS the
  filter). Per-section Play-all buttons (`sectionHtml`, `data-play-section`).
  Home load-more (`get_home_feed_more`, `home_feed_more`, section-list
  continuation). Daily Discover + personal sections stay stable across chip
  changes (only the feed refetches).
- **Still MISSING (S):** home-section reorder / randomize
  (`HomeFeedOrderScreen.kt` in APK). Video rows: feed video items render as
  generic cards — no dedicated video-row UI.

### #2 Daily Discover — DONE
- `get_daily_discover` (`src-tauri/src/lib.rs:145-210`): seeds from
  liked ∪ recent ∪ most-played (`library.seed_ids()`, truncated to 3),
  `related_for_video` per seed, 6/seed, seed excluded, deduped;
  keep-last-good (`set_last_daily_discover` / `last_daily_discover`);
  no-previous-list fallback = `search("top songs", song)`, shuffled, 25.
- Pinned first by `renderHome` (`views.js:143-150`); loaded in parallel with
  the feed (`app.js:110-117`).
- **Parallelized in the parity pass (R1):** the 3 seeds are now fetched with
  `tokio::join!` over `related_or_empty()` (`lib.rs`), so Daily Discover no
  longer waits on 6 sequential round trips.

### #3 Quick Picks / Keep Listening / Forgotten Favorites / Speed Dial — DONE (parity pass 2026-10-31)
- `personal_sections()` (`library.rs`): Quick Picks (liked ∪ recent, 12),
  Keep Listening (history recency, 12), Forgotten Favorites (liked sorted by
  last play ascending, never-played first, 12), Speed Dial (pinned ids
  resolved in pin order). Served by `get_home_personal` (`lib.rs`); rendered
  by `renderHome` after Daily Discover, before the feed — a section renders
  only when it has real items (`views.js`). No fake data.
- **UNTESTED on Windows** (PC test: sections appear with real content on a
  library that has likes + history).

### #4 Mood & Genres (explore page) — DONE
- `get_explore` + `browse_mood` (`lib.rs:117-123`); `renderMoods`
  (`views.js:252-271`); mood tap → `browseMood(params)` → results view
  (`app.js`, moods section). Covers #10 as well.

### #5 Search — DONE (core); continuations + local search MISSING (S–M)
- Tabs All/Songs/Albums/Artists/Playlists (`src/index.html:79-85`);
  filter mapping incl. plurals (`api.js:78-91`); Rust `search`
  (`innertube.rs:251`, `search_filter_params` at `:89`).
- History: localStorage, cap 20, clearable (`app.js:122-147`,
  `SEARCH_HISTORY_KEY = "aello.searchHistory.v1"`); history click re-runs.
- URL paste: `youtubeIdFromUrl` (`app.js:153-157`) handles
  `watch?v=`, `embed/`, `shorts/`, `youtu.be` → plays the video directly
  (`app.js:165-173`). Playlist URLs (`list=`) now open the playlist detail
  (`youtubeListIdFromUrl`, parity pass); channel URLs fall through to a
  text search.
- **DONE in the parity pass:** search continuations — Rust `search()` now
  returns `SearchPage { items, continuation }` (first shelf continuation;
  unfiltered takes the first shelf's), `search_more()` follows
  `musicShelfContinuation`; the UI has a Load more button
  (`data-search-more`, `renderSearchView`/`appendSearchResults`).
- **Still MISSING:** local search (APK `LocalSearchScreen`), video filter
  tab, and search suggestions/autocomplete (APK has none inline either
  — its "Suggestions" is the separate tab, #11 — so this is parity, not a gap).

### #6 Trending / Charts — DONE (core); load-more MISSING (S)
- `get_trending` (`lib.rs:112`); `renderTrending` (`views.js:274`);
  "Top music videos" kept as its own section per `innertube.rs:517` comment.
  Trending now shows a skeleton while loading (R4, parity pass).
- **MISSING:** continuation / load-more — `charts()` (`innertube.rs:519`)
  uses `FEmusic_charts` + params with no continuation handling, so there is
  nothing to page with yet. No pull-to-refresh equivalent (desktop: no
  refresh button on the view). New Releases screen (APK `NewReleaseScreen`)
  is not a separate exe view.

### #7 Artist page — DONE (core); subscribe DONE (parity pass)
- `get_artist` (`lib.rs:107`); `renderArtist` (`views.js:298-386`) with top
  songs, albums, singles, videos, related (`artist-related`, `artist-albums`
  hosts); artist shuffle exists (`app.js:279-287`).
- **DONE in the parity pass:** subscribe/unsubscribe — the header button now
  calls `subscribe_artist` (`subscription/subscribe|unsubscribe`,
  `SubscribeBody`-equivalent `{context, channelIds:[id]}`, `session.rs`),
  with an in-memory subscribed toggle; when not signed in the Rust command
  errors and the UI keeps the honest sign-in toast. Artist radio is part of
  #28 (the global Radio toggle extends from any playing track).
  Artist background video / canvas is part of #29 (absent).

### #8 Album page — DONE (core + header actions, parity pass)
- `get_album` (`lib.rs:102`); Rust follows album continuations (cap 10,
  `innertube.rs:1045`).
- **DONE in the parity pass:** `renderDetail` now has Play all / Shuffle /
  Like all / Download all / Copy link header actions (`data-detail-action`,
  `views.js` + `handleDetailAction`, `app.js`). Like-all toggles: likes every
  unliked song, or unlikes all when all are already liked. Copy link uses
  `music.youtube.com/browse/<id>` for albums, `playlist?list=<id>` for
  playlists.

### #9 Playlist detail — PARTIAL
- Online: `get_playlist` (`lib.rs:97`); Rust follows shelf continuations
  (cap 10, `innertube.rs:893-897`) eagerly, so long playlists do load fully.
- Local CRUD: `create/rename/delete_playlist`, `playlist_add/remove_song`
  (`lib.rs`, `library.rs`); Library "playlists" segment renders auto-playlist
  cards + user playlists with rename/delete (`views.js:453-530`);
  auto-playlist click handled (`app.js:459`, `[data-auto-playlist]`).
- Auto-playlists present: Liked, Downloaded, "My top 50" (from stats),
  Local (`views.js:474-497`).
- **DONE in the parity pass:** header actions (play-all/shuffle/like-all/
  download-all/copy-link) — shared `renderDetail`, see #8. Playlist-URL
  paste opens the detail directly (see #5).
- **Still MISSING:** Cached / Uploaded auto-playlist cards are not rendered —
  correct per GAP_TABLE (they depend on the stream cache, #39); the colour
  map still lists them (`views.js:33-34`, dead entries). Top (charts)
  playlist variant has no dedicated view.

### #10 Moods & moments detail — DONE
- Via `browse_mood` (see #4). Same endpoint family as APK
  (`MoodAndGenresViewModel.refresh()` → InnerTube browse).

### #11 Suggestions tab — MISSING / DEFERRED (M)
- No Suggestions tab, no Apple Music scraper, no region sheet in the exe.
  GAP_TABLE deferred it ("separate scraper; cut for this pass") — confirmed
  still absent, and the UI does not promise it (no fake data). Build = new
  Apple-Music scrape (APK `AppleMusicScraper.kt` + `SuggestionsViewModel`) or
  an equivalent trending source, plus the tab UI.

---

## 2. Library (#12–18)

### #12 Library persistence — DONE
- JSON store in the app-data dir (`src-tauri/src/library.rs:1-17`):
  `library.json` (liked, playlists, speed-dial ids, last Daily Discover),
  `history.json` (cap 500, newest first), `downloads.json`. No SQLite —
  matches the settings-store pattern as the table specified.
- Play counts after 30s: `add_to_history` (`lib.rs`, `library.rs:209-226`),
  called by the player after 30s of playback (`api.js`, `addToHistory`).

### #13 Liked auto-playlist + like sync — PARTIAL (like push DONE, parity pass)
- Local like/unlike: `like_song` (`lib.rs:340-370`), `isLiked`/heart UI,
  Liked auto-playlist card with count (`views.js:474-480`). DONE local.
- **DONE in the parity pass:** the like now also pushes to YouTube at tap
  time when signed in — `like_video_raw` (`like/like` or `like/removelike`,
  `{context, target:{videoId}}`, no retry on a mutation, `session.rs`),
  spawned best-effort from `like_song`; a failed push never blocks the
  local like. Full library sync is #45.

### #14 Local files — DONE (core); persisted index PARTIAL (parity pass)
- Picker: `pick_local_files` (`src-tauri/src/local_files.rs`, dialog plugin);
  `openLocalFiles` (`app.js:519-558`); playback via asset protocol
  (`api.localFileUrl`, `app.js:1959-1961`).
- **DONE in the parity pass:** picked file paths now persist across restarts
  (localStorage `aello.localFiles.v1`, restored on startup before the first
  Library render). A file moved/deleted since then fails honestly at play
  time. The UI copy was corrected ("remembered on this computer").
- **Still MISSING:** folder allow-list and local scan (`LocalAudioScanner`) —
  picking is still manual, file by file. DEFERRED, S–M.

### #15 Recents / History + clear — DONE
- History segment renders from `library.history`
  (`views.js`, `LIBRARY_EMPTY.history`: "Songs you play for over 30 seconds
  show up here"); `clear_history` command exists (`lib.rs`, `api.clearHistory`).

### #16 Stats screen — DONE (core + artists + Wrapped, parity pass)
- `get_stats` → `most_played(20)` (`library.rs:240-259`); "My top 50" card
  (`views.js:486-492`).
- **DONE in the parity pass:** the history segment now shows a Wrapped block
  (total plays, distinct songs, top song, top artist) and a Top artists list
  (top 5 by plays, `views.js` history segment). History rows render chunked
  (100/chunk, R2).
- **Still MISSING (S):** albums breakdowns (APK `StatsScreen` shows
  songs/artists/albums + history-duration stats); the most-played songs list
  itself is DONE.

### #17 Backup / restore + CSV import — DONE (parity pass 2026-10-31)
- Export backup: `export_library_json` (`library.rs`,
  format `aello-library-backup-v1`: likedSongs, playlists, speedDialIds,
  speedDialSongs, librarySongs, history) → JSON file download from the
  Library toolbar. Import backup: `import_library_json` validates the
  format and rejects empty/foreign files. CSV import: `import_csv_playlist`
  (`videoId,title,artist,…` per line, header skipped, invalid 11-char ids
  skipped, BOM stripped) → new playlist named by the user (filename
  suggested). Downloads are NOT in the backup (files stay on disk).
- **UNTESTED on Windows** (PC test: export → delete a playlist → import →
  playlist returns).

### #18 Speed-dial pinning (max 27) — DONE (parity pass 2026-10-31)
- Pin/unpin: right-click a song card anywhere (Home/search/trending hosts)
  or the pin button in the full player (`#fp-pin`, aria-pressed synced from
  `libraryCache.speedDialIds`). `toggle_speed_dial` (`library.rs`) enforces
  the 27-cap with the honest error "Speed Dial is full (27 songs max) —
  unpin one first." Speed Dial row renders on Home via #3.

---

## 3. Player (#19–32)

### #19 Play/pause/seek/next/prev/volume/mute — DONE
- `src/js/player.js` (1,130 lines): `audioA`/`audioB`, play/pause, seek,
  `previous()` with the 3s restart rule (`player.js:621-645`, mirrors APK),
  volume + mute, status snapshot for SMTC.

### #20 Queue (reorder/remove, persistent) — DONE
- Full queue saved to localStorage (`QUEUE_KEY = "aello.queue.v1"`,
  `player.js:43-44,337-363`) and restored on restart; Rust snapshot carries
  ids for SMTC. Reorder/remove/clear (`player.js:898-958`); queue panel in
  the full player; `shufflePlayQueue` (`app.js:741`).

### #21 Shuffle / repeat OFF→ALL→ONE (persisted) — DONE
- `prefs` in localStorage (`player.js:43-57`); cycle
  off→all→one (`player.js:697-719`); shuffle walk-back history
  (`shuffleHistory`, `player.js:585-635`).

### #22 Auto-advance — DONE
- `ended` handler (`player.js:1010`) → `next()`; verified against repeat
  modes (repeat-one replays, repeat-all wraps, off stops at the last track).

### #23 Crossfade 5s default + gapless — DONE (with a gapless caveat)
- Two-`<audio>`-element crossfade with gain ramps
  (`player.js:416-479`, `startWithCrossfade`); default ON, 5s
  (`crossfadeEnabled = true`, `crossfadeSecs = 5`, `player.js:60-61`);
  duration slider 1–10s in Settings; applied to both elements
  (`playbackRate` set on both, `:412-413`).
- **Caveat:** `setCrossfade` clamps seconds to min 1 (`player.js:745-748`),
  so a true 0s "gapless" path is not reachable from the UI; crossfade OFF is
  a hard cut, not a pre-buffered gapless handoff. True gapless (preload next
  track) is MISSING, S.

### #24 Playback speed — DONE
- `SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2]` (`player.js:723`),
  `playbackRate` on both elements, persisted in prefs, cycles on click.

### #25 Audio quality AUTO/HIGH/LOW — DONE
- `select_aac_format` (`src-tauri/src/innertube/resolve.rs:55-92`): LOW →
  itag 139 (AAC ~48kbps), AUTO/HIGH → itag 140 (AAC ~128kbps), with
  bitrate fallbacks; Opus never selected (WebView2 cannot play Opus).
  Setting wired in Settings (`setting-quality`) and in `resolve_stream`.

### #26 Skip silence — BLOCKED
- No Web Audio equivalent without heavy DSP; the GAP_TABLE reason stands
  (Analyser-based gating is unreliable and would clip intros). No code
  attempts it. Correctly absent, not faked.

### #27 Cast (Google Cast) — BLOCKED
- No Cast SDK exists for Tauri/WebView2; Android has it GMS-only. Zero Cast
  references in the exe. About should state this (see #67 — it does not
  currently mention Cast; minor doc gap).

### #28 Radio / autoplay (related queue) — DONE (parity pass 2026-10-31)
- `related_for_video` (`innertube.rs:1114-1120`) is now wired to the player:
  `get_related` (`lib.rs`) + `api.getRelated`; the player prefetches related
  songs when ≤2 tracks remain (`maybePrefetchRadio`, `radioPrefetchFor`
  guard) and extends the queue at the end (`extendRadio` → appends up to 8
  related for the current track, then plays the next). `next()` at the last
  track with repeat off calls `extendRadio()` instead of pausing; it pauses
  with the honest status "Radio: no more related songs." only when no
  related songs come back. Radio toggle in the full player (`#fp-radio`)
  and queue panel (`#queue-radio`), default ON, aria-pressed synced in
  `refreshNowPlaying`. Matches APK `startRadioSeamlessly` behaviour
  (extend queue, keep playing) using the same related endpoint.
- **UNTESTED on Windows** (PC test: play a 1-song queue to the end — a
  related song must start, not silence).

### #29 Canvas / background video artwork — MISSING / DEFERRED (M)
- No canvas source, no background video element, no `show_album_canvas` /
  artist-video setting. WebView2 can play mp4 canvas; needs the canvas
  source endpoints (APK `CanvasArtworkPlayer.kt`, Apple/Tidal/Echo/ViviMusic
  sources). Cut for this pass per GAP_TABLE; still absent.

### #30 8D audio — DONE (parity pass 2026-10-31)
- StereoPannerNode LFO in the player graph (`player.js`): 0.07 Hz
  oscillator → depth gain 0.85 → `panner.pan`, toggled by the "8D audio"
  checkbox in the EQ panel (`#eq-8d`, `set8d`/`is8d`, synced by
  `renderEqPanel`). No depth slider (fixed depth) — a deliberate
  simplification, stated here, not a hidden gap.

### #31 DJ / Auto-DJ mixing — BLOCKED
- Beat tracking + key detection DSP is a project of its own; no Rust/JS
  equivalent in-tree. Correctly absent.

### #32 Audio normalization — DONE as a compressor approximation (parity pass 2026-10-31)
- A `DynamicsCompressorNode` now sits in the graph between the preamp and
  the master gain (`player.js`): pass-through (threshold 0 / ratio 1) when
  off, gentle -18 dB threshold / ratio 4 / knee 12 when on ("Normalize
  volume" checkbox, `#eq-normalize`, `setNormalization`/`isNormalization`).
- **Honest limit:** this is NOT Android `LoudnessEnhancer` parity — no
  per-track loudness metadata is fetched by the Rust client, so there is no
  per-track gain. The UI copy says "gentle compressor", never "loudness".
  True per-track loudness normalization is still MISSING (needs loudness
  data in the resolve path).

---

## 4. Lyrics (#33–35)

### #33 Synced lyrics — DONE (core)
- Rust `get_lyrics` (`src-tauri/src/lyrics.rs`, 399 lines): LrcLib first,
  4 strategies (cleaned title+artist → title only → `q=artist title` →
  title query), title cleaning incl. `(Official Video/Lyrics/Remaster/…)`,
  primary-artist reduction, best-match by duration (±5s relaxed);
  `[offset:]` applied to every timestamp before returning; YouTube
  timed-text subtitle fallback; overall timeout 6s.
- Frontend (`src/js/views.js:821-…`): LRC parse incl. `[offset:]`,
  binary-search current line (+300ms, same as APK `findCurrentLineIndex`),
  100ms highlight poll (`app.js:784-790`), provider header row-0 with the
  `listIndexForLine` +1 fix, tap-to-seek, per-song offset persisted
  (`LYRICS_OFFSET_KEY = "aello.lyricsOffset.v1"`, `views.js:885-900`),
  auto-scroll with a scroll-to-current button, offset input
  (`app.js:778-781`).
- **UNVERIFIED from this read:** rich-sync (karaoke) word timestamps and the
  alternate `<…>` word-timestamp line format — the parser I read covers
  standard LRC; word-level highlighting code was not found. Needs a
  targeted grep or a rich-sync test lyric to confirm/deny.

### #34 Lyrics 9-provider race — MISSING / DEFERRED (M)
- Only 2 sources: LrcLib + YouTube subtitles (`lyrics.rs:7-8` states this
  explicitly). Missing: YouLyPlus, PaxSenix, Musixmatch, BetterLyrics,
  SimpMusic, KuGou, YouTube Music lyrics endpoint. The Rust command shape
  (`get_lyrics`) can host more providers behind it later.

### #35 Lyrics styles / romanization / AI translation — PARTIAL (styles DONE, parity pass)
- **DONE in the parity pass:** a lyrics-style picker (`#lyrics-style`,
  Default / Fade / Glow, persisted `aello.lyricsStyle.v1`,
  `getLyricsStyle`/`applyLyricsStyle`, `views.js`; classes
  `lyrics-style-fade`/`lyrics-style-glow` on `#lyrics-lines`, static CSS
  only — no motion, per the polish-pass boundary).
- **Still MISSING / DEFERRED (M–L):** the rest of the APK style list
  (Slide/Karaoke/Apple/ViviMusic/Metro…), text-size/position settings,
  romanization (Kuromoji/Pinyin equivalents absent), and AI translation
  (no OpenRouter/DeepL key field in Settings). Translation is gated on a
  user-provided key by design.

---

## 5. EQ / Sound Enhance (#36–38)

### #36 Sound Enhance (exact curve) — DONE
- `ENHANCE_BANDS` (`src/js/player.js:82-88`): 80 Hz lowshelf +3.0 Q0.7;
  300 Hz peaking +1.0 Q1.0; 3000 Hz peaking +2.0 Q1.0; 8000 Hz highshelf
  +1.5 Q0.7; `ENHANCE_PREAMP_DB = -3.0` (`:88`). Matches APK
  `SoundEnhanceProfile` exactly. Default ON (`soundEnhanceEnabled = true`,
  `:62`; settings default `true`, `settings.rs:98`).
- Chain: `MediaElementSource → 5× BiquadFilter → preamp GainNode → master
  GainNode → destination` (`player.js:96-140`); falls back to plain element
  (hard cuts, no EQ) if Web Audio is unavailable — never silence.

### #37 Custom EQ profiles + APO import — PARTIAL
- Custom profiles: CRUD in the settings store, 5-band panel, precedence
  custom > Enhance > flat, never stacked (`player.js:154-213`);
  "No Equalization" stores null profile AND switches Enhance off
  (`app.js:800-808`, mirroring APK `AxionEqViewModel`).
- **DONE in the parity pass:** APO/Peace text import — `parseApo`
  (`views.js`) reads `Preamp:` + `Filter N: ON PK|LS|HS Fc … Gain … Q …`
  lines and applies them to the panel (`applyEqCurveToPanel`); the EQ panel
  has an "Import APO file" button + hidden file input (`#eq-import-apo`,
  `#eq-apo-file`). Max 5 bands — extras are reported, not silently dropped
  ("N filter(s) skipped (5-band limit)").
- **Still MISSING:** unlimited bands — the panel has 5 bands and no Q input
  (`views.js:1114` comment); unlimited-band support needs a dynamic graph
  rebuild, S–M.

### #38 EQ presets UI — DONE (desktop simplification)
- One clean EQ panel (`renderEqPanel`, `views.js:1164`), presets list
  (Flat, Bass Boost, Treble Boost, Vocal Boost, Sound Enhance —
  `EQ_PRESETS`, `views.js:1074-1142`), custom save via prompt
  (`app.js:832-850`), delete (`app.js:852-867`). APK has two UIs
  (classic + Axion); the exe's single panel is a legitimate desktop
  simplification, noted here, not a gap.

---

## 6. Downloads / offline (#39–41)

### #39 Song downloads + stream cache — PARTIAL
- Downloads DONE: `download_song` (`lib.rs:289`, `download_song_inner`
  `:236-287`) resolves the stream at the chosen quality, writes
  `downloads/<videoId>.m4a`, skips re-download if the file exists, records
  the row in `downloads.json`; offline playback via the asset protocol
  (`app.js:1959-1961`); Downloaded auto-playlist; `get_downloads` /
  `remove_download`; download-dir display in Settings.
- **CORRECTED in the parity pass (see §15.2):** the GAP_TABLE / audit claim
  that only zero-byte files are rejected was WRONG for this tree.
  `download_to_file_inner` (`session.rs:382,408`) already compares the
  downloaded byte total against the response `Content-Length` and errors on
  mismatch, and `lib.rs:271-276` is a second zero-byte guard. Download
  integrity vs expected size is DONE — no code change was needed.
- **MISSING:** stream cache — no LRU player cache (1024 MB) exists; the
  Cached/Uploaded auto-playlist cards are therefore not rendered, exactly as
  the GAP_TABLE notes say. DEFERRED, M.

### #40 Auto-download on like — DONE
- `like_song` spawns a best-effort background `download_song_inner` when
  the `autoDownloadOnLike` setting is on (`lib.rs:352-368`); setting checkbox
  in Settings (`views.js:727`, `api.js:44` default `false`).

### #41 Song export — DONE (parity pass 2026-10-31)
- `export_song` (`lib.rs`): blocking folder picker
  (`tauri_plugin_dialog::DialogExt::blocking_pick_folder`) → copies the
  downloaded file to the chosen folder, returns the destination path (or
  null on cancel). Wired to the Export button on every downloads row
  (`data-download-action="export"`, `views.js` + `app.js`). (Export also
  unblocks #61 Share's file-export path.)

---

## 7. Listen Together (#42–43)

### #42 Listen Together (remote rooms) — PARTIAL (core DONE; 6.1.45 deltas DONE in parity pass; guest wiring code-verified, live-untested)
Implemented and verified in `src/js/app.js` (~700 lines, `:1065-1870`):
- WS client, default servers Global
  `wss://devilmi-vivi-music-listen-together.hf.space` + USA
  `wss://vivimusic-listen-together.onrender.com` (`app.js:1065-1066`),
  custom-server setting; room create via Rust `lt_create_room`
  (POST `/api/rooms`, `lib.rs:547-571`).
- Join/approve/reject UI (`app.js:1364-1377`, `1813-1822`); `control_mode`
  owner/everyone select (`index.html:163`, `app.js:1408-1412,1785`) with
  per-track `suggested_by` ownership ("only they or the host can skip it",
  `app.js:1129-1142`); buffer gate — load paused, send `buffer_ready` on
  buffered (`app.js:1629-1649`); chat (`:1438`, `:1787-1795`); suggestions
  with approve/reject (`:1442-1478`, `:1835-1843`); reconnect 15 attempts,
  exp backoff capped at 120s (`:1298-1316`); session token persisted and
  sent on open (`:1267-1281,1336,1346`); `host_changed` handling
  (`:1398-…`); guest muted on join + mute restored on leave
  (`:1350-1354`, `:1746`); `room_expiring` / `room_closed` / `kicked`
  received (`:1483-1496`); ping every 25s (`:1266`).
- **DONE in the parity pass (2026-10-31) — the 6.1.45 deltas:**
  1. **Auto-join.** The host now approves every join request immediately
     (`join_request` case, `app.js`); blocked users are rejected with the
     reason "You are blocked". The pending list is kept only for requests
     that arrive while this client is not the host.
  2. **Control-mode preference persisted.** `lt_control_mode` setting
     (default `"owner"`, `settings.rs`); on `room_created` the host
     re-applies the stored mode (`set_control_mode`); on
     `control_mode_changed` (host) the choice is saved via
     `saveSettingsQuietly()`; the select change handler also saves it.
  3. **Kick / transfer-host / block UI.** Host sees Kick / Make host /
     Block buttons on other members (`renderLtMembers`, `views.js`;
     handlers in `app.js` send `kick_user` / `transfer_host` with a
     confirm dialog / `kick_user` + local block). Blocked usernames persist
     in localStorage (`aello.lt.blocked.v1`) — the Android
     `ListenTogetherBlockedUsersKey` equivalent.
  4. **`extend_session` send.** Host-only Extend button (`#lt-extend`,
     `showLtRoom` toggles visibility); `room_expiring` tells the host to
     extend and guests to ask the host.
  5. **Guest control wiring — CODE-VERIFIED, LIVE-UNTESTED.** Traced
     end-to-end in this pass: guest play/pause (`togglePlayback` sends
     `play`/`pause`), seek (debounced `seek` send), and track change
     (`refreshNowPlaying` sends `change_track`) all go through
     `sendPlaybackAction`, which gates on `canControlPlayback()`; next/prev
     are LT-gated (`ltGuardedNext`/`ltGuardedPrev`). This matches the
     Android 6.1.45 `wirePlayerControls` fix in code. A live two-client
     room test is still required — the VM cannot run one.
  6. Blocked-users list: DONE (see 3). Join-request desktop notifications:
     absent (notifications are deferred, #55; Android 6.1.45 removed that
     path anyway via auto-join).
- Transport note: the exe is JSON-only over the socket. That matches the
  Cloudflare server (JSON-only, must stay so); the Android protobuf+gzip
  flip only matters for non-Cloudflare servers. Not a gap for the default
  servers. **UNTESTED live** (no real room was joined from this VM).

### #43 Listen Together LAN mode — BLOCKED
- Raw TCP + NSD `_pulsewave-sync._tcp` (`AelloOffline`); no NSD/mDNS
  discovery in Tauri without a plugin, and the raw-TCP framing would need a
  Rust transport. Reason stands; correctly absent.

---

## 8. Account (#44–46)

> Note: a separate agent is implementing automatic cookie capture right now.
> This section audits the CURRENT tree only and does not duplicate that work.

### #44 Google sign-in (WebView cookie harvest, no OAuth) — DONE (sign-in agent, commit 2a1454d)
- `signin_start` returns the exact Android URL
  (`https://accounts.google.com/ServiceLogin?continue=https%3A%2F%2Fmusic.youtube.com`,
  `lib.rs:597-602`); the exe opens a Tauri `WebviewWindow("aello-signin")`
  (`app.js:965-1041`).
- **DONE by the sign-in agent:** automatic cookie capture — Rust polls the
  WebView2 cookie store (including the HttpOnly SAPISID that
  `document.cookie` can never see) and validates the moment SAPISID appears;
  there is no paste form. `signin_finish` validates the cookie via InnerTube
  `account_menu`, stores cookie + visitorData in the **Rust** settings store,
  never in JS storage (`lib.rs:608-649`); a blank/invalid cookie is rejected
  and the stale cookie is cleared (`:633-648`); the cookie is restored into
  the session on startup (`lib.rs:687-691`); `signin_status` reports
  signed-in state + account name.
- **DONE by the sign-in agent:** channel picker — `select#account-channel`
  is populated by `refreshSigninStatus` / `setAccountChannels`
  (`app.js:1615-…`, `views.js:1221`); channel switch via
  `signinChooseChannel` (`app.js:1599-1610`).
- Guest mode stays fully working throughout.
- **UNTESTED** on a real Google account (VM cannot test; never use the
  user's credentials here). His PC is the verdict. UA hardening (strip `wv`,
  suppress `X-Requested-With`) is in the sign-in agent's commit — see
  `SIGNIN_USER_AGENT` (`app.js`).

### #45 YT two-way sync (liked/library/playlists/subscriptions) — PARTIAL (parity pass 2026-10-31)
- **DONE:** `sync_youtube` (`lib.rs`) — ADDITIVE ONLY by design: remote
  merges into the local library; nothing local is deleted; nothing is
  uploaded except likes tapped in the app (see #13). Steps, each with its
  own error collected while the others still run:
  1. Liked songs: `YouTube.playlist("LM")` → `add_liked_songs`.
  2. Library songs: `library_page("FEmusic_liked_videos")` + continuations
     (cap 3) → `add_library_songs` → Library "From YouTube" segment
     (`library_songs`, `LibraryData`).
  3. Saved playlists: `library_page("FEmusic_liked_playlists")` skipping
     `LM`/`SE`, cap 10 playlists/run → `import_synced_playlist` (dedupes by
     browse_id) with songs via `playlist()`.
  Library browse uses `library_page(browse_id, tab_index)` +
  `library_page_more()` (`innertube.rs`), reading grid two-row or shelf
  list items — the Android `YouTube.library()` shape. UI: "Sync with
  YouTube now" button in Settings → Account (`#account-sync`,
  `#account-sync-status` with counts/errors) + auto-sync once after a fresh
  sign-in (best-effort, `pollSigninCapture` signed-in branch).
- **NOT synced (no local stores exist — see below):** subscriptions
  (`FEmusic_library_corpus_artists`), liked albums
  (`FEmusic_liked_albums`), uploaded albums
  (`FEmusic_library_privately_owned_releases`), uploaded songs
  (`FEmusic_library_privately_owned_tracks`, tabIndex 1). What each would
  take: a subscribed-artists store + artists UI for subscriptions; an
  albums store + Library albums segment for liked/uploaded albums; an
  uploads store + tabIndex live test for uploaded songs. Two-way conflict
  policy is additive-only by design — local deletions are never pushed
  (except like taps), so a "delete on YouTube" direction does not exist.
- **UNTESTED on Windows with a real signed-in account** (PC test: sign in
  → sync → liked songs, library songs, and saved playlists appear;
  re-sync adds nothing twice).

### #46 Sign-out / channel switch — DONE (sign-in agent, commit 2a1454d)
- Sign-out DONE: clears cookie + visitorData in session and store
  (`lib.rs:663-676`).
- Channel switch DONE: picker populated + `signinChooseChannel` (see #44).

---

## 9. Themes / appearance (#47–49)

### #47 Per-entity page color — DONE
- JS canvas dominant-color extraction (`views.js:1551-1700`): dominant swatch
  first, then vibrant variants, population-weighted; vividness **×1.12 when
  vibrant / ×1.0 otherwise** (`views.js:1557,1668`) — the corrected factor,
  not the audit §13's unused 1.4 default; achromatic fallback `#1C1C1E`;
  cached (128-entry cap); applied as the page's uniform color with text
  pushed to readable extremes. Matches the GAP_TABLE correction.

### #48 Themes — PARTIAL
- Dark / light / system (`setting-appearance`, `index.html`); 4 accents
  (Aello Red default, violet, teal, amber). DONE for the desktop subset.
- **MISSING vs APK:** custom theme colours, fonts (System/Google Sans/Sans
  Flex/Outfit/Plus Jakarta Sans), display density, grid cell size, DIY
  theming / player themes / presets, canvas selection. M (themes+fonts) to L
  (DIY editor). The APK's Material 3 dynamic colour has no desktop
  equivalent — correctly not attempted.

### #49 Liquid glass effects — PARTIAL (see top-10 #10)
- **Correction to GAP_TABLE:** the table says "no backdrop-blur effect chain
  on desktop (WebView2 has no equivalent). Use solid surfaces + subtle
  borders." That is stale — the exe **does** use backdrop blur:
  `--glass` / `--glass-elevated` / `--glass-border` vars
  (`styles.css:18-19,55-57`); `backdrop-filter: blur(16px)` on the sidebar /
  player surfaces (`:254,293,391`); `blur(24px) saturate(1.4)` on dialogs /
  panels (`:822,890`); `blur(4px)` on modal backdrops (`:951`). The Android
  glass kill-switch does not apply (CSS glass, no effect chain) — correct.
- **MISSING for "full motion glass ui":** the glass is static. No animated
  gradients, blobs, shimmer, or motion treatment; `transition`/`animation`
  occur only 8 times in `styles.css`. Reduced-motion is respected
  (`@media (prefers-reduced-motion: reduce)`, `:985`). Animated glass is
  M, and is the most visible interpretation gap against the user's order.

---

## 10. Settings / system (#50–61)

### #50 Settings screens — PARTIAL
- One-page Settings form (`index.html:232-318`): Account, Accent colour,
  Appearance (dark/light/system), Playback (quality, volume, crossfade +
  duration, persistent queue, auto-download-on-like, update check),
  Listen Together (server Global/USA/Custom), Updates (check now).
- **DONE in the parity pass:** Storage management — "Clear all downloads"
  (`#settings-clear-downloads`, `clear_downloads`, removes rows + files,
  returns count) in a new Storage fieldset; YouTube sync button + status in
  the Account fieldset (see #45).
- **MISSING vs APK (M, as one settings pass):** Content filters (hide
  explicit, hide video songs, hide YT Shorts, data saver, local-only mode,
  content language/country); Library settings; EQ lives in the player panel,
  not Settings; searchable settings index; home-feed order screen; About is
  a separate nav view (fine). No setting promises a feature that does not
  exist — the form is honest about its scope.

### #51 In-app Windows update check — DONE (core + 6.1.45 fallback + flag, parity pass)
- `check_for_update` (`src-tauri/src/update.rs`): GET GitHub releases list,
  latest tag `^v*` carrying an `Aello_*_x64-setup.exe` asset, drafts and
  prereleases skipped, strictly-newer compare, notes from release body
  (truncated 2000 chars); on ANY fetch failure returns
  `error=Some(..)` — the UI shows "couldn't check", never "latest"
  (the 6.1.41 rule, kept); respects `checkForUpdatesOnStartup`; dialog with
  Download (opens release page) + Later (`views.js:1302 showUpdateDialog`).
- **DONE in the parity pass (S):** (a) the `releases/latest` redirect
  fallback (`check_via_latest_page`, `update.rs`) — GET
  `github.com/Niczzxo/Aello-Music/releases/latest` via `session.get_text`
  (reqwest follows the redirect), scrape the first
  `…/releases/download/<tag>/Aello_*_x64-setup.exe` link, version from the
  tag segment; notes are empty on the fallback path. (b) The persisted
  failure flag (`last_update_check_failed`, `settings.rs`,
  `set_last_update_check_failed`) — `check_for_update` sets it from
  `info.error.is_some()`; Settings shows "The last update check failed"
  on the next launch instead of looking up-to-date; `readSettingsForm`
  preserves it across saves.
- **Still absent by design:** no beta channel, no `changelog.json` fetch,
  no in-app download/install of the exe (opens the release page instead —
  acceptable for unsigned NSIS).

### #52 SMTC / media keys / taskbar mini-player — DONE (code) / UNTESTED (runtime)
- Rust `SmtcManager` (`src-tauri/src/smtc.rs`, Windows-only, `#[cfg(windows)]`
  via `ISystemMediaTransportControlsInterop` per HEAD commit `612c046`);
  driven by the player snapshot (`lib.rs:481-482`, `player.js:297-320`);
  events `smtc-play` / `smtc-pause` / `smtc-next` / `smtc-previous` wired in
  `app.js:2047-2072`. Media keys come free with SMTC.
- **UNTESTED:** cannot run on the Linux VM; needs his PC (title/artist/
  artwork/position in the Windows overlay + media-key behaviour). Taskbar
  mini-player: none — SMTC overlay only. Note for the parent: the Gili
  SMTC-position-advance lesson (media-status lies) applies here too — the
  exe trusts the snapshot it pushes, which is the right side to trust.

### #53 Sleep timer — DONE
- Presets 15/30/45/60/90 min + End of song (`index.html:586-592`);
  fade = final 60s (or half the timer if shorter) in 30 linear steps
  (`player.js:827-896`, `step = min(30, floor(progress*30))`,
  `sleepMultiplier = 1 - step/30`); volume restores to the user's intended
  volume (multiplier back to 1, mute-aware); countdown text
  (`app.js:885-903`, 1s interval); end-of-song mode fires on `ended`.

### #54 Keyboard shortcuts — DONE
- Global handler (`app.js:1855-1908`): Space play/pause, ←/→ seek ±10s,
  ↑/↓ volume ±0.05, M mute, N/P next/prev, S shuffle, R repeat, L like.
  All suppressed while typing in a field or when a button is focused
  (avoids double-toggle). Extends the GAP_TABLE list as specified.

### #55 Notifications — MISSING / DEFERRED
- In-app toasts/status text only. No Tauri notification plugin wired
  (playback / download / LT join-request notifications). Deferred per
  GAP_TABLE; still absent. (This is also why LT join requests have no
  desktop notification — Android 6.1.45 removed that path anyway via
  auto-join.)
- **Exact reason it was not added in the parity pass:** `tauri-plugin-notification`
  is not a dependency in `Cargo.toml` and is not vendored in the offline
  cargo cache (`~/.cargo/registry/cache`) — adding it needs a network crate
  fetch plus a Windows runtime test. No new plugin was added per the task
  boundary (check before adding any new plugin).

### #56 Widgets (Android home-screen) — BLOCKED
- Windows widgets are a different platform (Widget Board); out of scope.
  Correctly absent.

### #57 Song recognition (Shazam) — BLOCKED
- Needs mic capture + Shazam signature service; no equivalent in-tree.
  Correctly absent.

### #58 Discord Rich Presence / Last.fm — MISSING / DEFERRED
- No Discord RPC, no Last.fm/ ListenBrainz code, no settings entries.
  Deferred per GAP_TABLE — and correctly, the settings do not promise them.
  Discord would need the Discord client + a Rust RPC lib; Last.fm needs an
  API-key decision (the Android key is compiled in — do not reuse blindly).

### #59 JioSaavn — BLOCKED
- Separate region-specific module; out of scope for the Windows build.

### #60 Desktop Wi-Fi Sync (phone ↔ desktop) — BLOCKED
- Transport UNDETERMINED in the audit (APK `DesktopSyncViewModel.kt`);
  companion-side protocol unknown. Cannot build against an unknown protocol.
  Correctly absent.

### #61 Share / comments / Wrapped / ringtone / AOD
- **Share: PARTIAL (parity pass).** Copy link on detail pages
  (`data-detail-action="copy-link"`, `music.youtube.com` playlist/album
  URLs) + song export to a picked folder (#41) give the desktop file-export
  path. No OS share sheet on desktop (none exists for Tauri/WebView2).
- **Comments: MISSING / DEFERRED (S–M).** No InnerTube comments endpoint in
  the Rust client; no comments sheet. Needs the comments endpoint parse.
- **Wrapped: DONE (parity pass).** The history segment now shows a Wrapped
  block (total plays, distinct songs, top song, top artist) + Top artists
  list — see #16. No separate year-in-review screen (seasonal; the block
  covers the underlying data).
- **Ringtone / AOD: BLOCKED.** Android-only (ringtone setter, always-on
  display). Correctly absent.

---

## 11. Professional desktop UI (#62–67)

### #62 Desktop layout — DONE
- Left sidebar Home / Search / Library / Trending / Listen Together /
  Settings / About (`index.html:26-52`); main content; persistent bottom
  player bar; queue panel in the full-player view; views
  `home/search/library/trending/together/settings/about/player/artist`
  (`app.js:62-87`, `VIEW_IDS`).

### #63 Full player with lyrics + EQ panels — DONE
- Full-player view (`view-player`) with lyrics panel (toggle
  `pb-lyrics-toggle` / `fp-lyrics-toggle`, `app.js:708-715`) and EQ panel
  (`renderEqPanel`, `views.js:1164`).

### #64 SVG icons everywhere — DONE
- 49 `<svg` in `index.html`, 13 in `views.js` (Lucide-style, 2px stroke,
  `stroke="currentColor"`); **zero** emoji glyphs found in any JS/HTML
  (grep for ⏮ ▶ 🔊 ⏸ ⏭ 🎵 ❤️ 🔍 🏠 ⚙️ returned nothing). The old bar's
  ⏮ ▶ 🔊 are gone.

### #65 Design-system MASTER.md compliance — PARTIAL / UNVERIFIED
- `design-system/aello-desktop/MASTER.md` exists; Aello Red `#FB2831`
  (`views.js:20`, CSS accent), dark OLED surfaces, spacing rhythm, focus
  states, `prefers-reduced-motion` (`styles.css:985`). This audit did NOT
  diff every MASTER.md rule against the CSS — full compliance is
  **UNVERIFIED** (needs a rule-by-rule pass against MASTER.md). No
  violations were spotted in the sampled rules.

### #66 Per-entity colors, circular logo, no black loading screens — DONE (with one UNVERIFIED)
- Per-entity colour DONE (see #47). Real logo wired (`src/img/logo.png`).
  "No black loading screens": no splash/loading-screen code exists in the
  exe — nothing to violate it — but that is absence of evidence, so marked
  **UNVERIFIED** rather than DONE for that sub-item (a first-run blank
  window before JS loads would still be the WebView default).

### #67 About / version / GPL / no-Spotify — DONE
- About view (`index.html:485-489`): "Version 6.1.45 · Windows desktop
  (unsigned build)", "Developer: Niczzxo", GPL-3.0, "based on the open-source
  BeatWave and PulseWave projects". `tauri.conf.json` version `6.1.45`,
  identifier `com.averon.music`.
- **No Spotify anywhere:** zero `Spotify`/`spotify` hits in `src/` and
  `src-tauri/src/`. Verified.
- **DONE in the parity pass:** About now states the BLOCKED items (Cast,
  LAN mode, widgets, recognition, JioSaavn, Wi-Fi Sync, DJ, skip silence,
  ringtone/AOD) in the same paragraph that already stated Cast — GAP_TABLE's
  "stated in About" requirement is met.

---

## 12. Android 6.1.45 changes the gap table could not know about

Diffed `averon-6.1.44` → `averon-6.1.45` (`app/src/main` + `gradle/`).
Beyond the version bump (PATCH 44→45), the material changes are:

| Android 6.1.45 change | Exe status |
|---|---|
| **Material3 1.5.0-beta02** (from alpha18) + Compose 1.12.1, Media3 1.11.1, Kotlin 2.3.21, AGP 9.5.0, Room 2.8.5, Coil 3.6.3, etc. — crash fix: alpha18's TextField styleable lambdas crash under foundation 1.12.1's restructured Style API | **N/A.** Android-only dependency bump; the exe has no Compose/Material3. No exe action. |
| **Search field moved from the bottom nav bar to the top of the Search screen** (`SearchScreen.kt`: `TopSearchBar` added, "6.1.45: the search field moved here from the bottom nav bar") | **DONE (already).** The exe's search field is at the top of the Search view (`index.html` search view, input focused on entry, `app.js:76-79`). No move needed. |
| **Listen Together: auto-join** — every join request approved immediately, host-approval notification path removed (`ListenTogetherClient.kt`) | **DONE (parity pass).** Host auto-approves; blocked users rejected (see #42.1). |
| **Listen Together: control-mode preference restored** (`ListenTogetherControlModeKey`, server starts owner-only, host's last choice re-applied) | **DONE (parity pass).** `lt_control_mode` setting, re-applied on `room_created`, saved on change (see #42.2). |
| **Listen Together: guest wire-up fix** — `wirePlayerControls()` called from `JoinApproved` so guests' presses reach the room (`ListenTogetherManager.kt`) | **CODE-VERIFIED (parity pass), LIVE-UNTESTED.** Guest play/pause/seek/track-change all emit `playback_action` gated on `canControlPlayback` (see #42.5). Needs a live two-client room test. |
| **Listen Together: local fan-out `applySyncPlayback` refactor** (host acts as server for local `PLAYBACK_ACTION`) | **N/A / UNVERIFIED.** LAN mode is BLOCKED on the exe (#43); the cloud path does not need it. |
| **Updater: `releases/latest` redirect fallback** (avoids `api.github.com` 403 on shared/CGNAT IPs) + persisted failure flag (`KEY_LAST_UPDATE_CHECK_FAILED`) so Settings shows "Can't check for updates" | **DONE (parity pass).** `check_via_latest_page` + `last_update_check_failed` (see #51). |
| Misc 6.1.45 diffs (Player/MiniPlayer/Queue/Thumbnail/Lyrics UI polish, `RecognitionHistory` entity change, `MusicDatabase` migration, `App.kt`/`MainActivity.kt` changes, `AudioDeviceBottomSheet`, notification-provider tweak, `LocalPlaylistViewModel`, `ModuleDetailScreen`, `AccountSettingsScreen`, `ContentSettings`, `UpdateSettings`, `TabNewsSuggestion`) | **Not separately audited.** These are Android UI/polish + DB-migration changes with no direct exe counterpart named in the gap table. The exe's equivalents (where they exist) are covered by their rows above. No exe action identified from the diff summaries alone; a line-level diff review was out of scope for this pass. |

---

## 13. Responsiveness audit (UI jank / freezes)

Method: grepped for synchronous/blocking calls on the UI thread, un-debounced
inputs, full-list renders, and image loading without size reservation.
No runtime profiling was possible on the VM (no Windows run); these are
code-level hotspots, ordered by likely user pain.

- **R1 — Daily Discover seeds fetched sequentially (HIGH) — FIXED in the
  parity pass.** Seeds are now fetched with `tokio::join!` over
  `related_or_empty()` (`lib.rs`); the 6 round trips run in parallel.
- **R2 — No virtualization; full `innerHTML` rebuilds (HIGH) — MITIGATED in
  the parity pass (hot paths).** Home sections now render chunked via
  `requestAnimationFrame` (first section immediately, rest appended);
  search results chunked 40/chunk; history rows chunked 100/chunk; load-more
  appends instead of rebuilding (`appendHomeSections` /
  `appendSearchResults`). Full-list rebuilds still exist on cold renders
  (Library segments, artist page) — true virtualization is still absent, M.
- **R3 — Images without size reservation; no lazy loading (MEDIUM) —
  FIXED in the parity pass.** `cardHtml` now emits `loading="lazy"
  decoding="async"` + width/height from the artwork dims; `.card-art`
  already had a fixed 140px height in CSS.
- **R4 — Long Rust commands awaited with no feedback, timeout, or cancel
  (MEDIUM) — FIXED in the parity pass.** `timedCall` (`api.js`, 30s
  default, `Promise.race`) wraps the long commands (`get_home_feed`,
  `get_playlist`/`get_album`/`get_artist`/`get_trending`/`browse_mood`,
  `search`); Home + Trending show static grey skeleton blocks while loading
  (`renderHomeSkeleton`, `.skeleton-*` CSS — no motion, polish pass owns
  animation).
- **R5 — Lyrics 100ms poll (LOW, acceptable).** `setInterval(…, 100)`
  (`app.js:784-790`) runs for the app lifetime but is guarded by panel
  visibility + current-song checks, and the highlight itself is a binary
  search + class toggle. Cheap; not a hotspot. Noted for completeness.
- **R6 — EQ panel `input` handler (LOW, acceptable).** Every slider movement
  reads the whole panel and re-applies the live curve
  (`app.js:794-819`, `views.readEqPanel`). Cheap (5 biquads); no debounce
  needed. Noted for completeness.
- **R7 — No debounced search input — by design (NOT A GAP).** Search runs on
  Enter / button click only (`app.js:190-194`); there is no per-keystroke
  search, so there is nothing to debounce. Correct as built.
- **R8 — Crossfade timer + sleep timer (LOW).** `setTimeout`/`setInterval`
  based (`player.js:463,872`); 500ms sleep tick is fine. No hotspot found.
- **No synchronous/blocking JS found:** no long `for` loops over large
  arrays on the UI thread outside rendering (R2 covers the render case);
  no `localStorage` writes in hot paths except prefs/queue saves (small
  JSON); no `alert()`/sync XHR. Rust commands are `async` throughout.

---

## 14. Home-page-onward walk (exe nav order vs APK)

Walked in exe nav order (sidebar: Home → Search → Library → Trending →
Listen Together → Settings → Player/About).

| Exe screen | APK has | Exe state |
|---|---|---|
| Home | Feed + Daily Discover (first) + Quick Picks + Keep Listening + Forgotten Favorites + Speed Dial + From the Community + Similar-to rows + Mood & Genres + Wrapped card (seasonal) | Feed + Daily Discover + Quick Picks / Keep Listening / Forgotten Favorites / Speed Dial DONE (parity pass). From the Community / Similar-to rows still missing (no feed source for them). Mood & Genres present (as a section, via Explore). Wrapped card missing (seasonal; the Wrapped data block is in Library history instead). Home greeting is a nice extra the APK does not have. |
| Search | Tabs Explore/Suggestions/Albums + online results, URL paste, history, continuations, local search | Tabs + history + URL paste + continuations DONE (parity pass). Playlist-URL paste added. **Suggestions tab missing**; local search missing. |
| Library | Songs/Artists/Albums/Playlists/Library tabs, auto-playlists (Liked/Downloaded/Uploaded/Local), manual playlists, local files, history, stats, backup/CSV | Playlists segment (Liked/Downloaded/Top 50/Local cards + CRUD) DONE. Songs segment shows synced YouTube library songs + local files. History shows Wrapped block + Top artists. Local file paths persist now. **Backup/CSV DONE** (parity pass). **Uploaded/Cached cards missing**; no Artists/Albums tabs (only songs/playlists/downloads/history segments). |
| Trending | Charts sections + Top music videos + load-more + New Releases | Sections + Top-videos DONE. Load-more missing (no continuation in `charts()`); New Releases not a separate view. |
| Listen Together | Remote rooms + LAN, control modes, chat, suggestions, reconnect, host transfer/kick, expiry/extend, auto-join (6.1.45) | Remote rooms core + auto-join + kick/transfer/block + extend + control-mode preference DONE (parity pass). LAN blocked. |
| Settings | ~20 screens (appearance, player, account, content, storage, EQ, LT, integrations, privacy, backup, about…) | **One-page form only** (Account/Accent/Appearance/Playback/LT/Updates) + Storage (clear downloads) + sync button. Content filters, searchable settings, home-order, integrations all missing. |
| Player | Full player (2 layouts), mini players, queue sheet, lyrics, EQ, canvas, sleep, speed, radio, Cast | Full player + queue + lyrics + EQ + sleep + speed + radio DONE (parity pass). **Canvas, Cast missing**; one layout (fine for desktop). |
| About | Version, changelog, commits, links, Story of Aello | Version + GPL + dev credit + BLOCKED-items list DONE. Changelog/commits screens missing (update dialog shows release notes — partial cover). |

---

## 15. Corrections to GAP_TABLE (where the table was wrong or stale)

1. **#49 glass:** table says "no backdrop-blur effect chain on desktop
   (WebView2 has no equivalent)". The exe HAS `backdrop-filter` blur
   (12/16/24px) on sidebar, player bar, dialogs (`styles.css`). The claim
   is stale; glass exists, motion does not.
2. **#39 download integrity:** the table claims "integrity = size check vs
   expected" and the audit (v1) claimed the code only rejects zero-byte
   files (`lib.rs:271-276`). BOTH are stale: `download_to_file_inner`
   (`session.rs:382,408`) already compares the downloaded byte total
   against the response `Content-Length` and errors on mismatch; the
   zero-byte check in `lib.rs` is a second guard. Download integrity vs
   expected size is DONE — no code change was needed (verified in the
   parity pass, 2026-10-31).
3. **#5 search:** table's "PARTIAL (songs only, no tabs/filters)" for v0.1.0
   is outdated — the current tree has all five tabs + history + URL paste.
   The remaining gap is continuations, not tabs.
4. **#28 radio:** table says "via `next`/`related` endpoints (same as Daily
   Discover seeds)" as if the wiring were trivial — the endpoints are in
   fact only wired to Daily Discover; radio itself was never built.
5. **#42 LT:** table lists the full feature set as the 6.1.45 build target;
   the build delivered the core but not kick/transfer/extend, and the
   6.1.45 Android auto-join + control-mode-preference changes (which
   postdate the table) were not picked up.
6. **#51 update check:** table predates the Android 6.1.45 redirect fallback
   and failure-flag persistence; the exe matches the 6.1.44 updater, not
   the 6.1.45 one.
7. **#33 lyrics:** table is accurate; the only open question is rich-sync /
   word timestamps (UNVERIFIED, see #33).
8. **#42.5 guest wiring (parity pass):** the audit (v1) left guest control
   wiring UNVERIFIED ("not traced end-to-end"). It has now been traced:
   guest play/pause/seek/track-change all emit `playback_action` gated on
   `canControlPlayback()` (`app.js`: `togglePlayback`, debounced seek send,
   `refreshNowPlaying` change_track, `sendPlaybackAction`). Code-verified;
   a live two-client room test is still required.
9. **#44/#46 channel picker + auto-capture (sign-in agent, 2a1454d):** the
   audit (v1) said the picker had zero JS references and capture was manual
   paste. Both are now DONE (see #44).

---

## 16. UNVERIFIED / UNTESTED register (everything not proven from code)

- **UNTESTED — SMTC on Windows** (#52): code exists, VM is Linux. Needs his
  PC: overlay title/artist/artwork/position + media keys.
- **UNTESTED — Sign-in on a real Google account** (#44): needs a real
  account; never use the user's credentials here. Also the UA-hardening
  question (UNVERIFIED from code).
- **UNTESTED — Listen Together live room** (#42): no room was joined from
  this VM; guest control wiring is code-verified (#42.5) but needs a live
  two-client test.
- **UNVERIFIED — Lyrics rich-sync / word timestamps** (#33): standard LRC
  proven; karaoke word-level support not found in the read — needs a
  rich-sync test lyric.
- **UNVERIFIED — MASTER.md full compliance** (#65): sampled rules pass;
  a rule-by-rule diff was not done.
- **UNVERIFIED — "no black loading screens"** (#66): no splash code exists,
  but first-paint behaviour before JS loads was not observed.
- **UNVERIFIED — Artist page detail depth** (#7): top songs/albums/videos/
  related proven; monthly listeners / subscriber count / description
  toggles not confirmed from the read.
- **UNVERIFIED — Library Artists/Albums tabs**: the Library segment list
  was not fully enumerated in this pass (songs/playlists/downloads/history
  seen; artists/albums segments not confirmed).
- **UNTESTED — everything built in the parity pass (2026-10-31):** Home
  personal sections (#3), radio/autoplay (#28), LT auto-join + kick/transfer/
  block/extend + control-mode pref (#42), detail header actions (#8/#9),
  speed-dial pinning (#18), chips filtering + home/search load-more (#1/#5),
  update fallback + failure flag (#51), YT sync pull + like push (#45/#13),
  backup/restore/CSV (#17), song export (#41), 8D/normalization (#30/#32),
  APO import (#37), lyrics styles (#35), clear downloads (#50). All
  code-verified only (cargo check Linux + Windows target, node --check);
  none has run on Windows. His PC is the verdict for each.

---

## 17. Count summary

- **DONE:** #1, #2, #3, #4, #5(core+cont.), #6(core), #7, #8, #9(core),
  #10, #12, #14(core+paths), #15, #16(core+artists+Wrapped), #17, #18,
  #19, #20, #21, #22, #23, #24, #25, #28, #30, #32(approx.), #36, #38,
  #40, #41, #44, #46, #47, #51, #53, #54, #61(Wrapped), #62,
  #63, #64, #67 — **41 rows**
- **PARTIAL:** #9(Cached/Uploaded cards), #13(full sync), #14(scan),
  #16(albums), #32(true loudness), #35(styles beyond Fade/Glow),
  #37(unlimited bands), #39(cache), #42, #45(subscriptions/albums/
  uploads), #48, #49, #50, #61(share/comments) — **14 rows**
- **MISSING (implementable):** #1(reorder/randomize), #5(local search),
  #6(load-more), #11, #29, #34, #35(romanization/translation),
  #37(unlimited bands), #39(cache), #45(subscriptions/albums/uploads),
  #55, #58, #61(comments), #65(full compliance unverified) — **~14 rows**
- **BLOCKED:** #26, #27, #31, #43, #56, #57, #59, #60, #61(ringtone/AOD)
  — **9 rows**
- **DEFERRED (in GAP_TABLE, confirmed still absent):** #11, #14(scan),
  #29, #34, #35(romanization/translation), #39(cache), #55, #58, #61
  (comments) — **9 rows** (overlap with MISSING by design: deferred =
  buildable but cut)
- **UNVERIFIED items:** 9 (see §16).

Bottom line for the implementation team: the parity pass (2026-10-31) closed
the user-visible gaps in the order the user would notice them — **#3 home
personal sections, #28 radio, #42 auto-join + kick/transfer/extend, #45 YT
sync (partial), #8/#9 detail actions, #51 updater fallback, and R1–R4
responsiveness** are all built and code-verified. What remains for the next
passes: **#11 Suggestions tab** (Apple Music scraper), **#45 sync
completion** (subscriptions/albums/uploads stores), **#34 extra lyrics
providers**, **#35 romanization/translation** (user key), **#55
notifications** (plugin fetch + Windows test), **#61 comments**, and the
**polish pass (#49 full-motion glass)**. Nothing in this pass is proven on
Windows until his PC test — code, build, and his device test must all agree
before anything is called fixed.
