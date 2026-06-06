use crate::mpv::SeekFlag;
use crate::tui::App;
use media_controls::{MediaControlEvent, NowPlaying, PlaybackStatus, SeekDirection};
use std::time::Duration;

impl App {
    pub fn update_mpris_position(&mut self, secs: f64) -> Option<()> {
        let controls = self.controls.as_ref()?;
        let status = match (self.paused, self.stopped) {
            (_, true) => PlaybackStatus::Stopped,
            (true, _) => PlaybackStatus::Paused,
            (false, _) => PlaybackStatus::Playing,
        };
        controls.update(NowPlaying {
            position: Some(Duration::try_from_secs_f64(secs).unwrap_or(Duration::ZERO)),
            status: Some(status),
            ..Default::default()
        });
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
                MediaControlEvent::Seek(direction, duration) => {
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
                    let secs = position.as_secs_f64();
                    self.update_mpris_position(secs);
                    self.mpv_handle.seek(secs, SeekFlag::Absolute).await;
                }
                MediaControlEvent::SetVolume(volume) => {
                    let volume = volume.clamp(0.0, 1.5);
                    self.mpv_handle.set_volume((volume * 100.0) as i64).await;
                    self.state.current_playback_state.volume = (volume * 100.0) as i64;
                    if let Some(ref controls) = self.controls {
                        controls.update(NowPlaying {
                            volume: Some(volume),
                            ..Default::default()
                        });
                    }
                }
                MediaControlEvent::Raise | MediaControlEvent::Quit => {}
            }
        }
    }
}
