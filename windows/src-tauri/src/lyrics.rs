//! Synced lyrics: LrcLib first (4 strategies, best-match by duration),
//! YouTube timed-text transcript as fallback.
//!
//! Port of Android `lrclib/LrcLib.kt` (title cleanup, primary-artist
//! reduction, `bestMatchingForRelaxed` ±5s) and `lyrics/LyricsUtils.kt`
//! (`[offset:]` added to every timestamp before returning). The
//! 9-provider race is out of scope for this pass (GAP_TABLE #34);
//! LrcLib + YouTube subtitles cover the common case. Overall timeout
//! 6s, per the contract (Android's `LyricsHelper` overall timeout).

use crate::innertube::session::InnerTubeSession;
use crate::models::{LyricLine, LyricsResult};
use crate::music_service::MusicService;
use regex::Regex;
use serde::Deserialize;
use std::time::Duration;

const LRCLIB_BASE: &str = "https://lrclib.net/api/search";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcLibTrack {
    track_name: String,
    artist_name: String,
    duration: f64,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

// ── title / artist cleaning (LrcLib.kt) ─────────────────────────────────

fn clean_title(title: &str) -> String {
    let patterns = [
        r"\s*\(.*?(official|video|audio|lyrics|lyric|visualizer|hd|hq|4k|remaster|remix|live|acoustic|version|edit|extended|radio|clean|explicit).*?\)",
        r"\s*\[.*?(official|video|audio|lyrics|lyric|visualizer|hd|hq|4k|remaster|remix|live|acoustic|version|edit|extended|radio|clean|explicit).*?\]",
        r"\s*",
        r"\s*\|.*$",
        r"\s*-\s*(official|video|audio|lyrics|lyric|visualizer).*$",
        r"\s*\(feat\..*?\)",
        r"\s*\(ft\..*?\)",
        r"\s*feat\..*$",
        r"\s*ft\..*$",
    ];
    let mut cleaned = title.trim().to_string();
    for pat in patterns {
        if let Ok(re) = Regex::new(&format!("(?i){pat}")) {
            cleaned = re.replace_all(&cleaned, "").into_owned();
        }
    }
    cleaned.trim().to_string()
}

fn clean_artist(artist: &str) -> String {
    const SEPARATORS: &[&str] = &[
        " & ",
        " and ",
        ", ",
        " x ",
        " X ",
        " feat. ",
        " feat ",
        " ft. ",
        " ft ",
        " featuring ",
        " with ",
    ];
    let mut cleaned = artist.trim().to_string();
    let lower = cleaned.to_lowercase();
    for sep in SEPARATORS {
        if let Some(pos) = lower.find(sep) {
            cleaned = cleaned[..pos].trim().to_string();
            break;
        }
    }
    cleaned
}

// ── LrcLib search (4 strategies) ────────────────────────────────────────

async fn lrclib_search(
    session: &InnerTubeSession,
    track_name: Option<&str>,
    artist_name: Option<&str>,
    album_name: Option<&str>,
    query: Option<&str>,
) -> Vec<LrcLibTrack> {
    let mut url = match url::Url::parse(LRCLIB_BASE) {
        Ok(u) => u,
        Err(_) => return Vec::new(),
    };
    {
        let mut qp = url.query_pairs_mut();
        if let Some(q) = query {
            qp.append_pair("q", q);
        }
        if let Some(t) = track_name {
            qp.append_pair("track_name", t);
        }
        if let Some(a) = artist_name {
            qp.append_pair("artist_name", a);
        }
        if let Some(a) = album_name {
            qp.append_pair("album_name", a);
        }
    }
    let Ok(value) = session.http_get_json(url.as_str(), &[]).await else {
        return Vec::new();
    };
    serde_json::from_value(value).unwrap_or_default()
}

async fn lrclib_query(
    session: &InnerTubeSession,
    artist: &str,
    title: &str,
    album: Option<&str>,
) -> Vec<LrcLibTrack> {
    let cleaned_title = clean_title(title);
    let cleaned_artist = clean_artist(artist);
    let has_lyrics = |t: &LrcLibTrack| t.synced_lyrics.is_some() || t.plain_lyrics.is_some();

    // Strategy 1: cleaned title + artist (+ album).
    let mut results: Vec<LrcLibTrack> = lrclib_search(
        session,
        Some(&cleaned_title),
        Some(&cleaned_artist),
        album,
        None,
    )
    .await
    .into_iter()
    .filter(has_lyrics)
    .collect();
    if !results.is_empty() {
        return results;
    }
    // Strategy 2: cleaned title only.
    results = lrclib_search(session, Some(&cleaned_title), None, None, None)
        .await
        .into_iter()
        .filter(has_lyrics)
        .collect();
    if !results.is_empty() {
        return results;
    }
    // Strategy 3: q = "artist title".
    results = lrclib_search(
        session,
        None,
        None,
        None,
        Some(&format!("{cleaned_artist} {cleaned_title}")),
    )
    .await
    .into_iter()
    .filter(has_lyrics)
    .collect();
    if !results.is_empty() {
        return results;
    }
    // Strategy 4: q = title.
    lrclib_search(session, None, None, None, Some(&cleaned_title))
        .await
        .into_iter()
        .filter(has_lyrics)
        .collect()
}

/// String similarity for the no-duration best match (Android
/// `LrcLib.calculateStringSimilarity`): equal → 1.0, contains → 0.8,
/// else 1 − Levenshtein/max-length.
fn string_similarity(a: &str, b: &str) -> f64 {
    let s1 = a.trim().to_lowercase();
    let s2 = b.trim().to_lowercase();
    if s1 == s2 {
        return 1.0;
    }
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }
    if s1.contains(&s2) || s2.contains(&s1) {
        return 0.8;
    }
    let max_len = s1.len().max(s2.len()) as f64;
    1.0 - (levenshtein(&s1, &s2) as f64 / max_len)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Best match by duration: synced lyrics within ±5s first, then any
/// lyrics within ±5s (Android `bestMatchingForRelaxed`). With no
/// duration, score by title/artist similarity (synced preferred),
/// mirroring Android's `findBestMatch`.
fn best_match<'a>(
    tracks: &'a [LrcLibTrack],
    duration_secs: Option<u64>,
    title: &str,
    artist: &str,
) -> Option<&'a LrcLibTrack> {
    if tracks.is_empty() {
        return None;
    }
    let Some(duration) = duration_secs else {
        return tracks.iter().max_by(|a, b| {
            let score = |t: &&LrcLibTrack| {
                let mut s = (string_similarity(title, &t.track_name)
                    + string_similarity(artist, &t.artist_name))
                    / 2.0;
                if t.synced_lyrics.is_some() {
                    s += 1.0;
                }
                s
            };
            score(a)
                .partial_cmp(&score(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    };
    let within = |t: &&LrcLibTrack| (t.duration as i64 - duration as i64).abs() <= 5;
    tracks
        .iter()
        .filter(|t| t.synced_lyrics.is_some())
        .filter(within)
        .min_by_key(|t| (t.duration as i64 - duration as i64).abs())
        .or_else(|| {
            tracks
                .iter()
                .filter(within)
                .min_by_key(|t| (t.duration as i64 - duration as i64).abs())
        })
}

// ── LRC parsing (LyricsUtils.parseLyrics) ───────────────────────────────

fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

/// Parse LRC text. Returns `(lines, synced)`: `synced=false` when the
/// text has no timestamps (plain lyrics → `timeMs=-1` lines).
fn parse_lrc(text: &str) -> (Vec<LyricLine>, bool) {
    let offset_re = Regex::new(r"\[offset:([+-]?\d+)\]").expect("static regex");
    // Fraction is optional: [mm:ss] timestamps are valid LRC too.
    let time_re = Regex::new(r"\[(\d\d):(\d\d)(?:\.(\d{2,3}))?\]").expect("static regex");
    let line_re = Regex::new(r"((?:\[\d\d:\d\d(?:\.\d{2,3})?\]?)+)(.+)").expect("static regex");
    // LRC metadata tags ([ti:], [ar:], [al:], [by:], [length:], …) —
    // never lyric text; the plain-text fallback must drop them.
    let meta_re = Regex::new(r"^\[[a-zA-Z#][a-zA-Z0-9_#]*:").expect("static regex");

    // `[offset:]` is extracted FIRST and added to every timestamp.
    let offset_ms: i64 = offset_re
        .captures(text)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);

    let mut lines: Vec<LyricLine> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || offset_re.is_match(line) {
            continue;
        }
        let Some(caps) = line_re.captures(line) else {
            continue;
        };
        let lyric = decode_entities(caps.get(2).map(|m| m.as_str()).unwrap_or("").trim());
        if lyric.is_empty() {
            continue;
        }
        let stamps = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        for tcap in time_re.captures_iter(stamps) {
            let min: i64 = tcap[1].parse().unwrap_or(0);
            let sec: i64 = tcap[2].parse().unwrap_or(0);
            // No fraction ([mm:ss]) means 0ms — the group is optional.
            let frac = tcap.get(3).map(|m| m.as_str()).unwrap_or("");
            let ms: i64 = if frac.is_empty() {
                0
            } else if frac.len() == 2 {
                frac.parse::<i64>().unwrap_or(0) * 10
            } else {
                frac.parse().unwrap_or(0)
            };
            let time_ms = (min * 60_000 + sec * 1000 + ms + offset_ms).max(0);
            lines.push(LyricLine {
                time_ms,
                text: lyric.clone(),
            });
        }
    }
    if lines.is_empty() {
        // No timestamps anywhere: plain text, one line per non-empty row.
        // Metadata tag lines ([ti:], [ar:], …) and bare timestamp lines
        // are not lyrics and are dropped.
        let plain: Vec<LyricLine> = text
            .lines()
            .map(|l| decode_entities(l.trim()))
            .filter(|l| {
                !l.is_empty()
                    && !offset_re.is_match(l)
                    && !meta_re.is_match(l)
                    && !time_re.is_match(l)
            })
            .map(|l| LyricLine {
                time_ms: -1,
                text: l,
            })
            .collect();
        return (plain, false);
    }
    lines.sort_by_key(|l| l.time_ms);
    (lines, true)
}

// ── entry point ─────────────────────────────────────────────────────────

async fn get_lyrics_inner(
    session: &InnerTubeSession,
    song_id: &str,
    title: &str,
    artist: &str,
    album: Option<&str>,
    duration_secs: Option<u64>,
) -> Result<LyricsResult, String> {
    // LrcLib first.
    let tracks = lrclib_query(session, artist, title, album).await;
    if let Some(track) = best_match(&tracks, duration_secs, title, artist) {
        if let Some(synced) = &track.synced_lyrics {
            let (lines, synced_flag) = parse_lrc(synced);
            if !lines.is_empty() {
                return Ok(LyricsResult {
                    lines,
                    provider: "LrcLib".to_string(),
                    synced: synced_flag,
                });
            }
        }
        if let Some(plain) = &track.plain_lyrics {
            let (lines, _) = parse_lrc(plain);
            if !lines.is_empty() {
                return Ok(LyricsResult {
                    lines,
                    provider: "LrcLib".to_string(),
                    synced: false,
                });
            }
        }
    }
    // Fallback: YouTube timed-text transcript for the video id, routed
    // through the MusicService trait like every other backend call.
    let lines = MusicService::transcript(session, song_id).await?;
    if lines.is_empty() {
        return Err("No lyrics found (LrcLib and YouTube subtitles both empty)".to_string());
    }
    Ok(LyricsResult {
        lines,
        provider: "YouTube Subtitle".to_string(),
        synced: true,
    })
}

/// Lyrics for a song. Overall timeout 6s (Android `LyricsHelper`).
pub async fn get_lyrics(
    session: &InnerTubeSession,
    song_id: &str,
    title: &str,
    artist: &str,
    album: Option<&str>,
    duration_secs: Option<u64>,
) -> Result<LyricsResult, String> {
    match tokio::time::timeout(
        Duration::from_secs(6),
        get_lyrics_inner(session, song_id, title, artist, album, duration_secs),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err("Lyrics lookup timed out after 6s".to_string()),
    }
}
