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

use crate::models::{
    AccountChannel, Album, AlbumDetail, Artist, ArtistDetail, ExploreData, HomeChip, HomeFeed,
    HomeItem, HomeSection, LyricLine, MoodItem, Playlist, PlaylistDetail, SearchPage, Song,
};
use crate::music_service::{MusicService, ServiceResult};
use parse::{
    carousel_more_endpoint, carousel_title, direct_runs, get, music_artworks, nav_button_mood,
    next_continuation, odd_elements, parse_album_song, parse_list_item, parse_playlist_song,
    parse_two_row_item, shelf_continuation, text_of,
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

/// Android `SearchFilter` param values (`YouTube.kt:1479`). Only the
/// four filters the contract exposes; anything else means "all".
fn search_filter_params(filter: &str) -> Option<&'static str> {
    match filter {
        // Accept plurals too: the frontend search tabs send
        // "songs" | "albums" | "artists" | "playlists".
        "song" | "songs" => Some("EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D"),
        "album" | "albums" => Some("EgWKAQIYAWoKEAkQChAFEAMQBA%3D%3D"),
        "artist" | "artists" => Some("EgWKAQIgAWoKEAkQChAFEAMQBA%3D%3D"),
        // Featured playlists (Android FILTER_FEATURED_PLAYLIST).
        "playlist" | "playlists" => Some("EgeKAQQoADgBagwQDhAKEAMQBRAJEAQ%3D"),
        _ => None,
    }
}

fn dedupe_songs(items: Vec<HomeItem>) -> Vec<HomeItem> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let key = match &item {
            HomeItem::Song(s) => format!("s:{}", s.id),
            HomeItem::Album(a) => format!("a:{}", a.id),
            HomeItem::Playlist(p) => format!("p:{}", p.id),
            HomeItem::Artist(a) => format!("r:{}", a.id),
        };
        if seen.insert(key) {
            out.push(item);
        }
    }
    out
}

