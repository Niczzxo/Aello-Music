//! HTTP layer + session state for the InnerTube client.
//!
//! Port of `InnerTube.kt` (headers, `withRetry`, endpoints) and the
//! `visitorData()` flow in `YouTube.kt` (`GET sw.js_data`, strip the
//! `)]}'` prefix, walk `[0][2]`, first string matching `^Cg[ts]`).
//!
//! Guest only: no cookie, no `dataSyncId`, no PoToken.

use crate::innertube::client::{
    API_URL_YOUTUBE_MUSIC, ORIGIN_YOUTUBE_MUSIC, REFERER_YOUTUBE_MUSIC, USER_AGENT_WEB,
    WEB, WEB_REMIX, YouTubeClient,
};
use crate::innertube::error::StreamError;
use regex::Regex;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;

/// Retries like Android's `withRetry`: 3 attempts, 500ms initial backoff
/// ×2, honoring `Retry-After`. Only IO errors and 429/5xx are retried.
const MAX_ATTEMPTS: usize = 3;
const INITIAL_BACKOFF_MS: u64 = 500;

/// `expiresInSeconds` fallback when a player response omits it
/// (Android `DEFAULT_STREAM_EXPIRY_SECONDS`).
pub const DEFAULT_STREAM_EXPIRY_SECONDS: i64 = 21600;

pub struct InnerTubeSession {
    http: Client,
    visitor_data: RwLock<Option<String>>,
    /// Google sign-in cookie (raw `Cookie` header value), set by
    /// `signin_finish`, cleared by `sign_out`. `std` lock (not tokio):
    /// the cookie is set synchronously from Tauri `setup`, which runs
    /// outside the async runtime. `None` = guest mode.
    cookie: std::sync::RwLock<Option<String>>,
    /// player JS URL → source text (cached; base.js is ~1MB and stable
    /// per release).
    player_js: Mutex<HashMap<String, String>>,
    /// player JS URL → `signatureTimestamp` found in it (cached).
    sig_ts: Mutex<HashMap<String, Option<i64>>>,
    /// Watch-page-derived player JS URL (session-wide cache; the URL is
    /// stable per YouTube release).
    js_url_cache: Mutex<Option<String>>,
}

impl Default for InnerTubeSession {
    fn default() -> Self {
        Self::new()
    }
}

