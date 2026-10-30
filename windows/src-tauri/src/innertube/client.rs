//! InnerTube client definitions + request-context builder.
//!
//! Port of `YouTubeClient.kt` (client name/version/id/user-agent strings
//! copied exactly) and `Context.kt` / `YouTubeClient.toContext`.
//! Guest only: no login cookie, no `dataSyncId`, no PoToken.

use serde_json::{json, Value};

pub const API_URL_YOUTUBE_MUSIC: &str = "https://music.youtube.com/youtubei/v1/";
pub const ORIGIN_YOUTUBE_MUSIC: &str = "https://music.youtube.com";
pub const REFERER_YOUTUBE_MUSIC: &str = "https://music.youtube.com/";
pub const USER_AGENT_WEB: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0";

/// Default locale for the Windows client. Windows has no Android locale to
/// read, so this is fixed and explicit: US English.
pub const DEFAULT_GL: &str = "US";
pub const DEFAULT_HL: &str = "en";

/// One InnerTube client identity. Field-for-field the Kotlin
/// `YouTubeClient`; only the fields that enter the request context or
/// headers are kept. (`login_supported` / `use_web_po_tokens` are kept
/// for the future login/PoToken work; guest mode does not read them.)
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct YouTubeClient {
    pub client_name: &'static str,
    pub client_version: &'static str,
    /// NOTE: the `X-YouTube-Client-Name` header carries this id, not the name.
    pub client_id: &'static str,
    pub user_agent: &'static str,
    pub os_name: Option<&'static str>,
    pub os_version: Option<&'static str>,
    pub device_make: Option<&'static str>,
    pub device_model: Option<&'static str>,
    pub android_sdk_version: Option<&'static str>,
    pub login_supported: bool,
    pub login_required: bool,
    pub use_signature_timestamp: bool,
    pub is_embedded: bool,
    pub use_web_po_tokens: bool,
    /// Short label for logs / `StreamUrl.client_name`.
    pub label: &'static str,
}

impl YouTubeClient {
    /// `context.client` object for this client, mirroring
    /// `YouTubeClient.toContext`. `visitor_data` is omitted when `None`
    /// (kotlinx `explicitNulls = false`).
    pub fn context(&self, visitor_data: Option<&str>) -> Value {
        let mut client = json!({
            "clientName": self.client_name,
            "clientVersion": self.client_version,
            "gl": DEFAULT_GL,
            "hl": DEFAULT_HL,
        });
        let obj = client.as_object_mut().expect("context client object");
        if let Some(v) = self.os_name {
            obj.insert("osName".into(), json!(v));
        }
        if let Some(v) = self.os_version {
            obj.insert("osVersion".into(), json!(v));
        }
        if let Some(v) = self.device_make {
            obj.insert("deviceMake".into(), json!(v));
        }
        if let Some(v) = self.device_model {
            obj.insert("deviceModel".into(), json!(v));
        }
        if let Some(v) = self.android_sdk_version {
            obj.insert("androidSdkVersion".into(), json!(v));
        }
        if let Some(v) = visitor_data {
            obj.insert("visitorData".into(), json!(v));
        }
        json!({
            "client": client,
            "request": { "internalExperimentFlags": [], "useSsl": true },
            "user": { "lockedSafetyMode": false },
        })
    }
}

/// Main playback client: fast, no PoToken, no login.
/// (`ANDROID_VR_1_43_32` in `YouTubeClient.kt`.)
pub const ANDROID_VR_1_43_32: YouTubeClient = YouTubeClient {
    client_name: "ANDROID_VR",
    client_version: "1.43.32",
    client_id: "28",
    user_agent: "com.google.android.apps.youtube.vr.oculus/1.43.32 (Linux; U; Android 12; en_US; Quest 3; Build/SQ3A.220605.009.A1; Cronet/107.0.5284.2)",
    os_name: Some("Android"),
    os_version: Some("12"),
    device_make: Some("Oculus"),
    device_model: Some("Quest 3"),
    android_sdk_version: Some("32"),
    login_supported: false,
    login_required: false,
    use_signature_timestamp: false,
    is_embedded: false,
    use_web_po_tokens: false,
    label: "ANDROID_VR_1_43_32",
};

/// (`ANDROID_VR_NO_AUTH` in `YouTubeClient.kt`.)
pub const ANDROID_VR_NO_AUTH: YouTubeClient = YouTubeClient {
    client_name: "ANDROID_VR",
    client_version: "1.61.48",
    client_id: "28",
    user_agent: "com.google.android.apps.youtube.vr.oculus/1.61.48 (Linux; U; Android 12; en_US; Oculus Quest 3; Build/SQ3A.220605.009.A1; Cronet/132.0.6808.3)",
    os_name: None,
    os_version: None,
    device_make: None,
    device_model: None,
    android_sdk_version: None,
    login_supported: false,
    login_required: false,
    use_signature_timestamp: false,
    is_embedded: false,
    use_web_po_tokens: false,
    label: "ANDROID_VR_NO_AUTH",
};

