/* --------------------------
Keyboard related functions
    - The entry point for handling keyboard events is the `handle_events` function
    - Handles all key events - movement within the program, seeking, volume control, etc.
    - Also used for searching
-------------------------- */

use crate::{client::{Artist, Playlist}, helpers, tui::{App, Repeat}};

use std::io;
use std::time::Duration;
use crossterm::event::{self, Event, KeyEvent, KeyModifiers, KeyCode};
use ratatui::widgets::ScrollbarState;
use serde::{Deserialize, Serialize};

pub trait Searchable {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
}

pub enum Selectable {
    Artist,
    Track,
    Playlist,
    PlaylistTrack,
}

/// Search results as a vector of IDs. Used in all searchable areas
///
pub fn search_results<T: Searchable>(items: &Vec<T>, search_term: &str) -> Vec<String> {
    let items = items
        .iter()
        .filter(|item| item.id() != "_album_")
        .filter(|item| {
            !helpers::find_all_subsequences(
                &search_term.to_lowercase(), &item.name().to_lowercase()
            ).is_empty()
        })
        .map(|item| String::from(item.id()))
        .collect::<Vec<String>>();
    items
}

impl App {
    /// Poll for events and handle them
    pub async fn handle_events(&mut self) -> io::Result<()> {
        while event::poll(Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(key_event) => {
                    self.handle_key_event(key_event).await;
                }
                Event::Mouse(mouse_event) => {
                    self.handle_mouse_event(mouse_event);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Switch to the next section
    fn toggle_search_section(&mut self, forwards: bool) {
        match forwards {
            true => match self.search_section {
                SearchSection::Artists => self.search_section = SearchSection::Albums,
                SearchSection::Albums => self.search_section = SearchSection::Tracks,
                SearchSection::Tracks => self.search_section = SearchSection::Artists,
            },
            false => match self.search_section {
                SearchSection::Artists => self.search_section = SearchSection::Tracks,
                SearchSection::Albums => self.search_section = SearchSection::Artists,
                SearchSection::Tracks => self.search_section = SearchSection::Albums,
            },
        }
    }

    /// Move the cursor right in the search
    fn vim_search_left(&mut self) {
        match self.search_section {
            SearchSection::Tracks => self.search_section = SearchSection::Albums,
            SearchSection::Albums => self.search_section = SearchSection::Artists,
            _ => {}
        }
    }

    /// Move the cursor left in the search
    fn vim_search_right(&mut self) {
        match self.search_section {
            SearchSection::Artists => self.search_section = SearchSection::Albums,
            SearchSection::Albums => self.search_section = SearchSection::Tracks,
            _ => {}
        }
    }


    // use the ID of the artist that is selected and set the cursor to the appropriate index
    pub fn reposition_artist_cursor(&mut self, artist_id: &str) {
        if artist_id.is_empty() {
            if !self.artists.is_empty() {
                self.artist_select_by_index(0);
            }
            return;
        }
        if !self.artists_search_term.is_empty() {
            let items = search_results(&self.artists, &self.artists_search_term);
            if let Some(index) = items.iter().position(|id| id == artist_id) {
                self.artist_select_by_index(index);
            }
            return;
        }
        if let Some(index) = self.artists.iter().position(|a| a.id == artist_id) {
            self.artist_select_by_index(index);
        }
    }

    fn reposition_playlist_cursor(&mut self, playlist_id: &str) {
        if playlist_id.is_empty() {
            if !self.playlists.is_empty() {
                self.playlist_select_by_index(0);
            }
            return;
        }
        if !self.playlists_search_term.is_empty() {
            let items = search_results(&self.playlists, &self.playlists_search_term);
            if let Some(index) = items.iter().position(|id| id == playlist_id) {
                self.playlist_select_by_index(index);
            }
            return;
        }
        if let Some(index) = self.playlists.iter().position(|p| p.id == playlist_id) {
            self.playlist_select_by_index(index);
        }
    }

    fn playlist_select_by_index(&mut self, index: usize) {
        if index >= self.playlists.len() {
            return;
        }
        self.selected_playlist.select(Some(index));
        // if searching
        if !self.playlists_search_term.is_empty() {
            self.playlists_scroll_state = self.playlists_scroll_state.content_length(
                search_results(&self.playlists, &self.playlists_search_term).len()
            ).position(index);
            return;
        }
        self.playlists_scroll_state = self.playlists_scroll_state.content_length(self.playlists.len()).position(index);
    }

    pub fn get_id_of_selected<T: Searchable>(&self, items: &Vec<T>, selectable: Selectable) -> String {
        let search_term = match selectable {
            Selectable::Artist => &self.artists_search_term,
            Selectable::Track => &self.tracks_search_term,
            Selectable::Playlist => &self.playlists_search_term,
            Selectable::PlaylistTrack => &self.playlist_tracks_search_term,
        };
        let selected = match selectable {
            Selectable::Artist => self.selected_artist.selected(),
            Selectable::Track => self.selected_track.selected(),
            Selectable::Playlist => self.selected_playlist.selected(),
            Selectable::PlaylistTrack => self.selected_playlist_track.selected(),
        };
        if !search_term.is_empty() {
            let items = search_results(items, search_term);
            if items.is_empty() {
                return String::from("");
            }
            let selected = selected.unwrap_or(0);
            return items[selected].clone();
        }
        if items.is_empty() {
            return String::from("");
        }
        let selected = selected.unwrap_or(0);
        String::from(items[selected].id())
    }

    fn reposition_track_cursor(&mut self, track_id: &str) {
        if track_id.is_empty() {
            if !self.tracks.is_empty() {
                self.selected_track.select(Some(0));
            }
            return;
        }
        if !self.tracks_search_term.is_empty() {
            let items = search_results(&self.tracks, &self.tracks_search_term);
            if let Some(index) = items.iter().position(|id| id == track_id) {
                self.track_select_by_index(index);
            }
            return;
        }
        if let Some(index) = self.tracks.iter().position(|t| t.id == track_id) {
            self.track_select_by_index(index);
        }
    }

    fn reposition_playlist_track_cursor(&mut self, track_id: &str) {
        if track_id.is_empty() {
            if !self.tracks_playlist.is_empty() {
                self.selected_playlist_track.select(Some(0));
            }
            return;
        }
        if !self.playlist_tracks_search_term.is_empty() {
            let items = search_results(&self.tracks_playlist, &self.playlist_tracks_search_term);
            if let Some(index) = items.iter().position(|id| id == track_id) {
                self.playlist_track_select_by_index(index);
            }
            return;
        }
        if let Some(index) = self.tracks_playlist.iter().position(|t| t.id == track_id) {
            self.playlist_track_select_by_index(index);
        }
    }

    pub fn track_select_by_index(&mut self, index: usize) {
        if index >= self.tracks.len() {
            return;
        }
        self.selected_track.select(Some(index));
        // if searching
        if !self.tracks_search_term.is_empty() {
            self.tracks_scroll_state = ScrollbarState::new(search_results(&self.tracks, &self.tracks_search_term).len());
            self.tracks_scroll_state = self.tracks_scroll_state.position(index);
            return;
        }
        self.tracks_scroll_state = ScrollbarState::new(self.tracks.len());
        self.tracks_scroll_state = self.tracks_scroll_state.position(index);
    }

    fn playlist_track_select_by_index(&mut self, index: usize) {
        if index >= self.tracks_playlist.len() {
            return;
        }
        self.selected_playlist_track.select(Some(index));
        // if searching
        if !self.playlist_tracks_search_term.is_empty() {
            self.playlist_tracks_scroll_state = self.playlist_tracks_scroll_state.content_length(
                search_results(&self.tracks_playlist, &self.playlist_tracks_search_term).len()
            ).position(index);
            return;
        }
        self.playlist_tracks_scroll_state = self.playlist_tracks_scroll_state.content_length(self.tracks_playlist.len()).position(index);
    }

    pub fn artist_select_by_index(&mut self, index: usize) {
        if index >= self.artists.len() {
            return;
        }
        self.selected_artist.select(Some(index));
        // if searching
        if !self.artists_search_term.is_empty() {
            self.artists_scroll_state = self.artists_scroll_state.content_length(
                search_results(&self.artists, &self.artists_search_term).len()
            ).position(index);
            return;
        }
        self.artists_scroll_state = self.artists_scroll_state.content_length(self.artists.len()).position(index);
    }

    async fn handle_key_event(&mut self, key_event: KeyEvent) {

        self.dirty = true;

        if key_event.code == KeyCode::Char('c') && key_event.modifiers == KeyModifiers::CONTROL {
            self.exit();
            return;
        }

        if self.active_section == ActiveSection::Popup {
            self.popup_handle_keys(key_event).await;
            return;
        }

        if self.locally_searching {
            match key_event.code {
                KeyCode::Esc | KeyCode::F(1) => {
                    self.locally_searching = false;
                    let artist_id = self.get_id_of_selected(&self.artists, Selectable::Artist);
                    let track_id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                    let playlist_id = self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                    let playlist_track_id = self.get_id_of_selected(&self.tracks_playlist, Selectable::PlaylistTrack);

                    match self.active_tab {
                        ActiveTab::Library => {
                            match self.active_section {
                                ActiveSection::Artists => {
                                    self.artists_search_term = String::from("");
                                    self.reposition_artist_cursor(&artist_id);
                                }
                                ActiveSection::Tracks => {
                                    self.tracks_search_term = String::from("");
                                    self.reposition_track_cursor(&track_id);
                                }
                                _ => {}
                            }
                        }
                        ActiveTab::Playlists => {
                            match self.active_section {
                                ActiveSection::Artists => {
                                    self.playlists_search_term = String::from("");
                                    self.reposition_playlist_cursor(&playlist_id);
                                }
                                ActiveSection::Tracks => {
                                    self.playlist_tracks_search_term = String::from("");
                                    self.reposition_playlist_track_cursor(&playlist_track_id);
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }

                    return;
                }
                KeyCode::Enter => {
                    match self.active_tab {
                        ActiveTab::Library => {
                            self.locally_searching = false;
                            if self.active_section == ActiveSection::Artists {
                                self.tracks_search_term = String::from("");
                            }
                        }
                        ActiveTab::Playlists => {
                            self.locally_searching = false;
                            if self.active_section == ActiveSection::Artists {
                                self.playlist_tracks_search_term = String::from("");
                            }   
                        }
                        _ => {}
                    }
                    return;
                }
                KeyCode::Backspace => {
                    match self.active_tab {
                        ActiveTab::Library => {
                            match self.active_section {
                                ActiveSection::Artists => {
                                    let selected_id = self.get_id_of_selected(&self.artists, Selectable::Artist);
                                    self.artists_search_term.pop();
                                    self.reposition_artist_cursor(&selected_id);
                                }
                                ActiveSection::Tracks => {
                                    let selected_id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                                    self.tracks_search_term.pop();
                                    self.reposition_track_cursor(&selected_id);
                                }
                                _ => {}
                            }
                        }
                        ActiveTab::Playlists => {
                            match self.active_section {
                                ActiveSection::Artists => {
                                    let selected_id = self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                                    self.playlists_search_term.pop();
                                    self.reposition_playlist_cursor(&selected_id);
                                }
                                ActiveSection::Tracks => {
                                    let selected_id = self.get_id_of_selected(&self.tracks_playlist, Selectable::PlaylistTrack);
                                    self.playlist_tracks_search_term.pop();
                                    self.reposition_playlist_track_cursor(&selected_id);
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                KeyCode::Delete => {
                    match self.active_tab {
                        ActiveTab::Library => {
                            match self.active_section {
                                ActiveSection::Artists => {
                                    let selected_id = self.get_id_of_selected(&self.artists, Selectable::Artist);
                                    self.artists_search_term.clear();
                                    self.reposition_artist_cursor(&selected_id);
                                }
                                ActiveSection::Tracks => {
                                    let selected_id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                                    self.tracks_search_term.clear();
                                    self.reposition_track_cursor(&selected_id);
                                }
                                _ => {}
                            }
                        }
                        ActiveTab::Playlists => {
                            match self.active_section {
                                ActiveSection::Artists => {
                                    let selected_id = self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                                    self.playlists_search_term.clear();
                                    self.reposition_playlist_cursor(&selected_id);
                                }
                                ActiveSection::Tracks => {
                                    let selected_id = self.get_id_of_selected(&self.tracks_playlist, Selectable::PlaylistTrack);
                                    self.playlist_tracks_search_term.clear();
                                    self.reposition_playlist_track_cursor(&selected_id);
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                KeyCode::Char(c) => {
                    match self.active_tab {
                        ActiveTab::Library => {
                            match self.active_section {
                                ActiveSection::Artists => {
                                    self.artists_search_term.push(c);
                                    self.artist_select_by_index(0);
                                }
                                ActiveSection::Tracks => {
                                    self.tracks_search_term.push(c);
                                    self.track_select_by_index(0);
                                }
                                _ => {}
                            }    
                        }
                        ActiveTab::Playlists => {
                            match self.active_section {
                                ActiveSection::Artists => {
                                    self.playlists_search_term.push(c);
                                    self.playlist_select_by_index(0);
                                }
                                ActiveSection::Tracks => {
                                    self.playlist_tracks_search_term.push(c);
                                    self.playlist_track_select_by_index(0);
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            return;
        }

        if self.active_tab == ActiveTab::Search {
            self.handle_search_tab_events(key_event).await;
            return;
        }

        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            // Seek backward
            KeyCode::Left => {
                let secs = f64::max(0.0, self.current_playback_state.duration * self.current_playback_state.percentage / 100.0 - 5.0);
                self.update_mpris_position(secs);

                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.command("seek", &["-5.0"]);
                }
            }
            // Seek forward
            KeyCode::Right => {
                let secs = self.current_playback_state.duration * self.current_playback_state.percentage / 100.0 + 5.0;
                self.update_mpris_position(secs);

                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.command("seek", &["5.0"]);
                }
            }
            // Previous track
            KeyCode::Char('n') => {
                if let Some(client) = &self.client {
                    let _ = client.stopped(
                        &self.active_song_id,
                        // position ticks
                        (self.current_playback_state.duration * self.current_playback_state.percentage * 100000.0) as u64,
                    ).await;
                    if let Ok(mpv) = self.mpv_state.lock() {
                        let _ = mpv.mpv.command("playlist_next", &["force"]);
                    }
                }
                self.update_mpris_position(0.0);
            }
            // Next track
            KeyCode::Char('N') => {
                if let Ok(mpv) = self.mpv_state.lock() {
                    let current_time = self.current_playback_state.duration * self.current_playback_state.percentage / 100.0;
                    if current_time > 5.0 {
                        let _ = mpv.mpv.command("seek", &["0.0", "absolute"]);
                        return;
                    }
                    let _ = mpv.mpv.command("playlist_prev", &["force"]);
                }
                self.update_mpris_position(0.0);
            }
            // Play/Pause
            KeyCode::Char(' ') => {
                if let Ok(mpv) = self.mpv_state.lock() {
                    if self.paused {
                        let _ = mpv.mpv.set_property("pause", false);
                        self.paused = false;
                    } else {
                        let _ = mpv.mpv.set_property("pause", true);
                        self.paused = true;
                    }
                }
            }
            KeyCode::Char('x') => {
                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.command("stop", &[]);
                    self.queue.clear();
                }
            }
            KeyCode::Char('t') => {
                if let Some(client) = self.client.as_mut() {
                    client.transcoding.enabled = !client.transcoding.enabled;
                }
            }
            // Volume up
            KeyCode::Char('+') => {
                if self.current_playback_state.volume >= 500 {
                    return;
                }
                self.current_playback_state.volume += 5;
                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.set_property("volume", self.current_playback_state.volume);
                }
                if let Some(ref mut controls) = self.controls {
                    let _ = controls.set_volume(self.current_playback_state.volume as f64 / 100.0);
                }
            }
            // Volume down
            KeyCode::Char('-') => {
                if self.current_playback_state.volume <= 0 {
                    return;
                }
                self.current_playback_state.volume -= 5;
                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.set_property("volume", self.current_playback_state.volume);
                }
                if let Some(ref mut controls) = self.controls {
                    let _ = controls.set_volume(self.current_playback_state.volume as f64 / 100.0);
                }
            }
            KeyCode::Tab => {
                self.toggle_section(true);
            }
            KeyCode::BackTab => {
                self.toggle_section(false);
            }
            // Move down
            KeyCode::Down | KeyCode::Char('j') => match self.active_section {
                ActiveSection::Artists => {
                    if self.active_tab == ActiveTab::Library {
                        if !self.artists_search_term.is_empty() {
                            let items = search_results(&self.artists, &self.artists_search_term);
                            let selected = self.selected_artist.selected().unwrap_or(items.len() - 1);
                            if selected == items.len() - 1 {
                                self.artist_select_by_index(selected);
                                return;
                            }
                            self.artist_select_by_index(selected + 1);
                            return;
                        }

                        let selected = self.selected_artist.selected().unwrap_or(self.artists.len() - 1);
                        if selected == self.artists.len() - 1 {
                            self.artist_select_by_index(selected);
                            return;
                        }
                        self.artist_select_by_index(selected + 1);
                    }
                    if self.active_tab == ActiveTab::Playlists {
                        if !self.playlists_search_term.is_empty() {
                            let items = search_results(&self.playlists, &self.playlists_search_term);
                            let selected = self.selected_playlist.selected().unwrap_or(items.len() - 1);
                            if selected == items.len() - 1 {
                                self.playlist_select_by_index(selected);
                                return;
                            }
                            self.playlist_select_by_index(selected + 1);
                            return;
                        }

                        let selected = self.selected_playlist.selected().unwrap_or(self.playlists.len() - 1);
                        if selected == self.playlists.len() - 1 {
                            self.playlist_select_by_index(selected);
                            return;
                        }
                        self.playlist_select_by_index(selected + 1);
                    }
                }
                ActiveSection::Tracks => {
                    if self.active_tab == ActiveTab::Library {
                        if !self.tracks_search_term.is_empty() {
                            let items = search_results(&self.tracks, &self.tracks_search_term);
                            let selected = self
                                .selected_track
                                .selected()
                                .unwrap_or(items.len() - 1);
                            if selected == items.len() - 1 {
                                self.track_select_by_index(selected);
                                return;
                            }
                            self.track_select_by_index(selected + 1);
                            return;
                        }

                        let selected = self
                            .selected_track
                            .selected()
                            .unwrap_or(self.tracks.len() - 1);
                        if selected == self.tracks.len() - 1 {
                            self.track_select_by_index(selected);
                            return;
                        }
                        self.track_select_by_index(selected + 1);
                    }
                    if self.active_tab == ActiveTab::Playlists {
                        if !self.playlist_tracks_search_term.is_empty() {
                            let items = search_results(&self.tracks_playlist, &self.playlist_tracks_search_term);
                            let selected = self
                                .selected_playlist_track
                                .selected()
                                .unwrap_or(items.len() - 1);
                            if selected == items.len() - 1 {
                                self.playlist_track_select_by_index(selected);
                                return;
                            }
                            self.playlist_track_select_by_index(selected + 1);
                            return;
                        }

                        let selected = self
                            .selected_playlist_track
                            .selected()
                            .unwrap_or(self.tracks_playlist.len() - 1);
                        if selected == self.tracks_playlist.len() - 1 {
                            self.playlist_track_select_by_index(selected);
                            return;
                        }
                        self.playlist_track_select_by_index(selected + 1);
                    }
                }
                ActiveSection::Queue => {
                    if key_event.modifiers == KeyModifiers::SHIFT {
                        self.move_queue_item_down().await;
                        return;
                    }
                    self.selected_queue_item_manual_override = true;
                    if self.queue.is_empty() {
                        return;
                    }
                    let selected = self.selected_queue_item.selected().unwrap_or(0);
                    if selected == self.queue.len() - 1 {
                        self.selected_queue_item.select(Some(selected));
                        return;
                    }
                    self.selected_queue_item.select(Some(selected + 1));
                }
                ActiveSection::Lyrics => {
                    self.selected_lyric_manual_override = true;
                    if let Some((_, lyrics_vec, _)) = &self.lyrics {
                        if lyrics_vec.is_empty() {
                            return;
                        }
                        self.selected_lyric.select_next();
                    }
                }
                ActiveSection::Popup => {
                    self.popup.selected.select_next();
                },
            },
            KeyCode::Up | KeyCode::Char('k') => match self.active_section {
                ActiveSection::Artists => {
                    if self.active_tab == ActiveTab::Library {
                        if !self.artists_search_term.is_empty() {
                            let selected = self
                                .selected_artist
                                .selected()
                                .unwrap_or(0);
                            if selected == 0 {
                                self.artist_select_by_index(selected);
                                return;
                            }
                            self.artist_select_by_index(selected - 1);
                            return;
                        }

                        let selected = self.selected_artist.selected().unwrap_or(0);
                        if selected == 0 {
                            self.artist_select_by_index(selected);
                            return;
                        }
                        self.artist_select_by_index(selected - 1);
                    }
                    if self.active_tab == ActiveTab::Playlists {
                        if !self.playlists_search_term.is_empty() {
                            let selected = self
                                .selected_playlist
                                .selected()
                                .unwrap_or(0);
                            if selected == 0 {
                                self.playlist_select_by_index(selected);
                                return;
                            }
                            self.playlist_select_by_index(selected - 1);
                            return;
                        }

                        let selected = self.selected_playlist.selected().unwrap_or(0);
                        if selected == 0 {
                            self.playlist_select_by_index(selected);
                            return;
                        }
                        self.playlist_select_by_index(selected - 1);
                    }
                }
                ActiveSection::Tracks => {

                    match self.active_tab {
                        ActiveTab::Library => {
                            if !self.tracks_search_term.is_empty() {
                                let selected = self
                                    .selected_track
                                    .selected()
                                    .unwrap_or(0);
                                self.track_select_by_index(std::cmp::max(selected as i32 - 1, 0) as usize);
                                return;
                            }
                            
                            let selected = self.selected_track.selected().unwrap_or(0);
                            self.track_select_by_index(std::cmp::max(selected as i32 - 1, 0) as usize);
                        }
                        ActiveTab::Playlists => {
                            if !self.playlist_tracks_search_term.is_empty() {
                                let selected = self
                                    .selected_playlist_track
                                    .selected()
                                    .unwrap_or(0);
                                self.playlist_track_select_by_index(std::cmp::max(selected as i32 - 1, 0) as usize);
                                return;
                            }
                            
                            let selected = self.selected_playlist_track.selected().unwrap_or(0);
                            self.playlist_track_select_by_index(std::cmp::max(selected as i32 - 1, 0) as usize);
                        }
                        _ => {}
                    }
                }
                ActiveSection::Queue => {
                    if key_event.modifiers == KeyModifiers::SHIFT {
                        self.move_queue_item_up().await;
                        return;
                    }
                    self.selected_queue_item_manual_override = true;
                    let selected = self.selected_queue_item.selected().unwrap_or(0);
                    self.selected_queue_item.select(Some(std::cmp::max(selected as i32 - 1, 0) as usize));
                }
                ActiveSection::Lyrics => {
                    self.selected_lyric_manual_override = true;
                    self.selected_lyric.select_previous();
                }
                ActiveSection::Popup => {
                    self.popup.selected.select_previous();
                }
            },
            KeyCode::Char('g') => match self.active_section {
                ActiveSection::Artists => {
                    match self.active_tab {
                        ActiveTab::Library => {
                            self.artist_select_by_index(0);
                        }
                        ActiveTab::Playlists => {
                            self.playlist_select_by_index(0);
                        }
                        _ => {}
                    }
                }
                ActiveSection::Tracks => {
                    match self.active_tab {
                        ActiveTab::Library => {
                            if self.tracks.len() != 0 {
                                self.track_select_by_index(0);
                            }
                        }
                        ActiveTab::Playlists => {
                            if self.tracks_playlist.len() != 0 {
                                self.playlist_track_select_by_index(0);
                            }
                        }
                        _ => {}
                    }
                }
                ActiveSection::Queue => {
                    self.selected_queue_item_manual_override = true;
                    self.selected_queue_item.select_first();
                }
                ActiveSection::Lyrics => {
                    self.selected_lyric_manual_override = true;
                    self.selected_lyric.select_first();
                }
                ActiveSection::Popup => {
                    self.popup.selected.select_first();
                }
            },
            KeyCode::Char('G') => match self.active_section {
                ActiveSection::Artists => {
                    match self.active_tab {
                        ActiveTab::Library => {
                            if self.artists.len() != 0 {
                                self.artist_select_by_index(self.artists.len() - 1);
                            }
                        }
                        ActiveTab::Playlists => {
                            if self.playlists.len() != 0 {
                                self.playlist_select_by_index(self.playlists.len() - 1);
                            }
                        }
                        _ => {}
                    }
                }
                ActiveSection::Tracks => {
                    match self.active_tab {
                        ActiveTab::Library => {
                            if self.tracks.len() != 0 {
                                self.track_select_by_index(self.tracks.len() - 1);
                            }
                        }
                        ActiveTab::Playlists => {
                            if self.tracks_playlist.len() != 0 {
                                self.playlist_track_select_by_index(self.tracks_playlist.len() - 1);
                            }
                        }
                        _ => {}
                    }
                }
                ActiveSection::Queue => {
                    if self.queue.len() != 0 {
                        self.selected_queue_item_manual_override = true;
                        self.selected_queue_item.select_last();
                        return;
                    }
                }
                ActiveSection::Lyrics => {
                    self.selected_lyric_manual_override = true;
                    if let Some((_, lyrics_vec, _)) = &self.lyrics {
                        if !lyrics_vec.is_empty() {
                            self.selected_lyric.select_last();
                        }
                    }
                }
                ActiveSection::Popup => {
                    self.popup.selected.select_last();
                }
            },
            KeyCode::Char('a') => match self.active_tab {
                ActiveTab::Library => {
                    match self.active_section {
                        // first artist with following letter
                        ActiveSection::Artists => {
                            if self.artists.is_empty() {
                                return;
                            }
                            let ids = search_results(&self.artists, &self.artists_search_term);
                            let mut artists = self.artists.iter().filter(|artist| ids.contains(&artist.id)).collect::<Vec<&Artist>>();
                            if artists.is_empty() {
                                artists = self.artists.iter().collect::<Vec<&Artist>>();
                            }
                            let selected = self.selected_artist.selected().unwrap_or(0);
                            let current_artist = artists[selected].name[0..1].to_lowercase();
                            let next_artist = artists.iter().skip(selected).find(|a| a.name[0..1].to_lowercase() != current_artist);

                            if let Some(next_artist) = next_artist {
                                let index = artists.iter().position(|a| a.id == next_artist.id).unwrap_or(0);
                                self.artist_select_by_index(index);
                            }
                        }
                        // this will go to the first song of the next album
                        ActiveSection::Tracks => {
                            if self.tracks.is_empty() {
                                return;
                            }
                            if let Some(selected) = self.selected_track.selected() {
                                let current_album = self.tracks[selected].album_id.clone();
                                let next_album = self.tracks.iter().skip(selected).find(|t| t.album_id != current_album && t.id != "_album_");

                                if let Some(next_album) = next_album {
                                    let index = self.tracks.iter().position(|t| t.album_id == next_album.album_id).unwrap_or(0);
                                    self.track_select_by_index(index);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                ActiveTab::Playlists => {
                    match self.active_section {
                        ActiveSection::Artists => {
                            if self.playlists.is_empty() {
                                return;
                            }
                            let ids = search_results(&self.playlists, &self.playlists_search_term);
                            let mut playlists = self.playlists.iter().filter(|playlist| ids.contains(&playlist.id)).collect::<Vec<&Playlist>>();
                            if playlists.is_empty() {
                                playlists = self.playlists.iter().collect::<Vec<&Playlist>>();
                            }
                            if let Some(selected) = self.selected_playlist.selected() {
                                let current_playlist = playlists[selected].name[0..1].to_lowercase();
                                let next_playlist = playlists.iter().skip(selected).find(|a| a.name[0..1].to_lowercase() != current_playlist);
        
                                if let Some(next_playlist) = next_playlist {
                                    let index = playlists.iter().position(|a| a.id == next_playlist.id).unwrap_or(0);
                                    self.playlist_select_by_index(index);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
            KeyCode::Char('A') => match self.active_tab {
                ActiveTab::Library => {
                    match self.active_section {
                        // first artist with previous letter
                        ActiveSection::Artists => {
                            if self.artists.is_empty() {
                                return;
                            }
                            let ids = search_results(&self.artists, &self.artists_search_term);
                            let mut artists = self.artists.iter().filter(|artist| ids.contains(&artist.id)).collect::<Vec<&Artist>>();
                            if artists.is_empty() {
                                artists = self.artists.iter().collect::<Vec<&Artist>>();
                            }
                            let selected = self.selected_artist.selected().unwrap_or(0);
                            let current_artist = artists[selected].name[0..1].to_lowercase();
                            let prev_artist = artists.iter().rev().skip(artists.len() - selected).find(|a| a.name[0..1].to_lowercase() != current_artist);

                            if let Some(prev_artist) = prev_artist {
                                let index = artists.iter().position(|a| a.id == prev_artist.id).unwrap_or(0);
                                self.artist_select_by_index(index);
                            }
                        }
                        // this will go to the first song of the previous album
                        ActiveSection::Tracks => {
                            if self.tracks.is_empty() {
                                return;
                            }
                            if let Some(selected) = self.selected_track.selected() {
                                let current_album = self.tracks[selected].album_id.clone();
                                let first_track_in_current_album = self.tracks.iter().position(|t| t.album_id == current_album).unwrap_or(0);
                                let prev_album = self.tracks.iter().rev().skip(self.tracks.len() - selected).find(|t| t.album_id != current_album && t.id != "_album_");

                                if selected != first_track_in_current_album {
                                    self.track_select_by_index(first_track_in_current_album);
                                    return;
                                }

                                if let Some(prev_album) = prev_album {
                                    let index = self.tracks.iter().position(|t| t.album_id == prev_album.album_id).unwrap_or(0);
                                    self.track_select_by_index(index);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                ActiveTab::Playlists => {
                    match self.active_section {
                        ActiveSection::Artists => {
                            if self.active_section != ActiveSection::Artists || self.playlists.is_empty() {
                                return;
                            }
                            let ids = search_results(&self.playlists, &self.playlists_search_term);
                            let mut playlists = self.playlists.iter().filter(|playlist| ids.contains(&playlist.id)).collect::<Vec<&Playlist>>();
                            if playlists.is_empty() {
                                playlists = self.playlists.iter().collect::<Vec<&Playlist>>();
                            }
                            if let Some(selected) = self.selected_playlist.selected() {
                                let current_playlist = playlists[selected].name[0..1].to_lowercase();
                                let prev_playlist = playlists.iter().rev().skip(playlists.len() - selected).find(|a| a.name[0..1].to_lowercase() != current_playlist);
        
                                if let Some(prev_playlist) = prev_playlist {
                                    let index = playlists.iter().position(|a| a.id == prev_playlist.id).unwrap_or(0);
                                    self.playlist_select_by_index(index);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
            KeyCode::Enter => {
                match self.active_section {
                    ActiveSection::Artists => {
                        if self.active_tab == ActiveTab::Library {
                            // if we are searching we need to account of the list index offsets caused by the search
                            if self.artists_search_term.len() > 0 {
                                let items = self
                                    .artists
                                    .iter()
                                    .filter(|artist| {
                                        if self.artists_search_term.is_empty() || self.active_section != ActiveSection::Artists {
                                            return true;
                                        }
                                        !helpers::find_all_subsequences(
                                            &self.artists_search_term.to_lowercase(), &artist.name.to_lowercase()
                                        ).is_empty()
                                    })
                                    .map(|artist| artist.id.clone())
                                    .collect::<Vec<String>>();
                                if items.len() == 0 {
                                    return;
                                }
                                self.tracks_search_term = String::from("");
                                let selected = self.selected_artist.selected().unwrap_or(0);
                                self.discography(&items[selected]).await;

                                if let Some(artist) = self.artists.iter_mut().find(|a| a.id == items[selected]) {
                                    artist.jellyfintui_recently_added = false;
                                }
                                self.selected_track.select(Some(0));
                                return;
                            }

                            let selected = self.selected_artist.selected().unwrap_or(0);
                            self.discography(&self.artists[selected].id.clone()).await;

                            self.artists[selected].jellyfintui_recently_added = false;

                            self.selected_track.select(Some(0));
                        }
                        if self.active_tab == ActiveTab::Playlists {
                            // if we are searching we need to account of the list index offsets caused by the search
                            if !self.playlists_search_term.is_empty() {
                                let ids = search_results(&self.playlists, &self.playlists_search_term);
                                if ids.is_empty() {
                                    return;
                                }
                                self.playlist_tracks_search_term = String::from("");
                                let selected = self.selected_playlist.selected().unwrap_or(0);
                                self.playlist(&ids[selected]).await;
                                let _ = self.playlist_tracks_scroll_state.content_length(self.tracks_playlist.len() - 1);
                                self.selected_playlist_track.select(Some(0));
                                return;
                            }
                            let selected = self.selected_playlist.selected().unwrap_or(0);
                            self.playlist(&self.playlists[selected].id.clone()).await;
                            self.selected_playlist_track.select(Some(0));
                            let _ = self.playlist_tracks_scroll_state.content_length(self.tracks_playlist.len() - 1);
                        }
                    }
                    ActiveSection::Tracks => {
                        let items = match self.active_tab {
                            ActiveTab::Library => {
                                let ids = search_results(&self.tracks, &self.tracks_search_term);
                                let items = self.tracks.iter()
                                    .filter(|t| ids.contains(&t.id) || ids.is_empty())
                                    .map(|t| t.clone())
                                    .collect();
                                items
                            }
                            ActiveTab::Playlists => {
                                let ids = search_results(&self.tracks_playlist, &self.playlist_tracks_search_term);
                                let items: Vec<crate::client::DiscographySong> = self.tracks_playlist.iter()
                                    .filter(|t| ids.contains(&t.id) || ids.is_empty())
                                    .map(|t| t.clone())
                                    .collect();
                                items
                            }
                            _ => vec![]
                        };

                        let selected = match self.active_tab {
                            ActiveTab::Library => self.selected_track.selected().unwrap_or(0),
                            ActiveTab::Playlists => self.selected_playlist_track.selected().unwrap_or(0),
                            _ => 0
                        };

                        if key_event.modifiers == KeyModifiers::CONTROL {
                            self.push_next_to_queue(&items, selected).await;
                            return;
                        }
                        if key_event.modifiers == KeyModifiers::SHIFT {
                            self.push_to_queue(&items, selected, 1).await;
                            return;
                        }
                        self.replace_queue(&items, selected);
                    }
                    ActiveSection::Queue => {
                       self.relocate_queue_and_play().await; 
                    }
                    ActiveSection::Lyrics => {
                        // jump to that timestamp
                        if let Some((_, lyrics_vec, _)) = &self.lyrics {
                            let selected = self.selected_lyric.selected().unwrap_or(0);
                            
                            if let Some(lyric) = lyrics_vec.get(selected) {
                                let time = lyric.start as f64 / 10_000_000.0;
                                
                                if time != 0.0 {
                                    if let Ok(mpv) = self.mpv_state.lock() {
                                        let _ = mpv.mpv.command("seek", &[&time.to_string(), "absolute"]);
                                        let _ = mpv.mpv.set_property("pause", false);
                                        self.paused = false;
                                        self.buffering = true;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Char('e') => {
                let items = match self.active_tab {
                    ActiveTab::Library => {
                        let ids = search_results(&self.tracks, &self.tracks_search_term);
                        let items = self.tracks.iter()
                            .filter(|t| ids.contains(&t.id) || ids.is_empty())
                            .map(|t| t.clone())
                            .collect();
                        items
                    }
                    ActiveTab::Playlists => {
                        let ids = search_results(&self.tracks_playlist, &self.playlist_tracks_search_term);
                        let items: Vec<crate::client::DiscographySong> = self.tracks_playlist.iter()
                            .filter(|t| ids.contains(&t.id) || ids.is_empty())
                            .map(|t| t.clone())
                            .collect();
                        items
                    }
                    _ => vec![]
                };

                let selected = match self.active_tab {
                    ActiveTab::Library => self.selected_track.selected().unwrap_or(0),
                    ActiveTab::Playlists => self.selected_playlist_track.selected().unwrap_or(0),
                    _ => 0
                };

                if key_event.modifiers == KeyModifiers::CONTROL {
                    self.push_next_to_queue(&items, selected).await;
                    return;
                }
                self.push_to_queue(&items, selected, 1).await;
            }
            // mark as favorite (works on anything)
            KeyCode::Char('f') => {
                match self.active_section {
                    ActiveSection::Artists => {
                        if let Some(client) = &self.client {
                            match self.active_tab {
                                ActiveTab::Library => {
                                    let id = self.get_id_of_selected(&self.artists, Selectable::Artist);
                                    if let Some(artist) = self.artists.iter_mut().find(|a| a.id == id) {
                                        let _ = client.set_favorite(&artist.id, !artist.user_data.is_favorite).await;
                                        artist.user_data.is_favorite = !artist.user_data.is_favorite;
                                    }
                                }
                                ActiveTab::Playlists => {
                                    let id = self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                                    if let Some(playlist) = self.playlists.iter_mut().find(|a| a.id == id) {
                                        let _ = client.set_favorite(&playlist.id, !playlist.user_data.is_favorite).await;
                                        playlist.user_data.is_favorite = !playlist.user_data.is_favorite;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    ActiveSection::Tracks => {
                        if let Some(client) = &self.client {
                            match self.active_tab {
                                ActiveTab::Library => {
                                    let selected = self.selected_track.selected().unwrap_or(0);
                                    let track = &self.tracks[selected].clone();
                                    let _ = client.set_favorite(&track.id, !track.user_data.is_favorite).await;
                                    self.tracks[selected].user_data.is_favorite = !track.user_data.is_favorite;
                                    if let Some(tr) = self.queue.iter_mut().find(|t| &t.id == &track.id) {
                                        tr.is_favorite = !track.user_data.is_favorite;
                                    }
                                }
                                ActiveTab::Playlists => {
                                    let selected = self.selected_playlist_track.selected().unwrap_or(0);
                                    let track = &self.tracks_playlist[selected].clone();
                                    let _ = client.set_favorite(&track.id, !track.user_data.is_favorite).await;
                                    self.tracks_playlist[selected].user_data.is_favorite = !track.user_data.is_favorite;
                                    if let Some(tr) = self.queue.iter_mut().find(|t| &t.id == &track.id) {
                                        tr.is_favorite = !track.user_data.is_favorite;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    ActiveSection::Queue => {
                        if let Some(client) = &self.client {
                            let selected = self.selected_queue_item.selected().unwrap_or(0);
                            let track = &self.queue[selected].clone();
                            let _ = client.set_favorite(&track.id, !track.is_favorite).await;
                            self.queue[selected].is_favorite = !track.is_favorite;
                            if let Some(tr) = self.tracks.iter_mut().find(|t| &t.id == &track.id) {
                                tr.user_data.is_favorite = !track.is_favorite;
                            }
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Char('r') => {
                if let Ok(mpv) = self.mpv_state.lock() {
                    match self.repeat {
                        Repeat::None => {
                            self.repeat = Repeat::All;
                            let _ = mpv.mpv.set_property("loop-playlist", "inf");
                        }
                        Repeat::All => {
                            self.repeat = Repeat::One;
                            let _ = mpv.mpv.set_property("loop-playlist", "no");
                            let _ = mpv.mpv.set_property("loop-file", "inf");
                        }
                        Repeat::One => {
                            self.repeat = Repeat::None;
                            let _ = mpv.mpv.set_property("loop-file", "no");
                            let _ = mpv.mpv.set_property("loop-playlist", "no");
                        }
                    }
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.popup.global = key_event.code == KeyCode::Char('P');

                if self.active_section == ActiveSection::Popup {
                    self.active_section = self.last_section;
                    self.popup.current_menu = None;
                } else {
                    self.last_section = self.active_section;
                    self.active_section = ActiveSection::Popup;
                }
            }
            KeyCode::Char('d') => {
                if self.active_section != ActiveSection::Queue {
                    return;
                }
                self.pop_from_queue().await;
            }
            KeyCode::Char('E') => {
                self.clear_queue().await;
            }
            KeyCode::Char('J') => {
                if self.active_section == ActiveSection::Queue {
                    self.move_queue_item_down().await;
                }
            }
            KeyCode::Char('K') => {
                if self.active_section == ActiveSection::Queue {
                    self.move_queue_item_up().await;
                }
            }
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                self.dirty_clear = true;
            }
            KeyCode::Esc => {
                if self.show_help {
                    self.show_help = false;
                    self.dirty_clear = true;
                    return;
                }
                let artist_id = self.get_id_of_selected(&self.artists, Selectable::Artist);
                let track_id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                let playlist_id = self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                let playlist_track_id = self.get_id_of_selected(&self.tracks_playlist, Selectable::PlaylistTrack);

                match self.active_tab {
                    ActiveTab::Library => {
                        match self.active_section {
                            ActiveSection::Artists => {
                                self.artists_search_term = String::from("");
                                self.reposition_artist_cursor(&artist_id);
                            }
                            ActiveSection::Tracks => {
                                self.tracks_search_term = String::from("");
                                self.reposition_track_cursor(&track_id);
                            }
                            _ => {}
                        }
                    }
                    ActiveTab::Playlists => {
                        match self.active_section {
                            ActiveSection::Artists => {
                                self.playlists_search_term = String::from("");
                                self.reposition_playlist_cursor(&playlist_id);
                            }
                            ActiveSection::Tracks => {
                                self.playlist_tracks_search_term = String::from("");
                                self.reposition_playlist_track_cursor(&playlist_track_id);
                            }
                            ActiveSection::Popup => {
                                self.active_section = self.last_section;
                            }
                            _ => {}
                        }
                    }
                    ActiveTab::Search => {
                        self.searching = false;
                        self.search_term = String::from("");
                        self.active_tab = ActiveTab::Library;
                    }
                }
            }
            KeyCode::F(1) => {
                self.active_tab = ActiveTab::Library;
                if self.tracks.is_empty() {
                    self.active_section = ActiveSection::Artists;
                }
            }
            KeyCode::F(2) => {
                self.active_tab = ActiveTab::Playlists;
                if self.tracks_playlist.is_empty() {
                    self.active_section = ActiveSection::Artists;
                }
            }
            KeyCode::F(3) => {
                self.active_tab = ActiveTab::Search;
                self.searching = true;
            }
            KeyCode::Char('/') => {
                self.locally_searching = true;
            }
            _ => {}
        }
    }

    async fn handle_search_tab_events(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc | KeyCode::F(1) => {
                if self.searching {
                    self.searching = false;
                    return;
                }
                self.active_tab = ActiveTab::Library;
            }
            KeyCode::F(2) => {
                self.active_tab = ActiveTab::Playlists;
                if self.tracks_playlist.is_empty() {
                    self.active_section = ActiveSection::Artists;
                }
            }
            KeyCode::F(3) => {
                self.searching = true;
            }
            KeyCode::Backspace => {
                self.search_term.pop();
            }
            KeyCode::Delete => {
                self.search_term.clear();
            }
            KeyCode::Tab => {
                self.toggle_search_section(true);
            }
            KeyCode::BackTab => {
                self.toggle_search_section(false);
            }
            KeyCode::Enter => {
                if let Some(client) = &self.client {
                    if self.searching {
                        match client.artists(self.search_term.clone()).await {
                            Ok(artists) => {
                                self.search_result_artists = artists;
                                self.selected_search_artist.select(Some(0));
                                self.search_artist_scroll_state = self.search_artist_scroll_state.content_length(self.search_result_artists.len());
                            }
                            _ => {}
                        }
                        match client.search_albums(self.search_term.clone()).await {
                            Ok(albums) => {
                                self.search_result_albums = albums;
                                self.selected_search_album.select(Some(0));
                                self.search_album_scroll_state = self.search_album_scroll_state.content_length(self.search_result_albums.len());
                            }
                            _ => {}
                        }
                        match client.search_tracks(self.search_term.clone()).await {
                            Ok(tracks) => {
                                self.search_result_tracks = tracks;
                                self.selected_search_track.select(Some(0));
                                self.search_track_scroll_state = self.search_track_scroll_state.content_length(self.search_result_tracks.len());
                            }
                            _ => {}
                        }

                        self.search_section = SearchSection::Artists;
                        if self.search_result_artists.len() == 0 {
                            self.search_section = SearchSection::Albums;
                        }
                        if self.search_result_albums.len() == 0 {
                            self.search_section = SearchSection::Tracks;
                        }
                        if self.search_result_tracks.len() == 0 && self.search_result_artists.len() == 0 && self.search_result_albums.len() == 0 {
                            self.search_section = SearchSection::Artists;
                        }

                        self.searching = false;
                        return;
                    }
                    // if not searching, we just go to the artist/etc we selected
                    match self.search_section {
                        SearchSection::Artists => {
                            let artist = match self.search_result_artists.get(
                                self.selected_search_artist.selected().unwrap_or(0)
                            ) {
                                Some(artist) => artist,
                                None => return,
                            };
                            let artist_id = artist.id.clone();

                            // in the Music tab, select this artist
                            self.active_tab = ActiveTab::Library;
                            self.active_section = ActiveSection::Artists;
                            self.artist_select_by_index(0);

                            // find the artist in the artists list using .id
                            let artist = self.artists.iter().find(|a| a.id == artist_id);

                            if let Some(art) = artist {
                                let index = self.artists.iter().position(|a| a.id == art.id).unwrap_or(0);
                                self.artist_select_by_index(index);

                                let selected = self.selected_artist.selected().unwrap_or(0);
                                self.discography(&self.artists[selected].id.clone()).await;
                                self.artists[selected].jellyfintui_recently_added = false;
                                self.track_select_by_index(0);
                            }
                        }
                        SearchSection::Albums => {
                            let album = match self.search_result_albums.get(
                                self.selected_search_album.selected().unwrap_or(0)
                            ) {
                                Some(album) => album,
                                None => return,
                            };

                            // in the Music tab, select this artist
                            self.active_tab = ActiveTab::Library;
                            self.active_section = ActiveSection::Artists;
                            let album_id = album.id.clone();

                            let artist_id = if album.album_artists.len() > 0 {
                                album.album_artists[0].id.clone()
                            } else {
                                String::from("")
                            };
                            self.artist_select_by_index(0);

                            // is rust crazy, or is it me?
                            if let Some(artist) = self.artists.iter().find(|a| a.id == artist_id) {
                                let index = self.artists.iter().position(|a| a.id == artist.id).unwrap_or(0);
                                self.artist_select_by_index(index);

                                let selected = self.selected_artist.selected().unwrap_or(0);
                                self.discography(&self.artists[selected].id.clone()).await;
                                self.artists[selected].jellyfintui_recently_added = false;
                                self.track_select_by_index(0);

                                // now find the first track that matches this album
                                if let Some(track) = self.tracks.iter().find(|t| t.album_id == album_id) {
                                    let index = self.tracks.iter().position(|t| t.id == track.id).unwrap_or(0);
                                    self.track_select_by_index(index);
                                }
                            }
                        }
                        SearchSection::Tracks => {
                            let track = match self.search_result_tracks.get(
                                self.selected_search_track.selected().unwrap_or(0)
                            ) {
                                Some(track) => track,
                                None => return,
                            };

                            // in the Music tab, select this artist
                            self.active_tab = ActiveTab::Library;
                            self.active_section = ActiveSection::Artists;

                            let track_id = track.id.clone();

                            let artist_id = if track.album_artists.len() > 0 {
                                track.album_artists[0].id.clone()
                            } else {
                                String::from("")
                            };
                            self.artist_select_by_index(0);

                            if let Some(artist) = self.artists.iter().find(|a| a.id == artist_id) {
                                let index = self.artists.iter().position(|a| a.id == artist.id).unwrap_or(0);
                                self.artist_select_by_index(index);

                                let selected = self.selected_artist.selected().unwrap_or(0);
                                self.discography(&self.artists[selected].id.clone()).await;
                                self.artists[selected].jellyfintui_recently_added = false;
                                self.track_select_by_index(0);

                                // now find the first track that matches this album
                                if let Some(track) = self.tracks.iter().find(|t| t.id == track_id) {
                                    let index = self.tracks.iter().position(|t| t.id == track.id).unwrap_or(0);
                                    self.track_select_by_index(index);
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                if self.searching {
                    if let KeyCode::Char(c) = key_event.code {
                        self.search_term.push(c);
                    }
                    return;
                }
                match key_event.code {
                    KeyCode::Down | KeyCode::Char('j') => match self.search_section {
                        SearchSection::Artists => {
                            let selected = self
                                .selected_search_artist
                                .selected()
                                .unwrap_or(self.search_result_artists.len() - 1);
                            if selected == self.search_result_artists.len() - 1 {
                                self.selected_search_artist.select(Some(selected));
                                self.search_artist_scroll_state = self.search_artist_scroll_state.position(selected);
                                return;
                            }
                            self.selected_search_artist.select(Some(selected + 1));
                            self.search_artist_scroll_state = self.search_artist_scroll_state.position(selected + 1);
                        }
                        SearchSection::Albums => {
                            let selected = self
                                .selected_search_album
                                .selected()
                                .unwrap_or(self.search_result_albums.len() - 1);
                            if selected == self.search_result_albums.len() - 1 {
                                self.selected_search_album.select(Some(selected));
                                self.search_album_scroll_state = self.search_album_scroll_state.position(selected);
                                return;
                            }
                            self.selected_search_album.select(Some(selected + 1));
                            self.search_album_scroll_state = self.search_album_scroll_state.position(selected + 1);
                        }
                        SearchSection::Tracks => {
                            let selected = self
                                .selected_search_track
                                .selected()
                                .unwrap_or(self.search_result_tracks.len() - 1);
                            if selected == self.search_result_tracks.len() - 1 {
                                self.selected_search_track.select(Some(selected));
                                self.search_track_scroll_state = self.search_track_scroll_state.position(selected);
                                return;
                            }
                            self.selected_search_track.select(Some(selected + 1));
                            self.search_track_scroll_state = self.search_track_scroll_state.position(selected + 1);
                        }
                    },
                    KeyCode::Up | KeyCode::Char('k') => match self.search_section {
                        SearchSection::Artists => {
                            let selected = self
                                .selected_search_artist
                                .selected()
                                .unwrap_or(0);
                            if selected == 0 {
                                self.selected_search_artist.select(Some(selected));
                                self.search_artist_scroll_state = self.search_artist_scroll_state.position(selected);
                                return;
                            }
                            self.selected_search_artist.select(Some(selected - 1));
                            self.search_artist_scroll_state = self.search_artist_scroll_state.position(selected - 1);
                        }
                        SearchSection::Albums => {
                            let selected = self
                                .selected_search_album
                                .selected()
                                .unwrap_or(0);
                            if selected == 0 {
                                self.selected_search_album.select(Some(selected));
                                self.search_album_scroll_state = self.search_album_scroll_state.position(selected);
                                return;
                            }
                            self.selected_search_album.select(Some(selected - 1));
                            self.search_album_scroll_state = self.search_album_scroll_state.position(selected - 1);
                        }
                        SearchSection::Tracks => {
                            let selected = self
                                .selected_search_track
                                .selected()
                                .unwrap_or(0);
                            if selected == 0 {
                                self.selected_search_track.select(Some(selected));
                                self.search_track_scroll_state = self.search_track_scroll_state.position(selected);
                                return;
                            }
                            self.selected_search_track.select(Some(selected - 1));
                            self.search_track_scroll_state = self.search_track_scroll_state.position(selected - 1);
                        }
                    },
                    KeyCode::Char('g') => match self.search_section {
                        SearchSection::Artists => {
                            self.selected_search_artist.select(Some(0));
                            self.search_artist_scroll_state = self.search_artist_scroll_state.position(0);
                        }
                        SearchSection::Albums => {
                            self.selected_search_album.select(Some(0));
                            self.search_album_scroll_state = self.search_album_scroll_state.position(0);
                        }
                        SearchSection::Tracks => {
                            self.selected_search_track.select(Some(0));
                            self.search_track_scroll_state = self.search_track_scroll_state.position(0);
                        }
                    },
                    KeyCode::Char('G') => match self.search_section {
                        SearchSection::Artists => {
                            self.selected_search_artist.select(Some(self.search_result_artists.len() - 1));
                            self.search_artist_scroll_state = self.search_artist_scroll_state.position(self.search_result_artists.len() - 1);
                        }
                        SearchSection::Albums => {
                            self.selected_search_album.select(Some(self.search_result_albums.len() - 1));
                            self.search_album_scroll_state = self.search_album_scroll_state.position(self.search_result_albums.len() - 1);
                        }
                        SearchSection::Tracks => {
                            self.selected_search_track.select(Some(self.search_result_tracks.len() - 1));
                            self.search_track_scroll_state = self.search_track_scroll_state.position(self.search_result_tracks.len() - 1);
                        }
                    },
                    KeyCode::Char('h') => {
                        self.vim_search_left();
                    }
                    KeyCode::Char('l') => {
                        self.vim_search_right();
                    }
                    KeyCode::Char('/') => {
                        self.searching = true;
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_mouse_event(&mut self, _mouse_event: crossterm::event::MouseEvent) {
        // println!("Mouse event: {:?}", _mouse_event);
        self.dirty = true;
    }

    fn toggle_section(&mut self, forwards: bool) {
        match forwards {
            true => match self.active_section {
                ActiveSection::Artists => self.active_section = ActiveSection::Tracks,
                ActiveSection::Tracks => self.active_section = ActiveSection::Artists,
                ActiveSection::Queue => {
                    match self.last_section {
                        ActiveSection::Artists => self.active_section = ActiveSection::Artists,
                        ActiveSection::Tracks => self.active_section = ActiveSection::Tracks,
                        _ => self.active_section = ActiveSection::Artists,
                    }
                    self.last_section = ActiveSection::Queue;
                    self.selected_queue_item_manual_override = false;
                }
                ActiveSection::Lyrics => {
                    match self.last_section {
                        ActiveSection::Artists => self.active_section = ActiveSection::Artists,
                        ActiveSection::Tracks => self.active_section = ActiveSection::Tracks,
                        _ => self.active_section = ActiveSection::Artists,
                    }
                    self.last_section = ActiveSection::Lyrics;
                    self.selected_lyric_manual_override = false;
                }
                _ => {}
            },
            false => match self.active_section {
                ActiveSection::Artists => {
                    self.last_section = ActiveSection::Artists;
                    self.active_section = ActiveSection::Lyrics;
                    self.last_section = ActiveSection::Artists;
                }
                ActiveSection::Tracks => {
                    self.last_section = ActiveSection::Tracks;
                    self.active_section = ActiveSection::Lyrics;
                    self.last_section = ActiveSection::Tracks;
                }
                ActiveSection::Lyrics => {
                    self.active_section = ActiveSection::Queue;
                    self.selected_lyric_manual_override = false;
                }
                ActiveSection::Queue => {
                    self.active_section = ActiveSection::Lyrics;
                    self.selected_queue_item_manual_override = false;
                }
                _ => {}
            },
        }
    }
}

/// Enum types for section switching

/// Active global tab
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ActiveTab {
    Library,
    Playlists,
    Search,
}
impl Default for ActiveTab {
    fn default() -> Self {
        ActiveTab::Library
    }
}

// Music - active "section"
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ActiveSection {
    Artists,
    Tracks,
    Queue,
    Lyrics,
    Popup,
}
impl Default for ActiveSection {
    fn default() -> Self {
        ActiveSection::Artists
    }
}

/// Search - active "section"
#[derive(Debug)]
pub enum SearchSection {
    Artists,
    Albums,
    Tracks,
}
impl Default for SearchSection {
    fn default() -> Self {
        SearchSection::Artists
    }
}
