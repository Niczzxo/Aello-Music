//! Automatic sign-in cookie capture from the login WebView2 window.
//!
//! The Android APK captures the session in its login WebView with
//! `CookieManager.getCookie(url)`, which returns ALL cookies including
//! the HttpOnly `SAPISID`. The Windows equivalent is WebView2's
//! `ICoreWebView2CookieManager::GetCookies`, reached through Tauri's
//! `WebviewWindow::with_webview` (the closure runs on the webview thread,
//! where the COM callback is delivered). `document.cookie` JS eval is
//! NOT used anywhere here: SAPISID is HttpOnly and invisible to it.
//!
//! The cookie header itself never leaves Rust: `capture_signin_cookie`
//! only reports whether a SAPISID-bearing cookie exists (and, through
//! the sign-in command in `lib.rs`, hands it straight to the validation
//! path). The pure helpers (`build_cookie_header`,
//! `cookie_has_sapisid`) are platform-independent and unit-tested on
//! Linux; the COM code below is `#[cfg(windows)]` only and is verified
//! by `cargo check --target x86_64-pc-windows-gnu` (a Linux-only check
//! would skip it entirely — the smtc.rs lesson).

#[cfg(windows)]
use std::time::Duration;
use tauri::AppHandle;
#[cfg(windows)]
use tauri::Manager;

#[cfg(windows)]
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2CookieList;

/// The URL whose cookies are the sign-in session (same URL Android
/// reads the cookie for in `LoginScreen.kt`).
#[cfg(windows)]
pub const SIGNIN_COOKIE_URL: &str = "https://music.youtube.com";

/// What a capture poll found.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    /// The login window has cookies, but no SAPISID yet — the user
    /// hasn't finished signing in. Keep polling.
    NotSignedIn,
    /// The login window is gone (closed, or never opened). Stop polling.
    WindowGone,
    /// Cookie header value containing SAPISID — ready to validate.
    SignedIn(String),
}

/// Build a `Cookie` header value from `(name, value)` pairs, in the
/// order the cookie manager returned them.
#[cfg(any(windows, test))]
pub fn build_cookie_header(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The APK's login check, verbatim: `"SAPISID" in parseCookieString(cookie)`.
/// A `SAPISID` cookie is only ever set by Google after a successful
/// sign-in, so its presence is the signed-in signal.
#[cfg(any(windows, test))]
pub fn cookie_has_sapisid(cookie: &str) -> bool {
    cookie.split(';').any(|part| {
        let part = part.trim();
        part.starts_with("SAPISID=") && part.len() > "SAPISID=".len()
    })
}

/// Read the login window's cookies for [`SIGNIN_COOKIE_URL`] and report
/// whether they carry SAPISID yet. Never panics: every failure mode is
/// a returned value. A stuck capture cannot hang the caller forever —
/// the wait is bounded by `CAPTURE_TIMEOUT`.
#[cfg(windows)]
pub async fn capture_signin_cookie(
    app: &AppHandle,
    label: &str,
) -> Result<CaptureOutcome, String> {
    const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

    let window = match app.get_webview_window(label) {
        Some(window) => window,
        None => return Ok(CaptureOutcome::WindowGone),
    };
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<(String, String)>, String>>();
    // `with_webview` dispatches the closure to the webview thread; it
    // returns once dispatched, NOT once the closure ran — the oneshot
    // carries the result back.
    window
        .as_ref()
        .with_webview(move |platform| {
            let outcome = read_cookies_on_webview_thread(platform);
            let _ = tx.send(outcome);
        })
        .map_err(|e| format!("could not reach the login webview: {e}"))?;

    let pairs = match tokio::time::timeout(CAPTURE_TIMEOUT, rx).await {
        Ok(Ok(Ok(pairs))) => pairs,
        Ok(Ok(Err(e))) => return Err(e),
        Ok(Err(_)) => return Err("cookie capture channel closed".to_string()),
        Err(_) => {
            // Timed out: the webview thread never answered. If the
            // window is gone now, say so so the frontend stops polling
            // instead of retrying a dead window forever.
            if app.get_webview_window(label).is_none() {
                return Ok(CaptureOutcome::WindowGone);
            }
            return Err("cookie capture timed out".to_string());
        }
    };
    let cookie = build_cookie_header(&pairs);
    if cookie_has_sapisid(&cookie) {
        Ok(CaptureOutcome::SignedIn(cookie))
    } else {
        Ok(CaptureOutcome::NotSignedIn)
    }
}

/// Non-Windows stub: there is no WebView2 here, so capture is
/// unavailable. (The Linux build exists only to check the shared code.)
#[cfg(not(windows))]
pub async fn capture_signin_cookie(
    _app: &AppHandle,
    _label: &str,
) -> Result<CaptureOutcome, String> {
    Err("automatic cookie capture is Windows-only".to_string())
}

/// Best-effort deletion of ALL WebView2 cookies for this app's profile
/// (sign-out: the next sign-in must prompt again instead of silently
/// reusing the old Google session). Fire-and-forget on the main
/// window's webview thread — the login window shares the same WebView2
/// profile, so its cookies are in the same store. Never fails sign-out:
/// every error is swallowed here.
#[cfg(windows)]
pub fn clear_webview_cookies(app: &AppHandle) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_2;
    use windows::core::Interface;

    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    // The closure runs asynchronously on the webview thread; dropping
    // the Result is deliberate — this must never block or fail sign-out.
    let _ = window.as_ref().with_webview(|platform| {
        let controller = platform.controller();
        let Ok(core) = (unsafe { controller.CoreWebView2() }) else {
            return;
        };
        let Ok(core2) = core.cast::<ICoreWebView2_2>() else {
            return;
        };
        let Ok(manager) = (unsafe { core2.CookieManager() }) else {
            return;
        };
        let _ = unsafe { manager.DeleteAllCookies() };
    });
}

