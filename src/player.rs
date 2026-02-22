use crate::database::database::{Command, JellyfinCommand};
use crate::keyboard::ActiveSection;
use crate::popup::PopupMenu;
use crate::tui::{App, Repeat};

impl App {
    pub async fn play(&mut self) {
        if !self.paused || self.stopped {
            return;
        }
        self.mpv_handle.play().await;
        self.paused = false;

        let _ = self.handle_discord(true).await;
        let _ = self.report_progress_if_needed(true).await;

        self.update_mpris_position(self.state.current_playback_state.position);
    }

    pub async fn pause(&mut self) {
        if self.paused || self.stopped {
            return;
        }
        self.mpv_handle.pause().await;
        self.paused = true;

        let _ = self.handle_discord(true).await;
        let _ = self.report_progress_if_needed(true).await;

        self.update_mpris_position(self.state.current_playback_state.position);
    }

    pub async fn stop(&mut self) {
        self.stopped = true;
        self.paused = true;
        self.mpv_handle.stop().await;
        self.state.queue.clear();
        self.lyrics = None;
        self.cover_art = None;
        self.update_mpris_position(self.state.current_playback_state.position);
        if self.client.is_some() {
            let _ = self
                .db
                .cmd_tx
                .send(Command::Jellyfin(JellyfinCommand::Stopped {
                    id: Some(self.active_song_id.clone()),
                    position_ticks: Some(
                        self.state.current_playback_state.position as u64 * 10_000_000,
                    ),
                }))
                .await;
        }
    }

    pub async fn reset(&mut self) {
        self.stop().await;
        self.state = crate::helpers::State::new();
        self.state.selected_artist.select_first();
        self.state.selected_track.select_first();
        self.state.selected_playlist.select_first();
        self.state.selected_playlist_track.select_first();
        self.state.selected_album.select_first();
        self.state.selected_album_track.select_first();

        self.state.artists_scroll_state =
            self.state.artists_scroll_state.content_length(self.artists.len());
        self.state.albums_scroll_state =
            self.state.albums_scroll_state.content_length(self.albums.len());
        self.state.playlists_scroll_state =
            self.state.playlists_scroll_state.content_length(self.playlists.len());

        self.tracks.clear();
        self.album_tracks.clear();
        self.playlist_tracks.clear();
    }

    pub async fn toggle_transcoding(&mut self) {
        if self.client.is_none() {
            return;
        }
        self.transcoding.enabled = !self.transcoding.enabled;
        self.preferences.transcoding = self.transcoding.enabled;
        let _ = self.preferences.save();
    }

    pub async fn next(&mut self) {
        if self.stopped {
            return;
        }
        self.song_changed = true;
        self.mpv_handle.next().await;
        self.play().await;
        self.update_mpris_position(0.0);
        if self.client.is_some() {
            let _ = self
                .db
                .cmd_tx
                .send(Command::Jellyfin(JellyfinCommand::Stopped {
                    id: Some(self.active_song_id.clone()),
                    position_ticks: Some(
                        self.state.current_playback_state.position as u64 * 10_000_000,
                    ),
                }))
                .await;
        }
    }

    pub async fn previous(&mut self) {
        if self.stopped {
            return;
        }
        self.song_changed = true;
        self.mpv_handle.previous(self.state.current_playback_state.position).await;
        self.update_mpris_position(0.0);
    }

    pub async fn cycle_repeat_mode(&mut self) {
        match self.preferences.repeat {
            Repeat::None => {
                self.preferences.repeat = Repeat::All;
            }
            Repeat::All => {
                self.preferences.repeat = Repeat::One;
            }
            Repeat::One => {
                self.preferences.repeat = Repeat::None;
            }
        }
        self.mpv_handle.set_repeat(self.preferences.repeat).await;
        let _ = self.preferences.save();
    }

    pub async fn global_shuffle(&mut self) {
        self.state.last_section = self.state.active_section;
        self.state.active_section = ActiveSection::Popup;
        self.popup.current_menu = self.preferences.preferred_global_shuffle.clone();
        if self.popup.current_menu.is_none() {
            self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                tracks_n: 100,
                only_played: true,
                only_unplayed: false,
                only_favorite: false,
            });
        }
        self.popup.global = true;
        self.popup.selected.select_last();
    }

    pub async fn toggle_shuffle(&mut self) {
        match self.state.shuffle {
            true => {
                self.do_unshuffle().await;
                self.state.shuffle = false;
            }
            false => {
                self.do_shuffle(false).await;
                self.state.shuffle = true;
            }
        }
    }

    pub async fn volume_up(&mut self) {
        self.state.current_playback_state.volume =
            (self.state.current_playback_state.volume + 5).min(500);
        self.mpv_handle.set_volume(self.state.current_playback_state.volume).await;
        #[cfg(target_os = "linux")]
        {
            if let Some(ref mut controls) = self.controls {
                let _ =
                    controls.set_volume(self.state.current_playback_state.volume as f64 / 100.0);
            }
        }
    }

    pub async fn volume_down(&mut self) {
        self.state.current_playback_state.volume =
            (self.state.current_playback_state.volume - 5).max(0);

        self.mpv_handle.set_volume(self.state.current_playback_state.volume).await;

        #[cfg(target_os = "linux")]
        {
            if let Some(ref mut controls) = self.controls {
                let _ =
                    controls.set_volume(self.state.current_playback_state.volume as f64 / 100.0);
            }
        }
    }
}
