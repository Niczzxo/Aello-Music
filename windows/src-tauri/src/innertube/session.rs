//! HTTP layer + session state for the InnerTube client.
//!
//! Port of `InnerTube.kt` (headers, `withRetry`, endpoints) and the
//! `visitorData()` flow in `YouTube.kt` (`GET sw.js_data`, strip the
//! `)]}'` prefix, walk `[0][2]`, first string matching `^Cg[ts]`).
//!
//! Guest only: no cookie, no `dataSyncId`, no PoToken.

use crate::innertube::client::{
    API_URL_YOUTUBE_MUSIC, ORIGIN_YOUTUBE_MUSIC, REFERER_YOUTUBE_MUSIC, USER_AGENT_WEB,
    WEB_REMIX, YouTubeClient,
};
use crate::innertube::error::StreamError;
use regex::Regex;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::collections::HashMap;
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
            player_js: Mutex::new(HashMap::new()),
            sig_ts: Mutex::new(HashMap::new()),
            js_url_cache: Mutex::new(None),
        }
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
        let mut attempt = 0;
        let mut refreshed = false;
        loop {
            let visitor = self.visitor_data.read().await.clone();
            let req = self
                .http
                .post(&url)
                .header("Content-Type", "application/json")
                .header("X-Goog-Api-Format-Version", "1")
                // Not a typo (Android comment): the Client-Name header
                // carries the client *id*.
                .header("X-YouTube-Client-Name", client.client_id)
                .header("X-YouTube-Client-Version", client.client_version)
                .header("X-Origin", ORIGIN_YOUTUBE_MUSIC)
                .header("Referer", REFERER_YOUTUBE_MUSIC)
                .header("User-Agent", client.user_agent);
            let req = match &visitor {
                Some(v) => req.header("X-Goog-Visitor-Id", v),
                None => req,
            };
            match req.json(&body).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return resp.json::<Value>().await.map_err(|e| {
                            StreamError::Parse(format!("{endpoint} response JSON: {e}"))
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

    // ── InnerTube endpoints (InnerTube.kt) ──────────────────────────────

    fn context(&self, client: YouTubeClient, visitor: Option<&str>) -> Value {
        client.context(visitor)
    }

    pub async fn search_raw(&self, query: &str) -> Result<Value, StreamError> {
        self.ensure_visitor_data().await;
        let visitor = self.visitor_data().await;
        let body = json!({
            "context": self.context(WEB_REMIX, visitor.as_deref()),
            "query": query,
        });
        self.post(WEB_REMIX, "search", body).await
    }

    pub async fn browse_raw(
        &self,
        browse_id: Option<&str>,
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
        if let Some(c) = continuation {
            body["continuation"] = json!(c);
        }
        self.post(WEB_REMIX, "browse", body).await
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
