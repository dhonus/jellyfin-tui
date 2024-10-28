/* --------------------------
Keyboard related functions
    - The entry point for handling keyboard events is the `handle_events` function
    - Handles all key events - movement within the program, seeking, volume control, etc.
    - Also used for searching
-------------------------- */

use crate::tui::{App, Song};

use std::io;
use std::time::Duration;
use crossterm::event::{self, Event, KeyEvent, KeyModifiers, KeyCode};

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
    pub fn toggle_search_section(&mut self, forwards: bool) {
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

    /// Move the cursor left in the library
    fn vim_library_left(&mut self) {
        match self.active_section {
            ActiveSection::Queue => self.active_section = ActiveSection::Lyrics,
            ActiveSection::Lyrics => self.active_section = ActiveSection::Tracks,
            ActiveSection::Tracks => self.active_section = ActiveSection::Artists,
            _ => {}
        }
    }

    /// Move the cursor right in the library
    fn vim_library_right(&mut self) {
        match self.active_section {
            ActiveSection::Artists => self.active_section = ActiveSection::Tracks,
            ActiveSection::Tracks => self.active_section = ActiveSection::Lyrics,
            ActiveSection::Lyrics => self.active_section = ActiveSection::Queue,
            _ => {}
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

    /// Search results as a vector of IDs
    ///
    fn track_search_results(&self) -> Vec<String> {
        let items = self
            .tracks
            .iter()
            .filter(|track| {
                track.name.to_lowercase().contains(&self.tracks_search_term.to_lowercase()) && track.id != "_album_"
            })
            .map(|track| track.id.clone())
            .collect::<Vec<String>>();
        return items;
    }

    fn artist_search_results(&self) -> Vec<String> {
        let items = self
            .artists
            .iter()
            .filter(|artist| {
                artist.name.to_lowercase().contains(&self.artists_search_term.to_lowercase())
            })
            .map(|artist| artist.id.clone())
            .collect::<Vec<String>>();
        return items;
    }

    // use the ID of the artist that is selected and set the cursor to the appropriate index
    fn reposition_artist_cursor(&mut self, artist_id: &str) {
        if artist_id == "" {
            if self.artists.len() > 0 {
                self.selected_artist.select(Some(0));
            }
            return;
        }
        if self.artists_search_term.len() > 0 {
            let items = self.artist_search_results();
            match items.iter().position(|id| id == artist_id) {
                Some(index) => {
                    self.selected_artist.select(Some(index));
                }
                None => {}
            }
            return;
        }
        match self.artists.iter().position(|a| a.id == artist_id) {
            Some(index) => {
                self.selected_artist.select(Some(index));
            }
            None => {}
        }
    }   

    fn get_id_of_selected_artist(&self) -> String {
        if self.artists_search_term.len() > 0 {
            let items = self.artist_search_results();
            if items.len() == 0 {
                return String::from("");
            }
            let selected = self.selected_artist.selected().unwrap_or(0);
            return items[selected].clone();
        }
        if self.artists.len() == 0 {
            return String::from("");
        }
        let selected = self.selected_artist.selected().unwrap_or(0);
        return self.artists[selected].id.clone();
    }

    fn get_id_of_selected_track(&self) -> String {
        if self.tracks_search_term.len() > 0 {
            let items = self.track_search_results();
            if items.len() == 0 {
                return String::from("");
            }
            let selected = self.selected_track.selected().unwrap_or(0);
            return items[selected].clone();
        }
        if self.tracks.len() == 0 {
            return String::from("");
        }
        let selected = self.selected_track.selected().unwrap_or(0);
        return self.tracks[selected].id.clone();
    }

    fn reposition_track_cursor(&mut self, track_id: &str) {
        if track_id == "" {
            if self.tracks.len() > 0 {
                self.selected_track.select(Some(0));
            }
            return;
        }
        if self.tracks_search_term.len() > 0 {
            let items = self.track_search_results();
            if let Some(index) = items.iter().position(|id| id == track_id) {
                self.selected_track.select(Some(index));
            }
            return;
        }
        if let Some(index) = self.tracks.iter().position(|t| t.id == track_id) {
            self.selected_track.select(Some(index));
        }
    }

    async fn handle_key_event(&mut self, key_event: KeyEvent) {

        if key_event.code == KeyCode::Char('c') && key_event.modifiers == KeyModifiers::CONTROL {
            self.exit();
            return;
        }

        if self.locally_searching {
            match key_event.code {
                KeyCode::Esc | KeyCode::F(1) => {
                    self.locally_searching = false;
                    let artist_id = self.get_id_of_selected_artist();
                    let track_id = self.get_id_of_selected_track();

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

                    return;
                }
                KeyCode::Enter => {
                    self.locally_searching = false;
                    match self.active_section {
                        ActiveSection::Artists => {
                            self.tracks_search_term = String::from("");
                        }
                        _ => {}
                    }
                    return;
                }
                KeyCode::Backspace => {
                    match self.active_section {
                        ActiveSection::Artists => {
                            let selected_id = self.get_id_of_selected_artist();
                            self.artists_search_term.pop();
                            self.reposition_artist_cursor(&selected_id);
                        }
                        ActiveSection::Tracks => {
                            let selected_id = self.get_id_of_selected_track();
                            self.tracks_search_term.pop();
                            self.reposition_track_cursor(&selected_id);
                        }
                        _ => {}
                    }
                }
                KeyCode::Delete => {
                    match self.active_section {
                        ActiveSection::Artists => {
                            let selected_id = self.get_id_of_selected_artist();
                            self.artists_search_term.clear();
                            self.reposition_artist_cursor(&selected_id);
                        }
                        ActiveSection::Tracks => {
                            let selected_id = self.get_id_of_selected_track();
                            self.tracks_search_term.clear();
                            self.reposition_track_cursor(&selected_id);
                        }
                        _ => {}
                    }
                }
                KeyCode::Char(c) => {
                    match self.active_section {
                        ActiveSection::Artists => {
                            self.artists_search_term.push(c);
                            self.selected_artist.select(Some(0)); 
                        }
                        ActiveSection::Tracks => {
                            self.tracks_search_term.push(c);
                            self.selected_track.select(Some(0)); 
                        }
                        _ => {}
                    }    
                }
                _ => {}
            }
            return;
        }

        match self.active_tab {
            ActiveTab::Search => {
                match key_event.code {
                    KeyCode::Esc | KeyCode::F(1) => {
                        if self.searching {
                            self.searching = false;
                            return;
                        }
                        self.active_tab = ActiveTab::Library;
                    }
                    KeyCode::F(2) => {
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
                                    }
                                    _ => {}
                                }
                                match client.search_albums(self.search_term.clone()).await {
                                    Ok(albums) => {
                                        self.search_result_albums = albums;
                                        self.selected_search_album.select(Some(0));
                                    }
                                    _ => {}
                                }
                                match client.search_tracks(self.search_term.clone()).await {
                                    Ok(tracks) => {
                                        self.search_result_tracks = tracks;
                                        self.selected_search_track.select(Some(0));
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

                                    // in the Music tab, select this artist
                                    self.active_tab = ActiveTab::Library;
                                    self.active_section = ActiveSection::Artists;
                                    self.selected_artist.select(Some(0));

                                    // find the artist in the artists list using .id
                                    let artist = self.artists.iter().find(|a| a.id == artist.id);

                                    if let Some(artist) = artist {
                                        let index = self.artists.iter().position(|a| a.id == artist.id).unwrap();
                                        self.selected_artist.select(Some(index));

                                        let selected = self.selected_artist.selected().unwrap_or(0);
                                        self.discography(&self.artists[selected].id.clone()).await;
                                        self.artists[selected].jellyfintui_recently_added = false;
                                        self.selected_track.select(Some(1));
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
                                    self.selected_artist.select(Some(0));

                                    let artist_id = if album.album_artists.len() > 0 {
                                        album.album_artists[0].id.clone()
                                    } else {
                                        String::from("")
                                    };

                                    // is rust crazy, or is it me?
                                    if let Some(artist) = self.artists.iter().find(|a| a.id == artist_id) {
                                        let index = self.artists.iter().position(|a| a.id == artist.id).unwrap();
                                        self.selected_artist.select(Some(index));

                                        let selected = self.selected_artist.selected().unwrap_or(0);
                                        let album_id = album.id.clone();
                                        self.discography(&self.artists[selected].id.clone()).await;
                                        self.artists[selected].jellyfintui_recently_added = false;
                                        self.selected_track.select(Some(1));

                                        // now find the first track that matches this album
                                        if let Some(track) = self.tracks.iter().find(|t| t.album_id == album_id) {
                                            let index = self.tracks.iter().position(|t| t.id == track.id).unwrap();
                                            self.selected_track.select(Some(index));
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
                                    self.selected_artist.select(Some(0));

                                    let artist_id = if track.album_artists.len() > 0 {
                                        track.album_artists[0].id.clone()
                                    } else {
                                        String::from("")
                                    };

                                    if let Some(artist) = self.artists.iter().find(|a| a.id == artist_id) {
                                        let index = self.artists.iter().position(|a| a.id == artist.id).unwrap();
                                        self.selected_artist.select(Some(index));

                                        let selected = self.selected_artist.selected().unwrap_or(0);
                                        let track_id = track.id.clone();
                                        self.discography(&self.artists[selected].id.clone()).await;
                                        self.artists[selected].jellyfintui_recently_added = false;
                                        self.selected_track.select(Some(0));

                                        // now find the first track that matches this album
                                        if let Some(track) = self.tracks.iter().find(|t| t.id == track_id) {
                                            let index = self.tracks.iter().position(|t| t.id == track.id).unwrap();
                                            self.selected_track.select(Some(index));
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
                                        return;
                                    }
                                    self.selected_search_artist.select(Some(selected + 1));
                                }
                                SearchSection::Albums => {
                                    let selected = self
                                        .selected_search_album
                                        .selected()
                                        .unwrap_or(self.search_result_albums.len() - 1);
                                    if selected == self.search_result_albums.len() - 1 {
                                        self.selected_search_album.select(Some(selected));
                                        return;
                                    }
                                    self.selected_search_album.select(Some(selected + 1));
                                }
                                SearchSection::Tracks => {
                                    let selected = self
                                        .selected_search_track
                                        .selected()
                                        .unwrap_or(self.search_result_tracks.len() - 1);
                                    if selected == self.search_result_tracks.len() - 1 {
                                        self.selected_search_track.select(Some(selected));
                                        return;
                                    }
                                    self.selected_search_track.select(Some(selected + 1));
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
                                        return;
                                    }
                                    self.selected_search_artist.select(Some(selected - 1));
                                }
                                SearchSection::Albums => {
                                    let selected = self
                                        .selected_search_album
                                        .selected()
                                        .unwrap_or(0);
                                    if selected == 0 {
                                        self.selected_search_album.select(Some(selected));
                                        return;
                                    }
                                    self.selected_search_album.select(Some(selected - 1));
                                }
                                SearchSection::Tracks => {
                                    let selected = self
                                        .selected_search_track
                                        .selected()
                                        .unwrap_or(0);
                                    if selected == 0 {
                                        self.selected_search_track.select(Some(selected));
                                        return;
                                    }
                                    self.selected_search_track.select(Some(selected - 1));
                                }
                            },
                            KeyCode::Char('g') => match self.search_section {
                                SearchSection::Artists => {
                                    self.selected_search_artist.select(Some(0));
                                }
                                SearchSection::Albums => {
                                    self.selected_search_album.select(Some(0));
                                }
                                SearchSection::Tracks => {
                                    self.selected_search_track.select(Some(0));
                                }
                            },
                            KeyCode::Char('G') => match self.search_section {
                                SearchSection::Artists => {
                                    self.selected_search_artist.select(Some(self.search_result_artists.len() - 1));
                                }
                                SearchSection::Albums => {
                                    self.selected_search_album.select(Some(self.search_result_albums.len() - 1));
                                }
                                SearchSection::Tracks => {
                                    self.selected_search_track.select(Some(self.search_result_tracks.len() - 1));
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
                return;
            }
            _ => {}
        }

        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            // Seek backward
            KeyCode::Left | KeyCode::Char('r')  => {
                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.seek_backward(5.0);
                }
            }
            // Seek forward
            KeyCode::Right | KeyCode::Char('s') => {
                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.seek_forward(5.0);
                }
            }
            // Previous track
            KeyCode::Char('n') => {
                if let Some(client) = &self.client {
                    let _ = client.stopped(
                        self.active_song_id.clone(),
                        // position ticks
                        (self.current_playback_state.duration * self.current_playback_state.percentage * 100000.0) as u64,
                    ).await;
                    if let Ok(mpv) = self.mpv_state.lock() {
                        let _ = mpv.mpv.playlist_next_force();
                    }
                }
            }
            // Next track
            KeyCode::Char('N') => {
                if let Ok(mpv) = self.mpv_state.lock() {
                    let current_time = self.current_playback_state.duration * self.current_playback_state.percentage / 100.0;
                    if current_time > 5.0 {
                        let _ = mpv.mpv.seek_absolute(0.0);
                        return;
                    }
                    let _ = mpv.mpv.playlist_previous_force();
                }
            }
            // Play/Pause
            KeyCode::Char(' ') => {
                if let Ok(mpv) = self.mpv_state.lock() {
                    if self.paused {
                        let _ = mpv.mpv.unpause();
                        self.paused = false;
                    } else {
                        let _ = mpv.mpv.pause();
                        self.paused = true;
                    }
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
            }
            // Volume down
            KeyCode::Char('-') => {
                if self.current_playback_state.volume <= 0 {
                    return;
                }
                self.current_playback_state.volume -= 5;
                match self.mpv_state.lock() {
                    Ok(mpv) => {
                        let _ = mpv.mpv.set_property("volume", self.current_playback_state.volume);
                    }
                    Err(_) => {}
                }
            }
            KeyCode::Tab => {
                self.toggle_section(true);
            }
            KeyCode::BackTab => {
                self.toggle_section(false);
            }
            KeyCode::Char('h') => {
                self.vim_library_left();
            }
            KeyCode::Char('l') => {
                self.vim_library_right();
            }
            // Move down
            KeyCode::Down | KeyCode::Char('j') => match self.active_section {
                ActiveSection::Artists => {

                    if self.artists_search_term.len() > 0 {
                        let items = self.artist_search_results();
                        let selected = self
                            .selected_artist
                            .selected()
                            .unwrap_or(items.len() - 1);
                        if selected == items.len() - 1 {
                            self.selected_artist.select(Some(selected));
                            return;
                        }
                        self.selected_artist.select(Some(selected + 1));
                        return;
                    }

                    let selected = self
                        .selected_artist
                        .selected()
                        .unwrap_or(self.artists.len() - 1);
                    if selected == self.artists.len() - 1 {
                        self.selected_artist.select(Some(selected));
                        return;
                    }
                    self.selected_artist.select(Some(selected + 1));
                }
                ActiveSection::Tracks => {

                   if self.tracks_search_term.len() > 0 {
                        let items = self.track_search_results();
                        let selected = self
                            .selected_track
                            .selected()
                            .unwrap_or(items.len() - 1);
                        if selected == items.len() - 1 {
                            self.selected_track.select(Some(selected));
                            return;
                        }
                        self.selected_track.select(Some(selected + 1));
                        return;
                    }

                    let selected = self
                        .selected_track
                        .selected()
                        .unwrap_or(self.tracks.len() - 1);
                    if selected == self.tracks.len() - 1 {
                        self.selected_track.select(Some(selected));
                        return;
                    }
                    self.selected_track.select(Some(selected + 1));
                    if self.tracks.len() > 0 {
                        if self.tracks[self.selected_track.selected().unwrap()].id == "_album_" {
                            self.selected_track.select(Some(selected + 2));
                        }
                    }
                }
                ActiveSection::Queue => {
                    *self.selected_queue_item.offset_mut() += 1;
                }
                ActiveSection::Lyrics => {
                    self.selected_lyric_manual_override = true;
                    if let Some((_, lyrics_vec, _)) = &self.lyrics {
                        let selected = self
                            .selected_lyric
                            .selected()
                            .unwrap_or(lyrics_vec.len() - 1);
                            
                        if selected == lyrics_vec.len() - 1 {
                            self.selected_lyric.select(Some(selected));
                            return;
                        }
                        self.selected_lyric.select(Some(selected + 1));
                    }
                    self.selected_lyric_manual_override = true;
                }
            },
            KeyCode::Up | KeyCode::Char('k') => match self.active_section {
                ActiveSection::Artists => {

                    if self.artists_search_term.len() > 0 {
                        let selected = self
                            .selected_artist
                            .selected()
                            .unwrap_or(0);
                        if selected == 0 {
                            self.selected_artist.select(Some(selected));
                            return;
                        }
                        self.selected_artist.select(Some(selected - 1));
                        return;
                    }

                    let selected = self.selected_artist.selected().unwrap_or(0);
                    if selected == 0 {
                        self.selected_artist.select(Some(selected));
                        return;
                    }
                    self.selected_artist.select(Some(selected - 1));
                }
                ActiveSection::Tracks => {

                    if self.tracks_search_term.len() > 0 {
                        let selected = self
                            .selected_track
                            .selected()
                            .unwrap_or(0);
                        if selected == 0 {
                            self.selected_track.select(Some(selected));
                            return;
                        }
                        self.selected_track.select(Some(selected - 1));
                        return;
                    }

                    let selected = self.selected_track.selected().unwrap_or(0);
                    if selected == 0 {
                        self.selected_track.select(Some(selected));
                        return;
                    }
                    self.selected_track.select(Some(selected - 1));
                    if self.tracks.len() > 0 {
                        if self.tracks[self.selected_track.selected().unwrap()].id == "_album_" {
                            if selected == 1 {
                                self.selected_track.select(Some(1));
                            } else {
                                self.selected_track.select(Some(selected - 2));
                            }
                        }
                    }
                }
                ActiveSection::Queue => {
                    let lvalue = self.selected_queue_item.offset_mut();
                    if *lvalue == 0 {
                        return;
                    }
                    *lvalue -= 1;
                }
                ActiveSection::Lyrics => {
                    self.selected_lyric_manual_override = true;
                    let selected = self.selected_lyric.selected().unwrap_or(0);
                    if selected == 0 {
                        self.selected_lyric.select(Some(selected));
                        return;
                    }
                    self.selected_lyric.select(Some(selected - 1));
                }
            },
            KeyCode::Char('g') => match self.active_section {
                ActiveSection::Artists => {
                    self.selected_artist.select(Some(0));
                }
                ActiveSection::Tracks => {
                    if self.tracks_search_term.len() > 0 {
                        self.selected_track.select(Some(0));
                        return;
                    }
                    self.selected_track.select(Some(1));
                }
                ActiveSection::Queue => {
                    self.selected_queue_item.select(Some(0));
                }
                ActiveSection::Lyrics => {
                    self.selected_lyric_manual_override = true;
                    self.selected_lyric.select(Some(0));
                }
            },
            KeyCode::Char('G') => match self.active_section {
                ActiveSection::Artists => {
                    if self.artists.len() != 0 {
                        self.selected_artist.select(Some(self.artists.len() - 1));
                    }
                }
                ActiveSection::Tracks => {
                    if self.tracks.len() != 0 {
                        self.selected_track.select(Some(self.tracks.len() - 1));
                    }
                }
                ActiveSection::Queue => {
                    if self.playlist.len() != 0 {
                        self.selected_queue_item.select(Some(self.playlist.len() - 1));
                        return;
                    }
                }
                ActiveSection::Lyrics => {
                    self.selected_lyric_manual_override = true;
                    if let Some((_, lyrics_vec, _)) = &self.lyrics {
                        if !lyrics_vec.is_empty() {
                            self.selected_lyric.select(Some(lyrics_vec.len() - 1));
                        }
                    }
                }
            },
            KeyCode::Enter => {
                match self.active_section {
                    ActiveSection::Artists => {
                        // if we are searching we need to account of the list index offsets caused by the search
                        if self.artists_search_term.len() > 0 {
                            let items = self
                                .artists
                                .iter()
                                .filter(|artist| {
                                    if self.artists_search_term.is_empty() || self.active_section != ActiveSection::Artists {
                                        return true;
                                    }
                                    artist.name.to_lowercase().contains(&self.artists_search_term.to_lowercase())
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
                            self.selected_track.select(Some(1));
                            return;
                        }

                        let selected = self.selected_artist.selected().unwrap_or(0);
                        self.discography(&self.artists[selected].id.clone()).await;

                        self.artists[selected].jellyfintui_recently_added = false;

                        self.selected_track.select(Some(1));
                    }
                    ActiveSection::Tracks => {
                        let selected = self.selected_track.selected().unwrap_or(0);
                        if let Some(client) = &self.client {
                            if let Ok(mut mpv) = self.mpv_state.lock() {
                                let _ = mpv.should_stop = true;
                            }

                            // the playlist MPV will be getting
                            self.playlist = self
                                .tracks
                                .iter()
                                .skip(selected)
                                .filter(|track| track.id != "_album_")
                                .map(|track| {
                                    Song {
                                        id: track.id.clone(),
                                        url: client.song_url_sync(track.id.clone()),
                                        name: track.name.clone(),
                                        artist: track.album_artist.clone(),
                                        artist_items: track.artist_items.clone(),
                                        album: track.album.clone(),
                                        parent_id: track.parent_id.clone(),
                                        production_year: track.production_year,
                                    }
                                })
                                .collect();

                            if self.tracks_search_term.len() > 0 {
                                self.playlist = self.playlist.iter().filter(|track| {
                                    track.name.to_lowercase().contains(&self.tracks_search_term.to_lowercase())
                                }).map(|track| track.clone()).collect();
                            }
                            let _ = self.replace_playlist(); // TODO: inform user of error
                        }
                    }
                    ActiveSection::Queue => {
                        let _ = self.selected_queue_item.selected().unwrap_or(0);
                        // println!("Selected queue item: {:?}", selected);
                    }
                    ActiveSection::Lyrics => {
                        // jump to that timestamp
                        if let Some((_, lyrics_vec, _)) = &self.lyrics {
                            let selected = self.selected_lyric.selected().unwrap_or(0);
                            
                            if let Some(lyric) = lyrics_vec.get(selected) {
                                let time = lyric.start as f64 / 10_000_000.0;
                                
                                if time != 0.0 {
                                    if let Ok(mpv) = self.mpv_state.lock() {
                                        let _ = mpv.mpv.seek_absolute(time);
                                        let _ = mpv.mpv.unpause();
                                        self.paused = false;
                                        self.buffering = 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            KeyCode::Esc | KeyCode::F(1) => {
                self.active_tab = ActiveTab::Library;
                let artist_id = self.get_id_of_selected_artist();
                let track_id = self.get_id_of_selected_track();

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
            KeyCode::F(2) => {
                self.active_tab = ActiveTab::Search;
                self.searching = true;
            }
            KeyCode::Char('/') => {
                self.locally_searching = true;
            }
            _ => {}
        }
    }

    fn handle_mouse_event(&mut self, _mouse_event: crossterm::event::MouseEvent) {
        // println!("Mouse event: {:?}", _mouse_event);
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
                }
                ActiveSection::Lyrics => {
                    match self.last_section {
                        ActiveSection::Artists => self.active_section = ActiveSection::Artists,
                        ActiveSection::Tracks => self.active_section = ActiveSection::Tracks,
                        _ => self.active_section = ActiveSection::Artists,
                    }
                    self.selected_lyric_manual_override = false;
                }
            },
            false => match self.active_section {
                ActiveSection::Artists => {
                    self.last_section = ActiveSection::Artists;
                    self.active_section = ActiveSection::Tracks;
                }
                ActiveSection::Tracks => {
                    self.last_section = ActiveSection::Tracks;
                    self.active_section = ActiveSection::Lyrics;
                }
                ActiveSection::Lyrics => {
                    self.active_section = ActiveSection::Queue;
                    self.selected_lyric_manual_override = false;
                }
                ActiveSection::Queue => self.active_section = ActiveSection::Artists,
            },
        }
    }
}

/// Enum types for section switching

/// Active global tab
#[derive(Debug, Clone, Copy)]
pub enum ActiveTab {
    Library,
    Search,
}
impl Default for ActiveTab {
    fn default() -> Self {
        ActiveTab::Library
    }
}

// Music - active "section"
#[derive(Debug,PartialEq)]
pub enum ActiveSection {
    Artists,
    Tracks,
    Queue,
    Lyrics,
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