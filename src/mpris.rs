use crate::mpv::SeekFlag;
use crate::tui::App;
use souvlaki::PlatformConfig;
use souvlaki::{MediaControlEvent, MediaControls, MediaPosition, SeekDirection};
use std::time::Duration;

// Supported on Linux (MPRIS), macOS (MediaPlayer framework) and Windows (WinRT)
pub struct MprisControls {
    pub controls: MediaControls,
    #[cfg(windows)]
    pub _dummy_window: Option<windows::DummyWindow>,
}

pub fn mpris() -> Result<MprisControls, Box<dyn std::error::Error>> {
    #[cfg(not(windows))]
    {
        let config =
            PlatformConfig { dbus_name: "jellyfin-tui", display_name: "jellyfin-tui", hwnd: None };

        match MediaControls::new(config) {
            Ok(controls) => {
                log::info!("Media controls created successfully for platform");
                Ok(MprisControls { controls })
            }
            Err(e) => {
                log::error!("Failed to create media controls: {:?}", e);
                Err(Box::new(e))
            }
        }
    }

    #[cfg(windows)]
    {
        let (hwnd, dummy_window) = match windows::DummyWindow::new() {
            Ok(dw) => (Some(dw.handle.0 as *mut std::ffi::c_void), Some(dw)),
            Err(e) => {
                log::warn!(
                    "Failed to create dummy window, falling back to GetConsoleWindow: {}",
                    e
                );
                use windows_sys::Win32::System::Console::GetConsoleWindow;
                let handle = unsafe { GetConsoleWindow() };
                if handle == 0 {
                    (None, None)
                } else {
                    (Some(handle as *mut std::ffi::c_void), None)
                }
            }
        };

        let config =
            PlatformConfig { dbus_name: "jellyfin-tui", display_name: "jellyfin-tui", hwnd };

        match MediaControls::new(config) {
            Ok(controls) => {
                log::info!("Media controls created successfully for platform");
                Ok(MprisControls { controls, _dummy_window: dummy_window })
            }
            Err(e) => {
                log::error!("Failed to create media controls: {:?}", e);
                Err(Box::new(e))
            }
        }
    }
}

#[cfg(windows)]
pub fn pump_event_queue() {
    windows::pump_event_queue();
}

// demonstrates how to make a minimal window to allow use of media keys on the command line
#[cfg(windows)]
mod windows {
    use std::io::Error;
    use std::mem;

    use windows::core::PCWSTR;
    use windows::core::w;
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetAncestor,
        IsDialogMessageW, PeekMessageW, RegisterClassExW, TranslateMessage, GA_ROOT, MSG,
        PM_REMOVE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_QUIT, WNDCLASSEXW,
    };

    pub struct DummyWindow {
        pub handle: HWND,
    }

    impl DummyWindow {
        pub fn new() -> Result<DummyWindow, String> {
            let class_name = w!("SimpleTray");

            unsafe {
                let instance = GetModuleHandleW(None)
                    .map_err(|e| format!("Getting module handle failed: {e}"))?;

                let wnd_class = WNDCLASSEXW {
                    cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
                    hInstance: HINSTANCE(instance.0),
                    lpszClassName: PCWSTR::from(class_name),
                    lpfnWndProc: Some(Self::wnd_proc),
                    ..Default::default()
                };

                if RegisterClassExW(&wnd_class) == 0 {
                    return Err(format!("Registering class failed: {}", Error::last_os_error()));
                }

                let handle = CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    class_name,
                    w!(""),
                    WINDOW_STYLE::default(),
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(HINSTANCE(instance.0)),
                    None,
                )
                .map_err(|e| format!("Message only window creation failed: {e}"))?;

                if handle.0.is_null() {
                    return Err(format!(
                        "Message only window creation failed: {}",
                        Error::last_os_error()
                    ));
                }

                Ok(DummyWindow { handle })
            }
        }
        extern "system" fn wnd_proc(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }

    impl Drop for DummyWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.handle);
            }
        }
    }

    pub fn pump_event_queue() -> bool {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            let mut has_message = PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool();
            while msg.message != WM_QUIT && has_message {
                if !IsDialogMessageW(GetAncestor(msg.hwnd, GA_ROOT), &msg).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                has_message = PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool();
            }

            msg.message == WM_QUIT
        }
    }
}

impl App {
    pub fn register_controls(
        mpris_controls: &mut MprisControls,
        mpris_tx: std::sync::mpsc::Sender<MediaControlEvent>,
    ) {
        if let Err(e) = mpris_controls.controls.attach(move |event| {
            let _ = mpris_tx.send(event);
        }) {
            log::error!("Failed to attach media controls: {:#?}", e);
        }
    }

    pub fn update_mpris_position(&mut self, secs: f64) -> Option<()> {
        let progress = MediaPosition(Duration::try_from_secs_f64(secs).unwrap_or(Duration::ZERO));

        let mpris_controls = self.controls.as_mut()?;
        let controls = &mut mpris_controls.controls;

        let playback = match (self.paused, self.stopped) {
            (_, true) => souvlaki::MediaPlayback::Stopped,
            (true, _) => souvlaki::MediaPlayback::Paused { progress: Some(progress) },
            (false, _) => souvlaki::MediaPlayback::Playing { progress: Some(progress) },
        };

        if let Err(e) = controls.set_playback(playback) {
            log::error!("Failed to set playback: {:#?}", e);
        }

        Some(())
    }

    pub async fn handle_mpris_events(&mut self) {
        while let Ok(event) = self.mpris_rx.try_recv() {
            match event {
                MediaControlEvent::Toggle => {
                    if self.paused {
                        self.play().await;
                    } else {
                        self.pause().await;
                    }
                }

                MediaControlEvent::Play => {
                    self.play().await;
                }

                MediaControlEvent::Pause => {
                    self.pause().await;
                }

                MediaControlEvent::Stop => {
                    self.stop().await;
                }

                MediaControlEvent::Next => {
                    self.next().await;
                }

                MediaControlEvent::Previous => {
                    self.previous().await;
                }

                MediaControlEvent::SeekBy(direction, duration) => {
                    if self.stopped {
                        return;
                    }
                    let rel = duration.as_secs_f64()
                        * if matches!(direction, SeekDirection::Forward) { 1.0 } else { -1.0 };
                    self.update_mpris_position(self.state.current_playback_state.position + rel);
                    self.mpv_handle.seek(rel, SeekFlag::Relative).await;
                }

                MediaControlEvent::SetPosition(position) => {
                    if self.stopped {
                        return;
                    }
                    let secs = position.0.as_secs_f64();
                    self.update_mpris_position(secs);
                    self.mpv_handle.seek(secs, SeekFlag::Absolute).await;
                }

                MediaControlEvent::SetVolume(_volume) => {
                    #[cfg(target_os = "linux")]
                    {
                        let volume = _volume.clamp(0.0, 1.5);
                        self.mpv_handle.set_volume((volume * 100.0) as i64).await;
                        self.state.current_playback_state.volume = (volume * 100.0) as i64;
                        if let Some(ref mut mpris_controls) = self.controls {
                            let _ = mpris_controls.controls.set_volume(volume);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