/// Parse one `musicCarouselShelfRenderer` (or `gridRenderer`) content
/// node into a section. Shared by home feed, charts, and explore so the
/// three pages parse shelves identically.
fn parse_carousel_section(content: &Value) -> Option<HomeSection> {
    if let Some(shelf) = content.get("musicCarouselShelfRenderer") {
        let title = carousel_title(shelf)?;
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
            return None;
        }
        let subtitle = get(
            shelf,
            &[
                "header",
                "musicCarouselShelfBasicHeaderRenderer",
                "strapline",
            ],
        )
        .and_then(text_of);
        return Some(HomeSection {
            title: title.to_string(),
            subtitle,
            items,
        });
    }
    if let Some(grid) = content.get("gridRenderer") {
        let title = get(grid, &["header", "gridHeaderRenderer", "title"]).and_then(text_of)?;
        let items: Vec<HomeItem> = grid
            .get("items")
            .and_then(|c| c.as_array())
            .map(|arr| {
                dedupe_songs(
                    arr.iter()
                        .filter_map(|c| c.get("musicTwoRowItemRenderer"))
                        .filter_map(parse_two_row_item)
                        .collect(),
                )
            })
            .unwrap_or_default();
        if items.is_empty() {
            return None;
        }
        return Some(HomeSection {
            title,
            subtitle: None,
            items,
        });
    }
    None
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
/// Search YouTube Music (WEB_REMIX). With a filter, the `params`
    /// string is Android's `SearchFilter` value and results come from
    /// the first `musicShelfRenderer` (Android `YouTube.search`); without
    /// a filter every shelf on the first results tab is merged — songs,
    /// albums, playlists, and artist rows.
    /// Returns the page + the shelf continuation token (Android
    /// `SearchResult.continuation`), if YouTube served one.
    pub async fn search(
        &self,
        query: &str,
        filter: Option<&str>,
    ) -> Result<SearchPage, StreamError> {
        let params = filter.and_then(search_filter_params);
        let response = self.search_raw_params(query, params).await?;
        if params.is_some() {
            // Filtered shape: one musicShelfRenderer holds the rows.
            let shelf_contents = get(
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
            .and_then(|contents| {
                contents.iter().find_map(|c| {
                    c.get("musicShelfRenderer")
                        .and_then(|s| s.get("contents"))
                        .and_then(|c| c.as_array())
                        .cloned()
                })
            });
            if let Some(contents) = shelf_contents {
                let items: Vec<HomeItem> = contents
                    .iter()
                    .filter_map(|c| c.get("musicResponsiveListItemRenderer"))
                    .filter_map(parse_list_item)
                    .collect();
                // The shelf's own continuations carry the next page.
                let continuation = get(
                    &response,
                    &["contents", "tabbedSearchResultsRenderer", "tabs"],
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
                .and_then(|contents| {
                    contents.iter().find_map(|c| {
                        c.get("musicShelfRenderer")
                            .and_then(|s| s.get("continuations"))
                            .and_then(next_continuation)
                    })
                });
                return Ok(SearchPage {
                    items: dedupe_songs(items),
                    continuation,
                });
            }
            // Fall through to the unfiltered parse if YouTube served a
            // different shape than Android expects.
        }
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
        // Unfiltered ("All") has no single shelf: take the first shelf
        // continuation found, if any — load-more then continues that
        // shelf, which is the best the merged view can offer.
        let continuation = contents.iter().find_map(|c| {
            c.get("musicShelfRenderer")
                .and_then(|s| s.get("continuations"))
                .and_then(next_continuation)
        });
        Ok(SearchPage {
            items: dedupe_songs(items),
            continuation,
        })
    }

    /// Next search page (Android `YouTube.searchContinuation`): the
    /// response carries `continuationContents.musicShelfContinuation`.
    pub async fn search_more(&self, continuation: &str) -> Result<SearchPage, StreamError> {
        let response = self.search_raw_continuation(continuation).await?;
        let shelf = response
            .get("continuationContents")
            .and_then(|c| c.get("musicShelfContinuation"));
        let items: Vec<HomeItem> = shelf
            .and_then(|s| s.get("contents"))
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.get("musicResponsiveListItemRenderer"))
                    .filter_map(parse_list_item)
                    .collect()
            })
            .unwrap_or_default();
        let next = shelf.and_then(|s| s.get("continuations")).and_then(next_continuation);
        let items = dedupe_songs(items);
        let empty = items.is_empty();
        Ok(SearchPage {
            items,
            continuation: if empty { None } else { next },
        })
    }

    /// Home feed (`FEmusic_home`, WEB_REMIX) → chips + carousel
    /// sections + continuation (Android `YouTube.home` → `HomePage`).
    /// `params` is a chip's browse params; None is the unfiltered feed.
    /// A chip tap MUST re-fetch with that chip's params (Android
    /// `HomeViewModel.toggleChip`); "All" re-fetches with None.
    pub async fn home_feed(&self, params: Option<&str>) -> Result<HomeFeed, StreamError> {
        let response = self
            .browse_raw_params(Some("FEmusic_home"), params, None)
            .await?;
        let section_list = get(
            &response,
            &[
                "contents",
                "singleColumnBrowseResultsRenderer",
                "tabs",
            ],
        )
        .and_then(|t| t.as_array())
        .and_then(|tabs| tabs.first())
        .and_then(|tab| get(tab, &["tabRenderer", "content", "sectionListRenderer"]));
        // Chips live in the section-list header
        // (`chipCloudRenderer.chips[].chipCloudChipRenderer`: title from
        // text.runs[0], params from
        // navigationEndpoint.browseEndpoint.params).
        let chips: Vec<HomeChip> = section_list
            .and_then(|s| get(s, &["header", "chipCloudRenderer", "chips"]))
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|chip| {
                        let renderer = chip.get("chipCloudChipRenderer")?;
                        let title = renderer
                            .get("text")
                            .and_then(|t| t.get("runs"))
                            .and_then(|r| r.as_array())
                            .and_then(|a| a.first())
                            .and_then(|r| r.get("text"))
                            .and_then(|t| t.as_str())?
                            .to_string();
                        let params = get(
                            renderer,
                            &[
                                "navigationEndpoint",
                                "browseEndpoint",
                                "params",
                            ],
                        )
                        .and_then(|p| p.as_str())
                        .map(|p| p.to_string());
                        Some(HomeChip { title, params })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let continuation = section_list
            .and_then(|s| s.get("continuations"))
            .and_then(next_continuation);
        let contents = section_list_contents(&response);
        Ok(HomeFeed {
            chips,
            sections: contents
                .iter()
                .filter_map(parse_carousel_section)
                .collect(),
            continuation,
        })
    }

    /// Next home page (Android `YouTube.home` with a continuation):
    /// `continuationContents.sectionListContinuation.contents` holds
    /// more carousel shelves. Chips are not re-served here.
    pub async fn home_feed_more(&self, continuation: &str) -> Result<HomeFeed, StreamError> {
        let response = self.browse_raw(None, Some(continuation)).await?;
        let cont = response
            .get("continuationContents")
            .and_then(|c| c.get("sectionListContinuation"));
        let contents = cont
            .and_then(|c| c.get("contents"))
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        let next = cont.and_then(|c| c.get("continuations")).and_then(next_continuation);
        Ok(HomeFeed {
            chips: Vec::new(),
            sections: contents
                .iter()
                .filter_map(parse_carousel_section)
                .collect(),
            continuation: next,
        })
    }

    /// A signed-in library page (Android `YouTube.library(browseId,
    /// tabIndex)` → `LibraryPage`): the FIRST content of the tab's
    /// section list — a grid of two-row items (albums/playlists) or a
    /// shelf of list items (songs). `tab_index` selects the tab (0 =
    /// first; uploaded songs use tab 1 on
    /// `FEmusic_library_privately_owned_tracks`).
    pub async fn library_page(
        &self,
        browse_id: &str,
        tab_index: usize,
    ) -> Result<SearchPage, StreamError> {
        let response = self.browse_raw(Some(browse_id), None).await?;
        let tabs = get(
            &response,
            &[
                "contents",
                "singleColumnBrowseResultsRenderer",
                "tabs",
            ],
        )
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
        let contents = tabs
            .get(tab_index)
            .and_then(|tab| {
                get(
                    tab,
                    &["tabRenderer", "content", "sectionListRenderer", "contents"],
                )
            })
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .cloned();
        Ok(self.library_page_contents(contents))
    }

    /// Next library page (Android `YouTube.libraryContinuation`):
    /// `continuationContents.gridContinuation` (two-row items) or
    /// `musicShelfContinuation` (list items).
    pub async fn library_page_more(
        &self,
        continuation: &str,
    ) -> Result<SearchPage, StreamError> {
        let response = self.browse_raw(None, Some(continuation)).await?;
        let contents = response.get("continuationContents");
        if let Some(grid) = contents.and_then(|c| c.get("gridContinuation")) {
            let items: Vec<HomeItem> = grid
                .get("items")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| c.get("musicTwoRowItemRenderer"))
                        .filter_map(parse_two_row_item)
                        .collect()
                })
                .unwrap_or_default();
            let next = grid
                .get("continuations")
                .and_then(next_continuation);
            let items = dedupe_songs(items);
            let empty = items.is_empty();
            return Ok(SearchPage {
                items,
                continuation: if empty { None } else { next },
            });
        }
        let shelf = contents.and_then(|c| c.get("musicShelfContinuation"));
        let items: Vec<HomeItem> = shelf
            .and_then(|s| s.get("contents"))
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.get("musicResponsiveListItemRenderer"))
                    .filter_map(parse_list_item)
                    .collect()
            })
            .unwrap_or_default();
        let next = shelf.and_then(|s| s.get("continuations")).and_then(next_continuation);
        let items = dedupe_songs(items);
        let empty = items.is_empty();
        Ok(SearchPage {
            items,
            continuation: if empty { None } else { next },
        })
    }

    /// Parse one library-page content node (first section-list content)
    /// into items + continuation.
    fn library_page_contents(&self, contents: Option<Value>) -> SearchPage {
        let Some(content) = contents else {
            return SearchPage {
                items: Vec::new(),
                continuation: None,
            };
        };
        if let Some(grid) = content.get("gridRenderer") {
            let items: Vec<HomeItem> = grid
                .get("items")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| c.get("musicTwoRowItemRenderer"))
                        .filter_map(parse_two_row_item)
                        .collect()
                })
                .unwrap_or_default();
            let next = grid
                .get("continuations")
                .and_then(next_continuation);
            return SearchPage {
                items: dedupe_songs(items),
                continuation: next,
            };
        }
        if let Some(shelf) = content.get("musicShelfRenderer") {
            let items: Vec<HomeItem> = shelf
                .get("contents")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| c.get("musicResponsiveListItemRenderer"))
                        .filter_map(parse_list_item)
                        .collect()
                })
                .unwrap_or_default();
            let next = shelf
                .get("continuations")
                .and_then(next_continuation);
            return SearchPage {
                items: dedupe_songs(items),
                continuation: next,
            };
        }
        SearchPage {
            items: Vec::new(),
            continuation: None,
        }
    }

    /// Like / unlike a video on YouTube (Android `YouTube.likeVideo` →
    /// `InnerTube.likeVideo` / `unlikeVideo`). Requires a signed-in
    /// session (cookie); the caller keeps the local like regardless.
    pub async fn like_video(&self, video_id: &str, liked: bool) -> Result<(), StreamError> {
        self.like_video_raw(video_id, liked).await?;
        Ok(())
    }

    /// Subscribe / unsubscribe a channel (Android
    /// `YouTube.subscribeChannel` → `InnerTube.subscribeChannel` /
    /// `unsubscribeChannel`). Requires a signed-in session.
    pub async fn subscribe_channel(
        &self,
        channel_id: &str,
        subscribe: bool,
    ) -> Result<(), StreamError> {
        self.subscribe_raw(channel_id, subscribe).await?;
        Ok(())
    }

    /// Artist page (`YouTube.artist`, browseId). Sections are classified
    /// by title into top songs / albums / singles / related artists;
    /// unclassified all-song shelves fill top songs, unclassified
    /// all-album shelves fill albums.
    pub async fn artist(&self, browse_id: &str) -> Result<ArtistDetail, StreamError> {
        let response = self.browse_raw(Some(browse_id), None).await?;
        let header = ["musicImmersiveHeaderRenderer", "musicVisualHeaderRenderer"]
            .iter()
            .find_map(|k| get(&response, &["header", k]).cloned())
            .or_else(|| get(&response, &["header", "musicDetailHeaderRenderer"]).cloned())
            .or_else(|| get(&response, &["header", "musicHeaderRenderer"]).cloned());
        let title = header
            .as_ref()
            .and_then(|h| h.get("title"))
            .and_then(text_of)
            .unwrap_or_default();
        // NOTE: the header thumbnail exists but `Artist` carries no
        // artwork field (contract shape), so it is not read here.
        let monthly_listeners = header
            .as_ref()
            .and_then(|h| h.get("monthlyListenerCount"))
            .and_then(text_of);
        let subscriber_count = header.as_ref().and_then(|h| {
            get(
                h,
                &[
                    "subscriptionButton2",
                    "subscribeButtonRenderer",
                    "subscriberCountWithSubscribeText",
                ],
            )
            .and_then(text_of)
            .or_else(|| {
                get(
                    h,
                    &[
                        "subscriptionButton",
                        "subscribeButtonRenderer",
                        "longSubscriberCountText",
                    ],
                )
                .and_then(text_of)
            })
            .or_else(|| {
                get(
                    h,
                    &[
                        "subscriptionButton",
                        "subscribeButtonRenderer",
                        "shortSubscriberCountText",
                    ],
                )
                .and_then(text_of)
            })
        });
        let desc_from = |d: &Value| -> Option<String> {
            let s: String = direct_runs(d)
                .iter()
                .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
                .collect();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        };
        let description = section_list_contents(&response)
            .iter()
            .find_map(|c| c.get("musicDescriptionShelfRenderer"))
            .and_then(|s| s.get("description"))
            .and_then(desc_from)
            .or_else(|| {
                header
                    .as_ref()
                    .and_then(|h| h.get("description"))
                    .and_then(desc_from)
            });

        let mut top_songs: Vec<Song> = Vec::new();
        let mut albums: Vec<Album> = Vec::new();
        let mut singles: Vec<Album> = Vec::new();
        let mut related: Vec<Artist> = Vec::new();
        for content in &section_list_contents(&response) {
            let Some(section) = parse_carousel_section(content)
                .or_else(|| {
                    // musicShelfRenderer sections (top songs shelf).
                    content.get("musicShelfRenderer").and_then(|shelf| {
                        let title = shelf.get("title").and_then(text_of)?;
                        let items: Vec<HomeItem> = shelf
                            .get("contents")
                            .and_then(|c| c.as_array())
                            .map(|arr| {
                                dedupe_songs(
                                    arr.iter()
                                        .filter_map(|c| {
                                            c.get("musicResponsiveListItemRenderer")
                                        })
                                        .filter_map(parse_list_item)
                                        .collect(),
                                )
                            })
                            .unwrap_or_default();
                        if items.is_empty() {
                            return None;
                        }
                        Some(HomeSection {
                            title,
                            subtitle: None,
                            items,
                        })
                    })
                })
            else {
                continue;
            };
            let lower = section.title.to_lowercase();
            let kinds: Vec<&str> = section
                .items
                .iter()
                .map(|i| match i {
                    HomeItem::Song(_) => "song",
                    HomeItem::Album(_) => "album",
                    HomeItem::Playlist(_) => "playlist",
                    HomeItem::Artist(_) => "artist",
                })
                .collect();
            let all_songs = !kinds.is_empty() && kinds.iter().all(|k| *k == "song");
            let all_albums = !kinds.is_empty() && kinds.iter().all(|k| *k == "album");
            let all_artists = !kinds.is_empty() && kinds.iter().all(|k| *k == "artist");
            if all_artists
                || lower.contains("related")
                || lower.contains("similar")
                || lower.contains("fans also like")
            {
                for item in section.items {
                    if let HomeItem::Artist(a) = item {
                        related.push(a);
                    }
                }
            } else if lower.contains("single") {
                for item in section.items {
                    if let HomeItem::Album(a) = item {
                        singles.push(a);
                    }
                }
            } else if lower.contains("album") {
                for item in section.items {
                    if let HomeItem::Album(a) = item {
                        albums.push(a);
                    }
                }
            } else if lower.contains("video") {
                // Artist videos are not in the contract model; skip.
                continue;
            } else if all_songs {
                if top_songs.is_empty() {
                    for item in section.items {
                        if let HomeItem::Song(s) = item {
                            top_songs.push(s);
                        }
                    }
                }
            } else if all_albums {
                if albums.is_empty() {
                    for item in section.items {
                        if let HomeItem::Album(a) = item {
                            albums.push(a);
                        }
                    }
                }
            }
        }
        Ok(ArtistDetail {
            artist: Artist {
                id: browse_id.to_string(),
                name: title,
            },
            top_songs,
            albums,
            singles,
            related,
            monthly_listeners,
            subscriber_count,
            description,
        })
    }

    /// Charts page (`FEmusic_charts` + `ggMGCgQIgAQ%3D`, Android
    /// `YouTube.getChartsPage`). "Top music videos" is kept as its own
    /// section, exactly as served.
    pub async fn charts(&self) -> Result<Vec<HomeSection>, StreamError> {
        let response = self
            .browse_raw_params(Some("FEmusic_charts"), Some("ggMGCgQIgAQ%3D"), None)
            .await?;
        let contents = section_list_contents(&response);
        Ok(contents
            .iter()
            .filter_map(parse_carousel_section)
            .collect())
    }

    /// Explore page (`FEmusic_explore`, Android `YouTube.explore`):
    /// carousel sections + Mood & Genres chips (nav-button shelves and
    /// grid shelves both handled).
    pub async fn explore(&self) -> Result<ExploreData, StreamError> {
        let response = self.browse_raw(Some("FEmusic_explore"), None).await?;
        let contents = section_list_contents(&response);
        let sections: Vec<HomeSection> = contents
            .iter()
            .filter_map(parse_carousel_section)
            .collect();
        let mut moods: Vec<MoodItem> = Vec::new();
        for content in &contents {
            // Carousel shelf whose "more" button points at the moods page.
            if let Some(shelf) = content.get("musicCarouselShelfRenderer") {
                let is_moods = carousel_more_endpoint(shelf)
                    .map(|(id, _)| id == "FEmusic_moods_and_genres")
                    .unwrap_or(false);
                if is_moods {
                    if let Some(items) = shelf.get("contents").and_then(|c| c.as_array()) {
                        for item in items {
                            if let Some(btn) = item.get("musicNavigationButtonRenderer") {
                                if let Some((title, id, params)) = nav_button_mood(btn) {
                                    moods.push(MoodItem { id, title, params });
                                }
                            }
                        }
                    }
                }
            }
            // Grid shelf of nav buttons (MoodAndGenres page shape).
            if let Some(grid) = content.get("gridRenderer") {
                if let Some(items) = grid.get("items").and_then(|c| c.as_array()) {
                    for item in items {
                        if let Some(btn) = item.get("musicNavigationButtonRenderer") {
                            if let Some((title, id, params)) = nav_button_mood(btn) {
                                moods.push(MoodItem { id, title, params });
                            }
                        }
                    }
                }
            }
        }
        Ok(ExploreData { sections, moods })
    }

    /// Mood & Genres detail page (Android `YouTube.browse`, as used by
    /// `MoodAndGenresScreen` → `BrowseViewModel`): browse
    /// `FEmusic_moods_and_genres` with the mood's params, flattening the
    /// grid + carousel sections into items. The browseId is fixed: every
    /// mood nav-button endpoint carries `FEmusic_moods_and_genres` (the
    /// explore parser matches the moods shelf on exactly that id); the
    /// params are what distinguish one mood from another. An empty or
    /// unrecognized params value degrades to an empty list, never an
    /// error (the moods list page itself has no playable items).
    pub async fn browse_mood(&self, params: &str) -> Result<Vec<HomeItem>, StreamError> {
        // Empty / unrecognized params degrade to an empty list, never
        // an error (and never a wasted browse call). Network errors
        // from a real call still return Err, as they should.
        if params.trim().is_empty() {
            return Ok(Vec::new());
        }
        let response = self
            .browse_raw_params(Some("FEmusic_moods_and_genres"), Some(params), None)
            .await?;
        let contents = section_list_contents(&response);
        let mut items: Vec<HomeItem> = Vec::new();
        for content in &contents {
            if let Some(grid) = content.get("gridRenderer") {
                if let Some(arr) = grid.get("items").and_then(|c| c.as_array()) {
                    items.extend(
                        arr.iter()
                            .filter_map(|c| c.get("musicTwoRowItemRenderer"))
                            .filter_map(parse_two_row_item),
                    );
                }
            }
            if let Some(shelf) = content.get("musicCarouselShelfRenderer") {
                if let Some(arr) = shelf.get("contents").and_then(|c| c.as_array()) {
                    items.extend(
                        arr.iter()
                            .filter_map(|c| c.get("musicTwoRowItemRenderer"))
                            .filter_map(parse_two_row_item),
                    );
                }
            }
        }
        Ok(dedupe_songs(items))
    }

    /// `next` for a video id → the `related` browse endpoint
    /// (`browseId` + optional `params`), from tab 2 of the watch-next
    /// response (Android `YouTube.next` → `NextResult.relatedEndpoint`).
    pub async fn next_related_endpoint(
        &self,
        video_id: &str,
    ) -> Result<(String, Option<String>), StreamError> {
        let response = self.next_raw(video_id).await?;
        let tabs = get(
            &response,
            &[
                "contents",
                "singleColumnMusicWatchNextResultsRenderer",
                "tabbedRenderer",
                "watchNextTabbedResultsRenderer",
                "tabs",
            ],
        )
        .and_then(|t| t.as_array())
        .ok_or_else(|| {
            StreamError::Parse("next response has no watch-next tabs".to_string())
        })?;
        let endpoint = tabs
            .get(2)
            .and_then(|t| get(t, &["tabRenderer", "endpoint", "browseEndpoint"]))
            .ok_or_else(|| StreamError::Parse("next response has no related tab".to_string()))?;
        let browse_id = endpoint
            .get("browseId")
            .and_then(|b| b.as_str())
            .ok_or_else(|| StreamError::Parse("related endpoint has no browseId".to_string()))?
            .to_string();
        let params = endpoint
            .get("params")
            .and_then(|p| p.as_str())
            .map(|p| p.to_string());
        Ok((browse_id, params))
    }

    /// Related page for a browse endpoint (Android `YouTube.related`):
    /// songs only, from carousel / shelf / item-section contents.
    pub async fn related(
        &self,
        browse_id: &str,
        params: Option<&str>,
    ) -> Result<Vec<Song>, StreamError> {
        let response = self.browse_raw_params(Some(browse_id), params, None).await?;
        let mut contents = section_list_contents(&response);
        if contents.is_empty() {
            // `related` reads the top-level sectionListRenderer directly.
            contents = get(&response, &["contents", "sectionListRenderer", "contents"])
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
        }
        let mut songs = Vec::new();
        for content in &contents {
            for key in ["musicCarouselShelfRenderer", "musicShelfRenderer", "itemSectionRenderer"]
            {
                let Some(node) = content.get(key) else {
                    continue;
                };
                let Some(items) = node.get("contents").and_then(|c| c.as_array()) else {
                    continue;
                };
                for item in items {
                    if let Some(r) = item.get("musicResponsiveListItemRenderer") {
                        if let Some(HomeItem::Song(s)) = parse_list_item(r) {
                            songs.push(s);
                        }
                    } else if let Some(r) = item.get("musicTwoRowItemRenderer") {
                        if let Some(HomeItem::Song(s)) = parse_two_row_item(r) {
                            songs.push(s);
                        }
                    }
                }
            }
        }
        Ok(songs)
    }

    /// Timed-text transcript lines for a video (YouTube Subtitle lyrics
    /// fallback). Cue groups → `startOffsetMs` + `simpleText`, with the
    /// `♪` markers Android trims.
    pub async fn transcript(&self, video_id: &str) -> Result<Vec<LyricLine>, StreamError> {
        let response = self.transcript_raw(video_id).await?;
        let cue_groups = get(
            &response,
            &[
                "actions",
            ],
        )
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|act| {
            get(
                act,
                &[
                    "updateEngagementPanelAction",
                    "content",
                    "transcriptRenderer",
                    "body",
                    "transcriptBodyRenderer",
                    "cueGroups",
                ],
            )
        })
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
        let mut lines = Vec::new();
        for group in &cue_groups {
            let Some(cue) = get(
                group,
                &[
                    "transcriptCueGroupRenderer",
                    "cues",
                ],
            )
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .and_then(|c| c.get("transcriptCueRenderer")) else {
                continue;
            };
            let time_ms = cue
                .get("startOffsetMs")
                .and_then(|t| t.as_i64())
                .unwrap_or(0);
            let text = cue
                .get("cue")
                .and_then(|c| c.get("simpleText"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .trim_matches('♪')
                .trim()
                .to_string();
            if text.is_empty() {
                continue;
            }
            lines.push(LyricLine { time_ms, text });
        }
        Ok(lines)
    }

    /// Signed-in account `(display name, avatar URL)` via `account_menu`
    /// (Android `YouTube.accountInfo` →
    /// `activeAccountHeaderRenderer.toAccountInfo()`). The avatar is the
    /// response's own `accountPhoto.thumbnails` last URL — never guessed.
    /// Errors when there is no valid cookie.
    pub async fn account_info(&self) -> Result<(String, Option<String>), StreamError> {
        let response = self.account_menu_raw().await?;
        let action = response
            .get("actions")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first());
        let renderer = action.and_then(|act| {
            get(
                act,
                &[
                    "openPopupAction",
                    "popup",
                    "multiPageMenuRenderer",
                    "header",
                    "activeAccountHeaderRenderer",
                ],
            )
        });
        let name = renderer
            .and_then(|r| r.get("accountName"))
            .and_then(text_of)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                StreamError::Parse(
                    "account menu has no active account (cookie invalid or signed out)"
                        .to_string(),
                )
            })?;
        // Avatar: `accountPhoto.thumbnails` — last (largest) entry.
        let avatar_url = renderer
            .and_then(|r| r.get("accountPhoto"))
            .and_then(|p| p.get("thumbnails"))
            .and_then(|t| t.as_array())
            .and_then(|t| t.last())
            .and_then(|t| t.get("url"))
            .and_then(|u| u.as_str())
            .filter(|u| !u.is_empty())
            .map(|u| u.to_string());
        Ok((name, avatar_url))
    }

    /// Every YouTube channel the current cookie can act as (Android
    /// `YouTube.getAccountChannels` — `GET
    /// music.youtube.com/getAccountSwitcherEndpoint`). A channel without
    /// a `dataSyncId` is dropped, exactly as Android does (it cannot be
    /// selected). An empty list is legitimate (single-channel account)
    /// — only a real network/parse failure is an error. A non-SUCCESS
    /// `code` means the account has no switchable channels: empty list.
    pub async fn account_channels(&self) -> Result<Vec<AccountChannel>, StreamError> {
        let response = self.account_channels_raw().await?;
        if response
            .get("code")
            .and_then(|c| c.as_str())
            .is_some_and(|c| c != "SUCCESS")
        {
            return Ok(Vec::new());
        }
        let contents = response
            .get("data")
            .and_then(|d| d.get("contents"))
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .and_then(|c| c.get("accountSectionListRenderer"))
            .and_then(|r| r.get("contents"))
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .and_then(|c| c.get("accountItemSectionRenderer"))
            .and_then(|r| r.get("contents"))
            .and_then(|c| c.as_array());
        let mut channels = Vec::new();
        if let Some(contents) = contents {
            for item_el in contents {
                let Some(item) = item_el.get("accountItem") else {
                    continue;
                };
                let Some(name) = item.get("accountName").and_then(text_of) else {
                    continue;
                };
                if name.is_empty() {
                    continue;
                }
                let data_sync_id = item
                    .get("serviceEndpoint")
                    .and_then(|e| e.get("selectActiveIdentityEndpoint"))
                    .and_then(|e| e.get("supportedTokens"))
                    .and_then(|t| t.as_array())
                    .and_then(|tokens| {
                        tokens.iter().find_map(|t| {
                            t.get("datasyncIdToken")
                                .and_then(|d| d.get("datasyncIdToken"))
                                .and_then(|d| d.as_str())
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string())
                        })
                    });
                // No dataSyncId → cannot be selected → dropped (Android).
                let Some(data_sync_id) = data_sync_id else {
                    continue;
                };
                let channel_handle = item.get("channelHandle").and_then(text_of);
                let thumbnail_url = item
                    .get("accountPhoto")
                    .and_then(|p| p.get("thumbnails"))
                    .and_then(|t| t.as_array())
                    .and_then(|t| t.last())
                    .and_then(|t| t.get("url"))
                    .and_then(|u| u.as_str())
                    .filter(|u| !u.is_empty())
                    .map(|u| u.to_string());
                channels.push(AccountChannel {
                    name,
                    channel_handle,
                    thumbnail_url,
                    data_sync_id: Some(data_sync_id),
                });
            }
        }
        Ok(channels)
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
    pub async fn resolve_stream(
        &self,
        video_id: &str,
        audio_quality: &str,
    ) -> Result<StreamUrl, StreamError> {
        resolve::resolve_stream(self, video_id, audio_quality).await
    }
}

