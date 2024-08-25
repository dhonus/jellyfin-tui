use crate::tui::{App, MpvState};
use souvlaki::{MediaControlEvent, MediaControls, PlatformConfig};
use std::sync::{Arc, Mutex};

pub fn mpris() -> MediaControls {
    #[cfg(not(target_os = "windows"))]
    let hwnd = None;

    #[cfg(target_os = "windows")]
    let hwnd = {
        use raw_window_handle::windows::WindowsHandle;

        let handle: WindowsHandle = unimplemented!();
        Some(handle.hwnd)
    };

    let config = PlatformConfig {
        dbus_name: "jellyfin-tui",
        display_name: "jellyfin-tui",
        hwnd,
    };

    return MediaControls::new(config).unwrap();
}

impl App {

    /// Registers the media controls to the MpvState. Called after each mpv thread re-init.
    pub fn register_controls(&mut self, mpv_state: Arc<Mutex<MpvState>>) {
        self.controls
            .attach(move |event: MediaControlEvent| {
                let lock = mpv_state.clone();
                let mut mpv = match lock.lock() {
                    Ok(mpv) => mpv,
                    Err(_) => {
                        return;
                    }
                };

                mpv.mpris_events.push(event.clone());

                drop(mpv);
            })
            .unwrap();
    }
    pub fn handle_mpris_events(&mut self) {
        let lock = self.mpv_state.clone();
        let mut mpv = lock.lock().unwrap();
        for event in mpv.mpris_events.iter() {
            match event {
                MediaControlEvent::Toggle => {
                    self.paused = !self.paused;
                    if self.paused {
                        let _ = mpv.mpv.pause();
                    } else {
                        let _ = mpv.mpv.unpause();
                    }
                }
                MediaControlEvent::Next => {
                    let _ = mpv.mpv.playlist_next_force();
                }
                MediaControlEvent::Previous => {
                    let _ = mpv.mpv.playlist_previous_force();
                }
                MediaControlEvent::Stop => {
                    // let _ = mpv.mpv.stop();
                }
                MediaControlEvent::Play => {
                    let _ = mpv.mpv.unpause();
                }
                MediaControlEvent::Pause => {
                    let _ = mpv.mpv.pause();
                }
                _ => {}
            }
        }
        mpv.mpris_events.clear();
    }
}
