//! In-app Windows update check (GAP_TABLE #51, audit §15).
//!
//! GET the GitHub releases list for `Niczzxo/Aello-Music` (the repo
//! that hosts the Windows NSIS releases) and pick the latest tag `^v*`
//! carrying an `Aello_*_x64-setup.exe` asset. Strictly-newer compare
//! against this build's version.
//!
//! The 6.1.41 rule: on ANY fetch failure the result carries
//! `error=Some(..)` — the UI must show "couldn't check", NEVER
//! "latest". `available=false, error=None` means genuinely latest.

use crate::innertube::session::InnerTubeSession;
use crate::models::UpdateInfo;

const RELEASES_URL: &str = "https://api.github.com/repos/Niczzxo/Aello-Music/releases";
/// Android 6.1.45 `releases/latest` redirect fallback
/// (`vivimusicupdater.kt`): unauthenticated api.github.com returns 403
/// on shared/CGNAT IPs, so when the API list fails, follow the
/// `github.com/releases/latest` redirect (reqwest follows it) and read
/// the Windows setup asset straight off the release page.
const LATEST_RELEASE_URL: &str = "https://github.com/Niczzxo/Aello-Music/releases/latest";
const RELEASE_DOWNLOAD_PREFIX: &str = "/Niczzxo/Aello-Music/releases/download/";

fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    let stripped = tag.strip_prefix('v').unwrap_or(tag);
    let mut parts = stripped.split('.');
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let patch: u64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

fn is_newer(latest: (u64, u64, u64), current: (u64, u64, u64)) -> bool {
    latest > current
}

pub async fn check_for_update(session: &InnerTubeSession) -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let current_ver = parse_version(&current).unwrap_or((0, 0, 0));

    let fail = |reason: String| UpdateInfo {
        available: false,
        version: current.clone(),
        notes: String::new(),
        exe_url: None,
        error: Some(reason),
    };

    let releases = match session
        .http_get_json(RELEASES_URL, &[("Accept", "application/vnd.github.v3+json")])
        .await
    {
        Ok(v) => v,
        Err(api_err) => {
            // API list failed (403 on shared/CGNAT IPs is the common
            // one) — try the redirect fallback before giving up.
            return check_via_latest_page(session, &current, current_ver, api_err.to_string()).await;
        }
    };
    let releases = match releases.as_array() {
        Some(a) => a,
        None => return fail("Update check failed: unexpected GitHub response".to_string()),
    };

    // Latest tag ^v* with a Windows setup exe asset. Drafts skipped
    // (Android rule); prereleases are NOT offered on the stable channel.
    for release in releases {
        if release
            .get("draft")
            .and_then(|d| d.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        if release
            .get("prerelease")
            .and_then(|p| p.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let Some(tag) = release.get("tag_name").and_then(|t| t.as_str()) else {
            continue;
        };
        if !tag.starts_with('v') {
            continue;
        }
        let Some(latest_ver) = parse_version(tag) else {
            continue;
        };
        let Some(exe_url) = release
            .get("assets")
            .and_then(|a| a.as_array())
            .and_then(|assets| {
                assets.iter().find_map(|asset| {
                    let name = asset.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    if name.starts_with("Aello_") && name.ends_with("_x64-setup.exe") {
                        asset
                            .get("browser_download_url")
                            .and_then(|u| u.as_str())
                            .map(|u| u.to_string())
                    } else {
                        None
                    }
                })
            })
        else {
            continue;
        };
        let notes = release
            .get("body")
            .and_then(|b| b.as_str())
            .unwrap_or("")
            .chars()
            .take(2000)
            .collect::<String>();
        let version = tag.trim_start_matches('v').to_string();
        if is_newer(latest_ver, current_ver) {
            return UpdateInfo {
                available: true,
                version,
                notes,
                exe_url: Some(exe_url),
                error: None,
            };
        }
        // Latest Windows release is not newer: genuinely latest.
        return UpdateInfo {
            available: false,
            version: current,
            notes: String::new(),
            exe_url: None,
            error: None,
        };
    }
    fail("Update check failed: no Windows release found".to_string())
}

/// Fallback after the api.github.com list fails: fetch the
/// `releases/latest` page (redirect followed by reqwest) and take the
/// first `…/releases/download/<tag>/Aello_*_x64-setup.exe` link off
/// the page. The tag between `/download/` and the next `/` is the
/// version. Release notes are not on this page — notes stay empty.
/// Any failure here is a failed check (error set, never "latest").
async fn check_via_latest_page(
    session: &InnerTubeSession,
    current: &str,
    current_ver: (u64, u64, u64),
    api_error: String,
) -> UpdateInfo {
    let fail = |reason: String| UpdateInfo {
        available: false,
        version: current.to_string(),
        notes: String::new(),
        exe_url: None,
        error: Some(reason),
    };
    let page = match session
        .get_text(
            LATEST_RELEASE_URL,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
        )
        .await
    {
        Ok(text) => text,
        Err(e) => {
            return fail(format!(
                "Update check failed: GitHub API ({api_error}) and releases page ({e}) both unreachable"
            ));
        }
    };
    // First download link for a Windows setup exe on the page.
    let mut search_from = 0;
    while let Some(rel) = page[search_from..].find(RELEASE_DOWNLOAD_PREFIX) {
        let start = search_from + rel;
        let rest = &page[start..];
        let Some(exe_end) = rest.find("_x64-setup.exe") else {
            break;
        };
        let asset_path = &rest[..exe_end + "_x64-setup.exe".len()];
        // Tag = the segment after /download/ up to the next /.
        let after_prefix = &asset_path[RELEASE_DOWNLOAD_PREFIX.len()..];
        let Some(tag) = after_prefix.split('/').next() else {
            break;
        };
        if !tag.starts_with('v') {
            search_from = start + RELEASE_DOWNLOAD_PREFIX.len();
            continue;
        }
        let Some(latest_ver) = parse_version(tag) else {
            search_from = start + RELEASE_DOWNLOAD_PREFIX.len();
            continue;
        };
        let version = tag.trim_start_matches('v').to_string();
        if is_newer(latest_ver, current_ver) {
            return UpdateInfo {
                available: true,
                version,
                notes: String::new(),
                exe_url: Some(format!("https://github.com{asset_path}")),
                error: None,
            };
        }
        return UpdateInfo {
            available: false,
            version: current.to_string(),
            notes: String::new(),
            exe_url: None,
            error: None,
        };
    }
    fail(format!(
        "Update check failed: GitHub API ({api_error}) and no Windows setup asset on the latest release page"
    ))
}