impl MusicService for InnerTubeSession {
    async fn home_feed(&self, params: Option<&str>) -> ServiceResult<HomeFeed> {
        self.home_feed(params).await.map_err(|e| e.to_string())
    }

    async fn home_feed_more(&self, continuation: &str) -> ServiceResult<HomeFeed> {
        self.home_feed_more(continuation)
            .await
            .map_err(|e| e.to_string())
    }

    async fn search(&self, query: &str, filter: Option<&str>) -> ServiceResult<SearchPage> {
        self.search(query, filter).await.map_err(|e| e.to_string())
    }

    async fn search_more(&self, continuation: &str) -> ServiceResult<SearchPage> {
        self.search_more(continuation)
            .await
            .map_err(|e| e.to_string())
    }

    async fn library_page(
        &self,
        browse_id: &str,
        tab_index: usize,
    ) -> ServiceResult<SearchPage> {
        self.library_page(browse_id, tab_index)
            .await
            .map_err(|e| e.to_string())
    }

    async fn library_page_more(&self, continuation: &str) -> ServiceResult<SearchPage> {
        self.library_page_more(continuation)
            .await
            .map_err(|e| e.to_string())
    }

    async fn like_video(&self, video_id: &str, liked: bool) -> ServiceResult<()> {
        self.like_video(video_id, liked)
            .await
            .map_err(|e| e.to_string())
    }