/// (`ANDROID_VR_1_61_48` in `YouTubeClient.kt`.)
pub const ANDROID_VR_1_61_48: YouTubeClient = YouTubeClient {
    client_name: "ANDROID_VR",
    client_version: "1.61.48",
    client_id: "28",
    user_agent: "com.google.android.apps.youtube.vr.oculus/1.61.48 (Linux; U; Android 12; en_US; Quest 3; Build/SQ3A.220605.009.A1; Cronet/132.0.6808.3)",
    os_name: Some("Android"),
    os_version: Some("12"),
    device_make: Some("Oculus"),
    device_model: Some("Quest 3"),
    android_sdk_version: Some("32"),
    login_supported: false,
    login_required: false,
    use_signature_timestamp: false,
    is_embedded: false,
    use_web_po_tokens: false,
    label: "ANDROID_VR_1_61_48",
};

/// (`VISIONOS` in `YouTubeClient.kt` — internal/unreleased client.)
pub const VISIONOS: YouTubeClient = YouTubeClient {
    client_name: "VISIONOS",
    client_version: "0.1",
    client_id: "101",
    user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15",
    os_name: Some("visionOS"),
    os_version: Some("1.3.21O771"),
    device_make: Some("Apple"),
    device_model: Some("RealityDevice14,1"),
    android_sdk_version: None,
    login_supported: false,
    login_required: false,
    use_signature_timestamp: false,
    is_embedded: false,
    use_web_po_tokens: false,
    label: "VISIONOS",
};

/// (`IOS` in `YouTubeClient.kt`.)
pub const IOS: YouTubeClient = YouTubeClient {
    client_name: "IOS",
    client_version: "21.03.1",
    client_id: "5",
    user_agent: "com.google.ios.youtube/21.03.1 (iPhone16,2; U; CPU iOS 18_2 like Mac OS X;)",
    os_name: None,
    os_version: Some("18.2.22C152"),
    device_make: None,
    device_model: None,
    android_sdk_version: None,
    login_supported: false,
    login_required: false,
    use_signature_timestamp: false,
    is_embedded: false,
    use_web_po_tokens: false,
    label: "IOS",
};

/// (`IPADOS` in `YouTubeClient.kt`.)
pub const IPADOS: YouTubeClient = YouTubeClient {
    client_name: "IOS",
    client_version: "21.03.3",
    client_id: "5",
    user_agent: "com.google.ios.youtube/21.03.3 (iPad7,6; U; CPU iPadOS 17_7_10 like Mac OS X; en-US)",
    os_name: Some("iPadOS"),
    os_version: Some("17.7.10.21H450"),
    device_make: Some("Apple"),
    device_model: Some("iPad7,6"),
    android_sdk_version: None,
    login_supported: false,
    login_required: false,
    use_signature_timestamp: false,
    is_embedded: false,
    use_web_po_tokens: false,
    label: "IPADOS",
};

/// (`ANDROID_NO_SDK` in `YouTubeClient.kt`.)
#[allow(dead_code)]
pub const ANDROID_NO_SDK: YouTubeClient = YouTubeClient {
    client_name: "ANDROID",
    client_version: "21.03.38",
    client_id: "3",
    user_agent: "com.google.android.youtube/21.03.38 (Linux; U; Android 14) gzip",
    os_name: None,
    os_version: None,
    device_make: None,
    device_model: None,
    android_sdk_version: None,
    login_supported: false,
    login_required: false,
    use_signature_timestamp: false,
    is_embedded: false,
    use_web_po_tokens: false,
    label: "ANDROID_NO_SDK",
};

/// (`MOBILE` in `YouTubeClient.kt`.)
pub const MOBILE: YouTubeClient = YouTubeClient {
    client_name: "ANDROID",
    client_version: "21.03.38",
    client_id: "3",
    user_agent: "com.google.android.youtube/21.03.38 (Linux; U; Android 14) gzip",
    os_name: None,
    os_version: None,
    device_make: None,
    device_model: None,
    android_sdk_version: None,
    login_supported: true,
    login_required: false,
    use_signature_timestamp: true,
    is_embedded: false,
    use_web_po_tokens: false,
    label: "MOBILE",
};

/// (`ANDROID_CREATOR` in `YouTubeClient.kt`.)
pub const ANDROID_CREATOR: YouTubeClient = YouTubeClient {
    client_name: "ANDROID_CREATOR",
    client_version: "25.03.101",
    client_id: "14",
    user_agent: "com.google.android.apps.youtube.creator/25.03.101 (Linux; U; Android 15; en_US; Pixel 9 Pro Fold; Build/AP3A.241005.015.A2; Cronet/132.0.6779.0)",
    os_name: Some("Android"),
    os_version: Some("15"),
    device_make: Some("Google"),
    device_model: Some("Pixel 9 Pro Fold"),
    android_sdk_version: Some("35"),
    login_supported: true,
    login_required: false,
    use_signature_timestamp: true,
    is_embedded: false,
    use_web_po_tokens: false,
    label: "ANDROID_CREATOR",
};

