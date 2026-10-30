# Contributing to Aello

Thanks for considering a contribution to **Aello**. This document is the real, current process — if something here goes stale, please send a PR fixing it rather than adding a second guide.

## Project Layout

Aello is a single Android Studio project (Gradle multi-module). The pieces you'll actually touch:

* **`app/`** — the app itself. Compose UI lives under `app/src/main/kotlin/com/beatwave/music/ui/`, screens under `ui/screens/`, `viewmodels/` per screen, navigation wired in `ui/screens/NavigationBuilder.kt`.
* **`ui/component/GlassEffect.kt`** + **`ui/component/backdrop/`** — the Liquid Glass system (a vendored, source-included copy of [Kyant0/backdrop](https://github.com/Kyant0/backdrop)). `Modifier.liquidGlass(...)` is the entry point most UI code needs; see the doc comment on `GlassCircleButton` for a worked example of a glass surface sampling a local backdrop. Note: the glass effect chain is currently switched off in builds (`LIQUID_GLASS_KILL_SWITCH` in `ui/component/GlassEffect.kt`) while a native crash in the blur path is investigated, so do not re-enable it for looks.
* **`innertube/`** — the unofficial YouTube Music (InnerTube) API client, kept independent of the app module.
* Other top-level modules (`kugou`, `lrclib`, `kizzy`, `lastfm`, `betterlyrics`, `simpmusic`, `youlyplus`, `shazamkit`, ...) are individually-scoped integrations — a lyrics provider, rich presence, Last.fm, etc. Keep changes to one of these scoped to that module. There is no Spotify module: the Spotify integration was fully removed in 6.1.37.

## Building locally

* **JDK 21**, latest stable **Android Studio**.
* `compileSdk 37`, `minSdk 26` — install those platforms via the SDK Manager if Android Studio prompts you to.
* Clone and open the project in Android Studio, let Gradle sync, then run/debug the `app` module as normal (`./gradlew assembleDebug` from the CLI works too).

## Branches & Commits

* Branch names: `feature/short-description`, `fix/short-description`.
* Commit messages: a short type prefix (`feat:`, `fix:`, `refactor:`, `docs:`) followed by an imperative summary — see `git log` for the house style. Explain *why* in the body when it isn't obvious from the diff.

## Pull Requests

1. Fork the repo and branch off `main`.
2. Keep the diff scoped to the PR's stated purpose — avoid drive-by reformatting or unrelated refactors, they make review harder.
3. Actually run the change on a device/emulator before opening the PR; this is a UI-heavy app and most regressions here are visual, not compile-time.
4. Describe *what* changed and *why* in the PR description; screenshots/recordings are expected for anything UI-visible.

## Reporting Bugs

Open an [issue](https://github.com/Niczzxo/Aello-Music/issues) with repro steps, your Android version/device, and `adb logcat` output if it's a crash. For anything real-time (playback glitches, Listen Together sync), a screen recording helps a lot.

## Questions

Open an [issue](https://github.com/Niczzxo/Aello-Music/issues) before starting anything large — happy to point you at the right file instead of you reverse-engineering it.
