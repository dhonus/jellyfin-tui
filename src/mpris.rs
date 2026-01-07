use crate::tui::App;
#[cfg(target_os = "linux")]
use souvlaki::PlatformConfig;
use souvlaki::{MediaControlEvent, MediaControls, MediaPosition, SeekDirection};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use crate::database::database::{Command, JellyfinCommand};

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
    /// Registers the media controls to the MpvState. Called after each mpv thread re-init.
    // pub fn register_controls(controls: &mut MediaControls, mpv_state: Arc<Mutex<MpvState>>) {
    //     if let Err(e) = controls
    //         .attach(move |event: MediaControlEvent| {
    //             let lock = mpv_state.clone();
    //             let mut mpv = match lock.lock() {
    //                 Ok(mpv) => mpv,
    //                 Err(_) => {
    //                     return;
    //                 }
    //             };
    // 
    //             mpv.mpris_events.push(event);
    // 
    //             drop(mpv);
    //         }) {
    //         log::error!("Failed to attach media controls: {:#?}", e);
    //     }
    // }
    // 
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
        let progress = MediaPosition(
            Duration::try_from_secs_f64(secs).unwrap_or(Duration::ZERO)
        );

        let controls = self.controls.as_mut()?;

        controls
            .set_playback(if self.paused {
                souvlaki::MediaPlayback::Paused { progress: Some(progress) }
            } else {
                souvlaki::MediaPlayback::Playing { progress: Some(progress) }
            })
            .ok()?;

        Some(())
    }

    //
    // pub async fn handle_mpris_events(&mut self) {
    //     while let Ok(event) = self.mpris_rx.try_recv() {
    //         match event {
    //             MediaControlEvent::Toggle => {
    //                 if self.paused {
    //                     self.play().await;
    //                 } else {
    //                     self.pause().await;
    //                 }
    //             }
    //
    //             MediaControlEvent::Play => {
    //                 self.play().await;
    //             }
    //
    //             MediaControlEvent::Pause => {
    //                 self.pause().await;
    //             }
    //
    //             MediaControlEvent::Stop => {
    //                 self.mpv_handle.stop(true).await;
    //             }
    //
    //             MediaControlEvent::Next => {
    //                 self.next().await;
    //             }
    //
    //             MediaControlEvent::Previous => {
    //                 self.previous().await;
    //             }
    //
    //             MediaControlEvent::SeekBy(direction, duration) => {
    //                 let rel = duration.as_secs_f64()
    //                     * if matches!(direction, SeekDirection::Forward) { 1.0 } else { -1.0 };
    //
    //                 self.seek_relative(rel).await;
    //             }
    //
    //             MediaControlEvent::SetPosition(position) => {
    //                 let secs = position.0.as_secs_f64();
    //                 self.seek_absolute(secs).await;
    //             }
    //
    //             MediaControlEvent::SetVolume(volume) => {
    //                 self.set_volume(volume).await;
    //             }
    //
    //             _ => {}
    //         }
    //     }
    // }
    //
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
                    if self.client.is_some() {
                        let _ = self
                            .db
                            .cmd_tx
                            .send(Command::Jellyfin(JellyfinCommand::Stopped {
                                id: Some(self.active_song_id.clone()),
                                position_ticks: Some(self.state.current_playback_state.position as u64
                                    * 10_000_000
                                ),
                            }))
                            .await;
                    }
                    let _ = mpv.mpv.command("playlist_next", &["force"]);
                    if self.paused {
                        let _ = mpv.mpv.set_property("pause", false);
                        self.paused = false;
                    }
                    self.update_mpris_position(0.0);
                }
                MediaControlEvent::Previous => {
                    if self.state.current_playback_state.position > 5.0 {
                        let _ = mpv.mpv.command("seek", &["0.0", "absolute"]);
                    } else {
                        let _ = mpv.mpv.command("playlist_prev", &["force"]);
                    }
                    self.update_mpris_position(0.0);
                }
                MediaControlEvent::Stop => {
                    self.mpv_handle.stop(true).await;
                }
                MediaControlEvent::Play => {
                    let _ = mpv.mpv.set_property("pause", false);
                    self.paused = false;
                    let _ = self.report_progress_if_needed(true).await;
                }
                MediaControlEvent::Pause => {
                    let _ = mpv.mpv.set_property("pause", true);
                    self.paused = true;
                    let _ = self.report_progress_if_needed(true).await;
                }
                MediaControlEvent::SeekBy(direction, duration) => {
                    let rel = duration.as_secs_f64()
                        * (if matches!(direction, SeekDirection::Forward) {
                            1.0
                        } else {
                            -1.0
                        });

                    self.update_mpris_position(self.state.current_playback_state.position + rel);
                    let _ = mpv.mpv.command("seek", &[&rel.to_string()]);
                }
                MediaControlEvent::SetPosition(position) => {
                    let secs = position.0.as_secs_f64();
                    self.update_mpris_position(secs);

                    let _ = mpv.mpv.command("seek", &[&secs.to_string(), "absolute"]);
                }
                MediaControlEvent::SetVolume(_volume) => {
                    #[cfg(target_os = "linux")]
                    {
                        let volume = _volume.clamp(0.0, 1.5);
                        let _ = mpv.mpv.set_property("volume", (volume * 100.0) as i64);
                        self.state.current_playback_state.volume = (volume * 100.0) as i64;
                        if let Some(ref mut controls) = self.controls {
                            let _ = controls.set_volume(volume);
                        }
                    }
                }
                _ => {}
            }
        }
        mpv.mpris_events.clear();
    }
}
