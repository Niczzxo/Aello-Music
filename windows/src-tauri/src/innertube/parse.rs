//! Response parsing: generic JSON navigation plus the renderer → model
//! mapping ported from the Android pages (`SearchPage`, `HomePage`,
//! `PlaylistPage`, `AlbumPage`) and models (`Runs.splitBySeparator`,
//! `oddElements`, `parseTime`, `getThumbnailUrl`).
//!
//! Everything is defensive: a missing optional field degrades the item
//! (or drops it) instead of failing the whole response.

use crate::models::{Album, AlbumRef, Artist, Artwork, HomeItem, Playlist, Song};
use serde_json::Value;

// ── generic JSON navigation ──────────────────────────────────────────────

/// Walk `v["a"]["b"]…`; `None` if any step is missing/null.
pub fn get<'a>(mut v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    for k in keys {
        v = v.get(*k)?;
        if v.is_null() {
            return None;
        }
    }
    Some(v)
}

/// `v[key]` as `&str`.
pub fn s<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

/// The `runs` array of a `{"text": {"runs": [...]}}` node.
pub fn runs<'a>(v: &'a Value) -> Vec<&'a Value> {
    get(v, &["text", "runs"])
        .and_then(|r| r.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

/// The `runs` array of a bare `{"runs": [...]}` node (two-row titles,
/// subtitles, header straplines — Android's `Runs` type directly).
pub fn direct_runs<'a>(v: &'a Value) -> Vec<&'a Value> {
    v.get("runs")
        .and_then(|r| r.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

/// Text of the first run of a bare `{"runs": [...]}` node.
pub fn first_direct_run_text(v: &Value) -> Option<String> {
    direct_runs(v)
        .first()
        .and_then(|r| s(r, "text"))
        .map(|t| t.to_string())
}

/// Text of the first run of `v.text.runs`.
pub fn first_run_text(v: &Value) -> Option<String> {
    runs(v)
        .first()
        .and_then(|r| s(r, "text"))
        .map(|t| t.to_string())
}

/// Split runs on the ` • ` separator run (Android `splitBySeparator`).
pub fn split_by_separator<'a>(runs: &[&'a Value]) -> Vec<Vec<&'a Value>> {
    let mut out: Vec<Vec<&'a Value>> = Vec::new();
    let mut cur: Vec<&'a Value> = Vec::new();
    for r in runs {
        if s(r, "text") == Some(" • ") {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(r);
        }
    }
    out.push(cur);
    out
}

/// Even-indexed elements (Android `oddElements` — misnamed upstream).
pub fn odd_elements<'a, T: Clone>(items: &[T]) -> Vec<T> {
    items
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .map(|(_, v)| v.clone())
        .collect()
}

/// `"m:ss"` / `"h:mm:ss"` → seconds (Android `String.parseTime`).
pub fn parse_time(text: &str) -> Option<u64> {
    let parts: Vec<&str> = text.split(':').collect();
    let nums: Option<Vec<u64>> = parts.iter().map(|p| p.parse().ok()).collect();
    let nums = nums?;
    match nums.len() {
        2 => Some(nums[0] * 60 + nums[1]),
        3 => Some(nums[0] * 3600 + nums[1] * 60 + nums[2]),
        _ => None,
    }
}

pub fn upgrade_thumbnail_url(url: &str) -> String {
    if url.contains("googleusercontent.com") || url.contains("ggpht.com") {
        if let Some(idx) = url.find("=w") {
            return format!("{}=w800-h800-l90-rj", &url[..idx]);
        }
        if let Some(idx) = url.find("=s") {
            return format!("{}=s800", &url[..idx]);
        }
    }
    url.to_string()
}

fn artworks(v: &Value) -> Vec<Artwork> {
    let thumbs = get(v, &["thumbnail", "thumbnails"])
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
    thumbs
        .iter()
        .filter_map(|t| {
            Some(Artwork {
                url: upgrade_thumbnail_url(s(t, "url")?),
                width: t.get("width").and_then(|w| w.as_u64()).map(|w| w as u32),
                height: t.get("height").and_then(|h| h.as_u64()).map(|h| h as u32),
            })
        })
        .collect()
}

/// All thumbnails of a `musicThumbnailRenderer` node as `Artwork`s.
pub fn music_artworks(v: &Value) -> Vec<Artwork> {
    v.get("musicThumbnailRenderer")
        .map(artworks)
        .unwrap_or_default()
}

