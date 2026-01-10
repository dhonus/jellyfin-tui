use crate::database::database::{Command, JellyfinCommand};
use crate::tui::App;

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
        self.mpv_handle
            .previous(self.state.current_playback_state.position)
            .await;
        self.update_mpris_position(0.0);
    }
}
