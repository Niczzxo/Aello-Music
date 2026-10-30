<div align="center">
  <h1>Aello</h1>
  <h3>An open-source music player for Android</h3>

  <p>
    <a href="https://aello-music.vercel.app">Website</a> ·
    <a href="https://github.com/Niczzxo/Aello-Music/releases/latest">Download the latest release</a>
  </p>
  <p>Current version: 6.1.40. Works on Android 8.0 and above.</p>
</div>

## What it is

Aello is an Android music player built on YouTube Music. It plays songs,
albums, playlists, artists and charts, keeps your library and downloads on
the device, and signs in with your Google account so your YouTube Music
library, playlists and history follow you. The app is written in Kotlin
with Jetpack Compose.

## Features

### Home

- Daily Discover, always first on the Home feed, built from your liked
  songs, recent plays and most played tracks.
- Keep Listening (your true recent plays), Quick Picks, Forgotten
  Favorites, recommendations similar to what you play, charts, and
  moods and genres.
- Mood and genre filter chips (Energize, Relax, Feel good, and more)
  from YouTube's own filters.
- Your account playlists and community playlists on the same feed.

### Library

- Auto playlists: Liked, Downloaded, My Top 50, Cached, Uploaded and
  Local. Each can be shown or hidden in settings.
- Your own playlists, plus songs, albums and artists from your library.
- Local music: songs stored on your phone, including local folders.
- Downloads live on the device and are never auto-deleted by the app.

### Search

- Online search across songs, albums, artists, playlists and videos
  from YouTube Music, with search suggestions.
- Local search across the music stored on your device.

### Player

- Full player and mini player, queue, shuffle, repeat, and a sleep
  timer.
- Synced lyrics (LRC format) from several lyrics providers, with
  translation support.
- Background playback and offline playback of downloaded songs.
- Google Cast support, home screen widgets, and Android Auto.
- Discord Rich Presence (optional, in settings).
- Listen Together (Aello Sync): listen in sync with other people in a
  room, with a room code and host controls.

### Sound

- Parametric EQ with custom profiles and a built-in equalizer screen.
- Sound Enhance (on by default): an automatic enhancement applied on
  playback when no custom EQ profile is active. The curve is a gentle
  low-frequency lift (80 Hz low-shelf, +3.0 dB), a little warmth
  (300 Hz, +1.0 dB), a presence lift in the vocal range (3000 Hz,
  +2.0 dB), a touch of air (8000 Hz high-shelf, +1.5 dB), and a
  -3.0 dB preamp so boosted bands cannot clip. A custom EQ profile
  always wins over Sound Enhance, and it can be switched off in
  Settings, then Player, then Sound Enhance.
- Loudness normalization (on by default).
- 8D Spatial Audio toggle (panning effect, no head tracking).

### Detail pages

- Playlist, artist, album, charts, explore and history pages.
- Each playlist and artist page takes its colour from its own artwork,
  and the whole page is that one colour from top to bottom. When a page
  has no artwork, it uses your selected theme colour.

### Appearance

- Theme presets, Day and Night modes, and a dynamic app icon that
  follows your theme colour and mode.
- Font choice, player theme and custom player icons.
- Home feed order can be changed in settings.

### Sign-in

- One Google sign-in, done in an embedded WebView. The app keeps the
  session so your library and history work, and you can sign out from
  account settings.
- Your Gmail password is never asked for or stored by the app.

### Settings and updates

- Player, Content, Privacy, Storage, Backup and restore, and About
  settings.
- Listen history can be paused in Settings, then Privacy.
- Playback logs can be shared from Settings, then Content, then Logs.
- Check for updates in Settings, then System Update. Updates download
  and install inside the app, with no browser involved.

## What is not in the app

- No Spotify. The Spotify integration was fully removed in 6.1.37.
- No video section. Video playback was removed in 6.1.12; Aello is
  music only.
- No iOS build and no Windows download yet. Only source scaffolds exist
  for those platforms.

## Screenshots

Screenshots of the current version will be added here. The screenshots
kept in `docs/screenshots` are old upstream images from before the Aello
rebrand and do not show the current app.

## Installation

Get the latest APK from the
[releases page](https://github.com/Niczzxo/Aello-Music/releases/latest)
and install it on your phone. You can also get it from the
[Aello website](https://aello-music.vercel.app), which always points to
the latest release. New releases install over previous ones,
so your library, downloads and settings stay in place. The app is
distributed as a GitHub release APK only.

## Tech stack

- Kotlin with Jetpack Compose and Material 3
- Media3 / ExoPlayer for playback, downloads and casting
- Hilt for dependency injection, Room for the local database
- InnerTube (YouTube Music) with NewPipe-based helpers for playback
  data, search and lyrics
- WorkManager for in-app update downloads

## Building

Open the project in Android Studio (JDK 21, Android SDK 37) and run the
`app` module, or build with Gradle:

```sh
./gradlew assembleDebug
```

Note: the Liquid Glass blur effect is currently switched off in builds
(`LIQUID_GLASS_KILL_SWITCH` in `ui/component/GlassEffect.kt`) while a
native crash in the blur path is investigated. The app uses theme
colours and translucent chrome instead.

## Credits and license

GPL-3.0. See `LICENSE`.

Aello is based on the open-source PulseWave project, which is based on
the open-source BeatWave project by vortexapps67. BeatWave itself is a
fork of the CONVX project.