/// Type labels YouTube prefixes to a subtitle's first segment in the
/// unfiltered (guest) search shape, e.g.
/// `Song • Guru Randhawa • …` / `Video • Channel • 55M views`.
/// The filtered shape Android parses has no such prefix for songs, so
/// this is only dropped when it is actually there: a single run with no
/// navigation endpoint whose text is a known label.
const TYPE_LABELS: &[&str] = &[
    "Song", "Video", "Album", "Single", "EP", "Playlist", "Artist", "Podcast", "Episode",
    "Audiobook",
];

/// Drop a leading type-label segment from `splitBySeparator` output.
fn drop_type_label<'a>(segments: Vec<Vec<&'a Value>>) -> Vec<Vec<&'a Value>> {
    if segments.len() < 2 {
        return segments;
    }
    let first = &segments[0];
    if first.len() == 1 {
        let run = first[0];
        let text = s(run, "text").unwrap_or("");
        if TYPE_LABELS.contains(&text) && run.get("navigationEndpoint").is_none() {
            return segments.into_iter().skip(1).collect();
        }
    }
    segments
}

/// `navigationEndpoint.browseEndpoint.browseEndpointContextSupportedConfigs
/// .browseEndpointContextMusicConfig.pageType`
pub fn page_type(v: &Value) -> Option<&str> {
    get(
        v,
        &[
            "navigationEndpoint",
            "browseEndpoint",
            "browseEndpointContextSupportedConfigs",
            "browseEndpointContextMusicConfig",
            "pageType",
        ],
    )
    .and_then(|p| p.as_str())
}

pub fn browse_id(v: &Value) -> Option<String> {
    get(v, &["navigationEndpoint", "browseEndpoint", "browseId"])
        .and_then(|b| b.as_str())
        .map(|b| b.to_string())
}

pub fn watch_video_id(v: &Value) -> Option<String> {
    get(v, &["navigationEndpoint", "watchEndpoint", "videoId"])
        .and_then(|b| b.as_str())
        .map(|b| b.to_string())
}

fn artist_from_run(run: &Value) -> Artist {
    Artist {
        id: browse_id(run).unwrap_or_default(),
        name: s(run, "text").unwrap_or_default().to_string(),
    }
}

/// Flex columns of a `musicResponsiveListItemRenderer`:
/// `flexColumns[i].musicResponsiveListItemFlexColumnRenderer`.
fn flex_column<'a>(v: &'a Value, i: usize) -> Option<&'a Value> {
    v.get("flexColumns")?
        .as_array()?
        .get(i)?
        .get("musicResponsiveListItemFlexColumnRenderer")
}

fn fixed_column<'a>(v: &'a Value, i: usize) -> Option<&'a Value> {
    v.get("fixedColumns")?
        .as_array()?
        .get(i)?
        .get("musicResponsiveListItemFlexColumnRenderer")
}

fn overlay_play_endpoint<'a>(v: &'a Value) -> Option<&'a Value> {
    get(
        v,
        &[
            "overlay",
            "musicItemThumbnailOverlayRenderer",
            "content",
            "musicPlayButtonRenderer",
            "playNavigationEndpoint",
        ],
    )
}

fn song_video_id(v: &Value) -> Option<String> {
    get(v, &["playlistItemData", "videoId"])
        .and_then(|x| x.as_str())
        .map(|x| x.to_string())
        .or_else(|| watch_video_id(v))
        .or_else(|| {
            overlay_play_endpoint(v).and_then(|e| {
                get(e, &["watchEndpoint", "videoId"])
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string())
            })
        })
        .or_else(|| {
            // Title run itself may carry the watch endpoint.
            flex_column(v, 0)
                .and_then(|c| runs(c).into_iter().next())
                .and_then(watch_video_id)
        })
}

fn is_explicit(v: &Value) -> bool {
    v.get("badges")
        .and_then(|b| b.as_array())
        .map(|badges| {
            badges.iter().any(|b| {
                get(
                    b,
                    &[
                        "musicInlineBadgeRenderer",
                        "icon",
                        "iconType",
                    ],
                )
                .and_then(|t| t.as_str())
                    == Some("MUSIC_EXPLICIT_BADGE")
            })
        })
        .unwrap_or(false)
}

fn song_count(text: &str) -> Option<usize> {
    // e.g. "123 songs" / "1 song" / "123 tracks"
    text.split_whitespace()
        .next()
        .and_then(|n| n.replace(',', "").parse().ok())
}

// ── musicResponsiveListItemRenderer → HomeItem (SearchPage.toYTItem) ───

