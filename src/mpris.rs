use crate::mpv::SeekFlag;
use crate::tui::{App, Repeat};
use media_controls::{
    Capabilities, Config, LoopStatus, MediaControlEvent, MediaControls, NowPlaying, PlaybackStatus,
    SeekDirection,
};
use std::time::Duration;
use tokio::sync::mpsc::Receiver;

pub async fn init_media_controls() -> (Option<MediaControls>, Receiver<MediaControlEvent>) {
    let mut mc = MediaControls::new(Config {
        dbus_name: "jellyfin-tui",
        display_name: "jellyfin-tui",
        capabilities: Capabilities {
            can_raise: false,
            ..Default::default()
        },
    })
    .await;
    let rx = if let Some(ref mut c) = mc {
        log::info!("Media controls initialized successfully");
        c.events()
    } else {
        log::warn!("Failed to initialize media controls; running without OS integration");
        let (_, r) = tokio::sync::mpsc::channel(1);
        r
    };
    (mc, rx)
}

impl App {
    pub fn update_mpris_metadata(&mut self) {
        let controls = match self.controls.as_ref() {
            Some(c) => c,
            None => return,
        };
        let playback = &self.state.current_playback_state;
        let status = if self.stopped {
            PlaybackStatus::Stopped
        } else if self.paused {
            PlaybackStatus::Paused
        } else {
            PlaybackStatus::Playing
        };
        if let Some(song) = self.state.queue.get(playback.current_index) {
            controls.update(NowPlaying {
                title: Some(song.name.clone()),
                artist: Some(song.artist.clone()),
                album: Some(song.album.clone()),
                cover_url: Some(format!("file://{}", self.cover_art_path)),
                duration: Duration::try_from_secs_f64(playback.duration).ok(),
                position: Duration::try_from_secs_f64(playback.position).ok(),
                status: Some(status),
                volume: None,
                track_number: (song.index_number > 0).then_some(song.index_number as u32),
                year: (song.production_year > 0).then_some(song.production_year as u32),
                shuffle: Some(self.state.shuffle),
                loop_status: Some(match self.preferences.repeat {
                    Repeat::None => LoopStatus::None,
                    Repeat::One => LoopStatus::Track,
                    Repeat::All | Repeat::Radio => LoopStatus::Playlist,
                }),
            });
        } else {
            controls.update(NowPlaying {
                status: Some(PlaybackStatus::Stopped),
                ..Default::default()
            });
        }
    }

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
                MediaControlEvent::SetShuffle(on) => {
                    if on != self.state.shuffle {
                        self.toggle_shuffle().await;
                    }
                }
                MediaControlEvent::SetLoopStatus(status) => {
                    self.preferences.repeat = match status {
                        LoopStatus::None => Repeat::None,
                        LoopStatus::Track => Repeat::One,
                        LoopStatus::Playlist => Repeat::All,
                    };
                    self.mpv_handle.set_repeat(self.preferences.repeat).await;
                    let _ = self.preferences.save();
                    self.dirty = true;
                }
                MediaControlEvent::Quit => {
                    self.exit().await;
                }
                _ => {}
            }
        }
    }
}
