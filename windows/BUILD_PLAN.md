# BUILD_PLAN.md — Aello for Windows: scaffold to signed installer

## Status — 2026-10-30 (v0.1.0 milestone)

User chose the FULL PLAYER (2026-10-30 21:14 +06). Phase 0 decisions
recorded: distribution = direct download via GitHub Releases (Store
NOT the route); signing = unsigned first (SmartScreen warning accepted,
same as Gili); auto-update = not built (settings toggle only);
backend = Rust InnerTube port (done); bundle identity =
`com.averon.music` / "Aello" (confirmed); icon = generated from the
Android logo master (done); Wi-Fi sync = standalone player (default).
Phase 1 done (icons, config v0.1.0, Actions workflow). Phase 2 done
(local-file playback + frontend hardening). Phase 3 done as a Rust
port, live-tested from the VM (search/home/resolve verified; known
limits in README.md — VR client bot-flagged from datacenter IPs,
no PoToken, decipher unproven live). NOT done: repo creation, first
Actions build, any run on a real Windows PC. The user's PC test is
the acceptance check for everything. Phase 4 (downloads, lyrics, EQ,
library persistence, SMTC, Listen Together, sleep timer) starts only
after the core player (search + home + playback + queue) is proven
on his PC.

Goal: a real, signed Windows installer for Aello that plays music. Each
phase ends with a verification gate. Nothing is claimed done until its
gate passes on a real Windows machine.

## Phase 0 — Decisions (before any code)

Decisions the user must make:

1. **Distribution route** — direct download (own site/GitHub Releases,
   fastest, no store review) vs. Microsoft Store (discoverability, but
   store review + policy risk for an unofficial client). Recommendation:
   direct download first; Store later only if policy review passes.
2. **Signing certificate** — EV vs. standard code-signing certificate,
   or unsigned first internal builds with SmartScreen warnings accepted
   during testing. A signed installer is required for a public release.
3. **Auto-update mechanism** — Tauri updater plugin + a release feed
   (GitHub Releases, same pattern as the Android in-app updater), or
   manual re-download. The settings toggle already exists; the mechanism
   does not.
4. **Backend approach** — port the InnerTube client into Rust (chosen
   direction, see README) vs. a local companion proxy service. Rust port
   is recommended: no extra process, no CORS problem, one binary.
5. **Bundle identity** — confirm `com.averon.music` (already in
   `tauri.conf.json`, matching Android) and the display name "Aello".
6. **Icon artwork** — supply the Aello logo master (same artwork as
   Android) so `tauri icon` can generate the `.ico` set.
7. **Wi-Fi sync scope** — the Android tree already ships an Electron
   companion (`desktop/`) that syncs playlists with the phone over LAN.
   Decide whether the Windows player should also offer LAN sync, or stay
   a standalone player. Default: standalone; sync is a separate decision.

Gate: all seven answered in writing.

## Phase 1 — Windows workstation + first build

1. Windows 10/11 PC. Install: Rust stable (rustup), Tauri CLI
   (`cargo install tauri-cli` or the npm CLI), Visual Studio Build
   Tools with the C++ workload (linker), WebView2 runtime (preinstalled
   on current Windows; verify), Node.js only if a frontend build step is
   added later (not needed for this scaffold).
2. Copy this `aello-windows/` tree to the PC. Run `cargo tauri dev`.
3. Fix whatever the CLI reports: config schema drift, capability
   permissions, icon handling. Regenerate icons with `tauri icon`
   from the supplied artwork.
4. Smoke test: window opens at 1200×800, sidebar navigation works,
   theme/accent switching applies everywhere, settings save and reload
   across restarts, empty states render, no console errors.

Gate: the app runs, and every control on every view does something
honest (navigates, saves, or says plainly that its backend is missing).

## Phase 2 — Frontend hardening (still no backend)

1. Decide: keep framework-free files or migrate to Vite + a framework.
   Migration is allowed only if it pays for itself (component reuse with
   the web app, if one is built).