impl InnerTubeSession {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(60))
            .build()
            .expect("reqwest client builds");
        Self {
            http,
            visitor_data: RwLock::new(None),
            cookie: std::sync::RwLock::new(None),
            player_js: Mutex::new(HashMap::new()),
            sig_ts: Mutex::new(HashMap::new()),
            js_url_cache: Mutex::new(None),
        }
    }

    // ── sign-in cookie ──────────────────────────────────────────────────

    /// Current cookie, if signed in.
    pub fn cookie(&self) -> Option<String> {
        self.cookie.read().ok().and_then(|c| c.clone())
    }

    /// Set (or clear) the sign-in cookie. Synchronous so Tauri `setup`
    /// can restore it from the settings store without an async context.
    pub fn set_cookie(&self, cookie: Option<String>) {
        if let Ok(mut slot) = self.cookie.write() {
            *slot = cookie;
        }
    }

    /// Override visitorData (e.g. the value harvested with the cookie at
    /// sign-in). Only used from async contexts; `setup` uses
    /// [`Self::set_visitor_data_blocking`] instead.
    pub async fn set_visitor_data(&self, visitor_data: Option<String>) {
        *self.visitor_data.write().await = visitor_data;
    }

    /// Synchronous visitorData override for Tauri `setup` only: the
    /// tokio `blocking_write` must never be called from an async context.
    pub fn set_visitor_data_blocking(&self, visitor_data: Option<String>) {
        *self.visitor_data.blocking_write() = visitor_data;
    }

    // ── visitor data ────────────────────────────────────────────────────

    /// Current visitorData, if fetched.
    pub async fn visitor_data(&self) -> Option<String> {
        self.visitor_data.read().await.clone()
    }

    /// Fetch visitorData if we have none. Best-effort: a failure leaves it
    /// `None` and calls proceed without it (Android tolerates null too).
    pub async fn ensure_visitor_data(&self) {
        if self.visitor_data.read().await.is_some() {
            return;
        }
        if let Ok(vd) = self.fetch_visitor_data().await {
            *self.visitor_data.write().await = Some(vd);
        }
    }

    /// Force a fresh visitorData (called on 401/403).
    pub async fn refresh_visitor_data(&self) {
        match self.fetch_visitor_data().await {
            Ok(vd) => *self.visitor_data.write().await = Some(vd),
            Err(_) => *self.visitor_data.write().await = None,
        }
    }

    async fn fetch_visitor_data(&self) -> Result<String, StreamError> {
        let text = self
            .get_text("https://music.youtube.com/sw.js_data", USER_AGENT_WEB)
            .await?;
        // Body starts with `)]}'\n` (5 chars); Android strips it.
        let json_text = text.strip_prefix(")]}'").unwrap_or(&text);
        let json_text = json_text.trim_start_matches('\n');
        let root: Value = serde_json::from_str(json_text)
            .map_err(|e| StreamError::Parse(format!("sw.js_data JSON: {e}")))?;
        let re = Regex::new(r"^Cg[ts]").expect("static regex");
        let candidates = root
            .get(0)
            .and_then(|a| a.get(2))
            .and_then(|a| a.as_array())
            .ok_or_else(|| {
                StreamError::Parse("sw.js_data: [0][2] array not found".to_string())
            })?;
        for c in candidates {
            if let Some(s) = c.as_str() {
                if re.is_match(s) {
                    return Ok(s.to_string());
                }
            }
        }
        Err(StreamError::Parse(
            "sw.js_data: no visitorData matching ^Cg[ts]".to_string(),
        ))
    }

    // ── low-level HTTP with retry ───────────────────────────────────────

    /// Is this failure worth retrying (IO error, or 429/5xx)?
    fn retryable_reqwest(e: &reqwest::Error) -> bool {
        e.is_connect() || e.is_timeout() || e.is_request() || e.is_body() || e.is_decode()
    }

    async fn backoff(attempt: usize, retry_after: Option<u64>) {
        let ms = retry_after
            .map(|s| s.saturating_mul(1000))
            .unwrap_or_else(|| INITIAL_BACKOFF_MS * (1 << attempt));
        sleep(Duration::from_millis(ms)).await;
    }

    /// GET a URL as text, with retry.
    pub async fn get_text(&self, url: &str, user_agent: &str) -> Result<String, StreamError> {
        let mut attempt = 0;
        loop {
            match self
                .http
                .get(url)
                .header("User-Agent", user_agent)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return resp
                            .text()
                            .await
                            .map_err(|e| StreamError::Network(e.to_string()));
                    }
                    if (status == StatusCode::TOO_MANY_REQUESTS
                        || status.is_server_error())
                        && attempt + 1 < MAX_ATTEMPTS
                    {
                        let ra = retry_after_secs(&resp);
                        attempt += 1;
                        Self::backoff(attempt - 1, ra).await;
                        continue;
                    }
                    return Err(StreamError::Http(status.as_u16()));
                }
                Err(e) => {
                    if Self::retryable_reqwest(&e) && attempt + 1 < MAX_ATTEMPTS {
                        attempt += 1;
                        Self::backoff(attempt - 1, None).await;
                        continue;
                    }
                    return Err(StreamError::Network(e.to_string()));
                }
            }
        }
    }

    /// POST JSON to an InnerTube endpoint, with retry + one visitor-data
    /// refresh on 401/403. Returns the parsed JSON body.
    pub async fn post(
        &self,
        client: YouTubeClient,
        endpoint: &str,
        body: Value,
    ) -> Result<Value, StreamError> {
        let url = format!("{API_URL_YOUTUBE_MUSIC}{endpoint}?prettyPrint=false");
        self.post_to_url(&url, client, body).await
    }

    /// POST JSON to an absolute URL (same headers/retry as [`Self::post`]).
    /// Used for endpoints outside the `youtubei/v1/` prefix, e.g.
    /// `get_transcript`.
    pub async fn post_to_url(
        &self,
        url: &str,
        client: YouTubeClient,
        body: Value,
    ) -> Result<Value, StreamError> {
        let mut attempt = 0;
        let mut refreshed = false;
        loop {
            let visitor = self.visitor_data.read().await.clone();
            let cookie = self.cookie();
            let mut req = self
                .http
                .post(url)
                .header("Content-Type", "application/json")
                .header("X-Goog-Api-Format-Version", "1")
                // Not a typo (Android comment): the Client-Name header
                // carries the client *id*.
                .header("X-YouTube-Client-Name", client.client_id)
                .header("X-YouTube-Client-Version", client.client_version)
                .header("X-Origin", ORIGIN_YOUTUBE_MUSIC)
                .header("Referer", REFERER_YOUTUBE_MUSIC)
                .header("User-Agent", client.user_agent);
            if let Some(v) = &visitor {
                req = req.header("X-Goog-Visitor-Id", v);
            }
            // Signed in: send the cookie + SAPISIDHASH, exactly as
            // Android's `ytClient(setLogin = true)` does. Guest mode
            // (cookie None) sends neither.
            if let Some(c) = &cookie {
                req = req.header("Cookie", c.clone());
                if let Some(sapisid) = cookie_value(c, "SAPISID") {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let hash = sha1_hex(&format!("{ts} {sapisid} {ORIGIN_YOUTUBE_MUSIC}"));
                    req = req.header("Authorization", format!("SAPISIDHASH {ts}_{hash}"));
                }
            }
            match req.json(&body).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return resp.json::<Value>().await.map_err(|e| {
                            StreamError::Parse(format!("{url} response JSON: {e}"))
                        });
                    }
                    if (status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN)
                        && !refreshed
                    {
                        refreshed = true;
                        self.refresh_visitor_data().await;
                        continue;
                    }
                    if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                        && attempt + 1 < MAX_ATTEMPTS
                    {
                        let ra = retry_after_secs(&resp);
                        attempt += 1;
                        Self::backoff(attempt - 1, ra).await;
                        continue;
                    }
                    return Err(StreamError::Http(status.as_u16()));
                }
                Err(e) => {
                    if Self::retryable_reqwest(&e) && attempt + 1 < MAX_ATTEMPTS {
                        attempt += 1;
                        Self::backoff(attempt - 1, None).await;
                        continue;
                    }
                    return Err(StreamError::Network(e.to_string()));
                }
            }
        }
    }

    /// GET a URL as parsed JSON (one attempt + 15s cap; no InnerTube
    /// headers). Used for LrcLib, GitHub releases, and the Listen
    /// Together room API.
    pub async fn http_get_json(
        &self,
        url: &str,
        extra_headers: &[(&str, &str)],
    ) -> Result<Value, StreamError> {
        let mut req = self
            .http
            .get(url)
            .header("User-Agent", USER_AGENT_WEB)
            .timeout(Duration::from_secs(15));
        for (k, v) in extra_headers {
            req = req.header(*k, *v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| StreamError::Network(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(StreamError::Http(status.as_u16()));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| StreamError::Parse(format!("{url} response JSON: {e}")))
    }

    /// POST with an empty body and parse the JSON response (one attempt
    /// + 15s cap). Used for the Listen Together room API, which mints a
    /// room code from an empty POST (Android `allocateRoomCode`).
    pub async fn http_post_empty(&self, url: &str) -> Result<Value, StreamError> {
        let resp = self
            .http
            .post(url)
            .header("User-Agent", USER_AGENT_WEB)
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| StreamError::Network(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(StreamError::Http(status.as_u16()));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| StreamError::Parse(format!("{url} response JSON: {e}")))
    }

    /// Stream a URL to a file (song downloads). Returns bytes written.
    /// No cookies: googlevideo URLs are already signed (see
    /// `validate_stream_url`).
    ///
    /// Each chunk has a 60s stall timeout (same value as the session
    /// client's overall timeout); a mid-stream error deletes the
    /// partial file instead of leaving a broken `.m4a` behind, and a
    /// `Content-Length` mismatch is an error for the same reason.
    pub async fn download_to_file(&self, url: &str, path: &Path) -> Result<u64, StreamError> {
        let result = self.download_to_file_inner(url, path).await;
        if result.is_err() {
            // Best-effort cleanup of the partial download.
            let _ = tokio::fs::remove_file(path).await;
        }
        result
    }

    async fn download_to_file_inner(
        &self,
        url: &str,
        path: &Path,
    ) -> Result<u64, StreamError> {
        use tokio::io::AsyncWriteExt;
        let mut resp = self
            .http
            .get(url)
            .header("User-Agent", USER_AGENT_WEB)
            .send()
            .await
            .map_err(|e| StreamError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(StreamError::Http(resp.status().as_u16()));
        }
        let expected_len = resp.content_length();
        let mut file = tokio::fs::File::create(path)
            .await
            .map_err(|e| StreamError::Network(format!("create download file: {e}")))?;
        let mut total: u64 = 0;
        loop {
            let chunk = tokio::time::timeout(Duration::from_secs(60), resp.chunk())
                .await
                .map_err(|_| {
                    StreamError::Network("download stalled: no data for 60s".to_string())
                })?
                .map_err(|e| StreamError::Network(e.to_string()))?;
            match chunk {
                Some(bytes) => {
                    total += bytes.len() as u64;
                    file.write_all(&bytes)
                        .await
                        .map_err(|e| StreamError::Network(format!("write download: {e}")))?;
                }
                None => break,
            }
        }
        file.flush()
            .await
            .map_err(|e| StreamError::Network(format!("flush download: {e}")))?;
        if let Some(expected) = expected_len {
            if total != expected {
                return Err(StreamError::Network(format!(
                    "download incomplete: got {total} bytes, expected {expected}"
                )));
            }
        }
        Ok(total)
    }

    // ── InnerTube endpoints (InnerTube.kt) ──────────────────────────────

    fn context(&self, client: YouTubeClient, visitor: Option<&str>) -> Value {
        client.context(visitor)
    }

    // NOTE: plain `search_raw` (no params) was removed in 6.1.45 —
    // `search_raw_params` below covers it with `params: None`.

    pub async fn browse_raw(
        &self,
        browse_id: Option<&str>,
        continuation: Option<&str>,
    ) -> Result<Value, StreamError> {
        self.browse_raw_params(browse_id, None, continuation).await
    }

    /// Browse with an optional `params` string (charts page needs
    /// `ggMGCgQIgAQ%3D`; related/mood endpoints carry their own params).
    pub async fn browse_raw_params(
        &self,
        browse_id: Option<&str>,
        params: Option<&str>,
        continuation: Option<&str>,
    ) -> Result<Value, StreamError> {
        self.ensure_visitor_data().await;
        let visitor = self.visitor_data().await;
        let mut body = json!({
            "context": self.context(WEB_REMIX, visitor.as_deref()),
        });
        if let Some(b) = browse_id {
            body["browseId"] = json!(b);
        }
        if let Some(p) = params {
            body["params"] = json!(p);
        }
        if let Some(c) = continuation {
            body["continuation"] = json!(c);
        }
        self.post(WEB_REMIX, "browse", body).await
    }

    /// Search with an optional filter `params` string (Android
    /// `SearchFilter` values, e.g. `FILTER_SONG`).
    pub async fn search_raw_params(
        &self,
        query: &str,
        params: Option<&str>,
    ) -> Result<Value, StreamError> {
        self.ensure_visitor_data().await;
        let visitor = self.visitor_data().await;
        let mut body = json!({
            "context": self.context(WEB_REMIX, visitor.as_deref()),
            "query": query,
        });
        if let Some(p) = params {
            body["params"] = json!(p);
        }
        self.post(WEB_REMIX, "search", body).await
    }

    /// Raw `next` call for a video id (Android `YouTube.next`).
    /// The response's tab 2 endpoint is the `related` browse endpoint.
    pub async fn next_raw(&self, video_id: &str) -> Result<Value, StreamError> {
        self.ensure_visitor_data().await;
        let visitor = self.visitor_data().await;
        let body = json!({
            "context": self.context(WEB_REMIX, visitor.as_deref()),
            "videoId": video_id,
        });
        self.post(WEB_REMIX, "next", body).await
    }

    /// `account/account_menu` with the sign-in cookie (Android
    /// `InnerTube.accountMenu`). Fails without a valid cookie — this is
    /// the sign-in validation call.
    pub async fn account_menu_raw(&self) -> Result<Value, StreamError> {
        self.ensure_visitor_data().await;
        let visitor = self.visitor_data().await;
        let body = json!({
            "context": self.context(WEB_REMIX, visitor.as_deref()),
        });
        self.post(WEB_REMIX, "account/account_menu", body).await
    }

    /// Timed-text transcript for a video (Android `InnerTube.getTranscript`
    /// / `YouTube.transcript`, the YouTube Subtitle lyrics fallback).
    /// Params are base64 of `"\n\x0b" + videoId`. No API key is sent:
    /// the youtubei endpoint does not need one (the key Android passes
    /// is the public web client key; this port never guesses keys).
    pub async fn transcript_raw(&self, video_id: &str) -> Result<Value, StreamError> {
        let mut param_bytes = vec![b'\n', 11u8];
        param_bytes.extend_from_slice(video_id.as_bytes());
        let params = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &param_bytes,
        );
        let body = json!({
            "context": WEB.context(None),
            "params": params,
        });
        self.post_to_url(
            "https://music.youtube.com/youtubei/v1/get_transcript",
            WEB,
            body,
        )
        .await
    }

    /// Raw `player` call for one client. `signature_timestamp` is only
    /// sent when the client declares `useSignatureTimestamp`.
    pub async fn player_raw(
        &self,
        client: YouTubeClient,
        video_id: &str,
        playlist_id: Option<&str>,
        signature_timestamp: Option<i64>,
    ) -> Result<Value, StreamError> {
        self.ensure_visitor_data().await;
        let visitor = self.visitor_data().await;
        let mut context = self.context(client, visitor.as_deref());
        if client.is_embedded {
            context["thirdParty"] =
                json!({ "embedUrl": format!("https://www.youtube.com/watch?v={video_id}") });
        }
        let mut body = json!({
            "context": context,
            "videoId": video_id,
            "contentCheckOk": true,
            "racyCheckOk": true,
        });
        if let Some(p) = playlist_id {
            body["playlistId"] = json!(p);
        }
        if client.use_signature_timestamp {
            if let Some(ts) = signature_timestamp {
                body["playbackContext"] =
                    json!({ "contentPlaybackContext": { "signatureTimestamp": ts } });
            }
        }
        self.post(client, "player", body).await
    }

    // ── player JS / signature timestamp ─────────────────────────────────

    /// `assets.js` path from a player response, as an absolute URL.
    pub fn js_url_from_player_response(v: &Value) -> Option<String> {
        let js = v.get("assets")?.get("js")?.as_str()?;
        Some(if js.starts_with("http") {
            js.to_string()
        } else {
            format!("https://www.youtube.com{js}")
        })
    }

    /// Player JS URL for a video: from a player response when available,
    /// else the (session-cached) watch-page HTML. The JS URL is stable
    /// per YouTube release, so the slow watch-page fetch happens at most
    /// once per session.
    pub async fn player_js_url(
        &self,
        video_id: &str,
        player_response: Option<&Value>,
    ) -> Option<String> {
        if let Some(u) = player_response.and_then(Self::js_url_from_player_response) {
            return Some(u);
        }
        {
            let cache = self.js_url_cache.lock().expect("js_url_cache lock poisoned");
            if let Some(u) = cache.as_ref() {
                return Some(u.clone());
            }
        }
        let u = self.watch_page_js_url(video_id).await.ok().flatten()?;
        *self
            .js_url_cache
            .lock()
            .expect("js_url_cache lock poisoned") = Some(u.clone());
        Some(u)
    }

    /// Fetch (cached) player JS source by URL.
    pub async fn fetch_player_js(&self, js_url: &str) -> Result<String, StreamError> {
        {
            let cache = self.player_js.lock().expect("player_js lock poisoned");
            if let Some(src) = cache.get(js_url) {
                return Ok(src.clone());
            }
        }
        let src = self.get_text(js_url, USER_AGENT_WEB).await?;
        self.player_js
            .lock()
            .expect("player_js lock poisoned")
            .insert(js_url.to_string(), src.clone());
        Ok(src)
    }

    /// `signatureTimestamp` for a client that needs one. Extracted from
    /// the player JS (`signatureTimestamp:12345`); the JS URL comes from
    /// the player response when available, else the watch page HTML.
    /// Returns `None` on any failure — the player call then goes out
    /// without a timestamp, which YouTube often still accepts.
    pub async fn signature_timestamp(
        &self,
        video_id: &str,
        player_response: Option<&Value>,
    ) -> Option<i64> {
        let js_url = self.player_js_url(video_id, player_response).await?;
        {
            let cache = self.sig_ts.lock().expect("sig_ts lock poisoned");
            if let Some(ts) = cache.get(&js_url) {
                return *ts;
            }
        }
        let ts = self
            .fetch_player_js(&js_url)
            .await
            .ok()
            .and_then(|src| {
                Regex::new(r"signatureTimestamp\s*:\s*(\d+)")
                    .ok()?
                    .captures(&src)?
                    .get(1)?
                    .as_str()
                    .parse()
                    .ok()
            });
        self.sig_ts
            .lock()
            .expect("sig_ts lock poisoned")
            .insert(js_url, ts);
        ts
    }

    /// Find the player JS URL embedded in the watch-page HTML.
    async fn watch_page_js_url(&self, video_id: &str) -> Result<Option<String>, StreamError> {
        let html = self
            .get_text(
                &format!("https://music.youtube.com/watch?v={video_id}"),
                USER_AGENT_WEB,
            )
            .await?;
        let re = Regex::new(r#""jsUrl"\s*:\s*"([^"]+base\.js)""#).expect("static regex");
        Ok(re.captures(&html).and_then(|c| c.get(1)).map(|m| {
            let u = m.as_str().replace("\\/", "/");
            if u.starts_with("http") {
                u
            } else {
                format!("https://www.youtube.com{u}")
            }
        }))
    }

    /// HEAD-validate a stream URL (Android `validateStatus`): no cookies —
    /// googlevideo rejects account cookies with 403; the URL is already
    /// signed. Falls back to a 1-byte Range GET when HEAD is refused.
    pub async fn validate_stream_url(&self, url: &str) -> bool {
        let head = self
            .http
            .head(url)
            .header("User-Agent", USER_AGENT_WEB)
            .send()
            .await;
        match head {
            Ok(resp) if resp.status().is_success() => true,
            Ok(_) => {
                // Some googlevideo endpoints refuse HEAD; try 1 byte.
                match self
                    .http
                    .get(url)
                    .header("User-Agent", USER_AGENT_WEB)
                    .header("Range", "bytes=0-0")
                    .send()
                    .await
                {
                    Ok(resp) => resp.status().is_success(),
                    Err(_) => false,
                }
            }
            Err(_) => false,
        }
    }
}

fn retry_after_secs(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
}

/// One `name=value` pair out of a `Cookie` header string.
fn cookie_value<'a>(cookie: &'a str, name: &str) -> Option<&'a str> {
    cookie.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix(&format!("{name}="))
    })
}

/// SHA-1 hex digest (FIPS 180-1), hand-rolled so the SAPISIDHASH auth
/// header needs no extra crate. Verified against the standard test
/// vectors in the unit tests below.
fn sha1_hex(input: &str) -> String {
    let mut bytes = input.as_bytes().to_vec();
    let bit_len = (bytes.len() as u64) * 8;
    bytes.push(0x80);
    while bytes.len() % 64 != 56 {
        bytes.push(0);
    }
    bytes.extend_from_slice(&bit_len.to_be_bytes());

    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    for chunk in bytes.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::sha1_hex;

    #[test]
    fn sha1_known_vectors() {
        assert_eq!(
            sha1_hex(""),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        assert_eq!(
            sha1_hex("abc"),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            sha1_hex("The quick brown fox jumps over the lazy dog"),
            "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12"
        );
    }
}
