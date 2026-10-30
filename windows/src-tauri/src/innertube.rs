//! InnerTube / YouTube Music backend for Aello Windows.
//!
//! Pure-Rust port of the Android app's InnerTube client
//! (`innertube/` module + `YTPlayerUtils` resolve chain). Guest only:
//! no login, no cookies, no PoToken — only `visitorData` from
//! `sw.js_data`.
//!
//! Wiring (parent adds to `lib.rs`):
//! ```ignore
//! mod innertube;
//! use innertube::InnerTubeSession;
//! // AppState: replace `music: StubMusicService` with
//! // `music: InnerTubeSession` — it implements `MusicService`.
//! ```

pub mod cipher;
pub mod client;
pub mod error;
pub mod parse;
pub mod resolve;
pub mod session;

pub use error::{StreamError, StreamUrl};
pub use session::InnerTubeSession;

use crate::models::{Album, AlbumDetail, HomeItem, HomeSection, Playlist, PlaylistDetail};
use crate::music_service::{MusicService, ServiceResult};
use parse::{
    direct_runs, get, music_artworks, next_continuation, odd_elements, parse_album_song,
    parse_list_item, parse_playlist_song, parse_two_row_item, shelf_continuation,
};
use serde_json::Value;

/// Recursively collect every `musicResponsiveListItemRenderer` under a
/// JSON subtree (search results nest them in shelves and in the top-result
/// card; a recursive walk is robust to YouTube's shape drift).
fn collect_list_items(v: &Value, out: &mut Vec<Value>) {
    match v {
        Value::Object(map) => {
            if let Some(item) = map.get("musicResponsiveListItemRenderer") {
                out.push(item.clone());
            }
            for child in map.values() {
                collect_list_items(child, out);
            }
        }
        Value::Array(arr) => {
            for child in arr {
                collect_list_items(child, out);
            }
        }
        _ => {}
    }
}

fn section_list_contents(response: &Value) -> Vec<Value> {
    // Browse/search responses put the section list under either the
    // single-column or the two-column results renderer.
    for path in [
        &[
            "contents",
            "singleColumnBrowseResultsRenderer",
            "tabs",
        ][..],
        &["contents", "twoColumnBrowseResultsRenderer", "tabs"][..],
    ] {
        if let Some(tabs) = get(response, path).and_then(|t| t.as_array()) {
            if let Some(tab) = tabs.first() {
                if let Some(contents) = get(
                    tab,
                    &["tabRenderer", "content", "sectionListRenderer", "contents"],
                )
                .and_then(|c| c.as_array())
                {
                    return contents.clone();
                }
            }
        }
    }
    Vec::new()
}

fn dedupe_songs(items: Vec<HomeItem>) -> Vec<HomeItem> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let key = match &item {
            HomeItem::Song(s) => format!("s:{}", s.id),
            HomeItem::Album(a) => format!("a:{}", a.id),
            HomeItem::Playlist(p) => format!("p:{}", p.id),
        };
        if seen.insert(key) {
            out.push(item);
        }
    }
    out
}

/// Extract shelf items + next continuation token from a playlist-shelf
/// browse response. First page: `…secondaryContents…musicPlaylistShelfRenderer`;
/// later pages: `continuationContents.musicPlaylistShelfContinuation` or
/// `onResponseReceivedActions[0].appendContinuationItemsAction`.
fn shelf_page(response: &Value, first_page: bool) -> (Vec<Value>, Option<String>) {
    if first_page {
        let contents = get(
            response,
            &[
                "contents",
                "twoColumnBrowseResultsRenderer",
                "secondaryContents",
                "sectionListRenderer",
                "contents",
            ],
        )
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .and_then(|c| c.get("musicPlaylistShelfRenderer"))
        .and_then(|s| s.get("contents"))
        .cloned()
        .unwrap_or(Value::Null);
        let items = contents
            .as_array()
            .cloned()
            .unwrap_or_default();
        let next = shelf_continuation(&contents);
        return (items, next);
    }
    let cont = response
        .get("continuationContents")
        .and_then(|c| c.get("musicPlaylistShelfContinuation"));
    if let Some(c) = cont {
        let items = c
            .get("contents")
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();
        let next = next_continuation(c);
        return (items, next);
    }
    let appended = response
        .get("onResponseReceivedActions")
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|a| a.get("appendContinuationItemsAction"))
        .and_then(|a| a.get("continuationItems"))
        .cloned()
        .unwrap_or(Value::Null);
    let items = appended.as_array().cloned().unwrap_or_default();
    let next = shelf_continuation(&appended);
    (items, next)
}