/// Non-Windows no-op (see [`capture_signin_cookie`]).
#[cfg(not(windows))]
pub fn clear_webview_cookies(_app: &AppHandle) {}

/// Runs on the webview thread (inside the `with_webview` closure).
/// `GetCookies` is async COM: its completion handler is invoked via a
/// posted window message, so a plain blocking wait on this thread would
/// deadlock — `GetCookiesCompletedHandler::wait_for_async_operation`
/// pumps the thread's message queue while waiting, which is exactly
/// what it was written for.
#[cfg(windows)]
fn read_cookies_on_webview_thread(
    platform: tauri::webview::PlatformWebview,
) -> Result<Vec<(String, String)>, String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_2;
    use webview2_com::GetCookiesCompletedHandler;
    use windows::core::{HSTRING, Interface};

    let controller = platform.controller();
    let core = unsafe { controller.CoreWebView2() }
        .map_err(|e| format!("WebView2 CoreWebView2 failed: {e}"))?;
    // `CookieManager` lives on `ICoreWebView2_2`, not the base interface.
    let core2: ICoreWebView2_2 = core
        .cast()
        .map_err(|e| format!("WebView2 ICoreWebView2_2 cast failed: {e}"))?;
    let manager = unsafe { core2.CookieManager() }
        .map_err(|e| format!("WebView2 CookieManager failed: {e}"))?;
    let uri = HSTRING::from(SIGNIN_COOKIE_URL);

    // The completed closure stashes the pairs; a failed HRESULT is
    // reported as an error through `wait_for_async_operation` itself.
    let pairs_cell = std::sync::Arc::new(std::sync::Mutex::new(None::<Vec<(String, String)>>));
    let cell = pairs_cell.clone();
    GetCookiesCompletedHandler::wait_for_async_operation(
        // The boxed closure is 'static, so it owns the manager + URI.
        // GetCookies takes the URI as a PCWSTR over the HSTRING buffer.
        Box::new(move |handler| {
            let uri_ptr = windows::core::PCWSTR::from_raw(uri.as_ptr());
            unsafe { manager.GetCookies(uri_ptr, &handler) }
                .map_err(webview2_com::Error::from)?;
            Ok(())
        }),
        Box::new(move |hr, list| {
            // First arg is already the HRESULT converted to Result<()>
            // by the generated Invoke shim — a failed op short-circuits
            // here and surfaces through wait_for_async_operation.
            hr?;
            let pairs = collect_cookies(list)?;
            if let Ok(mut slot) = cell.lock() {
                *slot = Some(pairs);
            }
            Ok(())
        }),
    )
    .map_err(|e| format!("WebView2 GetCookies failed: {e}"))?;
    Ok(pairs_cell
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .unwrap_or_default())
}

/// Walk an `ICoreWebView2CookieList` into `(name, value)` pairs.
/// `Name`/`Value` hand out CoTaskMem-allocated strings; `take_pwstr`
/// copies them into Rust `String`s and frees the COM buffers.
#[cfg(windows)]
fn collect_cookies(
    list: Option<ICoreWebView2CookieList>,
) -> windows::core::Result<Vec<(String, String)>> {
    use webview2_com::take_pwstr;
    use windows::core::PWSTR;

    let mut pairs = Vec::new();
    let Some(list) = list else {
        return Ok(pairs);
    };
    let mut count = 0u32;
    unsafe { list.Count(&mut count)? };
    for i in 0..count {
        let cookie = unsafe { list.GetValueAtIndex(i)? };
        let mut name = PWSTR::null();
        unsafe { cookie.Name(&mut name)? };
        let name = take_pwstr(name);
        let mut value = PWSTR::null();
        unsafe { cookie.Value(&mut value)? };
        let value = take_pwstr(value);
        pairs.push((name, value));
    }
    Ok(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_header_joins_pairs() {
        let pairs = vec![
            ("SID".to_string(), "abc".to_string()),
            ("SAPISID".to_string(), "xyz".to_string()),
        ];
        assert_eq!(build_cookie_header(&pairs), "SID=abc; SAPISID=xyz");
        assert_eq!(build_cookie_header(&[]), "");
    }

    #[test]
    fn sapisid_detection_matches_apk_check() {
        assert!(cookie_has_sapisid("SID=abc; SAPISID=xyz; HSID=1"));
        assert!(cookie_has_sapisid("SAPISID=xyz"));
        assert!(!cookie_has_sapisid("SID=abc; HSID=1"));
        assert!(!cookie_has_sapisid(""));
        // A bare name with no value is not a session.
        assert!(!cookie_has_sapisid("SAPISID=; SID=abc"));
        // A longer name that merely contains SAPISID is not SAPISID.
        assert!(!cookie_has_sapisid("XSAPISID=xyz"));
    }
}
