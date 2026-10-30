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
//! WebView2 — the usual place this works from); (2) the interop
//! interface comes from the `windows` crate itself (see below). If
//! either is wrong, `SmtcManager::new` returns `Err`, `lib.rs` stores
//! `None`, and the app runs without media controls instead of crashing
//! startup.

#![cfg(windows)]

use crate::player::PlaybackSnapshot;
use tauri::{AppHandle, Emitter};
use windows::core::{factory, Ref, HSTRING};
use windows::Foundation::{TypedEventHandler, Uri};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
};
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

/// `ISystemMediaTransportControlsInterop` — the Win32 (non-UWP) way to
/// get an SMTC instance for a window — comes from the `windows` crate
/// (`windows::Win32::System::WinRT`, feature `Win32_System_WinRT`):
/// IID ddb0472d-c911-4a1f-86d9-dc3d71a95f5a, IInspectable-based, per
/// systemmediatransportcontrolsinterop.h (confirmed against the SDK
/// header via Mozilla's WindowsSMTCProvider.cpp).
///
/// Do NOT hand-define it with `#[interface]`: the macro cannot compile
/// a trait deriving from IInspectable in windows 0.62 (it emits a
/// reference to `IInspectable_Impl`, which does not exist in
/// windows-core), and the IID previously written here by hand
/// (65A6F408-...) was wrong — with it, the activation-factory
/// QueryInterface would fail at runtime and SMTC would silently never
/// start. That hand definition is what broke the Windows build
/// (run 41137473016: E0433/E0405/E0425 in smtc.rs).

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
        let smtc: SystemMediaTransportControls =
            unsafe { interop.GetForWindow(hwnd) }?;
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
        // Title/artist live on MusicProperties, not on the updater itself
        // (the updater has no SetTitle/SetArtist — windows 0.62).
        let music = updater.MusicProperties()?;
        music.SetTitle(&HSTRING::from(title.as_str()))?;
        music.SetArtist(&HSTRING::from(
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
