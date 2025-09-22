/* --------------------------
Keyboard related functions
    - The entry point for handling keyboard events is the `handle_events` function
    - Handles all key events - movement within the program, seeking, volume control, etc.
    - Also used for searching
-------------------------- */

use crate::{client::{Album, Artist, DiscographySong, Playlist}, database::{
    database::{Command, DeleteCommand, DownloadCommand}, extension::{get_all_albums, get_all_artists, get_all_playlists, DownloadStatus}
}, helpers::{self, State}, popup::PopupMenu, sort, tui::{App, Repeat}};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::io;
use std::time::Duration;
use crate::database::extension::{get_discography, get_tracks, set_favorite_album, set_favorite_artist, set_favorite_playlist, set_favorite_track};

pub trait Searchable {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
}

pub enum Selectable {
    Artist,
    Album,
    AlbumTrack,
    Track,
    Playlist,
    PlaylistTrack,
    Popup,
}

/// Search results as a vector of IDs. Used in all searchable areas
///
pub fn search_results<T: Searchable>(
    items: &[T],
    search_term: &str,
    empty_returns_all: bool,
) -> Vec<String> {
    if empty_returns_all && search_term.is_empty() {
        return items.iter().map(|item| String::from(item.id())).collect();
    }
    let mut scored_items = items
        .iter()
        .filter(|item| !item.id().starts_with("_album_"))
        .filter_map(|item| {
            let name = item.name().to_lowercase();
            let matches = helpers::find_all_subsequences(&search_term.to_lowercase(), &name);

            if matches.is_empty() {
                None
            } else {
                let score = matches.last().unwrap().1 - matches.first().unwrap().0;
                Some((String::from(item.id()), score))
            }
        })
        .collect::<Vec<_>>();

    scored_items.sort_by_key(|&(_, score)| score);
    scored_items.into_iter().map(|(id, _)| id).collect()
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
            true => match self.state.search_section {
                SearchSection::Artists => self.state.search_section = SearchSection::Albums,
                SearchSection::Albums => self.state.search_section = SearchSection::Tracks,
                SearchSection::Tracks => self.state.search_section = SearchSection::Artists,
            },
            false => match self.state.search_section {
                SearchSection::Artists => self.state.search_section = SearchSection::Tracks,
                SearchSection::Albums => self.state.search_section = SearchSection::Artists,
                SearchSection::Tracks => self.state.search_section = SearchSection::Albums,
            },
        }
    }

    /// Move the cursor right in the search
    fn vim_search_left(&mut self) {
        match self.state.search_section {
            SearchSection::Tracks => self.state.search_section = SearchSection::Albums,
            SearchSection::Albums => self.state.search_section = SearchSection::Artists,
            _ => {}
        }
    }

    /// Move the cursor left in the search
    fn vim_search_right(&mut self) {
        match self.state.search_section {
            SearchSection::Artists => self.state.search_section = SearchSection::Albums,
            SearchSection::Albums => self.state.search_section = SearchSection::Tracks,
            _ => {}
        }
    }

    pub fn reposition_cursor(&mut self, id: &str, selectable: Selectable) {
        let search_term = match selectable {
            Selectable::Artist => &self.state.artists_search_term,
            Selectable::Album => &self.state.albums_search_term,
            Selectable::AlbumTrack => &self.state.album_tracks_search_term,
            Selectable::Track => &self.state.tracks_search_term,
            Selectable::Playlist => &self.state.playlists_search_term,
            Selectable::PlaylistTrack => &self.state.playlist_tracks_search_term,
            Selectable::Popup => &self.popup_search_term,
        };
        let ids = match selectable {
            Selectable::Artist => self
                .artists
                .iter()
                .map(|a| a.id.clone())
                .collect::<Vec<String>>(),
            Selectable::Album => self
                .albums
                .iter()
                .map(|a| a.id.clone())
                .collect::<Vec<String>>(),
            Selectable::AlbumTrack => self
                .album_tracks
                .iter()
                .map(|t| t.id.clone())
                .collect::<Vec<String>>(),
            Selectable::Track => self
                .tracks
                .iter()
                .map(|t| t.id.clone())
                .collect::<Vec<String>>(),
            Selectable::Playlist => self
                .playlists
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<String>>(),
            Selectable::PlaylistTrack => self
                .playlist_tracks
                .iter()
                .map(|t| t.id.clone())
                .collect::<Vec<String>>(),
            Selectable::Popup => {
                if let Some(menu) = &self.popup.current_menu {
                    menu.options()
                        .iter()
                        .map(|o| String::from(o.id()))
                        .collect::<Vec<String>>()
                } else {
                    vec![]
                }
            }
        };

        if id.is_empty() && !ids.is_empty() {
            match selectable {
                Selectable::Artist => self.artist_select_by_index(0),
                Selectable::Album => self.album_select_by_index(0),
                Selectable::AlbumTrack => self.album_track_select_by_index(0),
                Selectable::Track => self.track_select_by_index(0),
                Selectable::Playlist => self.playlist_select_by_index(0),
                Selectable::PlaylistTrack => self.playlist_track_select_by_index(0),
                Selectable::Popup => self.popup.selected.select_first(),
            }
            return;
        }

        if !search_term.is_empty() {
            let items = match selectable {
                Selectable::Artist => search_results(&self.artists, search_term, false),
                Selectable::Album => search_results(&self.albums, search_term, false),
                Selectable::AlbumTrack => search_results(&self.album_tracks, search_term, false),
                Selectable::Track => search_results(&self.tracks, search_term, false),
                Selectable::Playlist => search_results(&self.playlists, search_term, false),
                Selectable::PlaylistTrack => {
                    search_results(&self.playlist_tracks, search_term, false)
                }
                Selectable::Popup => self.popup.current_menu.as_ref().map_or(vec![], |menu| {
                    search_results(&menu.options(), search_term, false)
                })
            };
            if let Some(index) = items.iter().position(|i| i == id) {
                match selectable {
                    Selectable::Artist => self.artist_select_by_index(index),
                    Selectable::Album => self.album_select_by_index(index),
                    Selectable::AlbumTrack => self.album_track_select_by_index(index),
                    Selectable::Track => self.track_select_by_index(index),
                    Selectable::Playlist => self.playlist_select_by_index(index),
                    Selectable::PlaylistTrack => self.playlist_track_select_by_index(index),
                    Selectable::Popup => self.popup.selected.select(Some(index)),
                }
                return;
            }
        }
        if let Some(index) = ids.iter().position(|i| i == id) {
            match selectable {
                Selectable::Artist => self.artist_select_by_index(index),
                Selectable::Album => self.album_select_by_index(index),
                Selectable::AlbumTrack => self.album_track_select_by_index(index),
                Selectable::Track => self.track_select_by_index(index),
                Selectable::Playlist => self.playlist_select_by_index(index),
                Selectable::PlaylistTrack => self.playlist_track_select_by_index(index),
                Selectable::Popup => self.popup.selected.select(Some(index)),
            }
        }
    }

    pub fn get_id_of_selected<T: Searchable>(&self, items: &[T], selectable: Selectable) -> String {
        let search_term = match selectable {
            Selectable::Artist => &self.state.artists_search_term,
            Selectable::Album => &self.state.albums_search_term,
            Selectable::AlbumTrack => &self.state.album_tracks_search_term,
            Selectable::Track => &self.state.tracks_search_term,
            Selectable::Playlist => &self.state.playlists_search_term,
            Selectable::PlaylistTrack => &self.state.playlist_tracks_search_term,
            Selectable::Popup => &self.popup_search_term,
        };
        let selected = match selectable {
            Selectable::Artist => self.state.selected_artist.selected(),
            Selectable::Album => self.state.selected_album.selected(),
            Selectable::AlbumTrack => self.state.selected_album_track.selected(),
            Selectable::Track => self.state.selected_track.selected(),
            Selectable::Playlist => self.state.selected_playlist.selected(),
            Selectable::PlaylistTrack => self.state.selected_playlist_track.selected(),
            Selectable::Popup => self.popup.selected.selected(),
        };
        let selected = selected.unwrap_or(0);
        if !search_term.is_empty() {
            let items = search_results(items, search_term, false);
            if items.is_empty() || items.len() <= selected {
                return String::from("");
            }
            return items[selected].clone();
        }
        if items.is_empty() || items.len() <= selected {
            return String::from("");
        }
        String::from(items[selected].id())
    }

    pub fn artist_select_by_index(&mut self, index: usize) {
        let items = search_results(&self.artists, &self.state.artists_search_term, true);
        if items.is_empty() {
            return;
        }
        let index = std::cmp::min(index, items.len() - 1);
        self.state.selected_artist.select(Some(index));
        self.state.artists_scroll_state = self
            .state
            .artists_scroll_state
            .content_length(items.len())
            .position(index);
    }

    pub fn track_select_by_index(&mut self, index: usize) {
        let items = search_results(&self.tracks, &self.state.tracks_search_term, true);
        if items.is_empty() {
            return;
        }
        let index = std::cmp::min(index, items.len() - 1);
        self.state.selected_track.select(Some(index));
        self.state.tracks_scroll_state = self
            .state
            .tracks_scroll_state
            .content_length(items.len())
            .position(index);
    }

    pub fn album_select_by_index(&mut self, index: usize) {
        let items = search_results(&self.albums, &self.state.albums_search_term, true);
        if items.is_empty() {
            return;
        }
        let index = std::cmp::min(index, items.len() - 1);
        self.state.selected_album.select(Some(index));
        self.state.albums_scroll_state = self
            .state
            .albums_scroll_state
            .content_length(items.len())
            .position(index);
    }

    pub fn album_track_select_by_index(&mut self, index: usize) {
        let items = search_results(
            &self.album_tracks,
            &self.state.album_tracks_search_term,
            true,
        );
        if items.is_empty() {
            return;
        }
        let index = std::cmp::min(index, items.len() - 1);
        self.state.selected_album_track.select(Some(index));
        self.state.album_tracks_scroll_state = self
            .state
            .album_tracks_scroll_state
            .content_length(items.len())
            .position(index);
    }

    pub fn playlist_track_select_by_index(&mut self, index: usize) {
        let items = search_results(
            &self.playlist_tracks,
            &self.state.playlist_tracks_search_term,
            true,
        );
        if items.is_empty() {
            return;
        }
        let index = std::cmp::min(index, items.len() - 1);
        self.state.selected_playlist_track.select(Some(index));
        self.state.playlist_tracks_scroll_state = self
            .state
            .playlist_tracks_scroll_state
            .content_length(items.len())
            .position(index);
    }

    pub fn playlist_select_by_index(&mut self, index: usize) {
        let items = search_results(&self.playlists, &self.state.playlists_search_term, true);
        if items.is_empty() {
            return;
        }
        let index = std::cmp::min(index, items.len() - 1);
        self.state.selected_playlist.select(Some(index));
        self.state.playlists_scroll_state = self
            .state
            .playlists_scroll_state
            .content_length(items.len())
            .position(index);
    }

    async fn handle_key_event(&mut self, key_event: KeyEvent) {
        self.dirty = true;

        if key_event.code == KeyCode::Char('c') && key_event.modifiers == KeyModifiers::CONTROL {
            self.exit().await;
            return;
        }

        if self.state.active_section == ActiveSection::Popup {
            self.popup_handle_keys(key_event).await;
            return;
        }

        if self.locally_searching {
            match key_event.code {
                KeyCode::Esc | KeyCode::F(1) => {
                    self.locally_searching = false;
                    let artist_id = self.get_id_of_selected(&self.artists, Selectable::Artist);
                    let track_id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                    let album_id = self.get_id_of_selected(&self.albums, Selectable::Album);
                    let album_track_id =
                        self.get_id_of_selected(&self.album_tracks, Selectable::AlbumTrack);
                    let playlist_id =
                        self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                    let playlist_track_id =
                        self.get_id_of_selected(&self.playlist_tracks, Selectable::PlaylistTrack);

                    match self.state.active_tab {
                        ActiveTab::Library => match self.state.active_section {
                            ActiveSection::List => {
                                self.state.artists_search_term = String::from("");
                                self.reposition_cursor(&artist_id, Selectable::Artist);
                            }
                            ActiveSection::Tracks => {
                                self.state.tracks_search_term = String::from("");
                                self.reposition_cursor(&track_id, Selectable::Track);
                            }
                            _ => {}
                        },
                        ActiveTab::Albums => match self.state.active_section {
                            ActiveSection::List => {
                                self.state.albums_search_term = String::from("");
                                self.reposition_cursor(&album_id, Selectable::Album);
                            }
                            ActiveSection::Tracks => {
                                self.state.album_tracks_search_term = String::from("");
                                self.reposition_cursor(&album_track_id, Selectable::AlbumTrack);
                            }
                            _ => {}
                        },
                        ActiveTab::Playlists => match self.state.active_section {
                            ActiveSection::List => {
                                self.state.playlists_search_term = String::from("");
                                self.reposition_cursor(&playlist_id, Selectable::Playlist);
                            }
                            ActiveSection::Tracks => {
                                self.state.playlist_tracks_search_term = String::from("");
                                self.reposition_cursor(
                                    &playlist_track_id,
                                    Selectable::PlaylistTrack,
                                );
                            }
                            _ => {}
                        },
                        _ => {}
                    }

                    return;
                }
                KeyCode::Enter => {
                    match self.state.active_tab {
                        ActiveTab::Library => {
                            self.locally_searching = false;
                            if self.state.active_section == ActiveSection::List {
                                self.state.tracks_search_term = String::from("");
                            }
                        }
                        ActiveTab::Albums => {
                            self.locally_searching = false;
                            if self.state.active_section == ActiveSection::List {
                                self.state.album_tracks_search_term = String::from("");
                            }
                        }
                        ActiveTab::Playlists => {
                            self.locally_searching = false;
                            if self.state.active_section == ActiveSection::List {
                                self.state.playlist_tracks_search_term = String::from("");
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                KeyCode::Backspace => match self.state.active_tab {
                    ActiveTab::Library => match self.state.active_section {
                        ActiveSection::List => {
                            let selected_id =
                                self.get_id_of_selected(&self.artists, Selectable::Artist);
                            self.state.artists_search_term.pop();
                            self.reposition_cursor(&selected_id, Selectable::Artist);
                        }
                        ActiveSection::Tracks => {
                            let selected_id =
                                self.get_id_of_selected(&self.tracks, Selectable::Track);
                            self.state.tracks_search_term.pop();
                            self.reposition_cursor(&selected_id, Selectable::Track);
                        }
                        _ => {}
                    },
                    ActiveTab::Albums => match self.state.active_section {
                        ActiveSection::List => {
                            let selected_id =
                                self.get_id_of_selected(&self.albums, Selectable::Album);
                            self.state.albums_search_term.pop();
                            self.reposition_cursor(&selected_id, Selectable::Album);
                        }
                        ActiveSection::Tracks => {
                            let selected_id =
                                self.get_id_of_selected(&self.album_tracks, Selectable::AlbumTrack);
                            self.state.album_tracks_search_term.pop();
                            self.reposition_cursor(&selected_id, Selectable::AlbumTrack);
                        }
                        _ => {}
                    },
                    ActiveTab::Playlists => match self.state.active_section {
                        ActiveSection::List => {
                            let selected_id =
                                self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                            self.state.playlists_search_term.pop();
                            self.reposition_cursor(&selected_id, Selectable::Playlist);
                        }
                        ActiveSection::Tracks => {
                            let selected_id = self.get_id_of_selected(
                                &self.playlist_tracks,
                                Selectable::PlaylistTrack,
                            );
                            self.state.playlist_tracks_search_term.pop();
                            self.reposition_cursor(&selected_id, Selectable::PlaylistTrack);
                        }
                        _ => {}
                    },
                    _ => {}
                },
                KeyCode::Delete => match self.state.active_tab {
                    ActiveTab::Library => match self.state.active_section {
                        ActiveSection::List => {
                            let selected_id =
                                self.get_id_of_selected(&self.artists, Selectable::Artist);
                            self.state.artists_search_term.clear();
                            self.reposition_cursor(&selected_id, Selectable::Artist);
                        }
                        ActiveSection::Tracks => {
                            let selected_id =
                                self.get_id_of_selected(&self.tracks, Selectable::Track);
                            self.state.tracks_search_term.clear();
                            self.reposition_cursor(&selected_id, Selectable::Track);
                        }
                        _ => {}
                    },
                    ActiveTab::Albums => match self.state.active_section {
                        ActiveSection::List => {
                            let selected_id =
                                self.get_id_of_selected(&self.albums, Selectable::Album);
                            self.state.albums_search_term.clear();
                            self.reposition_cursor(&selected_id, Selectable::Album);
                        }
                        ActiveSection::Tracks => {
                            let selected_id =
                                self.get_id_of_selected(&self.album_tracks, Selectable::AlbumTrack);
                            self.state.album_tracks_search_term.clear();
                            self.reposition_cursor(&selected_id, Selectable::AlbumTrack);
                        }
                        _ => {}
                    },
                    ActiveTab::Playlists => match self.state.active_section {
                        ActiveSection::List => {
                            let selected_id =
                                self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                            self.state.playlists_search_term.clear();
                            self.reposition_cursor(&selected_id, Selectable::Playlist);
                        }
                        ActiveSection::Tracks => {
                            let selected_id = self.get_id_of_selected(
                                &self.playlist_tracks,
                                Selectable::PlaylistTrack,
                            );
                            self.state.playlist_tracks_search_term.clear();
                            self.reposition_cursor(&selected_id, Selectable::PlaylistTrack);
                        }
                        _ => {}
                    },
                    _ => {}
                },
                KeyCode::Char(c) => match self.state.active_tab {
                    ActiveTab::Library => match self.state.active_section {
                        ActiveSection::List => {
                            self.state.artists_search_term.push(c);
                            self.artist_select_by_index(0);
                        }
                        ActiveSection::Tracks => {
                            self.state.tracks_search_term.push(c);
                            self.track_select_by_index(0);
                        }
                        _ => {}
                    },
                    ActiveTab::Albums => match self.state.active_section {
                        ActiveSection::List => {
                            self.state.albums_search_term.push(c);
                            self.album_select_by_index(0);
                        }
                        ActiveSection::Tracks => {
                            self.state.album_tracks_search_term.push(c);
                            self.album_track_select_by_index(0);
                        }
                        _ => {}
                    },
                    ActiveTab::Playlists => match self.state.active_section {
                        ActiveSection::List => {
                            self.state.playlists_search_term.push(c);
                            self.playlist_select_by_index(0);
                        }
                        ActiveSection::Tracks => {
                            self.state.playlist_tracks_search_term.push(c);
                            self.playlist_track_select_by_index(0);
                        }
                        _ => {}
                    },
                    _ => {}
                },
                _ => {}
            }
            return;
        }

        if self.state.active_tab == ActiveTab::Search {
            self.handle_search_tab_events(key_event).await;
            return;
        }

        match key_event.code {
            KeyCode::Char('q') => self.exit().await,
            // Seek backward
            KeyCode::Left => {
                if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    self.preferences.widen_current_pane(&self.state.active_section, false);
                    return;
                }
                self.state.current_playback_state.position = f64::max(
                    0.0, self.state.current_playback_state.position - 5.0,
                );
                self.update_mpris_position(self.state.current_playback_state.position);
                let _ = self.handle_discord(true).await;

                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.command("seek", &["-5.0"]);
                }
            }
            // Seek forward
            KeyCode::Right => {
                if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    self.preferences.widen_current_pane(&self.state.active_section, true);
                    return;
                }
                self.state.current_playback_state.position =
                    f64::min(self.state.current_playback_state.position + 5.0, self.state.current_playback_state.duration);

                self.update_mpris_position(self.state.current_playback_state.position);
                let _ = self.handle_discord(true).await;

                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.command("seek", &["5.0"]);
                }
            }
            KeyCode::Char('h') => {
                if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    self.preferences.widen_current_pane(&self.state.active_section, false);
                    return;
                }
            }
            KeyCode::Char('l') => {
                if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    self.preferences.widen_current_pane(&self.state.active_section, true);
                    return;
                }
            }
            KeyCode::Char(',') => {
                self.state.current_playback_state.position =
                    f64::max(0.0, self.state.current_playback_state.position - 60.0);
                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.command("seek", &["-60.0"]);
                }
                let _ = self.handle_discord(true).await;
            }
            KeyCode::Char('.') => {
                self.state.current_playback_state.position =
                    f64::min(self.state.current_playback_state.duration,
                             self.state.current_playback_state.position + 60.0);
                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.command("seek", &["60.0"]);
                }
                let _ = self.handle_discord(true).await;
            }
            // Previous track
            KeyCode::Char('n') => {
                if let Some(client) = &self.client {
                    let _ = client
                        .stopped(
                            &self.active_song_id,
                            // position ticks
                            (self.state.current_playback_state.position
                                * 10_000_000.0) as u64,
                        )
                        .await;
                }
                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.command("playlist_next", &["force"]);
                }
                self.update_mpris_position(0.0);
            }
            // Next track
            KeyCode::Char('N') => {
                if let Ok(mpv) = self.mpv_state.lock() {
                    let current_time = self.state.current_playback_state.position;
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
                let _ = self.handle_discord(true).await;
            }
            // stop playback
            KeyCode::Char('x') => {
                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.command("stop", &[]);
                    self.state.queue.clear();
                }
            }
            // full state reset
            KeyCode::Char('X') => {
                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv.mpv.command("stop", &[]);
                    self.state.queue.clear();
                }
                self.state = State::new();
                self.state.selected_artist.select_first();
                self.state.selected_track.select_first();
                self.state.selected_playlist.select_first();
                self.state.selected_playlist_track.select_first();
                self.state.selected_album.select_first();
                self.state.selected_album_track.select_first();

                self.state.artists_scroll_state = self
                    .state
                    .artists_scroll_state
                    .content_length(self.artists.len());
                self.state.albums_scroll_state = self
                    .state
                    .albums_scroll_state
                    .content_length(self.albums.len());
                self.state.playlists_scroll_state = self
                    .state
                    .playlists_scroll_state
                    .content_length(self.playlists.len());

                self.tracks.clear();
                self.album_tracks.clear();
                self.playlist_tracks.clear();
                self.paused = true;
            }
            KeyCode::Char('T') => {
                if self.client.is_none() {
                    return;
                }
                self.transcoding.enabled = !self.transcoding.enabled;
                self.preferences.transcoding = self.transcoding.enabled;
                let _ = self.preferences.save();
            }
            // Volume up
            KeyCode::Char('+') => {
                if self.state.current_playback_state.volume >= 500 {
                    return;
                }
                self.state.current_playback_state.volume += 5;
                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv
                        .mpv
                        .set_property("volume", self.state.current_playback_state.volume);
                }
                #[cfg(target_os = "linux")]
                {
                    if let Some(ref mut controls) = self.controls {
                        let _ = controls
                            .set_volume(self.state.current_playback_state.volume as f64 / 100.0);
                    }
                }
            }
            // Volume down
            KeyCode::Char('-') => {
                if self.state.current_playback_state.volume <= 0 {
                    return;
                }
                self.state.current_playback_state.volume -= 5;
                if let Ok(mpv) = self.mpv_state.lock() {
                    let _ = mpv
                        .mpv
                        .set_property("volume", self.state.current_playback_state.volume);
                }
                #[cfg(target_os = "linux")]
                {
                    if let Some(ref mut controls) = self.controls {
                        let _ = controls
                            .set_volume(self.state.current_playback_state.volume as f64 / 100.0);
                    }
                }
            }
            KeyCode::Tab => {
                self.toggle_section(true);
            }
            KeyCode::BackTab => {
                self.toggle_section(false);
            }
            // Move down
            KeyCode::Down | KeyCode::Char('j') => match self.state.active_section {
                ActiveSection::List => {
                    match self.state.active_tab {
                        ActiveTab::Library => {
                            if !self.state.artists_search_term.is_empty() {
                                let items = search_results(
                                    &self.artists,
                                    &self.state.artists_search_term,
                                    false,
                                );
                                let selected = self
                                    .state
                                    .selected_artist
                                    .selected()
                                    .unwrap_or(items.len() - 1);
                                if selected == items.len() - 1 {
                                    self.artist_select_by_index(selected);
                                    return;
                                }
                                self.artist_select_by_index(selected + 1);
                                return;
                            }

                            let selected = self
                                .state
                                .selected_artist
                                .selected()
                                .unwrap_or(self.artists.len() - 1);
                            if selected == self.artists.len() - 1 {
                                self.artist_select_by_index(selected);
                                return;
                            }
                            self.artist_select_by_index(selected + 1);
                        }
                        ActiveTab::Albums => {
                            if !self.state.albums_search_term.is_empty() {
                                let items = search_results(
                                    &self.albums,
                                    &self.state.albums_search_term,
                                    false,
                                );
                                let selected = self
                                    .state
                                    .selected_album
                                    .selected()
                                    .unwrap_or(items.len() - 1);
                                if selected == items.len() - 1 {
                                    self.album_select_by_index(selected);
                                    return;
                                }
                                self.album_select_by_index(selected + 1);
                                return;
                            }

                            let selected = self
                                .state
                                .selected_album
                                .selected()
                                .unwrap_or(self.albums.len() - 1);
                            if selected == self.albums.len() - 1 {
                                self.album_select_by_index(selected);
                                return;
                            }
                            self.album_select_by_index(selected + 1);
                        }
                        ActiveTab::Playlists => {
                            if !self.state.playlists_search_term.is_empty() {
                                let items = search_results(
                                    &self.playlists,
                                    &self.state.playlists_search_term,
                                    false,
                                );
                                let selected = self
                                    .state
                                    .selected_playlist
                                    .selected()
                                    .unwrap_or(items.len() - 1);
                                if selected == items.len() - 1 {
                                    self.playlist_select_by_index(selected);
                                    return;
                                }
                                self.playlist_select_by_index(selected + 1);
                                return;
                            }

                            let selected = self
                                .state
                                .selected_playlist
                                .selected()
                                .unwrap_or(self.playlists.len() - 1);
                            if selected == self.playlists.len() - 1 {
                                self.playlist_select_by_index(selected);
                                return;
                            }
                            self.playlist_select_by_index(selected + 1);
                        }
                        ActiveTab::Search => {
                            // handle_search_tab_events()
                        }
                    }
                }
                ActiveSection::Tracks => {
                    if self.state.active_tab == ActiveTab::Library {
                        if !self.state.tracks_search_term.is_empty() {
                            let items =
                                search_results(&self.tracks, &self.state.tracks_search_term, false);
                            let selected = self
                                .state
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
                            .state
                            .selected_track
                            .selected()
                            .unwrap_or(self.tracks.len() - 1);
                        if selected == self.tracks.len() - 1 {
                            self.track_select_by_index(selected);
                            return;
                        }
                        self.track_select_by_index(selected + 1);
                    }
                    if self.state.active_tab == ActiveTab::Albums {
                        if !self.state.album_tracks_search_term.is_empty() {
                            let items = search_results(
                                &self.album_tracks,
                                &self.state.album_tracks_search_term,
                                false,
                            );
                            let selected = self
                                .state
                                .selected_album_track
                                .selected()
                                .unwrap_or(items.len() - 1);
                            if selected == items.len() - 1 {
                                self.album_track_select_by_index(selected);
                                return;
                            }
                            self.album_track_select_by_index(selected + 1);
                            return;
                        }

                        let selected = self
                            .state
                            .selected_album_track
                            .selected()
                            .unwrap_or(self.album_tracks.len() - 1);
                        if selected == self.album_tracks.len() - 1 {
                            self.album_track_select_by_index(selected);
                            return;
                        }
                        self.album_track_select_by_index(selected + 1);
                    }
                    if self.state.active_tab == ActiveTab::Playlists {
                        if !self.state.playlist_tracks_search_term.is_empty() {
                            let items = search_results(
                                &self.playlist_tracks,
                                &self.state.playlist_tracks_search_term,
                                false,
                            );
                            let selected = self
                                .state
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
                            .state
                            .selected_playlist_track
                            .selected()
                            .unwrap_or(self.playlist_tracks.len() - 1);
                        if selected == self.playlist_tracks.len() - 1 {
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
                    self.state.selected_queue_item_manual_override = true;
                    if self.state.queue.is_empty() {
                        return;
                    }
                    let selected = self.state.selected_queue_item.selected().unwrap_or(0);
                    if selected == self.state.queue.len() - 1 {
                        self.state.selected_queue_item.select(Some(selected));
                        return;
                    }
                    self.state.selected_queue_item.select(Some(selected + 1));
                }
                ActiveSection::Lyrics => {
                    self.state.selected_lyric_manual_override = true;
                    if let Some((_, lyrics_vec, _)) = &self.lyrics {
                        if lyrics_vec.is_empty() {
                            return;
                        }
                        self.state.selected_lyric.select_next();
                    }
                }
                ActiveSection::Popup => {
                    self.popup.selected.select_next();
                }
            },
            KeyCode::Up | KeyCode::Char('k') => match self.state.active_section {
                ActiveSection::List => {
                    match self.state.active_tab {
                        ActiveTab::Library => {
                            let selected = self.state.selected_artist.selected().unwrap_or(0);
                            if selected == 0 {
                                self.artist_select_by_index(selected);
                                return;
                            }
                            self.artist_select_by_index(selected - 1);
                        }
                        ActiveTab::Albums => {
                            let selected = self.state.selected_album.selected().unwrap_or(0);
                            if selected == 0 {
                                self.album_select_by_index(selected);
                                return;
                            }
                            self.album_select_by_index(selected - 1);
                        }
                        ActiveTab::Playlists => {
                            let selected = self.state.selected_playlist.selected().unwrap_or(0);
                            if selected == 0 {
                                self.playlist_select_by_index(selected);
                                return;
                            }
                            self.playlist_select_by_index(selected - 1);
                        }
                        ActiveTab::Search => {
                            // handle_search_tab_events()
                        }
                    }
                }
                ActiveSection::Tracks => match self.state.active_tab {
                    ActiveTab::Library => {
                        let selected = self.state.selected_track.selected().unwrap_or(0);
                        self.track_select_by_index(std::cmp::max(selected as i32 - 1, 0) as usize);
                    }
                    ActiveTab::Albums => {
                        let selected = self.state.selected_album_track.selected().unwrap_or(0);
                        self.album_track_select_by_index(
                            std::cmp::max(selected as i32 - 1, 0) as usize
                        );
                    }
                    ActiveTab::Playlists => {
                        let selected = self.state.selected_playlist_track.selected().unwrap_or(0);
                        self.playlist_track_select_by_index(
                            std::cmp::max(selected as i32 - 1, 0) as usize
                        );
                    }
                    _ => {}
                },
                ActiveSection::Queue => {
                    if key_event.modifiers == KeyModifiers::SHIFT {
                        self.move_queue_item_up().await;
                        return;
                    }
                    self.state.selected_queue_item_manual_override = true;
                    let selected = self.state.selected_queue_item.selected().unwrap_or(0);
                    self.state
                        .selected_queue_item
                        .select(Some(std::cmp::max(selected as i32 - 1, 0) as usize));
                }
                ActiveSection::Lyrics => {
                    self.state.selected_lyric_manual_override = true;
                    self.state.selected_lyric.select_previous();
                }
                ActiveSection::Popup => {
                    self.popup.selected.select_previous();
                }
            },
            KeyCode::PageUp => {
                self.page_up();
            },
            KeyCode::PageDown => {
                self.page_down();
            }
            KeyCode::Char('g') | KeyCode::Home => match self.state.active_section {
                ActiveSection::List => match self.state.active_tab {
                    ActiveTab::Library => {
                        self.artist_select_by_index(0);
                    }
                    ActiveTab::Albums => {
                        self.album_select_by_index(0);
                    }
                    ActiveTab::Playlists => {
                        self.playlist_select_by_index(0);
                    }
                    _ => {}
                },
                ActiveSection::Tracks => match self.state.active_tab {
                    ActiveTab::Library => {
                        if !self.tracks.is_empty() {
                            self.track_select_by_index(0);
                        }
                    }
                    ActiveTab::Albums => {
                        if !self.album_tracks.is_empty() {
                            self.album_track_select_by_index(0);
                        }
                    }
                    ActiveTab::Playlists => {
                        if !self.playlist_tracks.is_empty() {
                            self.playlist_track_select_by_index(0);
                        }
                    }
                    _ => {}
                },
                ActiveSection::Queue => {
                    self.state.selected_queue_item_manual_override = true;
                    self.state.selected_queue_item.select_first();
                }
                ActiveSection::Lyrics => {
                    self.state.selected_lyric_manual_override = true;
                    self.state.selected_lyric.select_first();
                }
                ActiveSection::Popup => {
                    self.popup.selected.select_first();
                }
            },
            KeyCode::Char('G') | KeyCode::End => match self.state.active_section {
                ActiveSection::List => match self.state.active_tab {
                    ActiveTab::Library => {
                        if !self.artists.is_empty() {
                            self.artist_select_by_index(self.artists.len() - 1);
                        }
                    }
                    ActiveTab::Albums => {
                        if !self.albums.is_empty() {
                            self.album_select_by_index(self.albums.len() - 1);
                        }
                    }
                    ActiveTab::Playlists => {
                        if !self.playlists.is_empty() {
                            self.playlist_select_by_index(self.playlists.len() - 1);
                        }
                    }
                    _ => {}
                },
                ActiveSection::Tracks => match self.state.active_tab {
                    ActiveTab::Library => {
                        if !self.tracks.is_empty() {
                            self.track_select_by_index(self.tracks.len() - 1);
                        }
                    }
                    ActiveTab::Albums => {
                        if !self.album_tracks.is_empty() {
                            self.album_track_select_by_index(self.album_tracks.len() - 1);
                        }
                    }
                    ActiveTab::Playlists => {
                        if !self.playlist_tracks.is_empty() {
                            self.playlist_track_select_by_index(self.playlist_tracks.len() - 1);
                        }
                    }
                    _ => {}
                },
                ActiveSection::Queue => {
                    if !self.state.queue.is_empty() {
                        self.state.selected_queue_item_manual_override = true;
                        self.state.selected_queue_item.select_last();
                    }
                }
                ActiveSection::Lyrics => {
                    self.state.selected_lyric_manual_override = true;
                    if let Some((_, lyrics_vec, _)) = &self.lyrics {
                        if !lyrics_vec.is_empty() {
                            self.state.selected_lyric.select_last();
                        }
                    }
                }
                ActiveSection::Popup => {
                    self.popup.selected.select_last();
                }
            },
            KeyCode::Char('a') => match self.state.active_tab {
                ActiveTab::Library => {
                    match self.state.active_section {
                        // first artist with following letter
                        ActiveSection::List => {
                            if self.artists.is_empty() {
                                return;
                            }
                            let ids = search_results(
                                &self.artists,
                                &self.state.artists_search_term,
                                false,
                            );
                            let mut artists = self
                                .artists
                                .iter()
                                .filter(|artist| ids.contains(&artist.id))
                                .collect::<Vec<&Artist>>();
                            if artists.is_empty() {
                                artists = self.artists.iter().collect::<Vec<&Artist>>();
                            }
                            let selected = self.state.selected_artist.selected().unwrap_or(0);
                                let current_artist = sort::strip_article(&artists[selected].name).chars().next().unwrap_or_default().to_ascii_lowercase();
                                let next_artist = artists.iter().skip(selected).find(|a| {
                                    sort::strip_article(&a.name).chars().next().map(|c| c.to_ascii_lowercase()) != Some(current_artist)
                                });
                                if let Some(next_artist) = next_artist {
                                    let index = artists
                                        .iter()
                                        .position(|a| a.id == next_artist.id)
                                        .unwrap_or(0);
                                    self.artist_select_by_index(index);
                                }
                        }
                        // this will go to the first song of the next album
                        ActiveSection::Tracks => {
                            if self.tracks.is_empty() {
                                return;
                            }
                            if let Some(selected) = self.state.selected_track.selected() {
                                let current_album = self.tracks[selected].album_id.clone();
                                let next_album = self.tracks.iter().skip(selected).find(|t| {
                                    t.album_id != current_album && !t.id.starts_with("_album_")
                                });

                                if let Some(next_album) = next_album {
                                    let index = self
                                        .tracks
                                        .iter()
                                        .position(|t| t.album_id == next_album.album_id)
                                        .unwrap_or(0);
                                    self.track_select_by_index(index);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                ActiveTab::Albums => {
                    if matches!(self.state.active_section, ActiveSection::List) {
                        if self.albums.is_empty() {
                            return;
                        }
                        let ids =
                            search_results(&self.albums, &self.state.albums_search_term, false);
                        let mut albums = self
                            .albums
                            .iter()
                            .filter(|album| ids.contains(&album.id))
                            .collect::<Vec<&Album>>();
                        if albums.is_empty() {
                            albums = self.albums.iter().collect::<Vec<&Album>>();
                        }
                        if let Some(selected) = self.state.selected_album.selected() {
                            let current_album = sort::strip_article(&albums[selected].name)
                                .chars().next().map(|c| c.to_ascii_lowercase());

                            if let Some(next_album) = albums.iter().skip(selected).find(|a| {
                                sort::strip_article(&a.name)
                                    .chars().next().map(|c| c.to_ascii_lowercase()) != current_album
                            }) {
                                let index = albums
                                    .iter()
                                    .position(|a| a.id == next_album.id)
                                    .unwrap_or(0);
                                self.album_select_by_index(index);
                            }
                        }
                    }
                }
                ActiveTab::Playlists => {
                    if matches!(self.state.active_section, ActiveSection::List) {
                        if self.playlists.is_empty() {
                            return;
                        }
                        let ids = search_results(
                            &self.playlists,
                            &self.state.playlists_search_term,
                            false,
                        );
                        let mut playlists = self
                            .playlists
                            .iter()
                            .filter(|playlist| ids.contains(&playlist.id))
                            .collect::<Vec<&Playlist>>();
                        if playlists.is_empty() {
                            playlists = self.playlists.iter().collect::<Vec<&Playlist>>();
                        }
                        if let Some(selected) = self.state.selected_playlist.selected() {
                            if let Some(current_playlist) = playlists[selected].name.chars().next()
                            {
                                let current_playlist = current_playlist.to_ascii_lowercase();
                                let next_playlist = playlists.iter().skip(selected).find(|a| {
                                    a.name.chars().next().map(|c| c.to_ascii_lowercase())
                                        != Some(current_playlist)
                                });

                                if let Some(next_playlist) = next_playlist {
                                    let index = playlists
                                        .iter()
                                        .position(|a| a.id == next_playlist.id)
                                        .unwrap_or(0);
                                    self.playlist_select_by_index(index);
                                }
                            }
                        }
                    }
                }
                _ => {}
            },
            KeyCode::Char('A') => match self.state.active_tab {
                ActiveTab::Library => {
                    match self.state.active_section {
                        // first artist with previous letter
                        ActiveSection::List => {
                            if self.artists.is_empty() {
                                return;
                            }
                            let ids = search_results(
                                &self.artists,
                                &self.state.artists_search_term,
                                false,
                            );
                            let mut artists = self
                                .artists
                                .iter()
                                .filter(|artist| ids.contains(&artist.id))
                                .collect::<Vec<&Artist>>();
                            if artists.is_empty() {
                                artists = self.artists.iter().collect::<Vec<&Artist>>();
                            }
                            let selected = self.state.selected_artist.selected().unwrap_or(0);
                            let current_artist = sort::strip_article(&artists[selected].name)
                                .chars().next().map(|c| c.to_ascii_lowercase());
                            let prev_artist = artists
                                .iter().rev().skip(artists.len() - selected)
                                .find(|a| {
                                    sort::strip_article(&a.name)
                                        .chars()
                                        .next()
                                        .map(|c| c.to_ascii_lowercase()) != current_artist
                                });
                            if let Some(prev_artist) = prev_artist {
                                let index = artists
                                    .iter()
                                    .position(|a| a.id == prev_artist.id)
                                    .unwrap_or(0);
                                self.artist_select_by_index(index);
                            }
                        }
                        // this will go to the first song of the previous album
                        ActiveSection::Tracks => {
                            if self.tracks.is_empty() {
                                return;
                            }
                            if let Some(selected) = self.state.selected_track.selected() {
                                let current_album = self.tracks[selected].album_id.clone();
                                let first_track_in_current_album = self
                                    .tracks
                                    .iter()
                                    .position(|t| t.album_id == current_album)
                                    .unwrap_or(0);
                                let prev_album = self
                                    .tracks
                                    .iter()
                                    .rev()
                                    .skip(self.tracks.len() - selected)
                                    .find(|t| {
                                        t.album_id != current_album && !t.id.starts_with("_album_")
                                    });

                                if selected != first_track_in_current_album {
                                    self.track_select_by_index(first_track_in_current_album);
                                    return;
                                }

                                if let Some(prev_album) = prev_album {
                                    let index = self
                                        .tracks
                                        .iter()
                                        .position(|t| t.album_id == prev_album.album_id)
                                        .unwrap_or(0);
                                    self.track_select_by_index(index);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                ActiveTab::Albums => {
                    if matches!(self.state.active_section, ActiveSection::List) {
                        if self.albums.is_empty() {
                            return;
                        }
                        let ids =
                            search_results(&self.albums, &self.state.albums_search_term, false);
                        let mut albums = self
                            .albums
                            .iter()
                            .filter(|album| ids.contains(&album.id))
                            .collect::<Vec<&Album>>();
                        if albums.is_empty() {
                            albums = self.albums.iter().collect::<Vec<&Album>>();
                        }
                        if let Some(selected) = self.state.selected_album.selected() {
                            let current_album = sort::strip_article(&albums[selected].name)
                                .chars().next().map(|c| c.to_ascii_lowercase());

                            let prev_album = albums
                                .iter()
                                .rev()
                                .skip(albums.len() - selected)
                                .find(|a| {
                                    sort::strip_article(&a.name)
                                        .chars().next().map(|c| c.to_ascii_lowercase()) != current_album
                                });
                            if let Some(prev_album) = prev_album {
                                let index = albums
                                    .iter()
                                    .position(|a| a.id == prev_album.id)
                                    .unwrap_or(0);
                                self.album_select_by_index(index);
                            }
                        }
                    }
                }
                ActiveTab::Playlists => {
                    if matches!(self.state.active_section, ActiveSection::List) {
                        if self.playlists.is_empty() {
                            return;
                        }
                        let ids = search_results(
                            &self.playlists,
                            &self.state.playlists_search_term,
                            false,
                        );
                        let mut playlists = self
                            .playlists
                            .iter()
                            .filter(|playlist| ids.contains(&playlist.id))
                            .collect::<Vec<&Playlist>>();
                        if playlists.is_empty() {
                            playlists = self.playlists.iter().collect::<Vec<&Playlist>>();
                        }
                        if let Some(selected) = self.state.selected_playlist.selected() {
                            if let Some(current_playlist) = playlists[selected].name.chars().next()
                            {
                                let current_playlist = current_playlist.to_ascii_lowercase();
                                let prev_playlist = playlists
                                    .iter()
                                    .rev()
                                    .skip(playlists.len() - selected)
                                    .find(|a| {
                                        a.name.chars().next().map(|c| c.to_ascii_lowercase())
                                            != Some(current_playlist)
                                    });

                                if let Some(prev_playlist) = prev_playlist {
                                    let index = playlists
                                        .iter()
                                        .position(|a| a.id == prev_playlist.id)
                                        .unwrap_or(0);
                                    self.playlist_select_by_index(index);
                                }
                            }
                        }
                    }
                }
                _ => {}
            },
            KeyCode::Enter => {
                match self.state.active_section {
                    ActiveSection::List => {
                        if self.state.active_tab == ActiveTab::Library {
                            self.state.tracks_search_term = String::from("");
                            self.state.selected_track.select(Some(0));

                            let search_results = search_results(
                                &self.artists,
                                &self.state.artists_search_term,
                                true,
                            );
                            let artists = search_results
                                .iter()
                                .map(|id| {
                                    self.artists.iter().find(|artist| artist.id == *id).unwrap()
                                })
                                .collect::<Vec<&Artist>>();
                            let selected = self.state.selected_artist.selected().unwrap_or(0);
                            if artists.is_empty() {
                                return;
                            }
                            self.discography(&artists[selected].id.clone()).await;
                        }

                        if self.state.active_tab == ActiveTab::Albums {
                            self.state.album_tracks_search_term = String::from("");
                            self.state.selected_album_track.select(Some(0));

                            let search_results =
                                search_results(&self.albums, &self.state.albums_search_term, true);
                            let albums = search_results
                                .iter()
                                .map(|id| self.albums.iter().find(|album| album.id == *id).unwrap())
                                .collect::<Vec<&Album>>();

                            let selected = self.state.selected_album.selected().unwrap_or(0);
                            if albums.is_empty() {
                                return;
                            }
                            self.album_tracks(&albums[selected].id.clone()).await;
                        }

                        if self.state.active_tab == ActiveTab::Playlists {
                            self.open_playlist(true).await;
                        }
                    }
                    ActiveSection::Tracks => {
                        let items = match self.state.active_tab {
                            ActiveTab::Library => {
                                let ids = search_results(
                                    &self.tracks,
                                    &self.state.tracks_search_term,
                                    true,
                                );
                                let items = ids
                                    .iter()
                                    .map(|id| self.tracks.iter().find(|t| t.id == *id).unwrap())
                                    .cloned()
                                    .collect();
                                items
                            }
                            ActiveTab::Albums => {
                                let ids = search_results(
                                    &self.album_tracks,
                                    &self.state.album_tracks_search_term,
                                    true,
                                );
                                let items = ids
                                    .iter()
                                    .map(|id| {
                                        self.album_tracks.iter().find(|t| t.id == *id).unwrap()
                                    })
                                    .cloned()
                                    .collect();
                                items
                            }
                            ActiveTab::Playlists => {
                                let ids = search_results(
                                    &self.playlist_tracks,
                                    &self.state.playlist_tracks_search_term,
                                    false,
                                );
                                let items: Vec<crate::client::DiscographySong> = self
                                    .playlist_tracks
                                    .iter()
                                    .filter(|t| ids.contains(&t.id) || ids.is_empty())
                                    .cloned()
                                    .collect();
                                items
                            }
                            _ => vec![],
                        };

                        if items.is_empty() {
                            return;
                        }

                        let selected = match self.state.active_tab {
                            ActiveTab::Library => self.state.selected_track.selected().unwrap_or(0),
                            ActiveTab::Albums => {
                                self.state.selected_album_track.selected().unwrap_or(0)
                            }
                            ActiveTab::Playlists => {
                                self.state.selected_playlist_track.selected().unwrap_or(0)
                            }
                            _ => 0,
                        };

                        if key_event.modifiers == KeyModifiers::CONTROL {
                            self.push_next_to_temporary_queue(&items, selected).await;
                            return;
                        }
                        if key_event.modifiers == KeyModifiers::SHIFT {
                            self.push_to_temporary_queue(&items, selected, 1).await;
                            return;
                        }
                        self.initiate_main_queue(&items, selected).await;
                    }
                    ActiveSection::Queue => {
                        self.relocate_queue_and_play().await;
                    }
                    ActiveSection::Lyrics => {
                        // jump to that timestamp
                        if let Some((_, lyrics_vec, _)) = &self.lyrics {
                            let selected = self.state.selected_lyric.selected().unwrap_or(0);

                            if let Some(lyric) = lyrics_vec.get(selected) {
                                let time = lyric.start as f64 / 10_000_000.0;

                                if time != 0.0 {
                                    if let Ok(mpv) = self.mpv_state.lock() {
                                        let _ = mpv
                                            .mpv
                                            .command("seek", &[&time.to_string(), "absolute"]);
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
                let items = match self.state.active_tab {
                    ActiveTab::Library => {
                        let ids =
                            search_results(&self.tracks, &self.state.tracks_search_term, true);
                        let items = ids
                            .iter()
                            .map(|id| self.tracks.iter().find(|t| t.id == *id).unwrap())
                            .cloned()
                            .collect();
                        items
                    }
                    ActiveTab::Albums => {
                        let ids = search_results(
                            &self.album_tracks,
                            &self.state.album_tracks_search_term,
                            true,
                        );
                        let items = ids
                            .iter()
                            .map(|id| self.album_tracks.iter().find(|t| t.id == *id).unwrap())
                            .cloned()
                            .collect();
                        items
                    }
                    ActiveTab::Playlists => {
                        let ids = search_results(
                            &self.playlist_tracks,
                            &self.state.playlist_tracks_search_term,
                            false,
                        );
                        let items: Vec<crate::client::DiscographySong> = self
                            .playlist_tracks
                            .iter()
                            .filter(|t| ids.contains(&t.id) || ids.is_empty())
                            .cloned()
                            .collect();
                        items
                    }
                    _ => vec![],
                };

                if items.is_empty() {
                    return;
                }

                let selected = match self.state.active_tab {
                    ActiveTab::Library => self.state.selected_track.selected().unwrap_or(0),
                    ActiveTab::Albums => self.state.selected_album_track.selected().unwrap_or(0),
                    ActiveTab::Playlists => self.state.selected_playlist_track.selected().unwrap_or(0),
                    _ => 0,
                };

                if key_event.modifiers == KeyModifiers::CONTROL {
                    self.push_next_to_temporary_queue(&items, selected).await;
                    return;
                }
                self.push_to_temporary_queue(&items, selected, 1).await;
            }
            // mark as favorite (works on anything)
            KeyCode::Char('f') => match self.state.active_section {
                ActiveSection::List => {
                    if let Some(client) = &self.client {
                        match self.state.active_tab {
                            ActiveTab::Library => {
                                let id = self.get_id_of_selected(&self.artists, Selectable::Artist);
                                if let Some(artist) =
                                    self.original_artists.iter_mut().find(|a| a.id == id)
                                {
                                    let _ = client
                                        .set_favorite(&artist.id, !artist.user_data.is_favorite)
                                        .await;
                                    let _ = set_favorite_artist(&self.db.pool, &artist.id, !artist.user_data.is_favorite).await;
                                    artist.user_data.is_favorite = !artist.user_data.is_favorite;
                                    self.reorder_lists();
                                    self.reposition_cursor(&id, Selectable::Artist);
                                }
                            }
                            ActiveTab::Albums => {
                                let id = self.get_id_of_selected(&self.albums, Selectable::Album);
                                if let Some(album) =
                                    self.original_albums.iter_mut().find(|a| a.id == id)
                                {
                                    let _ = client
                                        .set_favorite(&album.id, !album.user_data.is_favorite)
                                        .await;

                                    let _ = set_favorite_album(&self.db.pool, &album.id, !album.user_data.is_favorite).await;
                                    album.user_data.is_favorite = !album.user_data.is_favorite;
                                    self.reorder_lists();
                                    self.reposition_cursor(&id, Selectable::Album);
                                }
                                if let Some(album) = self
                                    .tracks
                                    .iter_mut()
                                    .find(|a| a.id == format!("_album_{}", id))
                                {
                                    album.user_data.is_favorite = !album.user_data.is_favorite;
                                }
                            }
                            ActiveTab::Playlists => {
                                let id =
                                    self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                                if let Some(playlist) =
                                    self.original_playlists.iter_mut().find(|a| a.id == id)
                                {
                                    let _ = client
                                        .set_favorite(&playlist.id, !playlist.user_data.is_favorite)
                                        .await;
                                    let _ = set_favorite_playlist(&self.db.pool, &playlist.id, !playlist.user_data.is_favorite).await;
                                    playlist.user_data.is_favorite =
                                        !playlist.user_data.is_favorite;
                                    self.reorder_lists();
                                    self.reposition_cursor(&id, Selectable::Playlist);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                ActiveSection::Tracks => {
                    if let Some(client) = &self.client {
                        match self.state.active_tab {
                            ActiveTab::Library => {
                                let id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                                    let _ = client
                                        .set_favorite(&track.id, !track.user_data.is_favorite)
                                        .await;
                                    let _ = set_favorite_track(&self.db.pool, &track.id, !track.user_data.is_favorite).await;
                                    track.user_data.is_favorite = !track.user_data.is_favorite;
                                    if let Some(tr) =
                                        self.state.queue.iter_mut().find(|t| t.id == track.id)
                                    {
                                        tr.is_favorite = !tr.is_favorite;
                                    }
                                    if track.id.starts_with("_album_") {
                                        let id = track.id.replace("_album_", "");
                                        if let Some(album) =
                                            self.albums.iter_mut().find(|a| a.id == id)
                                        {
                                            album.user_data.is_favorite =
                                                !album.user_data.is_favorite;
                                        }
                                        let _ = set_favorite_album(&self.db.pool, &id, !track.user_data.is_favorite).await;
                                        if let Some(album) =
                                            self.original_albums.iter_mut().find(|a| a.id == id)
                                        {
                                            album.user_data.is_favorite =
                                                !album.user_data.is_favorite;
                                        }
                                        self.reorder_lists();
                                    }
                                }
                            }
                            ActiveTab::Albums => {
                                let id = self
                                    .get_id_of_selected(&self.album_tracks, Selectable::AlbumTrack);
                                if let Some(track) =
                                    self.album_tracks.iter_mut().find(|t| t.id == id)
                                {
                                    let _ = client
                                        .set_favorite(&track.id, !track.user_data.is_favorite)
                                        .await;
                                    let _ = set_favorite_track(&self.db.pool, &track.id, !track.user_data.is_favorite).await;
                                    track.user_data.is_favorite = !track.user_data.is_favorite;
                                    if let Some(tr) =
                                        self.state.queue.iter_mut().find(|t| t.id == track.id)
                                    {
                                        tr.is_favorite = !tr.is_favorite;
                                    }
                                }
                            }
                            ActiveTab::Playlists => {
                                let id = self.get_id_of_selected(
                                    &self.playlist_tracks,
                                    Selectable::PlaylistTrack,
                                );
                                if let Some(track) =
                                    self.playlist_tracks.iter_mut().find(|t| t.id == id)
                                {
                                    let _ = client
                                        .set_favorite(&track.id, !track.user_data.is_favorite)
                                        .await;
                                    let _ = set_favorite_track(&self.db.pool, &track.id, !track.user_data.is_favorite).await;
                                    track.user_data.is_favorite = !track.user_data.is_favorite;
                                    if let Some(tr) =
                                        self.state.queue.iter_mut().find(|t| t.id == track.id)
                                    {
                                        tr.is_favorite = !tr.is_favorite;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                ActiveSection::Queue => {
                    if let Some(client) = &self.client {
                        let selected = self.state.selected_queue_item.selected().unwrap_or(0);
                        let track = &self.state.queue[selected].clone();
                        let _ = client.set_favorite(&track.id, !track.is_favorite).await;
                        self.state.queue[selected].is_favorite = !track.is_favorite;
                        if let Some(tr) = self.tracks.iter_mut().find(|t| t.id == track.id) {
                            tr.user_data.is_favorite = !track.is_favorite;
                        }
                    }
                }
                _ => {}
            },
            KeyCode::Char('d') => {
                match self.state.active_section {
                    ActiveSection::Tracks => match self.state.active_tab {
                        ActiveTab::Library => {
                            let id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                            if id.starts_with("_album_") {
                                let album_id = id.replace("_album_", "");
                                let album_tracks = self
                                    .tracks
                                    .iter()
                                    .filter(|t| t.album_id == album_id)
                                    .cloned()
                                    .collect::<Vec<DiscographySong>>();

                                // if all are downloaded, delete the album. Otherwise download every track
                                if album_tracks.iter().any(|ds| {
                                    self.tracks
                                        .iter()
                                        .find(|t| t.id == ds.id)
                                        .map(|t| matches!(t.download_status, DownloadStatus::NotDownloaded))
                                        == Some(true)
                                }) {
                                    let _ = self.db.cmd_tx
                                        .send(Command::Download(DownloadCommand::Tracks {
                                            tracks: album_tracks.into_iter()
                                                .filter(|t| !matches!(t.download_status, DownloadStatus::Downloaded))
                                                .collect::<Vec<DiscographySong>>()
                                        }))
                                        .await;
                                } else {
                                    let _ = self.db.cmd_tx
                                        .send(Command::Delete(DeleteCommand::Tracks {
                                            tracks: album_tracks.clone(),
                                        }))
                                        .await;
                                    if self.client.is_none() {
                                        for track in album_tracks {
                                            self.tracks.retain(|t| t.id != track.id);
                                            self.album_tracks.retain(|t| t.id != track.id);
                                            self.playlist_tracks.retain(|t| t.id != track.id);
                                            let _ = self.remove_from_queue_by_id(track.id).await;
                                        }
                                    }
                                }
                            } else {
                                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                                    match track.download_status {
                                        DownloadStatus::NotDownloaded => {
                                            let _ = self.db.cmd_tx
                                                .send(Command::Download(DownloadCommand::Track {
                                                    track: track.clone(),
                                                    playlist_id: None,
                                                }))
                                                .await;
                                        }
                                        _ => {
                                            track.download_status = DownloadStatus::NotDownloaded;
                                            let _ = self.db.cmd_tx
                                                .send(Command::Delete(DeleteCommand::Track {
                                                    track: track.clone(),
                                                }))
                                                .await;
                                            // if offline we need to remove the track from the list
                                            if self.client.is_none() {
                                                self.tracks.retain(|t| t.id != id);
                                                self.album_tracks.retain(|t| t.id != id);
                                                self.playlist_tracks.retain(|t| t.id != id);
                                                let _ = self.remove_from_queue_by_id(id).await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        ActiveTab::Albums => {
                            let id = self.get_id_of_selected(&self.album_tracks, Selectable::AlbumTrack);
                            if let Some(track) = self.album_tracks.iter_mut().find(|t| t.id == id) {
                                match track.download_status {
                                    DownloadStatus::NotDownloaded => {
                                        let _ = self.db.cmd_tx
                                            .send(Command::Download(DownloadCommand::Track {
                                                track: track.clone(),
                                                playlist_id: None,
                                            }))
                                            .await;
                                    }
                                    _ => {
                                        track.download_status = DownloadStatus::NotDownloaded;
                                        let _ = self.db.cmd_tx
                                            .send(Command::Delete(DeleteCommand::Track {
                                                track: track.clone(),
                                            }))
                                            .await;
                                        if self.client.is_none() {
                                            self.tracks.retain(|t| t.id != id);
                                            self.album_tracks.retain(|t| t.id != id);
                                            self.playlist_tracks.retain(|t| t.id != id);
                                            let _ = self.remove_from_queue_by_id(id).await;
                                        }
                                    }
                                }
                            }
                        }
                        ActiveTab::Playlists => {
                            let id = self.get_id_of_selected(&self.playlist_tracks, Selectable::PlaylistTrack);
                            if let Some(track) = self.playlist_tracks.iter_mut().find(|t| t.id == id) {
                                match track.download_status {
                                    DownloadStatus::NotDownloaded => {
                                        let _ = self.db.cmd_tx
                                            .send(Command::Download(DownloadCommand::Track {
                                                track: track.clone(),
                                                playlist_id: Some(self.state.current_playlist.id.clone()),
                                            }))
                                            .await;
                                    }
                                    _ => {
                                        track.download_status = DownloadStatus::NotDownloaded;
                                        let _ = self.db.cmd_tx
                                            .send(Command::Delete(DeleteCommand::Track {
                                                track: track.clone(),
                                            }))
                                            .await;
                                        if self.client.is_none() {
                                            self.playlist_tracks.retain(|t| t.id != id);
                                            self.tracks.retain(|t| t.id != id);
                                            self.album_tracks.retain(|t| t.id != id);
                                            let _ = self.remove_from_queue_by_id(id).await;
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
                // let's move that retaining logic here for all of them
                let album_order = crate::helpers::extract_album_order(&self.tracks);
                self.tracks = self.group_tracks_into_albums(self.tracks.clone(), Some(album_order));
                if self.tracks.is_empty() {
                    self.artists.retain(|t| t.id != self.state.current_artist.id);
                    self.original_artists.retain(|t| t.id != self.state.current_artist.id);
                }
                if self.album_tracks.is_empty() {
                    self.albums.retain(|t| t.id != self.state.current_album.id);
                    self.original_albums.retain(|t| t.id != self.state.current_album.id);
                }
                if self.playlist_tracks.is_empty() {
                    self.playlists.retain(|t| t.id != self.state.current_playlist.id);
                    self.original_playlists.retain(|t| t.id != self.state.current_playlist.id);
                }
                if self.tracks.is_empty() && self.album_tracks.is_empty() && self.playlist_tracks.is_empty() {
                    self.state.active_section = ActiveSection::List;
                    self.state.active_tab = ActiveTab::Library;
                    self.state.selected_artist.select(Some(0));
                    self.state.selected_album.select(Some(0));
                    self.state.selected_playlist.select(Some(0));
                }
                return;
            }
            KeyCode::Char('y') => {
                if !(self.artists_stale || self.albums_stale || self.playlists_stale) {
                    return;
                }
                self.original_artists = get_all_artists(&self.db.pool).await.unwrap_or_default();
                self.original_albums = get_all_albums(&self.db.pool).await.unwrap_or_default();
                self.original_playlists = get_all_playlists(&self.db.pool).await.unwrap_or_default();
                self.artists_stale = false;
                self.albums_stale = false;
                self.playlists_stale = false;
                self.reorder_lists();
            }
            KeyCode::Char('r') => {
                if let Ok(mpv) = self.mpv_state.lock() {
                    match self.preferences.repeat {
                        Repeat::None => {
                            self.preferences.repeat = Repeat::All;
                            let _ = mpv.mpv.set_property("loop-playlist", "inf");
                        }
                        Repeat::All => {
                            self.preferences.repeat = Repeat::One;
                            let _ = mpv.mpv.set_property("loop-playlist", "no");
                            let _ = mpv.mpv.set_property("loop-file", "inf");
                        }
                        Repeat::One => {
                            self.preferences.repeat = Repeat::None;
                            let _ = mpv.mpv.set_property("loop-file", "no");
                            let _ = mpv.mpv.set_property("loop-playlist", "no");
                        }
                    }
                    let _ = self.preferences.save();
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.popup.global = key_event.code == KeyCode::Char('P');

                if self.state.active_section == ActiveSection::Popup {
                    self.state.active_section = self.state.last_section;
                    self.popup.current_menu = None;
                } else {
                    self.state.last_section = self.state.active_section;
                    self.state.active_section = ActiveSection::Popup;
                }
            }
            KeyCode::Delete => {
                if self.state.active_section != ActiveSection::Queue {
                    return;
                }
                self.pop_from_queue().await;
            }
            KeyCode::Char('s') => {
                if key_event.modifiers == KeyModifiers::CONTROL {
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
                    return;
                }
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
            KeyCode::Char('E') => {
                self.clear_queue().await;
            }
            KeyCode::Char('J') => {
                if self.state.active_section == ActiveSection::Queue {
                    self.move_queue_item_down().await;
                }
            }
            KeyCode::Char('K') => {
                if self.state.active_section == ActiveSection::Queue {
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
                let album_id = self.get_id_of_selected(&self.albums, Selectable::Album);
                let album_track_id =
                    self.get_id_of_selected(&self.album_tracks, Selectable::AlbumTrack);
                let track_id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                let playlist_id = self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                let playlist_track_id =
                    self.get_id_of_selected(&self.playlist_tracks, Selectable::PlaylistTrack);

                match self.state.active_tab {
                    ActiveTab::Library => match self.state.active_section {
                        ActiveSection::List => {
                            self.state.artists_search_term = String::from("");
                            self.reposition_cursor(&artist_id, Selectable::Artist);
                        }
                        ActiveSection::Tracks => {
                            self.state.tracks_search_term = String::from("");
                            self.reposition_cursor(&track_id, Selectable::Track);
                        }
                        _ => {}
                    },
                    ActiveTab::Albums => match self.state.active_section {
                        ActiveSection::List => {
                            self.state.albums_search_term = String::from("");
                            self.reposition_cursor(&album_id, Selectable::Album);
                        }
                        ActiveSection::Tracks => {
                            self.state.album_tracks_search_term = String::from("");
                            self.reposition_cursor(&album_track_id, Selectable::AlbumTrack);
                        }
                        _ => {}
                    },
                    ActiveTab::Playlists => match self.state.active_section {
                        ActiveSection::List => {
                            self.state.playlists_search_term = String::from("");
                            self.reposition_cursor(&playlist_id, Selectable::Playlist);
                        }
                        ActiveSection::Tracks => {
                            self.state.playlist_tracks_search_term = String::from("");
                            self.reposition_cursor(&playlist_track_id, Selectable::PlaylistTrack);
                        }
                        ActiveSection::Popup => {
                            self.state.active_section = self.state.last_section;
                        }
                        _ => {}
                    },
                    ActiveTab::Search => {
                        self.searching = false;
                        self.search_term = String::from("");
                        self.state.active_tab = ActiveTab::Library;
                    }
                }
            }
            KeyCode::F(1) | KeyCode::Char('1') => {
                self.state.active_tab = ActiveTab::Library;
                if self.tracks.is_empty() {
                    self.state.active_section = ActiveSection::List;
                }
            }
            KeyCode::F(2) | KeyCode::Char('2') => {
                self.state.active_tab = ActiveTab::Albums;
                if self.album_tracks.is_empty() {
                    self.state.active_section = ActiveSection::List;
                }
            }
            KeyCode::F(3) | KeyCode::Char('3') => {
                self.state.active_tab = ActiveTab::Playlists;
                if self.playlist_tracks.is_empty() {
                    self.state.active_section = ActiveSection::List;
                }
            }
            KeyCode::F(4) | KeyCode::Char('4') => {
                self.state.active_tab = ActiveTab::Search;
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
                self.state.active_tab = ActiveTab::Library;
            }
            KeyCode::F(2) => {
                self.state.active_tab = ActiveTab::Albums;
                if self.playlist_tracks.is_empty() {
                    self.state.active_section = ActiveSection::List;
                }
            }
            KeyCode::F(3) => {
                self.state.active_tab = ActiveTab::Playlists;
                if self.playlist_tracks.is_empty() {
                    self.state.active_section = ActiveSection::List;
                }
            }
            KeyCode::F(4) => {
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
                self.global_search().await;
            }
            _ => {
                if self.searching {
                    if let KeyCode::Char(c) = key_event.code {
                        self.search_term.push(c);
                    }
                    return;
                }
                match key_event.code {
                    KeyCode::Char('1') => {
                        self.state.active_tab = ActiveTab::Library;
                    }
                    KeyCode::Char('2') => {
                        self.state.active_tab = ActiveTab::Albums;
                    }
                    KeyCode::Char('3') => {
                        self.state.active_tab = ActiveTab::Playlists;
                    }
                    KeyCode::Char('4') => {
                        self.searching = true;
                    }
                    KeyCode::Down | KeyCode::Char('j') => match self.state.search_section {
                        SearchSection::Artists => {
                            self.state.selected_search_artist.select_next();
                            self.state.search_artist_scroll_state.next();
                        }
                        SearchSection::Albums => {
                            self.state.selected_search_album.select_next();
                            self.state.search_album_scroll_state.next();
                        }
                        SearchSection::Tracks => {
                            self.state.selected_search_track.select_next();
                            self.state.search_track_scroll_state.next();
                        }
                    },
                    KeyCode::Up | KeyCode::Char('k') => match self.state.search_section {
                        SearchSection::Artists => {
                            self.state.selected_search_artist.select_previous();
                            self.state.search_artist_scroll_state.prev();
                        }
                        SearchSection::Albums => {
                            self.state.selected_search_album.select_previous();
                            self.state.search_album_scroll_state.prev();
                        }
                        SearchSection::Tracks => {
                            self.state.selected_search_track.select_previous();
                            self.state.search_track_scroll_state.prev();
                        }
                    },
                    KeyCode::Char('g') | KeyCode::Home => match self.state.search_section {
                        SearchSection::Artists => {
                            self.state.selected_search_artist.select_first();
                            self.state.search_artist_scroll_state.first();
                        }
                        SearchSection::Albums => {
                            self.state.selected_search_album.select_first();
                            self.state.search_album_scroll_state.first();
                        }
                        SearchSection::Tracks => {
                            self.state.selected_search_track.select_first();
                            self.state.search_track_scroll_state.first();
                        }
                    },
                    KeyCode::Char('G') | KeyCode::End => match self.state.search_section {
                        SearchSection::Artists => {
                            self.state.selected_search_artist.select_last();
                            self.state.search_artist_scroll_state.last();
                        }
                        SearchSection::Albums => {
                            self.state.selected_search_album.select_last();
                            self.state.search_album_scroll_state.last();
                        }
                        SearchSection::Tracks => {
                            self.state.selected_search_track.select_last();
                            self.state.search_track_scroll_state.last();
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
        // self.dirty = true;
    }

    fn toggle_section(&mut self, forwards: bool) {

        let has_lyrics = self
            .lyrics
            .as_ref()
            .is_some_and(|(_, l, _)| !l.is_empty());

        match forwards {
            true => match self.state.active_section {
                ActiveSection::List => self.state.active_section = ActiveSection::Tracks,
                ActiveSection::Tracks => self.state.active_section = ActiveSection::List,
                ActiveSection::Queue => {
                    match self.state.last_section {
                        ActiveSection::List => self.state.active_section = ActiveSection::List,
                        ActiveSection::Tracks => self.state.active_section = ActiveSection::Tracks,
                        _ => self.state.active_section = ActiveSection::List,
                    }
                    self.state.last_section = ActiveSection::Queue;
                    self.state.selected_queue_item_manual_override = false;
                }
                ActiveSection::Lyrics => {
                    match self.state.last_section {
                        ActiveSection::List => self.state.active_section = ActiveSection::List,
                        ActiveSection::Tracks => self.state.active_section = ActiveSection::Tracks,
                        _ => self.state.active_section = ActiveSection::List,
                    }
                    self.state.last_section = ActiveSection::Lyrics;
                    self.state.selected_lyric_manual_override = false;
                }
                _ => {}
            },
            false => match self.state.active_section {
                ActiveSection::List => {
                    self.state.last_section = ActiveSection::List;
                    self.state.active_section = if has_lyrics {
                        ActiveSection::Lyrics
                    } else {
                        ActiveSection::Queue
                    };
                }
                ActiveSection::Tracks => {
                    self.state.last_section = ActiveSection::Tracks;
                    self.state.active_section = if has_lyrics {
                        ActiveSection::Lyrics
                    } else {
                        ActiveSection::Queue
                    };
                }
                ActiveSection::Lyrics => {
                    self.state.selected_lyric_manual_override = false;
                    self.state.active_section = ActiveSection::Queue;
                }
                ActiveSection::Queue => {
                    self.state.selected_queue_item_manual_override = false;
                    self.state.active_section = if has_lyrics {
                        ActiveSection::Lyrics
                    } else {
                        match self.state.last_section {
                            ActiveSection::Tracks => ActiveSection::Tracks,
                            ActiveSection::List   => ActiveSection::List,
                            _ => ActiveSection::List,
                        }
                    };
                }
                _ => {}
            },
        }
    }

    fn page_up(&mut self) {
        match (self.state.active_section, self.state.active_tab) {
            (ActiveSection::List, ActiveTab::Library) => {
                page_up_list(
                    self.artists.len(), self.left_list_height,
                    &mut self.state.selected_artist, &mut self.state.artists_scroll_state
                );
            }
            (ActiveSection::List, ActiveTab::Albums) => {
                page_up_list(
                    self.albums.len(), self.left_list_height,
                    &mut self.state.selected_album, &mut self.state.albums_scroll_state
                );
            }
            (ActiveSection::List, ActiveTab::Playlists) => {
                page_up_list(
                    self.playlists.len(), self.left_list_height,
                    &mut self.state.selected_playlist, &mut self.state.playlists_scroll_state
                );
            }
            (ActiveSection::Tracks, ActiveTab::Library) => {
                page_up_table(
                    self.tracks.len(), self.track_list_height,
                    &mut self.state.selected_track, &mut self.state.tracks_scroll_state,
                );
            }
            (ActiveSection::Tracks, ActiveTab::Albums) => {
                page_up_table(
                    self.album_tracks.len(), self.track_list_height,
                    &mut self.state.selected_album_track, &mut self.state.album_tracks_scroll_state,
                );
            }
            (ActiveSection::Tracks, ActiveTab::Playlists) => {
                page_up_table(
                    self.playlist_tracks.len(), self.track_list_height,
                    &mut self.state.selected_playlist_track, &mut self.state.playlist_tracks_scroll_state,
                );
            }
            _ => {}
        }
        self.dirty = true;
    }

    fn page_down(&mut self) {
        match (self.state.active_section, self.state.active_tab) {
            (ActiveSection::List, ActiveTab::Library) => {
                page_down_list(
                    self.artists.len(), self.left_list_height,
                    &mut self.state.selected_artist, &mut self.state.artists_scroll_state
                );
            }
            (ActiveSection::List, ActiveTab::Albums) => {
                page_down_list(
                    self.albums.len(), self.left_list_height,
                    &mut self.state.selected_album, &mut self.state.albums_scroll_state
                );
            }
            (ActiveSection::List, ActiveTab::Playlists) => {
                page_down_list(
                    self.playlists.len(), self.left_list_height,
                    &mut self.state.selected_playlist, &mut self.state.playlists_scroll_state
                );
            }
            (ActiveSection::Tracks, ActiveTab::Library) => {
                page_down_table(
                    self.tracks.len(), self.track_list_height,
                    &mut self.state.selected_track, &mut self.state.tracks_scroll_state
                );
            }
            (ActiveSection::Tracks, ActiveTab::Albums) => {
                page_down_table(
                    self.album_tracks.len(), self.track_list_height,
                    &mut self.state.selected_album_track, &mut self.state.album_tracks_scroll_state
                );
            }
            (ActiveSection::Tracks, ActiveTab::Playlists) => {
                page_down_table(
                    self.playlist_tracks.len(), self.track_list_height,
                    &mut self.state.selected_playlist_track, &mut self.state.playlist_tracks_scroll_state
                );
            }

            _ => {}
        }
        self.dirty = true;
    }

    /// Opens the playlist with the given ID.
    /// limit: if true, the playlist will be opened with a limit on the number of tracks and fetched fully with a delay
    ///
    pub async fn open_playlist(&mut self, limit: bool) {
        self.state.playlist_tracks_search_term = String::from("");
        self.state.selected_playlist_track.select(Some(0));

        // if we are searching we need to account of the list index offsets caused by the search
        if !self.state.playlists_search_term.is_empty() {
            let ids = search_results(
                &self.playlists,
                &self.state.playlists_search_term,
                false,
            );
            if ids.is_empty() {
                return;
            }
            let selected = self.state.selected_playlist.selected().unwrap_or(0);
            self.playlist(&ids[selected], limit).await;
            let _ = self
                .state
                .playlist_tracks_scroll_state
                .content_length(self.playlist_tracks.len() - 1);
            return;
        }
        let selected = self.state.selected_playlist.selected().unwrap_or(0);
        if self.playlists.is_empty() {
            return;
        }
        self.playlist(&self.playlists[selected].id.clone(), limit).await;
        let _ = self
            .state
            .playlist_tracks_scroll_state
            .content_length(self.playlist_tracks.len() - 1);
    }

    async fn global_search(&mut self) {
        if self.searching {
            self.global_search_perform().await;
            return;
        }

        // if not searching, we just go to the artist/etc we selected
        match self.state.search_section {
            SearchSection::Artists => {
                let artist = match self
                    .search_result_artists
                    .get(self.state.selected_search_artist.selected().unwrap_or(0))
                {
                    Some(artist) => artist,
                    None => return,
                };
                let artist_id = artist.id.clone();

                // in the Music tab, select this artist
                self.state.active_tab = ActiveTab::Library;
                self.state.active_section = ActiveSection::List;
                self.artist_select_by_index(0);

                // find the artist in the artists list using .id
                let artist = self.artists.iter().find(|a| a.id == artist_id);

                if let Some(art) = artist {
                    let index = self
                        .artists
                        .iter()
                        .position(|a| a.id == art.id)
                        .unwrap_or(0);
                    self.artist_select_by_index(index);

                    let selected = self.state.selected_artist.selected().unwrap_or(0);
                    self.discography(&self.artists[selected].id.clone()).await;
                    self.track_select_by_index(0);
                }
            }
            SearchSection::Albums => {
                let album = match self
                    .search_result_albums
                    .get(self.state.selected_search_album.selected().unwrap_or(0))
                {
                    Some(album) => album,
                    None => return,
                };

                // in the Music tab, select this artist
                self.state.active_tab = ActiveTab::Library;
                self.state.active_section = ActiveSection::List;
                let album_id = album.id.clone();

                if album.album_artists.is_empty() {
                    return;
                }
                let mut artist_id = String::from("");
                for artist in &album.album_artists {
                    if self.original_artists.iter().any(|a| a.id == artist.id) {

                        let discography = match get_discography(&self.db.pool, &artist.id, self.client.as_ref()).await {
                            Ok(tracks) if !tracks.is_empty() => Some(tracks),
                            _ => if let Some(client) = self.client.as_ref() {
                                if let Ok(tracks) = client.discography(&artist.id).await {
                                    Some(tracks)
                                } else { None }
                            } else { None }
                        };
                        if let Some(discography) = discography {
                            if let Some(_) =
                                discography.iter().find(|t| t.id == album_id)
                            {
                                artist_id = artist.id.clone();
                                break;
                            }
                        }
                    }
                }

                if artist_id.is_empty() {
                    // if this fails, let's last attempt to find the artist by name
                    for artist in &album.album_artists {
                        if let Some(a) =
                            self.original_artists.iter().find(|a| a.name == artist.name)
                        {
                            artist_id = a.id.clone();
                            break;
                        }
                    }
                    if artist_id.is_empty() {
                        return;
                    }
                }

                let index = self
                    .artists
                    .iter()
                    .position(|a| a.id == artist_id)
                    .unwrap_or(0);

                self.artist_select_by_index(index);
                let selected = self.state.selected_artist.selected().unwrap_or(0);
                self.discography(&self.artists[selected].id.clone()).await;
                self.track_select_by_index(0);

                // now find the first track that matches this album
                if let Some(track) = self.tracks.iter().find(|t| t.album_id == album_id) {
                    let index = self
                        .tracks
                        .iter()
                        .position(|t| t.id == track.id)
                        .unwrap_or(0);
                    self.track_select_by_index(index);
                }
            }
            SearchSection::Tracks => {
                let track = match self
                    .search_result_tracks
                    .get(self.state.selected_search_track.selected().unwrap_or(0))
                {
                    Some(track) => track,
                    None => return,
                };

                // in the Music tab, select this artist
                self.state.active_tab = ActiveTab::Library;
                self.state.active_section = ActiveSection::List;

                let track_id = track.id.clone();
                let album_artists = track.album_artists.clone();
                if album_artists.is_empty() {
                    return;
                }
                let mut artist_id = String::from("");
                for artist in album_artists.clone() {
                    if self.original_artists.iter().any(|a| a.id == artist.id) {
                        let discography = match get_discography(&self.db.pool, &artist.id, self.client.as_ref()).await {
                            Ok(tracks) if !tracks.is_empty() => Some(tracks),
                            _ => if let Some(client) = self.client.as_ref() {
                                if let Ok(tracks) = client.discography(&artist.id).await {
                                    Some(tracks)
                                } else { None }
                            } else { None }
                        };
                        if let Some(discography) = discography {
                            if let Some(_) =
                                discography.iter().find(|t| t.id == track_id)
                            {
                                artist_id = artist.id.clone();
                                break;
                            }
                        }
                    }
                }
                if artist_id.is_empty() {
                    // if this fails, let's last attempt to find the artist by name
                    for artist in album_artists {
                        if let Some(a) =
                            self.original_artists.iter().find(|a| a.name == artist.name)
                        {
                            artist_id = a.id.clone();
                            break;
                        }
                    }
                    if artist_id.is_empty() {
                        return;
                    }
                }
                let index = self
                    .artists
                    .iter()
                    .position(|a| a.id == artist_id)
                    .unwrap_or(0);
                self.artist_select_by_index(index);

                self.state.artists_search_term = String::from("");

                let selected = self.state.selected_artist.selected().unwrap_or(0);
                self.discography(&self.artists[selected].id.clone()).await;
                self.track_select_by_index(0);

                // now find the first track that matches this album
                if let Some(track) = self.tracks.iter().find(|t| t.id == track_id) {
                    let index = self
                        .tracks
                        .iter()
                        .position(|t| t.id == track.id)
                        .unwrap_or(0);
                    self.track_select_by_index(index);
                }
            }
        }
    }

    async fn global_search_perform(&mut self) {
        let artists = self.original_artists.iter().filter(|a| {
            a.name.to_lowercase().contains(&self.search_term.to_lowercase())
        }).cloned().collect::<Vec<Artist>>();
        self.search_result_artists = artists;
        self.search_result_artists.sort_by(|a: &Artist, b: &Artist| sort::compare(&a.name, &b.name));

        self.state.selected_search_artist.select(Some(0));
        self.state.search_artist_scroll_state = self
            .state
            .search_artist_scroll_state
            .content_length(self.search_result_artists.len());

        let albums = self.original_albums.iter().filter(|a|    {
            a.name.to_lowercase().contains(&self.search_term.to_lowercase())
        }).cloned().collect::<Vec<Album>>();
        self.search_result_albums = albums;
        self.search_result_albums.sort_by(|a: &Album, b: &Album| sort::compare(&a.name, &b.name));

        self.state.selected_search_album.select(Some(0));
        self.state.search_album_scroll_state = self
            .state
            .search_album_scroll_state
            .content_length(self.search_result_albums.len());

        let tracks = match &self.client {
            Some(client) => client.search_tracks(self.search_term.clone()).await,
            None => Ok(get_tracks(
                &self.db.pool,
                &self.search_term,
            ).await.unwrap_or_default()),
        };
        if let Ok(tracks) = tracks {
            self.search_result_tracks = tracks;
            self.state.selected_search_track.select(Some(0));
            self.state.search_track_scroll_state = self
                .state
                .search_track_scroll_state
                .content_length(self.search_result_tracks.len());
        }

        self.state.search_section = SearchSection::Artists;
        if self.search_result_artists.is_empty() {
            self.state.search_section = SearchSection::Albums;
        }
        if self.search_result_albums.is_empty() {
            self.state.search_section = SearchSection::Tracks;
        }
        if self.search_result_tracks.is_empty()
            && self.search_result_artists.is_empty()
            && self.search_result_albums.is_empty()
        {
            self.state.search_section = SearchSection::Artists;
        }

        self.searching = false;
    }
}

fn page_up_list(len: usize, step: usize, state: &mut ratatui::widgets::ListState, scroll: &mut ratatui::widgets::ScrollbarState) {
    if len == 0 { return; }
    let cur = state.selected().unwrap_or(0);
    let new = cur.saturating_sub(step.max(1));
    state.select(Some(new));
    for _ in 0..step { scroll.prev(); }
}

fn page_down_list(len: usize, step: usize, state: &mut ratatui::widgets::ListState, scroll: &mut ratatui::widgets::ScrollbarState) {
    if len == 0 { return; }
    let cur = state.selected().unwrap_or(0);
    let new = (cur + step.max(1)).min(len.saturating_sub(1));
    state.select(Some(new));
    for _ in 0..step { scroll.next(); }
}

fn page_up_table(
    len: usize, step: usize, state: &mut ratatui::widgets::TableState, scroll: &mut ratatui::widgets::ScrollbarState,
) {
    if len == 0 { return; }
    let cur = state.selected().unwrap_or(0);
    let new = cur.saturating_sub(step.max(1));
    state.select(Some(new));
    for _ in 0..step { scroll.prev(); }
}

fn page_down_table(
    len: usize, step: usize, state: &mut ratatui::widgets::TableState, scroll: &mut ratatui::widgets::ScrollbarState,
) {
    if len == 0 { return; }
    let cur = state.selected().unwrap_or(0);
    let new = (cur + step.max(1)).min(len.saturating_sub(1));
    state.select(Some(new));
    for _ in 0..step { scroll.next(); }
}

/// Enum types for section switching
/// Active global tab
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum ActiveTab {
    #[default]
    Library,
    Albums,
    Playlists,
    Search,
}

// Music - active "section"
#[derive(Debug, PartialEq, Clone, Copy, Default, Serialize, Deserialize)]
pub enum ActiveSection {
    #[default]
    #[serde(alias = "Artists")] // TODO: remove -- backwards compatibility
    List,
    Tracks,
    Queue,
    Lyrics,
    Popup,
}

/// Search - active "section"
#[derive(Debug, Default, Serialize, Deserialize)]
pub enum SearchSection {
    #[default]
    Artists,
    Albums,
    Tracks,
}