2. Finish the detail views (playlist/album track lists wired to the
   player queue), queue management (reorder/remove), volume + mute,
   seek + time display polish, keyboard shortcuts beyond Space.
3. Accessibility pass: focus order, aria labels, visible focus, color
   contrast on both themes.
4. Add a local-files mode: play audio files from disk (real files the
   user picks) through the same player bar. This gives the app a working
   playback path before the network backend lands.

Gate: local-file playback works end to end; the interface has no dead
buttons.

## Phase 3 — Music backend (the hard phase)

Port the Android InnerTube client into Rust, in this order:

1. HTTP session: `reqwest` with a WEB_REMIX-style client payload;
   session creation + visitor data handling. No credentials stored.
2. `browse` (home feed sections) and `search` endpoints → the existing
   `MusicService` trait. Wire the real implementation behind the trait
   (the stub was removed — the trait is implemented directly by the
   InnerTube session).
3. `player` endpoint + signature deciphering with cached player JS.
   Gate: a spread of real tracks (popular, age-restricted-adjacent,
   ciphered, long) resolve to playable URLs.
4. Format selection for WebView2: prefer AAC/MP4; if the catalogue
   forces Opus/WebM, implement the MSE/DASH path in the frontend
   instead of shipping silent failures.
5. Error surfaces: every failure shows the real reason in the UI
   (network, blocked, no stream) — the Android app's "silent no-op"
   failure mode must not be repeated.

Gate: home feed, search, playlist/album pages, and playback work for
real tracks on a real Windows machine, with visible errors for the rest.

## Phase 4 — Feature parity

In priority order, each behind its own verification:

1. Downloads + offline playback (Rust filesystem downloads, integrity
   checks, storage management, offline queue).
2. Synced lyrics (LRCLIB-style provider, synced + plain fallback,
   offline cache).
3. Equalizer via the Web Audio API (`BiquadFilter` chain, presets,
   persisted bands).
4. Library persistence (SQLite via `tauri-plugin-sql`: playlists,
   history, liked songs, local files index).
5. SMTC + media keys + taskbar mini-player hooks (uses the existing
   Rust playback snapshot).
6. Listen Together client (only if the room protocol is documented and
   the user wants it; otherwise explicitly cut).
7. Sleep timer, theme-propagation audit (every surface follows the
   accent selection), splash/about polish.

Gate per feature: works on the machine, survives restart, fails
visibly instead of silently.

## Phase 5 — Packaging, signing, release

1. Bundle: `cargo tauri build` with the NSIS target; verify the
   installer (per-user install, Start Menu entry, uninstall clean,
   WebView2 bootstrapper behavior on a machine without WebView2).
2. Sign the installer and binaries with the certificate from Phase 0;
   verify the signature and SmartScreen behavior on a clean machine.
3. Auto-update: wire the updater plugin to the release feed; verify an
   update from the previous build installs in place.
4. Release checklist: version bump discipline (match Android
   versioning), GPL license + attribution notices in the installer and
   About view, changelog, VirusTotal/sandbox smoke run, install test
   on Windows 10 and Windows 11.
5. Distribution: publish the installer where Phase 0 decided, and link
   it from the Aello website's Windows download option (the website
   currently shows Windows as unplanned — update it only when the
   installer is real).

## Policy note (read before Phase 5)

Aello's music comes from an unofficial YouTube Music client. That
carries real risk: Microsoft Store review may reject it, and direct
distribution does not remove the underlying terms-of-service tension.
Do not promise Store acceptance, and do not present the app as
affiliated with YouTube, Google, or Spotify. The About view already
states this. If policy blocks a route, the fallback is direct download
with the same disclosures — never a renamed or disguised listing.

## What is deliberately out of scope

- macOS/Linux builds (separate decisions; the Tauri core would mostly
  carry over, but bundling/signing differ).
- Cast support (no Cast SDK for this stack; cut unless revisited).
- The Android app's Listen Together server (not ported unless Phase 4
  item 6 is approved).
