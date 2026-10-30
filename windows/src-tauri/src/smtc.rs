//! Windows System Media Transport Controls (SMTC) — GAP_TABLE #52.
//!
//! Drives the Windows media overlay / media keys from the playback
//! snapshot the frontend pushes via `update_player_snapshot`.
//! Button presses are forwarded to the frontend as Tauri events
//! (`smtc-play`, `smtc-pause`, `smtc-next`, `smtc-previous`); the
//! frontend owns actual playback.
//!
//! **UNTESTED**: this module only compiles on Windows (`#[cfg(windows)]`
//! + target-specific `windows` dependency). There is no Windows machine
//! on this Linux VM — the user's PC is the verdict, same as the Android
//! device verdicts. Two things in particular need a real Windows run:
//! (1) `GetForWindow` needs a WinRT apartment on the calling thread
//! (Tauri's main thread, where `setup` runs, initializes COM for
//! WebView2 — the usual place this works from); (2) the
//! `ISystemMediaTransportControlsInterop` IID below is the one from
//! Microsoft's `systemmediatransportcontrolsinterop.h` — the `windows`
//! crate 0.62 does NOT ship this interop interface (only UWP's
//! `GetForCurrentView`), so it is defined by hand here. If either is
//! wrong, `SmtcManager::new` returns `Err`, `lib.rs` stores `None`,
//! and the app runs without media controls instead of crashing startup.

#![cfg(windows)]

use crate::player::PlaybackSnapshot;
use tauri::{AppHandle, Manager};
use windows::core::{
    factory, interface, GUID, HRESULT, IInspectable, Interface, Ref, Type, HSTRING,
};
use windows::Foundation::{TypedEventHandler, Uri};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
};
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::Win32::Foundation::HWND;

/// `ISystemMediaTransportControlsInterop` — the Win32 (non-UWP) way to
/// get an SMTC instance for a window. IID from
/// `systemmediatransportcontrolsinterop.h`:
/// `MIDL_INTERFACE("65A6F408-1693-4505-BC0D-9735DC0C1312")`.
/// Not shipped by the `windows` crate, so it is defined here.
#[interface("65A6F408-1693-4505-BC0D-9735DC0C1312")]
unsafe trait ISystemMediaTransportControlsInterop: IInspectable {
    fn GetForWindow(
        &self,
        appwindow: HWND,
        riid: *const GUID,
        mediacontrol: *mut *mut core::ffi::c_void,
    ) -> HRESULT;
}

/// Owns one SMTC instance for the main window.
pub struct SmtcManager {
    smtc: SystemMediaTransportControls,
    // Kept so the button-pressed delegate can be removed on drop.
    #[allow(dead_code)]
    token: i64,
}

impl SmtcManager {
    /// Create SMTC for `hwnd` (the main WebViewWindow handle).
    pub fn new(app: AppHandle, hwnd: HWND) -> windows::core::Result<Self> {
        let interop: ISystemMediaTransportControlsInterop =
            factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()?;
        let mut abi: *mut core::ffi::c_void = std::ptr::null_mut();
        unsafe {
            interop.GetForWindow(
                hwnd,
                &SystemMediaTransportControls::IID,
                &mut abi,
            )?;
        }
        let smtc =
            unsafe { SystemMediaTransportControls::from_abi(abi) }?;
        smtc.SetIsEnabled(true)?;
        smtc.SetIsPlayEnabled(true)?;
        smtc.SetIsPauseEnabled(true)?;
        smtc.SetIsNextEnabled(true)?;
        smtc.SetIsPreviousEnabled(true)?;

        let app_for_handler = app.clone();
        let handler = TypedEventHandler::new(
            move |_sender: Ref<SystemMediaTransportControls>,
                  args: Ref<SystemMediaTransportControlsButtonPressedEventArgs>| {
                let Some(args) = args.as_ref() else {
                    return Ok(());
                };
                let event = match args.Button()? {
                    SystemMediaTransportControlsButton::Play => "smtc-play",
                    SystemMediaTransportControlsButton::Pause => "smtc-pause",
                    SystemMediaTransportControlsButton::Next => "smtc-next",
                    SystemMediaTransportControlsButton::Previous => "smtc-previous",
                    // Stop / channel / record / fast-forward / rewind:
                    // no frontend mapping in this pass.
                    _ => return Ok(()),
                };
                let _ = app_for_handler.emit(event, ());
                Ok(())
            },
        );
        let token = smtc.ButtonPressed(&handler)?;
        Ok(Self { smtc, token })
    }

    /// Push the current snapshot into SMTC (best-effort: every failure
    /// is swallowed — media controls must never break playback).
    pub fn update(&self, snapshot: &PlaybackSnapshot) {
        let _ = self.update_inner(snapshot);
    }

    fn update_inner(&self, snapshot: &PlaybackSnapshot) -> windows::core::Result<()> {
        let updater = self.smtc.DisplayUpdater()?;
        updater.SetType(MediaPlaybackType::Music)?;
        let title = snapshot
            .current_title
            .clone()
            .or_else(|| snapshot.current_song_id.clone())
            .unwrap_or_default();
        updater.SetTitle(&HSTRING::from(title.as_str()))?;
        updater.SetArtist(&HSTRING::from(
            snapshot.current_artist.clone().unwrap_or_default().as_str(),
        ))?;
        if let Some(artwork) = snapshot.current_artwork.as_deref() {
            if let Ok(uri) = Uri::CreateUri(&HSTRING::from(artwork)) {
                if let Ok(thumb) = RandomAccessStreamReference::CreateFromUri(&uri) {
                    let _ = updater.SetThumbnail(&thumb);
                }
            }
        }
        updater.Update()?;
        self.smtc.SetPlaybackStatus(if snapshot.is_playing {
            MediaPlaybackStatus::Playing
        } else {
            MediaPlaybackStatus::Paused
        })?;
        Ok(())
    }
}
