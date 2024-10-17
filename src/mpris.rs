use crate::tui::{App, MpvState};
use souvlaki::{MediaControlEvent, MediaControls, PlatformConfig};
use std::sync::{Arc, Mutex};

// linux only, macos requires a window and windows is unsupported
pub fn mpris() -> Result<MediaControls, Box<dyn std::error::Error>> {
    #[cfg(not(target_os = "linux"))]
    {
        return Err("mpris is only supported on linux".into());
    }

    #[cfg(target_os = "linux")]
    {
        let hwnd = None;

        let config = PlatformConfig {
            dbus_name: "jellyfin-tui",
            display_name: "jellyfin-tui",
            hwnd,
        };

        return Ok(MediaControls::new(config).unwrap());
    }
}

impl App {

    /// Registers the media controls to the MpvState. Called after each mpv thread re-init.
    pub fn register_controls(&mut self, mpv_state: Arc<Mutex<MpvState>>) {
        if let Some(ref mut controls) = self.controls {
            controls.attach(move |event: MediaControlEvent| {
                let lock = mpv_state.clone();
                let mut mpv = match lock.lock() {
                    Ok(mpv) => mpv,
                    Err(_) => {
                        return;
                    }
                };

                mpv.mpris_events.push(event);

                drop(mpv);
            })
            .ok();
        }
    }

    pub async fn handle_mpris_events(&mut self) {
        let lock = self.mpv_state.clone();
        let mut mpv = lock.lock().unwrap();
        for event in mpv.mpris_events.iter() {
            match event {
                MediaControlEvent::Toggle => {
                    self.paused = mpv.mpv.get_property("pause").unwrap_or(false);
                    if self.paused {
                        let _ = mpv.mpv.set_property("pause", false);
                    } else {
                        let _ = mpv.mpv.set_property("pause", true);
                    }
                    self.paused = !self.paused;
                }
                MediaControlEvent::Next => {
                    let client = self.client.as_ref().unwrap();
                    let _ = client.stopped(
                        &self.active_song_id,
                        // position ticks
                        (self.current_playback_state.duration * self.current_playback_state.percentage * 100000.0) as u64,
                    );
                    let _ = mpv.mpv.command("playlist_next", &["force"]);
                }
                MediaControlEvent::Previous => {
                    let current_time = self.current_playback_state.duration * self.current_playback_state.percentage / 100.0;
                    if current_time > 5.0 {
                        let _ = mpv.mpv.command("seek", &["0.0", "absolute"]);
                    } else {
                        let _ = mpv.mpv.command("playlist_prev", &["force"]);
                    }
                }
                MediaControlEvent::Stop => {
                    let _ = mpv.mpv.command("stop", &["keep-playlist"]);
                }
                MediaControlEvent::Play => {
                    let _ = mpv.mpv.set_property("pause", false);
                }
                MediaControlEvent::Pause => {
                    let _ = mpv.mpv.set_property("pause", true);
                _ => {}
            }
        }
        mpv.mpris_events.clear();
    }
}