impl InnerTubeSession {
    /// Search YouTube Music (WEB_REMIX). Merges every shelf on the first
    /// results tab — songs, albums and playlists. Artists are skipped:
    /// the shared `HomeItem` enum has no artist variant.
    pub async fn search(&self, query: &str) -> Result<Vec<HomeItem>, StreamError> {
        let response = self.search_raw(query).await?;
        let contents = get(
            &response,
            &[
                "contents",
                "tabbedSearchResultsRenderer",
                "tabs",
            ],
        )
        .and_then(|t| t.as_array())
        .and_then(|tabs| tabs.first())
        .and_then(|tab| {
            get(
                tab,
                &["tabRenderer", "content", "sectionListRenderer", "contents"],
            )
        })
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
        let mut raw_items = Vec::new();
        for content in &contents {
            collect_list_items(content, &mut raw_items);
        }
        let items: Vec<HomeItem> = raw_items.iter().filter_map(parse_list_item).collect();
        Ok(dedupe_songs(items))
    }

    /// Home feed (`FEmusic_home`, WEB_REMIX) → carousel sections.
    pub async fn home_feed(&self) -> Result<Vec<HomeSection>, StreamError> {
        let response = self.browse_raw(Some("FEmusic_home"), None).await?;
        let contents = section_list_contents(&response);
        let mut sections = Vec::new();
        for content in &contents {
            let Some(shelf) = content.get("musicCarouselShelfRenderer") else {
                continue;
            };
            let title = get(
                shelf,
                &[
                    "header",
                    "musicCarouselShelfBasicHeaderRenderer",
                    "title",
                ],
            )
            .and_then(|t| {
                t.get("runs")
                    .and_then(|r| r.as_array())
                    .and_then(|a| a.first())
                    .and_then(|r| r.get("text"))
                    .and_then(|t| t.as_str())
            })
            .or_else(|| {
                // Some shelves carry a plain `title` object.
                get(
                    shelf,
                    &["header", "musicCarouselShelfBasicHeaderRenderer", "title"],
                )
                .and_then(|t| t.get("simpleText"))
                .and_then(|t| t.as_str())
            });
            let Some(title) = title else { continue };
            let subtitle = get(
                shelf,
                &[
                    "header",
                    "musicCarouselShelfBasicHeaderRenderer",
                    "strapline",
                ],
            )
            .and_then(|s| {
                s.get("runs")
                    .and_then(|r| r.as_array())
                    .and_then(|a| a.first())
                    .and_then(|r| r.get("text"))
                    .and_then(|t| t.as_str())
            })
            .map(|t| t.to_string());
            let items: Vec<HomeItem> = shelf
                .get("contents")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    let mut items: Vec<HomeItem> = arr
                        .iter()
                        .filter_map(|c| c.get("musicTwoRowItemRenderer"))
                        .filter_map(parse_two_row_item)
                        .collect();
                    // Deviation from Android: guest home serves the
                    // "Quick picks" shelf as musicResponsiveListItemRenderer
                    // rows, not two-row cards. Android's home parser drops
                    // that shelf entirely; include the rows so the feed
                    // is not missing its top section.
                    items.extend(
                        arr.iter()
                            .filter_map(|c| c.get("musicResponsiveListItemRenderer"))
                            .filter_map(parse_list_item),
                    );
                    dedupe_songs(items)
                })
                .unwrap_or_default();
            if items.is_empty() {
                continue;
            }
            sections.push(HomeSection {
                title: title.to_string(),
                subtitle,
                items,
            });
        }
        Ok(sections)
    }

    /// A playlist (`VL{id}` browse) and its songs. Mixes (`RD…`) take
    /// the same `VL` prefix — their browseId is `VLRD…` (Android does
    /// `browse("VL$playlistId")` for every playlist id).
    pub async fn playlist(&self, id: &str) -> Result<PlaylistDetail, StreamError> {
        let browse_id = if let Some(stripped) = id.strip_prefix("VL") {
            format!("VL{stripped}")
        } else {
            format!("VL{id}")
        };
        let response = self.browse_raw(Some(&browse_id), None).await?;
        let two_col = get(&response, &["contents", "twoColumnBrowseResultsRenderer"]);
        let header_content = two_col
            .and_then(|t| get(t, &["tabs"]))
            .and_then(|t| t.as_array())
            .and_then(|tabs| tabs.first())
            .and_then(|tab| {
                get(
                    tab,
                    &["tabRenderer", "content", "sectionListRenderer", "contents"],
                )
            })
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .cloned();
        let header = header_content
            .as_ref()
            .and_then(|c| c.get("musicResponsiveHeaderRenderer"))
            .or_else(|| {
                header_content.as_ref().and_then(|c| {
                    get(
                        c,
                        &[
                            "musicEditablePlaylistDetailHeaderRenderer",
                            "header",
                            "musicResponsiveHeaderRenderer",
                        ],
                    )
                })
            });
        let title = header
            .and_then(|h| h.get("title"))
            .and_then(|t| {
                t.get("runs")
                    .and_then(|r| r.as_array())
                    .and_then(|a| a.first())
                    .and_then(|r| r.get("text"))
                    .and_then(|t| t.as_str())
            })
            .unwrap_or("")
            .to_string();
        // (Dead code removed: the header author was computed here and
        // discarded — `Playlist` has no author field.)
        let count_text = header
            .and_then(|h| h.get("secondSubtitle"))
            .and_then(|s| {
                s.get("runs")
                    .and_then(|r| r.as_array())
                    .and_then(|a| a.first())
                    .and_then(|r| r.get("text"))
                    .and_then(|t| t.as_str())
            })
            .unwrap_or("")
            .to_string();
        let song_count = count_text
            .split_whitespace()
            .next()
            .and_then(|n| n.replace(',', "").parse().ok());
        let artwork = header
            .and_then(|h| h.get("thumbnail"))
            .map(music_artworks)
            .unwrap_or_default();
        let description = header
            .and_then(|h| h.get("description"))
            .and_then(|d| {
                d.get("runs")
                    .and_then(|r| r.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
                            .collect::<String>()
                    })
            });

        let shelf = two_col
            .and_then(|t| get(t, &["secondaryContents", "sectionListRenderer", "contents"]))
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .and_then(|c| c.get("musicPlaylistShelfRenderer"));
        let mut songs: Vec<crate::models::Song> = shelf
            .and_then(|s| s.get("contents"))
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.get("musicResponsiveListItemRenderer"))
                    .filter_map(parse_playlist_song)
                    .collect()
            })
            .unwrap_or_default();
        // Follow the shelf continuation (cap: 10 extra pages), same
        // shapes as album continuations (`shelf_page`).
        let mut next = shelf
            .and_then(|s| s.get("contents"))
            .and_then(|c| shelf_continuation(c));
        let mut pages = 0;
        while let Some(token) = next.take() {
            if pages >= 10 {
                break;
            }
            pages += 1;
            let page = self.browse_raw(None, Some(&token)).await?;
            let (items, nxt) = shelf_page(&page, false);
            for item in &items {
                if let Some(r) = item.get("musicResponsiveListItemRenderer") {
                    if let Some(song) = parse_playlist_song(r) {
                        songs.push(song);
                    }
                }
            }
            next = nxt;
        }
        Ok(PlaylistDetail {
            playlist: Playlist {
                id: id.to_string(),
                title,
                description,
                artwork,
                song_count,
            },
            songs,
        })
    }

    /// An album (`MPREb_…` browse id) and its songs (via `VL{playlistId}`
    /// + continuations, like Android's `albumSongs`).
    pub async fn album(&self, id: &str) -> Result<AlbumDetail, StreamError> {
        let response = self.browse_raw(Some(id), None).await?;
        let header = section_list_contents(&response)
            .into_iter()
            .find_map(|c| c.get("musicResponsiveHeaderRenderer").cloned());
        let title = header
            .as_ref()
            .and_then(|h| h.get("title"))
            .and_then(|t| {
                t.get("runs")
                    .and_then(|r| r.as_array())
                    .and_then(|a| a.first())
                    .and_then(|r| r.get("text"))
                    .and_then(|t| t.as_str())
            })
            .unwrap_or("")
            .to_string();
        let artists = header
            .as_ref()
            .and_then(|h| h.get("straplineTextOne"))
            .map(|s| {
                odd_elements(&direct_runs(s))
                    .iter()
                    .map(|r| crate::models::Artist {
                        id: parse::browse_id(r).unwrap_or_default(),
                        name: r
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let year = header
            .as_ref()
            .and_then(|h| h.get("subtitle"))
            .and_then(|s| {
                s.get("runs")
                    .and_then(|r| r.as_array())
                    .and_then(|a| a.last())
                    .and_then(|r| r.get("text"))
                    .and_then(|t| t.as_str())
                    .and_then(|t| t.parse().ok())
            });
        let artwork = header
            .as_ref()
            .and_then(|h| h.get("thumbnail"))
            .map(music_artworks)
            .unwrap_or_default();
        // Album playlist id from the canonical URL's `list=` query
        // param, parsed properly: `rsplit('=')` took the text after the
        // LAST '=' (the `si=` value on `…?list=OLAK…&si=…`), which made
        // album_songs browse `VL<wrong-id>`. A `list=` value that is
        // not an `OLAK…` album playlist is a hard Parse error — the
        // fallback below only covers a missing canonical URL.
        let playlist_id = (|| -> Result<Option<String>, StreamError> {
            let Some(canonical) = get(
                &response,
                &["microformat", "microformatDataRenderer", "urlCanonical"],
            )
            .and_then(|u| u.as_str()) else {
                return Ok(None);
            };
            let url = url::Url::parse(canonical)
                .map_err(|e| StreamError::Parse(format!("album urlCanonical: {e}")))?;
            match url.query_pairs().find(|(k, _)| k == "list") {
                Some((_, list)) if list.starts_with("OLAK") => Ok(Some(list.into_owned())),
                Some((_, list)) => Err(StreamError::Parse(format!(
                    "album urlCanonical list param is not an album playlist id: {list}"
                ))),
                None => Ok(None),
            }
        })()?
        .or_else(|| {
            // Fallback: top-level button watchPlaylistEndpoint.
            get(
                &response,
                &[
                    "header",
                    "musicDetailHeaderRenderer",
                    "menu",
                    "menuRenderer",
                    "topLevelButtons",
                ],
            )
            .and_then(|b| b.as_array())
            .and_then(|a| a.first())
            .and_then(|b| {
                get(
                    b,
                    &[
                        "buttonRenderer",
                        "navigationEndpoint",
                        "watchPlaylistEndpoint",
                        "playlistId",
                    ],
                )
            })
            .and_then(|p| p.as_str())
            .map(|s| s.to_string())
        });
        let album = Album {
            id: id.to_string(),
            title,
            artists,
            year,
            artwork,
        };
        let mut songs = Vec::new();
        if let Some(pid) = playlist_id {
            songs = self.album_songs(&pid, &album).await?;
        }
        Ok(AlbumDetail { album, songs })
    }

    /// Songs of an album's playlist, following continuations (cap: 10
    /// extra requests, Android allows 50).
    async fn album_songs(
        &self,
        playlist_id: &str,
        album: &Album,
    ) -> Result<Vec<crate::models::Song>, StreamError> {
        let mut songs = Vec::new();
        let mut response = self
            .browse_raw(Some(&format!("VL{playlist_id}")), None)
            .await?;
        for page in 0..11 {
            let (items, next) = shelf_page(&response, page == 0);
            for item in &items {
                if let Some(r) = item.get("musicResponsiveListItemRenderer") {
                    if let Some(song) = parse_album_song(r, Some(album)) {
                        songs.push(song);
                    }
                }
            }
            let Some(token) = next else { break };
            response = self.browse_raw(None, Some(&token)).await?;
        }
        Ok(songs)
    }

    /// Resolve a playable stream URL for a song (see `resolve`).
    pub async fn resolve_stream(&self, video_id: &str) -> Result<StreamUrl, StreamError> {
        resolve::resolve_stream(self, video_id).await
    }
}

impl MusicService for InnerTubeSession {
    async fn home_feed(&self) -> ServiceResult<Vec<HomeSection>> {
        self.home_feed().await.map_err(|e| e.to_string())
    }

    async fn search(&self, query: &str) -> ServiceResult<Vec<HomeItem>> {
        self.search(query).await.map_err(|e| e.to_string())
    }

    async fn playlist(&self, id: &str) -> ServiceResult<PlaylistDetail> {
        self.playlist(id).await.map_err(|e| e.to_string())
    }

    async fn album(&self, id: &str) -> ServiceResult<AlbumDetail> {
        self.album(id).await.map_err(|e| e.to_string())
    }

    async fn resolve_stream(&self, song_id: &str) -> ServiceResult<Option<String>> {
        match self.resolve_stream(song_id).await {
            Ok(url) => Ok(Some(url.url)),
            Err(e) => Err(e.to_string()),
        }
    }
}
