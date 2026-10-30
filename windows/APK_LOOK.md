# APK Visual Reference — Aello Windows 6.1.45

**Rule (user order 2026-10-30 23:11 +06): the .exe must carry the SAME
professional look as the APK.** Same colors, same branding, same density
of polish — only the layout adapts to desktop (sidebar / bottom player /
queue panel). If any desktop screen looks cheaper or emptier than its APK
counterpart, that is a defect.

**Reference screenshots (current APK):** `~/workspace/aello-website-deploy/img/`
- `home.jpg` — Home: "Good evening" + chips + Daily Discover + Recently Played
- `cover.jpg` — Player (Cover tab): artwork-tinted page, big art, controls
- `lyrics.jpg` — Player (Lyrics tab): synced lines, current line bright
- `library.jpg` — Library: auto-playlist cards + frosted mini player
- `search.jpg` — Search: Explore/Suggestions/Album segmented control + Moods & moments + Genres
- `trending.jpg` — Search Suggestions tab: Trending Music Videos
- `videos.jpg` — Home video sections: horizontal rows + Play all

## Exact tokens (from Android source `ui/theme/AppleTokens.kt`)

| Token | Value | Notes |
|---|---|---|
| Brand red (`PulseElectric`) | `#FB2831` | Wordmark, active states, Play all |
| Accent red | `#FA2D48` | Secondary red accents |
| Background (dark) | `#08080A` | Page background — near-black, NOT pure black |
| Background elevated | `#111113` | Sheets, raised surfaces |
| Card | `#18181B` | Cards |
| Card secondary | `#222226` | Nested cards, chips |
| Metadata / muted text | `#94A3B8` | Subtitles, counts |
| Frosted glass surface | `rgba(255,255,255,0.10)` | Mini player, bottom nav pills |
| Frosted glass elevated | `rgba(255,255,255,0.16)` | Pressed/emphasized frosted |
| Frosted glass border | `rgba(255,255,255,0.15)` | Frosted pill borders |
| Frosted glass border subtle | `rgba(255,255,255,0.08)` | Dividers on frosted |
| Light bg / card | `#F8F9FA` / `#FFFFFF` | Light theme only |
| Light metadata | `#64748B` | Light theme muted text |
| Neutral fallback (per-entity) | `#1C1C1E` | When Palette yields nothing |

## Shapes (AppleTokens.kt — "four corners, no more")

- Artwork corner: **12px**. Control corner: **12px**.
- Card corner: **22px**. Large card corner: **28px**.
- All corners continuous (squircle-like) — never mix circular and
  continuous corners on the same screen. Artwork is the only rounded
  thing on a content tile.

## Spacing (AppleTokens.kt — the whole scale)

- Screen gutter: **20px** (the only gutter). Item gap: **16px**.
- Section gap: **24px**. Text gap inside an item: **2px**.

## Type scale (AppleTokens.kt — fixed sizes, each with its own line height)

- Screen title: **34px / 41px**. Section header: **22px / 28px**.
- Tile title / row primary: **15px / 20px**. Tile subtitle / metadata:
  **13px / 18px**. Captions: **12px / 16px**.
- Display face: **Chillax** (artist titles) + **Outfit** (brand wordmark
  and display headers). Headings in the screenshots ("Good evening",
  "Search", "Moods & moments") are the quirky display face — on desktop
  use a heavy rounded display stack; the exact Android fonts are bundled
  in the APK, not on the PC, so approximate with system fonts:
  display stack `"Outfit", "Plus Jakarta Sans", system-ui, sans-serif`
  at 700–800 weight. Body: `"Plus Jakarta Sans", "Google Sans Flex",
  system-ui, sans-serif`. Never fetch fonts from the network at runtime.

## Motion (AppleTokens.kt)

- Default: smooth spring, 0.5s period, zero bounce. Cross-fades:
  200ms ease-in-out. Selection feedback / hero entrances may bounce
  slightly. In CSS: `transition: all 200ms ease` for fades,
  `cubic-bezier(0.22, 1, 0.36, 1)` 400–500ms for entrances.
  Respect `prefers-reduced-motion`.

## Signature surfaces (from the screenshots)

- **Frosted pills:** the mini player and the bottom nav are floating
  frosted-glass pills (glass surface + glass border + blur, large
  radius ~28px), NOT solid bars. The desktop bottom player bar must
  read the same way: a floating frosted pill above the content, with
  artwork thumb, title/artist, waveform/progress, play/pause/next.
  CSS `backdrop-filter: blur(...)` is the desktop equivalent of the
  Android kill-switch-safe frosted fallback (solid translucent surface,
  no native backdrop sampling) — it is standard Chromium compositing,
  not the Android native effect chain that crashed. If it ever causes
  trouble, fall back to solid translucent surfaces.
- **Per-entity color:** the player page background is ONE uniform color
  taken from the current artwork (cover.jpg is olive-brown from the
  DOPAMINE art; lyrics.jpg is deep blue from its art) with no second
  color intruding. Text on tinted backgrounds keeps the hue and pushes
  lightness to a readable extreme (see `onColor` in AppleTokens.kt).
  Each detail page re-extracts from ITS artwork.
- **Auto-playlist cards (library.jpg):** solid saturated color blocks
  with a white icon centered and a green circular play button
  overlapping the bottom-right corner; title + "Playlist" below.
  Colors from the screenshot (verify against
  `AutoPlaylistScreen.kt` if in doubt): Liked purple `#7C3AED`,
  Downloaded green `#22C55E`, My top 50 rose `#F43F5E`,
  Cached blue `#3B82F6`, Uploaded violet `#8B5CF6`, Local amber `#F59E0B`.
- **Chips:** pill outlines; active chip = red outline + red text (home.jpg
  "All") or filled dark with red text (segmented control: active segment
  is a lighter pill with red text, see search.jpg).
- **"Play all"**: red text + red play triangle, top-right of a section.
- **Login:** pill button top-right ("Login" with an arrow icon), same
  frosted style.
- **Search:** segmented control (Explore/Suggestions/Album) as one
  frosted pill with three segments; "Moods & moments" and "Genres" as
  2-column grids of dark cards (22px corners) with the label bottom-left.
- **Lyrics:** current line bright white, large and bold; other lines
  dimmed to ~40% (same hue family as the page tint); generous line
  spacing; screen stays on the current line centered.
- **Video rows:** horizontal scrolling rows of 16:9 thumbnails with
  title/artist below; "Play all" per section.

## Desktop adaptations (the ONLY things that change)

- Left sidebar navigation instead of the floating bottom nav (the
  floating nav becomes the sidebar; the mini player stays a floating
  frosted pill at the bottom).
- Content uses the desktop width: grids flow to more columns, detail
  pages get a two-column header (art left, info right).
- Everything else — colors, corners, type, chips, frosted pills,
  play buttons, density of polish — stays identical to the APK.
