//! Stream resolution: the player-response chain ported from
//! `YTPlayerUtils.resolvePlaybackData` + `findFormat` + `findUrlOrNull`.
//!
//! Order: MAIN_CLIENT (`ANDROID_VR_1_43_32`) first, then
//! `STREAM_FALLBACK_CLIENTS` in the Android order (guest mode skips
//! `login_required` clients). No PoToken anywhere — the VR/IOS clients
//! do not use one, and the web clients run without it (they may fail on
//! bot-flagged tracks; see the note in `client.rs`).
//!
//! Format selection differs from Android in one deliberate way: Android
//! prefers Opus (`audio/webm`, +10240 score). WebView2's `<audio>`
//! cannot play Opus-in-WebM, so this port picks AAC/MP4 only and
//! returns [`StreamError::NoPlayableFormat`] when only Opus exists.

use crate::innertube::cipher::{decipher_signature, join_signature, parse_cipher};
use crate::innertube::client::{MAIN_CLIENT, STREAM_FALLBACK_CLIENTS};
use crate::innertube::error::{StreamError, StreamUrl};
use crate::innertube::parse::get;
use crate::innertube::session::{DEFAULT_STREAM_EXPIRY_SECONDS, InnerTubeSession};
use serde_json::Value;

/// `playabilityStatus` of a player response.
struct Playability {
    status: String,
    reason: Option<String>,
}

fn playability(v: &Value) -> Playability {
    Playability {
        status: get(v, &["playabilityStatus", "status"])
            .and_then(|s| s.as_str())
            .unwrap_or("UNKNOWN")
            .to_string(),
        reason: get(v, &["playabilityStatus", "reason"])
            .and_then(|s| s.as_str())
            .map(|s| s.to_string()),
    }
}

fn is_private_track(v: &Value) -> bool {
    get(v, &["videoDetails", "musicVideoType"])
        .and_then(|s| s.as_str())
        == Some("MUSIC_VIDEO_TYPE_PRIVATELY_OWNED_TRACK")
}

/// All audio adaptive formats (`width == null`, Android `Format.isAudio`).
fn audio_formats(v: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    if let Some(list) = get(v, &["streamingData", "adaptiveFormats"]).and_then(|a| a.as_array()) {
        out.extend(list.iter().filter(|f| f.get("width").is_none()));
    }
    out
}

/// Pick the AAC/MP4 audio format for the requested quality.
/// Returns `Err(NoPlayableFormat)` when audio exists but only Opus/WebM,
/// `Err(NoStream)` when there is no audio at all.
///
/// Quality (GAP_TABLE #25): "low" prefers itag 139 (AAC ~48kbps),
/// falling back to the lowest-bitrate AAC when 139 is absent;
/// "auto" / "high" prefer itag 140 (AAC ~128kbps), falling back to the
/// highest-bitrate AAC when 140 is absent.
fn select_aac_format<'a>(v: &'a Value, audio_quality: &str) -> Result<&'a Value, StreamError> {
    let audio = audio_formats(v);
    if audio.is_empty() {
        return Err(StreamError::NoStream);
    }
    let aac: Vec<&&Value> = audio
        .iter()
        .filter(|f| {
            f.get("mimeType")
                .and_then(|m| m.as_str())
                .map(|m| m.starts_with("audio/mp4"))
                .unwrap_or(false)
        })
        .collect();
    if aac.is_empty() {
        return Err(StreamError::NoPlayableFormat);
    }
    let itag_of = |f: &&Value| f.get("itag").and_then(|i| i.as_i64()).unwrap_or(0);
    let bitrate_of = |f: &&Value| f.get("bitrate").and_then(|b| b.as_i64()).unwrap_or(0);
    if audio_quality == "low" {
        if let Some(f) = aac.iter().find(|f| itag_of(f) == 139) {
            return Ok(f);
        }
        let lowest = aac
            .into_iter()
            .min_by_key(|f| bitrate_of(f))
            .expect("non-empty aac list");
        return Ok(lowest);
    }
    if let Some(f) = aac.iter().find(|f| itag_of(f) == 140) {
        return Ok(f);
    }
    let best = aac
        .into_iter()
        .max_by_key(|f| bitrate_of(f))
        .expect("non-empty aac list");
    Ok(best)
}

/// Resolve a format to a direct URL: direct `url` first, else decipher
/// `signatureCipher`/`cipher` via the player JS.
async fn url_for_format(
    session: &InnerTubeSession,
    format: &Value,
    _video_id: &str,
    player_response: &Value,
) -> Result<String, StreamError> {
    if let Some(url) = format.get("url").and_then(|u| u.as_str()) {
        if !url.is_empty() {
            return Ok(url.to_string());
        }
    }
    let cipher = format
        .get("signatureCipher")
        .or_else(|| format.get("cipher"))
        .and_then(|c| c.as_str());
    let cipher = match cipher {
        Some(c) if !c.is_empty() => c,
        _ => return Err(StreamError::NoStream),
    };
    let (base_url, sig, sp) = parse_cipher(cipher).ok_or_else(|| {
        StreamError::CipherUnsupported("could not parse signatureCipher params".to_string())
    })?;
    let js_url = InnerTubeSession::js_url_from_player_response(player_response)
        .ok_or_else(|| {
            StreamError::CipherUnsupported(
                "player response has no assets.js to decipher with".to_string(),
            )
        })?;
    let js = session.fetch_player_js(&js_url).await.map_err(|e| {
        StreamError::CipherUnsupported(format!("could not fetch player JS: {e}"))
    })?;
    let clear = decipher_signature(&js, &sig)?;
    Ok(join_signature(&base_url, &sp, &clear))
}

