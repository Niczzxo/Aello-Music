# DOM Contract — Aello Windows 6.1.45

The UI builder creates these elements; the JS builder uses ONLY these ids.
Neither side renames an id. If an id is missing, the JS side must degrade
gracefully (guard with a null check), never crash.

## Existing ids — KEEP ALL (v0.1.0)

`bridge-notice`, `search-input`, `search-btn`, `account-state`,
`view-home`, `home-status`, `home-sections`,
`view-library`, `library-list`, `library-open-files`,
`view-settings`, `settings-form`, `setting-appearance`, `setting-volume`,
`setting-updates`, `settings-status`, `download-dir-display`,
`view-player`, `fp-artwork`, `fp-title`, `fp-artist`, `fp-progress`,
`fp-current-time`, `fp-duration`, `fp-prev`, `fp-play`, `fp-next`,
`fp-mute`, `fp-queue`,
`view-about`,
`pb-artwork`, `pb-title`, `pb-artist`, `pb-prev`, `pb-play`, `pb-next`,
`pb-current-time`, `pb-progress`, `pb-duration`, `pb-mute`,
`pb-open-files`, `pb-volume`, `pb-expand`, `pb-status`, `audio`.

Nav buttons use `data-view="home|search|library|trending|together|settings|about|player"`.
Library segments use `data-segment="playlists|albums|artists|songs|downloads|liked|history"`.

## New views (UI builder creates, JS builder fills)

- `view-search` — contains `search-tabs` (buttons `data-search-tab="all|songs|albums|artists|playlists"`), `search-results`, `search-moods` (Moods & moments grid; each card is a `<button class="mood-card" data-mood-params="...">`), `search-history` (list + `search-history-clear`).
- `view-trending` — contains `trending-status`, `trending-sections`.
- `view-artist` — contains `artist-back` (`data-view="home"`), `artist-header` (artwork `artist-art`, name `artist-name`, meta `artist-meta`, buttons `artist-play`, `artist-shuffle`, `artist-subscribe`), `artist-songs` (track list), `artist-albums`, `artist-related`.
- `view-together` — contains `lt-username`, `lt-server`, `lt-create`, `lt-join-code`, `lt-join`, `lt-status`, `lt-room` (hidden until joined: `lt-room-code`, `lt-control-mode` select owner/everyone, `lt-members`, `lt-chat-log`, `lt-chat-input`, `lt-chat-send`, `lt-suggest`, `lt-leave`).
- `view-downloads` is a Library segment, not a separate view (see segments above).

## New player-bar controls (all `<button type="button">` with aria-labels)

- `pb-shuffle`, `pb-repeat`, `pb-like`, `pb-queue-toggle`, `pb-lyrics-toggle`,
  `pb-eq-toggle`, `pb-sleep`, `pb-download`, `pb-speed`.
- `queue-panel` (aside, toggled by `pb-queue-toggle`): contains `queue-list`,
  `queue-clear`, `queue-shuffle-play`.
- Repeat button shows its state via `data-repeat="off|all|one"` + visible label
  (text, not color alone). Shuffle shows `aria-pressed`.

## Full player additions (inside `view-player`)

- `fp-shuffle`, `fp-repeat`, `fp-like`, `fp-download`, `fp-speed`,
  `fp-sleep`, `fp-eq-toggle`, `fp-lyrics-toggle`.
- `lyrics-panel`: contains `lyrics-provider`, `lyrics-lines` (scroll container;
  each line is a `<button class="lyric-line" data-time-ms="...">`),
  `lyrics-offset` (range −5000..5000 step 100, label `lyrics-offset-label`),
  `lyrics-scroll-current` (re-enable auto-scroll button), `lyrics-status`.
- `eq-panel`: contains `eq-enhance` (checkbox, Sound Enhance),
  `eq-presets` (select), `eq-save-preset`, `eq-delete-preset`,
  `eq-bands` (5 band rows, each: label, `input.eq-gain[data-band]`,
  `input.eq-freq[data-band]`, `select.eq-type[data-band]`), `eq-preamp`,
  `eq-reset`.
- `sleep-dialog` (`<dialog>` or div with `hidden`): `sleep-title` (heading,
  used by `aria-labelledby`), `sleep-options` buttons
  `data-minutes="15|30|45|60|90|-1"` (−1 = end of song), `sleep-cancel`,
  `sleep-status` (countdown text, also mirrored in `pb-sleep` title).

## Update dialog

- `update-dialog`: `update-title`, `update-notes`, `update-version`,
  `update-download`, `update-later`, `update-status`.
- Settings adds a `Check for updates now` button: `settings-check-updates`
  + `settings-update-status`.

## Settings additions (inside `settings-form`)

- `setting-quality` (select auto/high/low), `setting-crossfade` (checkbox),
  `setting-crossfade-duration` (range 1..10s), `setting-persistent-queue`
  (checkbox), `setting-auto-download-like` (checkbox),
  `setting-listen-together-server` (select: global/usa/custom + text input
  `setting-lt-custom-server`).
- Account block: `account-block` with `account-signin`, `account-signout`,
  `account-channel` (select, hidden unless multi-channel).

## Rules

- No emoji anywhere in the UI (player glyphs, mute, queue actions, sleep,
  download — all inline SVG, Lucide-style, 2px stroke, 24px default).
- All artwork `<img>` gets `alt=""` (decorative) or a real alt for the
  now-playing art. All icon-only buttons get `aria-label`.
- Focus visible on every interactive element. `prefers-reduced-motion`
  respected (disable the stagger animation).
- No element may be created by JS with an id not listed here without
  adding it to this file first.