/// Port of `SearchPage.toYTItem`. Artist rows are intentionally
/// skipped in v0.1.0: the shared `HomeItem` enum has no artist variant
/// and there is no artist page to open one from (see `models.rs`).
pub fn parse_list_item(v: &Value) -> Option<HomeItem> {
    let nav = v.get("navigationEndpoint");
    let is_song = nav.is_none()
        || nav.and_then(|n| n.get("watchEndpoint")).is_some()
        || nav
            .and_then(|n| n.get("watchPlaylistEndpoint"))
            .is_some();
    let pt = page_type(v);
    let is_playlist = pt == Some("MUSIC_PAGE_TYPE_PLAYLIST");
    let is_album = pt == Some("MUSIC_PAGE_TYPE_ALBUM") || pt == Some("MUSIC_PAGE_TYPE_AUDIOBOOK");
    let is_artist =
        pt == Some("MUSIC_PAGE_TYPE_ARTIST") || pt == Some("MUSIC_PAGE_TYPE_LIBRARY_ARTIST");

    let title = flex_column(v, 0).and_then(first_run_text)?;
    let secondary: Vec<Vec<&Value>> = flex_column(v, 1)
        .map(|c| drop_type_label(split_by_separator(&runs(c))))
        .unwrap_or_default();
    let thumb = v
        .get("thumbnail")
        .map(music_artworks)
        .unwrap_or_default();

    // Check the artist pageType BEFORE the song branch: is_song is true
    // whenever nav is absent, so a nav-less artist row would otherwise
    // be classified as a song and only dropped because song_video_id
    // returns None.
    if is_artist {
        // Artist rows are intentionally skipped in v0.1.0 (no artist
        // page yet) — see the doc comment above.
        None
    } else if is_song {
        let id = song_video_id(v)?;
        let artists = secondary
            .first()
            .map(|seg| odd_elements(seg).iter().map(|r| artist_from_run(r)).collect())
            .unwrap_or_default();
        let album = secondary.get(1).and_then(|seg| seg.first()).and_then(|run| {
            browse_id(run).map(|bid| AlbumRef {
                id: bid,
                title: s(run, "text").unwrap_or_default().to_string(),
            })
        });
        let duration = secondary
            .last()
            .and_then(|seg| seg.first())
            .and_then(|r| s(r, "text"))
            .and_then(parse_time);
        let _ = is_explicit(v);
        Some(HomeItem::Song(Song {
            id,
            title,
            artists,
            album,
            duration_seconds: duration,
            artwork: thumb,
            stream_url: None,
        }))
    } else if is_album {
        let browse = browse_id(v)?;
        // After the label drop: [artists, year?, …]. (Android reads
        // artists at index 1 because the "Album" label sits at 0 there.)
        let artists = secondary
            .first()
            .map(|seg| odd_elements(seg).iter().map(|r| artist_from_run(r)).collect())
            .unwrap_or_default();
        let year = secondary
            .get(1)
            .or_else(|| secondary.get(2))
            .and_then(|seg| seg.first())
            .and_then(|r| s(r, "text"))
            .and_then(|t| t.parse().ok());
        Some(HomeItem::Album(Album {
            id: browse,
            title,
            artists,
            year,
            artwork: thumb,
        }))
    } else if is_playlist {
        let id = browse_id(v)?.trim_start_matches("VL").to_string();
        let count = flex_column(v, 1).and_then(|c| {
            let rs = runs(c);
            rs.last().and_then(|r| s(r, "text")).and_then(song_count)
        });
        Some(HomeItem::Playlist(Playlist {
            id,
            title,
            description: None,
            artwork: thumb,
            song_count: count,
        }))
    } else {
        None
    }
}

// ── musicTwoRowItemRenderer → HomeItem (HomePage.Section) ──────────────

