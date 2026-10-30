//! Error taxonomy for the InnerTube backend.
//!
//! The frontend shows `Display` output verbatim, so every variant carries
//! the REAL reason YouTube (or the network) gave — never a generic
//! "something went wrong".

use std::fmt;

/// Typed errors from the InnerTube client. `Display` yields the real reason
/// string so the UI can show it verbatim.
#[derive(Debug, Clone)]
pub enum StreamError {
    /// Transport failure (DNS, connect, TLS, timeout, …).
    Network(String),
    /// InnerTube answered with an HTTP status that is not retryable
    /// (e.g. 400/404) or survived all retries (429/5xx).
    Http(u16),
    /// YouTube refused to serve this item: `playabilityStatus` was
    /// `LOGIN_REQUIRED`, `UNPLAYABLE`, age-gated, etc. The payload is the
    /// exact `reason` string from the player response.
    Blocked(String),
    /// Player response was OK but carried no `streamingData` / no formats
    /// at all.
    NoStream,
    /// Audio formats exist, but only Opus-in-WebM. WebView2's `<audio>`
    /// element cannot play Opus-in-WebM, so there is nothing playable on
    /// the `<audio>` path. Distinct from [`StreamError::NoStream`] so the
    /// UI can say exactly this instead of failing silently.
    NoPlayableFormat,
    /// A chosen format only has a `signatureCipher` URL and the signature
    /// could not be deciphered (player JS shape changed, helper missing,
    /// …). Never return a broken URL instead.
    CipherUnsupported(String),
    /// A response could not be parsed (unexpected JSON shape, bad
    /// visitor-data payload, …).
    Parse(String),
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamError::Network(msg) => write!(f, "Network error: {msg}"),
            StreamError::Http(status) => write!(f, "YouTube returned HTTP {status}"),
            StreamError::Blocked(reason) => write!(f, "{reason}"),
            StreamError::NoStream => {
                write!(f, "No stream data in the player response for this track")
            }
            StreamError::NoPlayableFormat => write!(
                f,
                "Only Opus/WebM audio is available for this track; the player needs AAC/MP4"
            ),
            StreamError::CipherUnsupported(detail) => {
                write!(f, "Ciphered stream URL could not be deciphered: {detail}")
            }
            StreamError::Parse(detail) => {
                write!(f, "Failed to parse the YouTube response: {detail}")
            }
        }
    }
}

impl std::error::Error for StreamError {}

/// A resolved, validated stream URL for the `<audio>` element.
#[derive(Debug, Clone)]
pub struct StreamUrl {
    /// Direct googlevideo URL. Already validated with a HEAD request
    /// (except when it came from the last fallback client, see resolve).
    pub url: String,
    /// YouTube itag (140 = AAC 128kbps MP4 is the usual pick).
    pub itag: i32,
    /// e.g. `audio/mp4; codecs="mp4a.40.2"`.
    pub mime_type: String,
    /// Bits per second of the chosen format.
    pub bitrate: i32,
    /// Seconds until the URL expires (YouTube's usual value is 21600).
    pub expires_in_seconds: i64,
    /// Which InnerTube client produced this stream (diagnostics).
    pub client_name: String,
}