/// Metadata / browse / search client. (`WEB_REMIX` in `YouTubeClient.kt`.)
/// Needs a PoToken for player calls on bot-flagged tracks — this port has
/// no PoToken source, so it runs without one and may fail where Android's
/// WebView-minted token would succeed (see resolve.rs).
pub const WEB_REMIX: YouTubeClient = YouTubeClient {
    client_name: "WEB_REMIX",
    client_version: "1.20260213.01.00",
    client_id: "67",
    user_agent: USER_AGENT_WEB,
    os_name: None,
    os_version: None,
    device_make: None,
    device_model: None,
    android_sdk_version: None,
    login_supported: true,
    login_required: false,
    use_signature_timestamp: true,
    is_embedded: false,
    use_web_po_tokens: true,
    label: "WEB_REMIX",
};

/// (`WEB` in `YouTubeClient.kt`.)
pub const WEB: YouTubeClient = YouTubeClient {
    client_name: "WEB",
    client_version: "2.20260213.00.00",
    client_id: "1",
    user_agent: USER_AGENT_WEB,
    os_name: None,
    os_version: None,
    device_make: None,
    device_model: None,
    android_sdk_version: None,
    login_supported: false,
    login_required: false,
    use_signature_timestamp: false,
    is_embedded: false,
    use_web_po_tokens: false,
    label: "WEB",
};

/// (`WEB_CREATOR` in `YouTubeClient.kt`.) Login-required; skipped in guest mode.
pub const WEB_CREATOR: YouTubeClient = YouTubeClient {
    client_name: "WEB_CREATOR",
    client_version: "1.20260213.00.00",
    client_id: "62",
    user_agent: USER_AGENT_WEB,
    os_name: None,
    os_version: None,
    device_make: None,
    device_model: None,
    android_sdk_version: None,
    login_supported: true,
    login_required: true,
    use_signature_timestamp: true,
    is_embedded: false,
    use_web_po_tokens: false,
    label: "WEB_CREATOR",
};

/// (`TVHTML5_SIMPLY_EMBEDDED_PLAYER` in `YouTubeClient.kt`.)
pub const TVHTML5_SIMPLY_EMBEDDED_PLAYER: YouTubeClient = YouTubeClient {
    client_name: "TVHTML5_SIMPLY_EMBEDDED_PLAYER",
    client_version: "2.0",
    client_id: "85",
    user_agent: "Mozilla/5.0 (PlayStation; PlayStation 4/12.02) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/15.4 Safari/605.1.15",
    os_name: None,
    os_version: None,
    device_make: None,
    device_model: None,
    android_sdk_version: None,
    login_supported: true,
    login_required: false,
    use_signature_timestamp: true,
    is_embedded: true,
    use_web_po_tokens: false,
    label: "TVHTML5_SIMPLY_EMBEDDED_PLAYER",
};

/// (`TVHTML5` in `YouTubeClient.kt`.) Login-required; skipped in guest mode.
pub const TVHTML5: YouTubeClient = YouTubeClient {
    client_name: "TVHTML5",
    client_version: "7.20260213.00.00",
    client_id: "7",
    user_agent: "Mozilla/5.0(SMART-TV; Linux; Tizen 4.0.0.2) AppleWebkit/605.1.15 (KHTML, like Gecko) SamsungBrowser/9.2 TV Safari/605.1.15",
    os_name: None,
    os_version: None,
    device_make: None,
    device_model: None,
    android_sdk_version: None,
    login_supported: true,
    login_required: true,
    use_signature_timestamp: true,
    is_embedded: false,
    use_web_po_tokens: true,
    label: "TVHTML5",
};

/// Main playback client.
pub const MAIN_CLIENT: YouTubeClient = ANDROID_VR_1_43_32;

/// Fallback chain, in exactly the Android order
/// (`STREAM_FALLBACK_CLIENTS` in `YTPlayerUtils.kt`). Guest mode skips
/// the `login_required` entries (TVHTML5, WEB_CREATOR).
pub const STREAM_FALLBACK_CLIENTS: &[YouTubeClient] = &[
    VISIONOS,
    IOS,
    ANDROID_VR_1_61_48,
    WEB_REMIX,
    TVHTML5_SIMPLY_EMBEDDED_PLAYER,
    TVHTML5,
    ANDROID_CREATOR,
    IPADOS,
    ANDROID_VR_NO_AUTH,
    MOBILE,
    WEB,
    WEB_CREATOR,
];