/// Port of `HomePage.Section.fromMusicTwoRowItemRenderer`.
pub fn parse_two_row_item(v: &Value) -> Option<HomeItem> {
    let title = v.get("title").and_then(first_direct_run_text)?;
    let subtitle_runs = v.get("subtitle").map(direct_runs).unwrap_or_default();
    let subtitle_odd = odd_elements(&subtitle_runs);
    let thumb = v
        .get("thumbnailRenderer")
        .map(music_artworks)
        .unwrap_or_default();
    let nav = v.get("navigationEndpoint")?;
    let is_song = nav.get("watchEndpoint").is_some();
    let pt = page_type(v);
    let is_playlist = pt == Some("MUSIC_PAGE_TYPE_PLAYLIST");
    let is_album = pt == Some("MUSIC_PAGE_TYPE_ALBUM") || pt == Some("MUSIC_PAGE_TYPE_AUDIOBOOK");
    let is_artist = pt == Some("MUSIC_PAGE_TYPE_ARTIST");

    if is_song {
        let id = watch_video_id(v)?;
        let artists: Vec<Artist> = subtitle_odd
            .iter()
            .filter(|run| {
                browse_id(run)
                    .map(|b| b.starts_with("UC"))
                    .unwrap_or(false)
                    || (browse_id(run).is_some()
                        && !browse_id(run)
                            .map(|b| b.starts_with("MPREb_"))
                            .unwrap_or(false))
            })
            .map(|run| artist_from_run(run))
            .collect();
        let artists = if artists.is_empty() {
            subtitle_odd
                .first()
                .map(|run| vec![artist_from_run(run)])
                .unwrap_or_default()
        } else {
            artists
        };
        let album = subtitle_odd.iter().find(|run| {
            browse_id(run)
                .map(|b| b.starts_with("MPREb_"))
                .unwrap_or(false)
        }).and_then(|run| {
            browse_id(run).map(|bid| AlbumRef {
                id: bid,
                title: s(run, "text").unwrap_or_default().to_string(),
            })
        });
        Some(HomeItem::Song(Song {
            id,
            title,
            artists,
            album,
            duration_seconds: None,
            artwork: thumb,
            stream_url: None,
        }))
    } else if is_album {
        let browse = browse_id(v)?;
        let artists = subtitle_odd
            .iter()
            .skip(1)
            .map(|run| artist_from_run(run))
            .collect();
        Some(HomeItem::Album(Album {
            id: browse,
            title,
            artists,
            year: None,
            artwork: thumb,
        }))
    } else if is_playlist {
        let id = browse_id(v)?.trim_start_matches("VL").to_string();
        Some(HomeItem::Playlist(Playlist {
            id,
            title,
            description: None,
            artwork: thumb,
            song_count: None,
        }))
    } else if is_artist {
        // Artist rows are intentionally skipped in v0.1.0 (no artist
        // page yet) — see parse_list_item.
        None
    } else {
        None
    }
}

// ── playlist / album shelf songs ────────────────────────────────────────

/// Port of `PlaylistPage.fromMusicResponsiveListItemRenderer`, lenient:
/// no `setVideoId` requirement, thumbnail optional.
pub fn parse_playlist_song(v: &Value) -> Option<Song> {
    let id = song_video_id(v)?;
    let title = flex_column(v, 0).and_then(first_run_text)?;
    let secondary: Vec<Vec<&Value>> = flex_column(v, 1)
        .map(|c| drop_type_label(split_by_separator(&runs(c))))
        .unwrap_or_default();
    let artists = secondary
        .first()
        .map(|seg| odd_elements(seg).iter().map(|r| artist_from_run(r)).collect())
        .unwrap_or_default();
    let album = flex_column(v, 2)
        .and_then(|c| runs(c).into_iter().next())
        .and_then(|run| {
            browse_id(run).map(|bid| AlbumRef {
                id: bid,
                title: s(run, "text").unwrap_or_default().to_string(),
            })
        });
    let duration = fixed_column(v, 0)
        .and_then(|c| runs(c).into_iter().next())
        .and_then(|r| s(r, "text"))
        .and_then(parse_time);
    let thumb = v
        .get("thumbnail")
        .map(music_artworks)
        .unwrap_or_default();
    Some(Song {
        id,
        title,
        artists,
        album,
        duration_seconds: duration,
        artwork: thumb,
        stream_url: None,
    })
}

/// Port of `AlbumPage.getSong`: same as a playlist song, but the album ref
/// is forced to the parent album and duration is required upstream.
pub fn parse_album_song(v: &Value, album: Option<&Album>) -> Option<Song> {
    let mut song = parse_playlist_song(v)?;
    if let Some(a) = album {
        song.album = Some(AlbumRef {
            id: a.id.clone(),
            title: a.title.clone(),
        });
        if song.artwork.is_empty() {
            song.artwork = a.artwork.clone();
        }
    }
    Some(song)
}

/// Continuation token inside a shelf `contents` array
/// (`continuationItemRenderer.continuationEndpoint.continuationCommand.token`).
pub fn shelf_continuation(contents: &Value) -> Option<String> {
    contents.as_array()?.iter().find_map(|item| {
        get(
            item,
            &[
                "continuationItemRenderer",
                "continuationEndpoint",
                "continuationCommand",
                "token",
            ],
        )
        .and_then(|t| t.as_str())
        .map(|t| t.to_string())
    })
}

/// Next-continuation token from a continuation response
/// (`continuations[0].nextContinuationData.continuation`).
pub fn next_continuation(v: &Value) -> Option<String> {
    v.get("continuations")?
        .as_array()?
        .first()
        .and_then(|c| get(c, &["nextContinuationData", "continuation"]))
        .and_then(|t| t.as_str())
        .map(|t| t.to_string())
}
