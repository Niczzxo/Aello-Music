# Aello Windows 6.1.45 — Phase 0 Gap Table

Android source: `~/workspace/averon-6.1.44` (6.1.44). Windows base: v0.1.0
(`windows/` on Aello-Music main 2724b12). Full Android inventory:
`~/workspace/aello-6.1.44-feature-audit.md` (16 sections).

Status: **DONE** = in v0.1.0 already. **TO-BUILD** = build in 6.1.45.
**BLOCKED** = cannot build in this pass, with the exact reason.
**DEFERRED** = buildable but cut for this pass (note why).

## Core catalogue & browsing

| # | Android feature | Windows v0.1.0 | 6.1.45 |
|---|---|---|---|
| 1 | Home feed (sections from InnerTube home) | DONE (get_home_feed) | Keep; restyle per MASTER.md |
| 2 | Daily Discover (pinned first; seeds from liked/recent/most-played → `next`/`related`, 6/seed; keep-last-good fallback; top-songs fallback) | TO-BUILD | New Rust `get_daily_discover` (needs `next`/`related` InnerTube endpoints + library store for seeds) |
| 3 | Quick Picks / Keep Listening / Forgotten Favorites / Speed Dial | TO-BUILD | DEFERRED — home/library sections need the library store + history first; cut for 6.1.45, no placeholder UI. |
| 4 | Mood & Genres (explore page) | TO-BUILD | New Rust `get_explore` (moods + sections) |
| 5 | Search (tabs song/album/artist/playlist, URL-paste resolve, history, continuations) | PARTIAL (songs only, no tabs/filters) | TO-BUILD: filter param + artist results + search history + URL resolve |
| 6 | Trending / Charts (+ Top music videos) | TO-BUILD | New Rust `get_trending` (InnerTube charts/browse endpoint) |
| 7 | Artist page (top songs, albums, singles, videos, related, subscribe) | TO-BUILD | New Rust `get_artist` (InnerTube `artist` browseId) + frontend view |
| 8 | Album page | DONE (get_album) | Keep; add play-all/shuffle/like/download actions |
| 9 | Playlist detail (online/local/top/cache/auto) | DONE (get_playlist, online only) | TO-BUILD: local playlist CRUD, auto-playlists (Liked/Downloaded/Local), continuation |
| 10 | Moods & moments detail browsing | TO-BUILD | Via `get_explore` mood browse (same endpoint family) |
| 11 | Suggestions tab (Apple Music trending scrape) | DEFERRED | Apple Music scraper is a separate scraper; cut for this pass, note in UI as coming later. No fake data. |

## Library

