use crate::database::database::{Command, JellyfinCommand};
use crate::tui::App;

impl App {
    pub async fn play(&mut self) {
        if !self.paused {
            return;
        }
        self.mpv_handle.play().await;
        self.paused = false;

        let _ = self.handle_discord(true).await;
        let _ = self.report_progress_if_needed(true).await;

        self.update_mpris_position(self.state.current_playback_state.position);
    }

    pub async fn pause(&mut self) {
        if self.paused {
            return;
        }
        self.mpv_handle.pause().await;
        self.paused = true;

        let _ = self.handle_discord(true).await;
        let _ = self.report_progress_if_needed(true).await;

        self.update_mpris_position(self.state.current_playback_state.position);
    }

    pub async fn stop(&mut self, keep_playlist: bool) {
        self.mpv_handle.stop(keep_playlist).await;
        self.state.queue.clear();
        self.lyrics = None;
        self.cover_art = None;
        self.paused = true;
        if let Some(controls) = self.controls.as_mut() {
            let _ = controls.set_playback(souvlaki::MediaPlayback::Stopped);
        }
    }

    pub async fn next(&mut self) {
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
        self.mpv_handle.next().await;
        self.update_mpris_position(0.0);
    }

    pub async fn previous(&mut self) {
        self.mpv_handle.previous(self.state.current_playback_state.position).await;
        self.update_mpris_position(0.0);
    }
}