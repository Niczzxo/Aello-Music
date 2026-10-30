# Aello for Windows

**Aello — Sound in Motion.** A desktop music player for Windows, matching
the Aello Android app (version 6.1.27, package `com.averon.music`, brand
color `#FB2831`).

> **Status: v0.1.0 in progress — first milestone build underway.**
>
> The Windows build runs on GitHub Actions (windows-latest); see
> `.github/workflows/windows.yml`. The app is unsigned (SmartScreen will
> warn on first install — accepted for testing). Nothing here has been
> run on a real Windows PC yet: the user's PC test is the acceptance
> check, exactly like the Android app's device verdicts. There is no
> signed installer and no Microsoft Store listing — and none is claimed.
> See `BUILD_PLAN.md` for the phases.

## Stack decision: Tauri (Rust core + web frontend)

| Option | Verdict | Why |
|---|---|---|
| **Tauri 2** (Rust + WebView2) | **Chosen** | Small binaries, Rust core makes native HTTP calls (no browser CORS problem for the backend), WebView2 is preinstalled on Windows 10/11, and the web frontend mirrors the app's look with plain HTML/CSS/JS — no build step, no framework lock-in. |
| Electron | Rejected | 100+ MB binaries, a full Chromium per install, and a Node backend the project does not need. The existing `desktop/` Electron app in the Android tree is a *companion sync client* (it syncs playlists with the phone over Wi-Fi); it is PulseWave-branded, has no playback backend of its own, and is a different product. It was left untouched and is not reused here. |
| WinUI / WPF (C# + XAML) | Rejected | Fully native, but needs the Windows SDK and a separate XAML UI that cannot share anything with the web or Android work, for no playback advantage at this stage. |
| Flutter | Rejected | One codebase for desktop + mobile is attractive, but the Android app is Kotlin/Compose, not Flutter — adopting it means rewriting both apps instead of adding one. |

The frontend is deliberately framework-free (no Vite, no bundler): the
Tauri config points `frontendDist` at `src/` directly, so `cargo tauri dev`
serves the files as-is. A Vite/React migration is a Phase 1 decision, not
a prerequisite (see `BUILD_PLAN.md`).

Config reference: the `tauri.conf.json` shape follows the Tauri v2
configuration reference (https://v2.tauri.app/reference/config/). The
config has been validated as JSON locally; full validation by the
Tauri CLI happens on the Actions runner at build time.

## What exists

```
aello-windows/
├── README.md                 This file.
├── BUILD_PLAN.md             Phases to a signed Windows installer.
├── .github/workflows/
│   └── windows.yml           Tag-triggered unsigned release build
│                             (windows-latest, tauri-action, --no-sign).
├── src-tauri/
│   ├── Cargo.toml            Crate: tauri 2, serde, serde_json,
│   │                         reqwest (rustls), tokio, url, regex,
│   │                         tauri-plugin-dialog.
│   ├── build.rs              tauri_build::build().
│   ├── tauri.conf.json       Window 1200×800, CSP, NSIS bundle target,
│   │                         identifier com.averon.music, version 0.1.0.
│   │                         Icons wired (see icons/).
│   ├── icons/                Generated from the Aello Android logo
│   │                         master (averon-icon-master-v2.png):
│   │                         32x32, 128x128, 128x128@2x, icon.png,
│   │                         icon.ico (multi-size).
│   ├── capabilities/
│   │   └── default.json      core:default + dialog:allow-open.
│   └── src/
│       ├── main.rs           Thin binary entry point.
│       ├── lib.rs            Tauri setup + all commands + shared state.
│       │                     AppState.music is the real InnerTube
│       │                     session; local_files wired in.
│       ├── models.rs         Song / Album / Playlist / HomeSection.
│       ├── music_service.rs  Backend trait (implemented by innertube).
│       ├── innertube.rs      InnerTube backend: search, home, playlist,
│       │   ├── client.rs     album, stream resolve (AAC-first).
│       │   ├── session.rs    Client defs, HTTP + retry, visitorData.
│       │   ├── parse.rs      Renderer → models parsing.
│       │   ├── resolve.rs    Resolve chain + format selection.
│       │   ├── cipher.rs     Signature decipher (pure Rust).
│       │   └── error.rs      Typed errors with real reasons.
│       ├── local_files.rs    Native file picker for local audio files.
│       ├── player.rs         Playback snapshot shared with the frontend.
│       └── settings.rs       JSON settings in the app-data directory.
└── src/
    ├── index.html            Sidebar (Music / Library / Settings),
    │                         search bar, views, bottom player bar,
    │                         full-player view, About view.
    ├── styles.css            Dark theme, logo-red accent default,
    │                         accent + appearance switching, light theme.
    └── js/
        ├── api.js            Tauri bridge with an honest no-bridge
        │                     fallback for plain-browser previews.
        ├── player.js         <audio> playback, queue (reorder/remove),
        │                     mute, local files via asset protocol.
        ├── views.js          Rendering, escaping, theme application.
        └── app.js            Wiring: nav, search, settings, shortcuts.
```

What works without any backend: the full interface shell, navigation,
search box, theme/accent/appearance switching, volume, settings
persistence shape (Rust-side JSON file), the player bar and
full-player view, and keyboard control (Space toggles playback).

## Audio and backend path (honest account)

**Playback today:** the frontend plays through the `<audio>` element in
WebView2. The Rust core now has a real InnerTube backend
(`src-tauri/src/innertube.rs`, guest-only, no login/cookies/PoToken):
search, home feed, playlists, albums, and stream resolve all work
against the live API — verified 2026-10-30 from this VM (search
"Dopamine Guru Randhawa" → 23 items; resolve → itag 140 AAC URL,
HTTP 206 on a Range GET, so the URL is live; bad video → clean
"This video is unavailable"). Format selection prefers AAC/MP4
(itag 140) because WebView2 cannot play Opus-in-WebM.

**Known limits of the v0.1.0 backend (not hidden):**

1. **Bot-flagging is IP-dependent.** From this VM (datacenter egress
   IP) the Android VR main client is bot-flagged (`LOGIN_REQUIRED /
   "Sign in to confirm you're not a bot"`) and the chain recovers via
   VISIONOS. On a residential PC the VR client may work first-try.
   The Android client order is kept; do not reorder on VM evidence
   alone — the user's PC test decides.
2. **PoToken is not implemented.** WEB_REMIX/TVHTML5 fallbacks run
   without one; if a track is bot-flagged for VISIONOS/IOS too, the
   web fallbacks will likely fail. Android mints PoTokens in a WebView;
   there is no equivalent in this Rust port yet.
3. **Signature deciphering is implemented but unproven live.** The
   working clients return direct URLs, so no cipher was exercised;
   the player-JS fetch path is blocked from this VM (watch page 302s
   to `google.com/sorry`). If a ciphered format appears and
   deciphering fails, the error is `CipherUnsupported` — never a
   broken URL.
4. **Local files** play through the Tauri asset protocol (picker via
   `tauri-plugin-dialog`, session-only list, no persistence yet).
5. **Artist cards are not shown yet in v0.1.0.** Search and home
   results skip artist rows (there is no artist page to open one
   from); searching an artist still returns their songs.

**Planned or absent — never faked:**

| Feature | State in this scaffold |
|---|---|
| Downloads / offline | Absent. `download_dir` only resolves the folder path. Downloading, storage, and offline playback are Phase 4. |
| Synced lyrics | Absent. The full-player view shows a "planned" note. Lyric providers (LRCLIB etc., as on Android) are Phase 4. |
| Equalizer | Absent. Planned via the Web Audio API (`BiquadFilter` chain) in the frontend; no bands are persisted or applied yet. |
| Listen Together | Absent. The Android room/sync protocol would need a desktop client; Phase 4, decision needed. |
| Cast | Absent. No Cast SDK exists for this stack; treat as out of scope unless a decision says otherwise. |
| Account sign-in | Absent. The header shows "Not signed in" and there is no login flow. |
| System media controls (SMTC) / media keys | Absent. The Rust playback snapshot exists so this can be added without re-architecting. |
| Auto-update | Absent. The settings toggle is stored; the updater mechanism is a Phase 0 decision (Tauri updater plugin + release feed). |
| App icon / logo artwork | Done in v0.1.0. Icons generated from the Aello Android logo master (`averon-icon-master-v2.png`) into `src-tauri/icons/` and wired in `tauri.conf.json`. |

## What finishing requires

1. **Repo + first Windows build** — create public repo
   `Niczzxo/Aello-Windows`, push this tree (including
   `.github/workflows/windows.yml`), push tag `v0.1.0`. GitHub Actions
   (windows-latest) builds the unsigned NSIS installer and publishes
   the release. The Windows PC test (install, open, search, play) is
   the acceptance check — nothing is called working until then.
2. **First-build fixes, if the Actions log reports any** — config /
   capability validation by the Tauri CLI, icon handling, CSP or
   asset-protocol issues found only at bundle time.
3. **The music backend** (Phase 3) — DONE in v0.1.0 as the InnerTube
   port described above, live-tested from this VM; remaining risks
   are the four known limits listed above, to be settled by the PC test.
4. **A code-signing certificate** for a signed installer, and a
   distribution decision (direct download vs. Microsoft Store) — both
   still open; v0.1.0 ships unsigned like the Gili Windows builds.
5. **Policy review** — an unofficial YouTube Music client carries
   distribution and policy risk (see `BUILD_PLAN.md`). Acceptance
   anywhere cannot be promised.

## License

GPL-3.0, matching the Aello Android app and its BeatWave / PulseWave
origins. The Android tree was read-only during this work; no Android
code was copied verbatim — the Rust and JavaScript here are written for
this stack.