| # | Android feature | Windows v0.1.0 | 6.1.45 |
|---|---|---|---|
| 12 | Library persistence (liked, playlists, history, most-played) | TO-BUILD | Rust JSON store in app-data dir (`library.json` + `history.json`): liked songs, manual playlists, play history (counts after 30s), most-played. No SQLite plugin — JSON matches the existing settings-store pattern. |
| 13 | Liked songs auto-playlist + like sync | TO-BUILD | Local like/unlike (guest). YT sync needs sign-in (see #28). |
| 14 | Local files (picker, session-only) | DONE | DEFERRED — persisted local-file index cut for 6.1.45; local files stay session-only (re-pick after restart). |
| 15 | Recents / History screen + clear | TO-BUILD | From history store; clear action included |
| 16 | Stats screen (most-played) | TO-BUILD | From history store |
| 17 | Backup/restore + CSV import | TO-BUILD | DEFERRED — library backup/restore and CSV import cut for 6.1.45; no export/import UI. |
| 18 | Speed-dial pinning (max 27) | TO-BUILD | In library store |

## Player

| # | Android feature | Windows v0.1.0 | 6.1.45 |
|---|---|---|---|
| 19 | Play/pause/seek/next/prev/volume/mute | DONE | Keep |
| 20 | Queue (reorder/remove, persistent) | DONE (session only) | TO-BUILD: persist queue to disk, restore on restart |
| 21 | Shuffle / repeat OFF→ALL→ONE (persisted) | TO-BUILD | Frontend state + settings persistence |
| 22 | Auto-advance (next on ended) | DONE | Keep (verify against repeat modes) |
| 23 | Crossfade 5s default + gapless | TO-BUILD | Two-`<audio>`-element crossfade with gain ramps (frontend). Gapless = 0s crossfade path. |
| 24 | Playback speed | TO-BUILD | `<audio>.playbackRate` + UI control |
| 25 | Audio quality AUTO/HIGH/LOW | TO-BUILD | Resolve picks itag 140 (AAC, HIGH) vs 139 (LOW); AUTO = 140. WebView2 cannot play Opus — never select Opus. |
| 26 | Skip silence | BLOCKED | No Web Audio equivalent without heavy DSP (Analyser-based gating is unreliable and would clip intros). Revisit only with a native audio path. |
| 27 | Cast (Google Cast) | BLOCKED | No Cast SDK exists for this stack (Tauri/WebView2). Android has it GMS-only. Cut, stated in About. |
| 28 | Radio / autoplay (related queue) | TO-BUILD | Via `next`/`related` endpoints (same as Daily Discover seeds) |
| 29 | Canvas / background video artwork | DEFERRED | WebView2 can play mp4 canvas; needs canvas source endpoints. Cut for this pass. |
| 30 | 8D audio | DEFERRED | Feasible via StereoPanner LFO but needs tuning time; cut for this pass. |
| 31 | DJ / Auto-DJ mixing | BLOCKED | Beat tracking + key detection DSP is a project of its own; no Rust equivalent in-tree. Cut with reason. |
| 32 | Audio normalization (LoudnessEnhancer) | DEFERRED | Web Audio DynamicsCompressor could approximate; needs loudness metadata the Rust client does not fetch. Cut for this pass. |

## Lyrics

| # | Android feature | Windows v0.1.0 | 6.1.45 |
|---|---|---|---|
| 33 | Synced lyrics (LRC fetch/parse/highlight/auto-scroll) | TO-BUILD | Rust `get_lyrics` (LrcLib first: `https://lrclib.net/api/search`, 4 strategies as in audit §7; + YouTube timed-text subtitle fallback; overall timeout 6s). Frontend ports `LyricsUtils` parse incl. `[offset:]` applied to every timestamp, binary-search current line (+300ms), 100ms poll, `listIndexForLine` header-index fix, tap-to-seek, per-song offset. |
| 34 | Lyrics providers race (9 providers) | DEFERRED | Full 9-provider race (YouLyPlus, PaxSenix, Musixmatch, BetterLyrics, SimpMusic, KuGou…) is out of scope; LrcLib + YT subtitles cover the common case. Add providers later behind the same Rust command. |
| 35 | Lyrics styles / romanization / AI translation | DEFERRED | Basic synced highlight + plain fallback in this pass. Romanization needs Kuromoji/Pinyin (no JS equivalent in-tree). AI translation needs the user's own OpenRouter/DeepL key — settings field may exist, translation itself gated on a key the user provides. |

## EQ / Sound Enhance

| # | Android feature | Windows v0.1.0 | 6.1.45 |
|---|---|---|---|
| 36 | Sound Enhance (80Hz LS +3.0 Q0.7; 300Hz PK +1.0 Q1; 3000Hz PK +2.0 Q1; 8000Hz HS +1.5 Q0.7; preamp −3.0dB), default ON | TO-BUILD | Web Audio: `BiquadFilter` chain (lowshelf/peaking/peaking/highshelf) + `GainNode` preamp −3dB, built from the `<audio>` element via `MediaElementSource`. Default ON when no custom profile active. |
| 37 | Custom EQ profiles (parametric bands, APO/Peace import, per-profile store) | TO-BUILD | Same biquad chain driven by stored bands; profile CRUD in settings store; APO text import (basic parser). Precedence: custom > Enhance > flat, never stacked. |
| 38 | EQ presets UI (classic + Axion) | TO-BUILD | One clean EQ panel (desktop); presets list + custom save. |

## Downloads / offline

| # | Android feature | Windows v0.1.0 | 6.1.45 |
|---|---|---|---|
| 39 | Song downloads (never evicted) + stream cache (LRU 1024MB) | TO-BUILD | Rust `download_song` (reqwest GET of resolved stream URL → app-data `downloads/<videoId>.m4a`, integrity = size check vs expected); Downloaded auto-playlist; offline playback via asset protocol; storage settings (clear downloads, cache size display). Stream cache: DEFERRED — no stream cache is built in 6.1.45, so the Cached/Uploaded auto-playlist cards are not rendered (see Notes). |
| 40 | Auto-download on like | TO-BUILD | Setting + hook in like action |
| 41 | Song export | TO-BUILD | DEFERRED — song export (copy a downloaded file to a user-picked folder) cut for 6.1.45. |

## Listen Together

| # | Android feature | Windows v0.1.0 | 6.1.45 |
|---|---|---|---|
| 42 | Listen Together (remote rooms) | TO-BUILD | Frontend JS WebSocket client implementing the JSON protocol from `listen-together-server/src/protocol.ts` (in the Android tree — read it, do not guess). Default server `wss://devilmi-vivi-music-listen-together.hf.space`, fallback `wss://vivimusic-listen-together.onrender.com`. Room create via Rust `lt_create_room` (POST `/api/rooms`, avoids CORS) + JS socket for the protocol. Includes: join/approve/reject, `control_mode` owner/everyone (server-enforced + per-track `suggested_by` ownership), buffer gate (10s), chat, suggestions, reconnect (15 attempts, exp backoff cap 120s), session token persist, host grace/transfer, kick. Guest muted on join. |
| 43 | Listen Together LAN mode (raw TCP + NSD `_pulsewave-sync._tcp`) | BLOCKED | No NSD/mDNS discovery in Tauri without a plugin, and raw-TCP framing would need a Rust transport. Cut with reason. |

## Account

| # | Android feature | Windows v0.1.0 | 6.1.45 |
|---|---|---|---|
| 44 | Google sign-in (WebView cookie harvest, no OAuth) | TO-BUILD (attempt) | Tauri second WebView window at Google ServiceLogin → music.youtube.com, UA hardened like Android (strip `wv`, suppress `X-Requested-With`), harvest cookie via window JS eval, store in Rust settings (never in JS localStorage). Channel picker if multi-channel. **Cannot be tested on the VM** (needs a real Google account; never use the user's credentials here) — implement carefully, mark UNTESTED, his PC is the verdict. Guest mode stays fully working. Never fake a signed-in state. |
| 45 | YT two-way sync (liked/library/playlists/subscriptions) when signed in | TO-BUILD (partial) | Sync liked/library/playlists via InnerTube with the harvested cookie (Rust session gains a cookie field). If the cookie path proves unreliable, sync stays off and the UI says so honestly. |
| 46 | Sign-out / channel switch | TO-BUILD | Clear cookie locally; channel switch via picker. |

## Themes / appearance

| # | Android feature | Windows v0.1.0 | 6.1.45 |
|---|---|---|---|
| 47 | Per-entity page color (artwork → dominant color, one uniform color per page) | TO-BUILD | JS canvas dominant-color extraction from artwork (no Android Palette on desktop); apply as the page's uniform color; fallback = Aello Red. CORRECTION (JS builder verified against `PlayerColorExtractor.kt`): the live vividness factor is **×1.12 when vibrant / ×1.0 otherwise** — the "×1.4" in the audit §13 is only an unused default parameter (the old 1.3–1.4x was deliberately reduced). Implement 1.12/1.0. |
| 48 | Themes (dark/light/system, accent, fonts) | PARTIAL (accent + appearance) | Keep + extend: accent stays Aello Red default; dark/light/system already there. |
| 49 | Liquid glass effects | NOT APPLICABLE | Android glass kill-switch rule does not apply to desktop; no backdrop-blur effect chain on desktop (WebView2 has no equivalent). Use solid surfaces + subtle borders. |

## Settings / system

| # | Android feature | Windows v0.1.0 | 6.1.45 |
|---|---|---|---|
| 50 | Settings screens (desktop-relevant subset) | PARTIAL | TO-BUILD: Playback (quality, crossfade, persistent queue/shuffle/repeat), Content (hide explicit/video songs, data saver), Downloads/Storage, Library, Equalizer, Listen Together, Update, About. Skip Android-only (Cast, widgets, JioSaavn, Discord login). |
| 51 | In-app Windows update check | TO-BUILD | Rust `check_for_update`: GET GitHub releases list, latest tag `^v*` carrying an `Aello_*_x64-setup.exe` asset; strictly-newer compare; **never report "latest" on a failed check** (Android 6.1.41 rule — return `error` instead); prompt dialog with Download (opens the release page / downloads the exe) + Later. Respects the existing `checkForUpdatesOnStartup` setting. |
| 52 | SMTC / media keys / taskbar mini-player | TO-BUILD | Rust `windows-rs` SMTC implementation behind `#[cfg(windows)]`, driven by the existing playback snapshot. **Cannot be tested on the Linux VM** — build-gated, his PC is the verdict. Media keys come free with SMTC. |
| 53 | Sleep timer (fade 30 steps over final 60s, end-of-song mode, volume restore) | TO-BUILD | Frontend timer + gain ramp; mute-aware volume restore. Presets: 15/30/45/60/90 min + end-of-song. |
| 54 | Keyboard shortcuts (Space/←/→/M) | DONE | Keep + extend: N/P next/prev, S shuffle, R repeat, L like, ↑/↓ volume. |
| 55 | Notifications (playback/download/LT join-request) | DEFERRED | Tauri notification plugin exists but is not wired; desktop notifications cut for this pass (in-app toasts instead). |
| 56 | Widgets (Android home-screen) | BLOCKED | Windows widgets are a different platform (Widget Board); out of scope. |
| 57 | Song recognition (Shazam) | BLOCKED | Needs mic capture + Shazam signature service; no equivalent in-tree. Cut with reason. |
| 58 | Discord Rich Presence / Last.fm | DEFERRED | Discord RPC needs the Discord client + a Rust RPC lib; Last.fm needs an API key decision (Android key is compiled in — do not reuse blindly). Both cut for this pass; settings entries must not promise them. |
| 59 | JioSaavn | BLOCKED | Separate region-specific module; out of scope for the Windows build. |
| 60 | Desktop Wi-Fi Sync (phone ↔ desktop) | BLOCKED | Transport UNDETERMINED in the audit (check `DesktopSyncViewModel.kt`); companion-side protocol unknown. Cut with reason. |
| 61 | Share / comments / Wrapped / ringtone / AOD | BLOCKED or DEFERRED | Share: OS share via file export only (song export covers it). Comments: needs InnerTube comments endpoint — DEFERRED. Wrapped: DEFERRED (stats screen covers the data). Ringtone/AOD: Android-only, BLOCKED. |

## Professional desktop UI (Phase 1)

| # | Requirement | Status |
|---|---|---|
| 62 | Desktop layout: left sidebar (Home/Search/Library/Trending/Listen Together/Settings), main content, persistent bottom player bar, queue panel | TO-BUILD (sidebar exists in v0.1.0; add Search/Trending/Together nav + queue panel) |
| 63 | Full player with lyrics panel + EQ panel | TO-BUILD |
| 64 | SVG icons everywhere (no emoji glyphs — current bar uses ⏮ ▶ 🔊) | TO-BUILD (inline SVG, Lucide-style, consistent 2px stroke) |
| 65 | Design system MASTER.md compliance (Aello Red #FB2831, dark OLED, spacing rhythm, focus states, reduced-motion) | TO-BUILD |
| 66 | Per-entity colors, circular logo, no black loading screens | TO-BUILD (see #47) |
| 67 | About version shows 6.1.45; GPL attribution kept; no Spotify anywhere | TO-BUILD |

## Notes

- **Never reintroduce Spotify.** No Spotify module, client, auth, or strings.
- **InnerTube behavior unchanged** unless a feature provably requires it: guest-only, no PoToken, AAC/MP4-first itag 140 (WebView2 cannot play Opus).
- **Honesty rules:** every feature that cannot run on the VM (sign-in, SMTC, Listen Together live rooms) is implemented but marked UNTESTED in the report; nothing is claimed working until code + build agree, and his PC is the final verdict — same as Android device verdicts.
- **Workflow:** `.github/workflows/windows.yml` (repo-root file in `repo-root/`) already updated for the unified release model; not part of this gap table.
- **6.1.45 deferrals (review fixes):** Cached/Uploaded auto-playlist cards are DEFERRED pending the stream cache (#39) — they are not rendered at runtime or in the static placeholder. Home chip row, per-section Play-all, and video rows are DEFERRED (home feed keeps its sections without them). Speed Dial / Keep Listening / Quick Picks / Forgotten Favorites UI, persisted local-file index, library backup/restore, CSV import, song export, and stream cache are all DEFERRED — see rows #3, #14, #17, #39, #41.
