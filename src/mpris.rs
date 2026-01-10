use crate::mpv::SeekFlag;
use crate::tui::App;
#[cfg(target_os = "linux")]
use souvlaki::PlatformConfig;
use souvlaki::{MediaControlEvent, MediaControls, MediaPosition, SeekDirection};
use std::time::Duration;

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

        Ok(MediaControls::new(config).unwrap())
    }
}

impl App {
    pub fn register_controls(
        controls: &mut MediaControls,
        mpris_tx: std::sync::mpsc::Sender<MediaControlEvent>,
    ) {
        if let Err(e) = controls.attach(move |event| {
            let _ = mpris_tx.send(event);
        }) {
            log::error!("Failed to attach media controls: {:#?}", e);
        }
    }

    pub fn update_mpris_position(&mut self, secs: f64) -> Option<()> {
        let progress = MediaPosition(Duration::try_from_secs_f64(secs).unwrap_or(Duration::ZERO));

        let controls = self.controls.as_mut()?;

        let playback = match (self.paused, self.stopped) {
            (_, true) => souvlaki::MediaPlayback::Stopped,
            (true, _) => souvlaki::MediaPlayback::Paused {
                progress: Some(progress),
            },
            (false, _) => souvlaki::MediaPlayback::Playing {
                progress: Some(progress),
            },
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
                        * if matches!(direction, SeekDirection::Forward) {
                            1.0
                        } else {
                            -1.0
                        };
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
                        if let Some(ref mut controls) = self.controls {
                            let _ = controls.set_volume(volume);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