    async fn subscribe_channel(&self, channel_id: &str, subscribe: bool) -> ServiceResult<()> {
        self.subscribe_channel(channel_id, subscribe)
            .await
            .map_err(|e| e.to_string())
    }

    async fn playlist(&self, id: &str) -> ServiceResult<PlaylistDetail> {
        self.playlist(id).await.map_err(|e| e.to_string())
    }

    async fn album(&self, id: &str) -> ServiceResult<AlbumDetail> {
        self.album(id).await.map_err(|e| e.to_string())
    }

    async fn artist(&self, id: &str) -> ServiceResult<ArtistDetail> {
        self.artist(id).await.map_err(|e| e.to_string())
    }

    async fn charts(&self) -> ServiceResult<Vec<HomeSection>> {
        self.charts().await.map_err(|e| e.to_string())
    }

    async fn explore(&self) -> ServiceResult<ExploreData> {
        self.explore().await.map_err(|e| e.to_string())
    }

    async fn browse_mood(&self, params: &str) -> ServiceResult<Vec<HomeItem>> {
        self.browse_mood(params).await.map_err(|e| e.to_string())
    }

    /// Related songs for a video: `next` → related endpoint → `related`.
    async fn related_for_video(&self, video_id: &str) -> ServiceResult<Vec<Song>> {
        let (browse_id, params) = self
            .next_related_endpoint(video_id)
            .await
            .map_err(|e| e.to_string())?;
        self.related(&browse_id, params.as_deref())
            .await
            .map_err(|e| e.to_string())
    }

    async fn transcript(&self, video_id: &str) -> ServiceResult<Vec<LyricLine>> {
        self.transcript(video_id).await.map_err(|e| e.to_string())
    }

    async fn resolve_stream(
        &self,
        song_id: &str,
        audio_quality: &str,
    ) -> ServiceResult<Option<String>> {
        match self.resolve_stream(song_id, audio_quality).await {
            Ok(url) => Ok(Some(url.url)),
            Err(e) => Err(e.to_string()),
        }
    }
}