/// Try one client: player call → playability → format → URL → validate.
/// Returns `Ok(Some(url))` on success, `Ok(None)` when this client
/// cannot serve the track (caller tries the next one), or `Err` for a
/// terminal per-format error that should also move to the next client.
async fn try_client(
    session: &InnerTubeSession,
    client: crate::innertube::client::YouTubeClient,
    video_id: &str,
    playlist_id: Option<&str>,
    skip_validation: bool,
    audio_quality: &str,
) -> Result<Option<StreamUrl>, StreamError> {
    let sig_ts = if client.use_signature_timestamp {
        session.signature_timestamp(video_id, None).await
    } else {
        None
    };
    let resp = session
        .player_raw(client, video_id, playlist_id, sig_ts)
        .await?;
    let p = playability(&resp);
    if p.status != "OK" {
        return Err(StreamError::Blocked(
            p.reason
                .unwrap_or_else(|| format!("playability status: {}", p.status)),
        ));
    }
    // The player response may itself carry the JS URL for deciphering;
    // keep it attached by re-using `resp` below.
    let format = match select_aac_format(&resp, audio_quality) {
        Ok(f) => f,
        Err(e) => return Err(e),
    };
    let url = url_for_format(session, format, video_id, &resp).await?;
    // Web clients serve `n=`-protected URLs that need a JS n-transform
    // (Android runs it in a WebView). This port has no JS engine, so a
    // web-client URL is accepted only if it validates as-is. Validation
    // is skipped for the last fallback client and for privately owned
    // tracks, mirroring Android.
    let skip = skip_validation || is_private_track(&resp);
    if !skip && !session.validate_stream_url(&url).await {
        return Ok(None);
    }
    let itag = format.get("itag").and_then(|i| i.as_i64()).unwrap_or(0) as i32;
    let mime = format
        .get("mimeType")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();
    let bitrate = format
        .get("bitrate")
        .and_then(|b| b.as_i64())
        .unwrap_or(0) as i32;
    let expires = get(&resp, &["streamingData", "expiresInSeconds"])
        .and_then(|e| e.as_i64())
        .unwrap_or(DEFAULT_STREAM_EXPIRY_SECONDS);
    Ok(Some(StreamUrl {
        url,
        itag,
        mime_type: mime,
        bitrate,
        expires_in_seconds: expires,
        client_name: client.label.to_string(),
    }))
}

/// Resolve a playable stream URL for `video_id`.
///
/// Mirrors `YTPlayerUtils.resolvePlaybackData` minus the Android-only
/// intercepts (Saavn/Tidal/modules, metadata client, PoToken, NewPipe):
/// MAIN first, then the fallback chain. The last error is reported —
/// a `Blocked(reason)` from the final client is surfaced verbatim so
/// the UI can show YouTube's own reason.
pub async fn resolve_stream(
    session: &InnerTubeSession,
    video_id: &str,
    audio_quality: &str,
) -> Result<StreamUrl, StreamError> {
    resolve_stream_with_playlist(session, video_id, None, audio_quality).await
}

pub async fn resolve_stream_with_playlist(
    session: &InnerTubeSession,
    video_id: &str,
    playlist_id: Option<&str>,
    audio_quality: &str,
) -> Result<StreamUrl, StreamError> {
    // MAIN_CLIENT first.
    let mut last_err: Option<StreamError>;
    match try_client(session, MAIN_CLIENT, video_id, playlist_id, false, audio_quality).await {
        Ok(Some(url)) => return Ok(url),
        Ok(None) => {
            last_err = Some(StreamError::NoStream);
        }
        Err(e) => {
            // Age-restricted responses start the fallback chain at the
            // top; everything else also falls through to the chain.
            last_err = Some(e);
        }
    }

    // Fallback chain (guest: skip login-required clients).
    let fallbacks: Vec<_> = STREAM_FALLBACK_CLIENTS
        .iter()
        .filter(|c| !c.login_required)
        .collect();
    let total = fallbacks.len();
    for (i, client) in fallbacks.iter().enumerate() {
        let is_last = i + 1 == total;
        // Android skips validation for the last client (and for private
        // tracks, detected inside try_client via the response — here we
        // approximate with the last-client rule only).
        match try_client(session, **client, video_id, playlist_id, is_last, audio_quality).await {
            Ok(Some(url)) => return Ok(url),
            Ok(None) => {
                // A client that returns no URL must not overwrite an
                // earlier Blocked(reason) — that is the informative
                // error the UI should show.
                if !matches!(last_err, Some(StreamError::Blocked(_))) {
                    last_err = Some(StreamError::NoStream);
                }
            }
            Err(e) => {
                // A cipher failure on one client must not mask a later
                // client's direct URL; keep the most informative error.
                match (&last_err, &e) {
                    (Some(StreamError::Blocked(_)), _) => {}
                    _ => last_err = Some(e),
                }
            }
        }
    }

    // Report the most informative failure seen. A Blocked reason from
    // any client beats a generic NoStream.
    Err(last_err.unwrap_or(StreamError::NoStream))
}
