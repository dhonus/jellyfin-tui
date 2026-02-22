/* --------------------------
Keyboard related functions
    - The entry point for handling keyboard events is the `handle_events` function
    - Handles all key events - movement within the program, seeking, volume control, etc.
    - Also used for searching
-------------------------- */
use crate::{
    client::{Album, Artist, DiscographySong},
    database::{
        database::{Command, DownloadCommand, RemoveCommand},
        extension::DownloadStatus,
    },
    sort,
    tui::App,
};

use crate::database::extension::{
    get_discography, get_tracks, set_favorite_album, set_favorite_artist, set_favorite_playlist,
    set_favorite_track,
};
pub(crate) use crate::helpers::{search_ranked_indices, search_ranked_refs};
use crate::mpv::SeekFlag;

use crate::helpers::Searchable;
pub(crate) use crate::helpers::Selectable;
use crokey::{key, KeyCombination};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub enum Action {
    /// Exit the app
    Quit,
    /// Jump to tab by index (1-based)
    Tab(u8),
    /// Go up 1 in the current list
    Up,
    /// Go down 1 in the current list
    Down,
    /// Default accept action - e.g. play selected track, open selected album, etc.
    Enter,
    /// Cancel a temporary state. For example during editing, searching, or when a popup is open.
    Cancel,
    /// Toggle help
    Help,
    /// Backspace - e.g. delete last character of search term
    DeleteBack,
    /// Delete - e.g. delete search term, delete current from queue, etc.
    Delete,
    /// Type a character - e.g. for searching
    Type(char),
    /// Enable local searching within the active section
    SearchLocally,
    /// Seek forward by N seconds. By default comes with Seek(5 / -5) and Seek(60 / -60), but can be arbitrary
    Seek(i64),
    /// Jump to next section within active group
    CyclePrimarySections,
    /// Jump to previous section within active group
    CycleSecondarySections,
    /// Jump to next section sequentially (regardless of group)
    NextSectionSequential,
    /// Jump to previous section sequentially (regardless of group)
    PreviousSectionSequential,
    /// Widen current pane
    WidenPane,
    /// Shrink current pane
    ShrinkPane,
    /// Next track
    Next,
    /// Previous track
    Previous,
    /// Play / pause
    PlayPause,
    /// Stop
    Stop,
    /// Reset state
    Reset,
    /// Toggle transcoding on/off
    ToggleTranscoding,
    /// Louder volume
    VolumeUp,
    /// Quieter volume
    VolumeDown,
    /// Arbitrary shell command
    Shell(String),
    /// Move current item up (relocate)
    MoveUp,
    /// Move current item down (relocate)
    MoveDown,
    /// PageUp
    PageUp,
    /// PageDown
    PageDown,
    /// Go all the way up
    First,
    /// Go all the way down
    Last,
    /// Jump forward alphabetically or logically in a list. E.g. jump the next album in discography, jump to next artist starting with a different letter, etc.
    JumpForward,
    /// The opposite of JumpForward
    JumpBackward,
    /// Push to START of temporary queue
    EmplaceTempStart,
    /// Push to END of temporary queue
    EmplaceTempEnd,
    /// Clear the temporary queue
    ClearTemp,
    /// Mark item as favorite / toggle favorite
    ToggleFavorite,
    /// Download item (if not downloaded)
    Download,
    /// Remove downloaded item
    RemoveDownload,
    /// Cycle repeat modes (Off -> All -> One -> Off)
    Repeat,
    /// Shuffle / unshuffle
    Shuffle,
    /// Global shuffle (shuffle the entire library, ignoring current queue and playlist)
    GlobalShuffle,
    /// Open a popup (context-sensitive)
    Popup,
    /// Open the global popup with additional settings
    GlobalPopup,
}

const DEFAULT_BINDINGS: &[(KeyCombination, Action)] = &[
    (key!(ctrl - c), Action::Quit),
    // tabs are 1-based
    (key!(1), Action::Tab(1)),
    (key!(2), Action::Tab(2)),
    (key!(3), Action::Tab(3)),
    (key!(4), Action::Tab(4)),
    // up
    (key!(k), Action::Up),
    (key!(up), Action::Up),
    // down
    (key!(j), Action::Down),
    (key!(down), Action::Down),
    // navigation
    (key!(enter), Action::Enter),
    (key!(esc), Action::Cancel),
    (key!('?'), Action::Help),
    // text editing
    (key!(backspace), Action::DeleteBack),
    (key!(delete), Action::Delete),
    // local search
    (key!('/'), Action::SearchLocally),
    // seeking
    (key!(left), Action::Seek(-5)),
    (key!(right), Action::Seek(5)),
    (key!(','), Action::Seek(-60)),
    (key!('.'), Action::Seek(60)),
    // visual sections
    (key!(tab), Action::CyclePrimarySections),
    (key!(shift - backtab), Action::CycleSecondarySections),
    (key!('l'), Action::NextSectionSequential),
    (key!('h'), Action::PreviousSectionSequential),
    // pane resizing
    (key!(ctrl - right), Action::WidenPane),
    (key!(ctrl - left), Action::ShrinkPane),
    (key!(ctrl - 'l'), Action::WidenPane),
    (key!(ctrl - 'h'), Action::ShrinkPane),
    // playback
    (key!('n'), Action::Next),
    (key!(shift - n), Action::Previous),
    (key!(space), Action::PlayPause),
    (key!(x), Action::Stop),
    (key!(ctrl - x), Action::Reset),
    (key!(shift - t), Action::ToggleTranscoding),
    (key!('+'), Action::VolumeUp),
    (key!('-'), Action::VolumeDown),
    (key!(shift - up), Action::MoveUp),
    (key!(shift - down), Action::MoveDown),
    (key!(shift - k), Action::MoveUp),
    (key!(shift - j), Action::MoveDown),
    (key!(pageup), Action::PageUp),
    (key!(pagedown), Action::PageDown),
    (key!(home), Action::First),
    (key!(end), Action::Last),
    (key!('g'), Action::First),
    (key!(shift - g), Action::Last),
    (key!('a'), Action::JumpForward),
    (key!(shift - a), Action::JumpBackward),
    // queue
    (key!(ctrl - enter), Action::EmplaceTempStart),
    (key!(shift - enter), Action::EmplaceTempEnd),
    (key!(ctrl - e), Action::EmplaceTempStart),
    (key!('e'), Action::EmplaceTempEnd),
    (key!(shift - e), Action::ClearTemp),
    // row commands
    (key!('f'), Action::ToggleFavorite),
    (key!('d'), Action::Download),
    (key!(shift - d), Action::RemoveDownload),
    // global commands
    (key!(r), Action::Repeat),
    (key!(s), Action::Shuffle),
    (key!(shift - s), Action::GlobalShuffle),
    // popups
    (key!('p'), Action::Popup),
    (key!(shift - p), Action::GlobalPopup),
];

pub fn load_keymap(config: &serde_yaml::Value) -> HashMap<KeyCombination, Action> {
    let keymap_inherit = config.get("keymap_inherit").and_then(|v| v.as_bool()).unwrap_or(true);
    let mut keymap =
        if keymap_inherit { DEFAULT_BINDINGS.iter().cloned().collect() } else { HashMap::new() };

    if let Some(value) = config.get("keymap") {
        match serde_yaml::from_value::<HashMap<KeyCombination, Action>>(value.to_owned()) {
            Ok(overrides) => {
                log::info!("Loaded {} keymap overrides", overrides.len());
                keymap.extend(overrides);
            }
            Err(err) => {
                println!(" ! Failed to parse keymap from config: {}. Using default keymap.", err);
                log::error!("Failed to parse keymap from config: {}", err);
            }
        }
    }

    keymap
}
impl App {
    /// Poll for events and handle them
    pub async fn handle_events(&mut self) -> io::Result<()> {
        let idle_ms = self.recent_input_activity.elapsed().as_millis();
        let timeout = match idle_ms {
            0..=300 => Duration::from_millis(0),
            301..=2000 => Duration::from_millis(2),
            _ => Duration::from_millis(5),
        };

        while event::poll(timeout)? {
            match event::read()? {
                Event::Key(k) => {
                    self.recent_input_activity = tokio::time::Instant::now();
                    self.handle_key_event(k).await;
                }
                Event::Mouse(m) => {
                    self.recent_input_activity = tokio::time::Instant::now();
                    self.handle_mouse_event(m);
                }
                Event::Resize(c, r) => {
                    let (_, picker) = App::init_theme_and_picker(&self.config, &self.theme);
                    self.picker = picker;
                    self.refresh_cover_art().await;
                    if c == self.last_term_size.0 && r == self.last_term_size.1 {
                        // Size hasn't changed. Do a full redraw in case we are running under a terminal
                        // session manager which just restored  the session.
                        self.dirty_clear = true;
                    } else {
                        // Size has changed. So redraw whatever needs to be redrawn.
                        self.dirty = true;
                    }
                    self.last_term_size = (c, r);
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn handle_key_event(&mut self, key_event: KeyEvent) {
        if key_event.kind == KeyEventKind::Release {
            return;
        }
        // log::debug!("{:?}", crate::helpers::crokey_to_yaml(key_event));
        let combo = KeyCombination::from(key_event);
        // if inputting text, treat any Char events as text input - convert to Type(c)
        if self.locally_searching || self.popup.editing || self.searching {
            if let KeyCode::Char(c) = key_event.code {
                self.dirty = true;
                let action = Action::Type(c);
                if self.state.active_section == ActiveSection::Popup {
                    self.popup_handle_action(&action).await;
                    return;
                } else if self.state.active_tab == ActiveTab::Search {
                    self.handle_search_tab_action(&action).await;
                }
                self.dispatch_local_search(&action).await;
                return;
            }
        }

        if let Some(action) = self.keymap.get(&combo).cloned() {
            self.dirty = true;
            self.dispatch_action(&action).await;
            return;
        }
    }

    async fn dispatch_action(&mut self, action: &Action) {
        if self.state.active_section == ActiveSection::Popup {
            self.popup_handle_action(&action).await;
            return;
        }
        if self.locally_searching {
            self.dispatch_local_search(action).await;
            return;
        }

        if self.playlist_editing {
            match action {
                Action::Enter => self.commit_playlist_edit().await,
                Action::Cancel => self.cancel_playlist_edit(),
                Action::Down => self.move_playlist_edit_step(1),
                Action::Up => self.move_playlist_edit_step(-1),
                Action::MoveDown => self.move_playlist_edit_step(1),
                Action::MoveUp => self.move_playlist_edit_step(-1),
                _ => return,
            }
            return;
        }

        if self.state.active_tab == ActiveTab::Search {
            self.handle_search_tab_action(&action).await;
            return;
        }

        match action {
            Action::Quit => self.exit().await,
            Action::SearchLocally => {
                self.locally_searching = true;
                return;
            }
            Action::Seek(secs) => self.dispatch_seek(*secs).await,
            Action::CyclePrimarySections => self.cycle_section(true),
            Action::CycleSecondarySections => self.cycle_section(false),
            Action::NextSectionSequential => self.step_section(true),
            Action::PreviousSectionSequential => self.step_section(false),
            Action::WidenPane => {
                self.preferences.widen_current_pane(&self.state.active_section, true)
            }
            Action::ShrinkPane => {
                self.preferences.widen_current_pane(&self.state.active_section, false)
            }
            Action::Next => self.next().await,
            Action::Previous => self.previous().await,
            Action::Tab(index) => self.set_tab(std::cmp::min(*index, 4)).await,
            Action::PlayPause => match self.paused {
                true => self.play().await,
                false => self.pause().await,
            },
            Action::Stop => self.stop().await,
            Action::Reset => self.reset().await,
            Action::ToggleTranscoding => self.toggle_transcoding().await,
            Action::VolumeUp => self.volume_up().await,
            Action::VolumeDown => self.volume_down().await,
            Action::Up => self.handle_nav_up(),
            Action::Down => self.handle_nav_down(),
            Action::MoveUp => self.handle_move_item_up().await,
            Action::MoveDown => self.handle_move_item_down().await,
            Action::PageUp => self.page_up(),
            Action::PageDown => self.page_down(),
            Action::First => self.go_first(),
            Action::Last => self.go_last(),
            Action::JumpForward => self.jump_forward(),
            Action::JumpBackward => self.jump_backward(),
            Action::Enter => self.handle_enter().await,
            Action::Cancel => self.handle_cancel().await,
            Action::Help => self.show_help(),
            Action::EmplaceTempStart => self.emplace_temp(true).await,
            Action::EmplaceTempEnd => self.emplace_temp(false).await,
            Action::ClearTemp => self.clear_temporary_queue().await,
            Action::ToggleFavorite => self.toggle_favorite().await,
            Action::Download => self.download(false).await,
            Action::RemoveDownload => self.download(true).await,
            Action::Repeat => self.cycle_repeat_mode().await,
            Action::Shuffle => self.toggle_shuffle().await,
            Action::GlobalShuffle => self.global_shuffle().await,
            Action::Delete => self.pop_from_queue().await,
            Action::Popup => self.request_popup(false).await,
            Action::GlobalPopup => self.request_popup(true).await,
            // noops
            Action::DeleteBack => {}
            Action::Type(_) => {}
            Action::Shell(cmd) => {
                crate::helpers::run_shell_command(&cmd).await;
            }
        }
    }

    async fn set_tab(&mut self, index: u8) {
        match index {
            1 => {
                self.state.active_tab = ActiveTab::Library;
                if self.tracks.is_empty() {
                    self.state.active_section = ActiveSection::List;
                }
            }
            2 => {
                self.state.active_tab = ActiveTab::Albums;
                if self.album_tracks.is_empty() {
                    self.state.active_section = ActiveSection::List;
                }
            }
            3 => {
                self.state.active_tab = ActiveTab::Playlists;
                if self.playlist_tracks.is_empty() {
                    self.state.active_section = ActiveSection::List;
                }
            }
            4 => {
                self.state.active_tab = ActiveTab::Search;
                self.searching = true;
            }
            _ => {}
        }
    }

    async fn dispatch_local_search(&mut self, action: &Action) {
        match action {
            Action::Cancel | Action::Tab(1) => {
                self.locally_searching = false;
                let artist_id = self.get_id_of_selected(&self.artists, Selectable::Artist);
                let track_id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                let album_id = self.get_id_of_selected(&self.albums, Selectable::Album);
                let album_track_id =
                    self.get_id_of_selected(&self.album_tracks, Selectable::AlbumTrack);
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
                        _ => {}
                    },
                    _ => {}
                }
                return;
            }
            Action::Enter => {
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
            Action::DeleteBack => match self.state.active_tab {
                ActiveTab::Library => match self.state.active_section {
                    ActiveSection::List => {
                        let selected_id =
                            self.get_id_of_selected(&self.artists, Selectable::Artist);
                        self.state.artists_search_term.pop();
                        self.reposition_cursor(&selected_id, Selectable::Artist);
                    }
                    ActiveSection::Tracks => {
                        let selected_id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                        self.state.tracks_search_term.pop();
                        self.reposition_cursor(&selected_id, Selectable::Track);
                    }
                    _ => {}
                },
                ActiveTab::Albums => match self.state.active_section {
                    ActiveSection::List => {
                        let selected_id = self.get_id_of_selected(&self.albums, Selectable::Album);
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
                        let selected_id = self
                            .get_id_of_selected(&self.playlist_tracks, Selectable::PlaylistTrack);
                        self.state.playlist_tracks_search_term.pop();
                        self.reposition_cursor(&selected_id, Selectable::PlaylistTrack);
                    }
                    _ => {}
                },
                _ => {}
            },
            Action::Delete => match self.state.active_tab {
                ActiveTab::Library => match self.state.active_section {
                    ActiveSection::List => {
                        let selected_id =
                            self.get_id_of_selected(&self.artists, Selectable::Artist);
                        self.state.artists_search_term.clear();
                        self.reposition_cursor(&selected_id, Selectable::Artist);
                    }
                    ActiveSection::Tracks => {
                        let selected_id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                        self.state.tracks_search_term.clear();
                        self.reposition_cursor(&selected_id, Selectable::Track);
                    }
                    _ => {}
                },
                ActiveTab::Albums => match self.state.active_section {
                    ActiveSection::List => {
                        let selected_id = self.get_id_of_selected(&self.albums, Selectable::Album);
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
                        let selected_id = self
                            .get_id_of_selected(&self.playlist_tracks, Selectable::PlaylistTrack);
                        self.state.playlist_tracks_search_term.clear();
                        self.reposition_cursor(&selected_id, Selectable::PlaylistTrack);
                    }
                    _ => {}
                },
                _ => {}
            },
            Action::Type(c) => match self.state.active_tab {
                ActiveTab::Library => match self.state.active_section {
                    ActiveSection::List => {
                        self.state.artists_search_term.push(*c);
                        self.artist_select_by_index(0);
                    }
                    ActiveSection::Tracks => {
                        self.state.tracks_search_term.push(*c);
                        self.track_select_by_index(0);
                    }
                    _ => {}
                },
                ActiveTab::Albums => match self.state.active_section {
                    ActiveSection::List => {
                        self.state.albums_search_term.push(*c);
                        self.album_select_by_index(0);
                    }
                    ActiveSection::Tracks => {
                        self.state.album_tracks_search_term.push(*c);
                        self.album_track_select_by_index(0);
                    }
                    _ => {}
                },
                ActiveTab::Playlists => match self.state.active_section {
                    ActiveSection::List => {
                        self.state.playlists_search_term.push(*c);
                        self.playlist_select_by_index(0);
                    }
                    ActiveSection::Tracks => {
                        self.state.playlist_tracks_search_term.push(*c);
                        self.playlist_track_select_by_index(0);
                    }
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }
    }

    async fn dispatch_seek(&mut self, secs: i64) {
        if self.stopped {
            return;
        }
        let rel = secs as f64;
        self.state.current_playback_state.position = f64::max(
            0.0,
            f64::min(
                self.state.current_playback_state.position + rel,
                self.state.current_playback_state.duration,
            ),
        );
        self.update_mpris_position(self.state.current_playback_state.position);
        let _ = self.handle_discord(true).await;
        self.mpv_handle.seek(rel, SeekFlag::Relative).await;
    }

    async fn handle_search_tab_action(&mut self, action: &Action) {
        if self.searching {
            match action {
                Action::Cancel => {
                    self.searching = false;
                    return;
                }

                Action::DeleteBack => {
                    self.search_term.pop();
                    return;
                }

                Action::Delete => {
                    self.search_term.clear();
                    return;
                }

                Action::Enter => {
                    self.global_search().await;
                    return;
                }

                Action::Type(c) => {
                    self.search_term.push(*c);
                    return;
                }

                _ => {}
            }
        }

        // NORMAL SEARCH TAB MODE — bindings control navigation
        match action {
            Action::Cancel => {
                self.state.active_tab = ActiveTab::Library;
            }

            Action::Tab(1) => {
                self.state.active_tab = ActiveTab::Library;
            }

            Action::Tab(2) => {
                self.state.active_tab = ActiveTab::Albums;
                if self.playlist_tracks.is_empty() {
                    self.state.active_section = ActiveSection::List;
                }
            }

            Action::Tab(3) => {
                self.state.active_tab = ActiveTab::Playlists;
                if self.playlist_tracks.is_empty() {
                    self.state.active_section = ActiveSection::List;
                }
            }

            Action::Tab(4) => {
                self.searching = true;
            }

            Action::CyclePrimarySections => {
                self.toggle_search_section(true);
            }

            Action::CycleSecondarySections => {
                self.toggle_search_section(false);
            }

            Action::Enter => {
                self.global_search().await;
            }

            Action::Down => match self.state.search_section {
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

            Action::Up => match self.state.search_section {
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

            Action::First => match self.state.search_section {
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

            Action::Last => match self.state.search_section {
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

            Action::PreviousSectionSequential => {
                self.vim_search_left();
            }

            Action::NextSectionSequential => {
                self.vim_search_right();
            }

            Action::SearchLocally => {
                self.searching = true;
            }

            _ => {}
        }
    }

    fn handle_mouse_event(&mut self, _mouse_event: crossterm::event::MouseEvent) {
        // println!("Mouse event: {:?}", _mouse_event);
        // self.dirty = true;
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
            Selectable::Artist => {
                self.artists.iter().map(|a| a.id.clone()).collect::<Vec<String>>()
            }
            Selectable::Album => self.albums.iter().map(|a| a.id.clone()).collect::<Vec<String>>(),
            Selectable::AlbumTrack => {
                self.album_tracks.iter().map(|t| t.id.clone()).collect::<Vec<String>>()
            }
            Selectable::Track => self.tracks.iter().map(|t| t.id.clone()).collect::<Vec<String>>(),
            Selectable::Playlist => {
                self.playlists.iter().map(|p| p.id.clone()).collect::<Vec<String>>()
            }
            Selectable::PlaylistTrack => {
                self.playlist_tracks.iter().map(|t| t.id.clone()).collect::<Vec<String>>()
            }
            Selectable::Popup => {
                if let Some(menu) = &self.popup.current_menu {
                    menu.options().iter().map(|o| String::from(o.id())).collect::<Vec<String>>()
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
            let indices: Vec<usize> = match selectable {
                Selectable::Artist => search_ranked_indices(&self.artists, search_term, false),
                Selectable::Album => search_ranked_indices(&self.albums, search_term, false),
                Selectable::AlbumTrack => {
                    search_ranked_indices(&self.album_tracks, search_term, false)
                }
                Selectable::Track => search_ranked_indices(&self.tracks, search_term, false),
                Selectable::Playlist => search_ranked_indices(&self.playlists, search_term, false),
                Selectable::PlaylistTrack => {
                    search_ranked_indices(&self.playlist_tracks, search_term, false)
                }
                Selectable::Popup => self.popup.current_menu.as_ref().map_or(vec![], |menu| {
                    search_ranked_indices(&menu.options(), search_term, false)
                }),
            };

            if let Some(index) = indices.iter().position(|&i| match selectable {
                Selectable::Artist => self.artists[i].id == id,
                Selectable::Album => self.albums[i].id == id,
                Selectable::AlbumTrack => self.album_tracks[i].id == id,
                Selectable::Track => self.tracks[i].id == id,
                Selectable::Playlist => self.playlists[i].id == id,
                Selectable::PlaylistTrack => self.playlist_tracks[i].id == id,
                Selectable::Popup => self
                    .popup
                    .current_menu
                    .as_ref()
                    .map_or(false, |menu| menu.options()[i].id() == id),
            }) {
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
            let indices = search_ranked_indices(items, search_term, false);
            if indices.is_empty() || indices.len() <= selected {
                return String::new();
            }
            return items[indices[selected]].id().to_string();
        }
        if items.is_empty() || items.len() <= selected {
            return String::from("");
        }
        String::from(items[selected].id())
    }

    pub fn artist_select_by_index(&mut self, index: usize) {
        let indices = search_ranked_indices(&self.artists, &self.state.artists_search_term, true);
        if indices.is_empty() {
            return;
        }

        let index = index.min(indices.len() - 1);
        self.state.selected_artist.select(Some(index));
        self.state.artists_scroll_state =
            self.state.artists_scroll_state.content_length(indices.len()).position(index);
    }

    pub fn track_select_by_index(&mut self, index: usize) {
        let indices = search_ranked_indices(&self.tracks, &self.state.tracks_search_term, true);
        if indices.is_empty() {
            return;
        }

        let index = index.min(indices.len() - 1);
        self.state.selected_track.select(Some(index));
        self.state.tracks_scroll_state =
            self.state.tracks_scroll_state.content_length(indices.len()).position(index);
    }

    pub fn album_select_by_index(&mut self, index: usize) {
        let indices = search_ranked_indices(&self.albums, &self.state.albums_search_term, true);
        if indices.is_empty() {
            return;
        }
        let index = index.min(indices.len() - 1);
        self.state.selected_album.select(Some(index));
        self.state.albums_scroll_state =
            self.state.albums_scroll_state.content_length(indices.len()).position(index);
    }

    pub fn album_track_select_by_index(&mut self, index: usize) {
        let indices =
            search_ranked_indices(&self.album_tracks, &self.state.album_tracks_search_term, true);
        if indices.is_empty() {
            return;
        }

        let index = index.min(indices.len() - 1);
        self.state.selected_album_track.select(Some(index));
        self.state.album_tracks_scroll_state =
            self.state.album_tracks_scroll_state.content_length(indices.len()).position(index);
    }

    pub fn playlist_track_select_by_index(&mut self, index: usize) {
        let indices = search_ranked_indices(
            &self.playlist_tracks,
            &self.state.playlist_tracks_search_term,
            true,
        );
        if indices.is_empty() {
            return;
        }

        let index = index.min(indices.len() - 1);
        self.state.selected_playlist_track.select(Some(index));
        self.state.playlist_tracks_scroll_state =
            self.state.playlist_tracks_scroll_state.content_length(indices.len()).position(index);
    }

    pub fn playlist_select_by_index(&mut self, index: usize) {
        let indices =
            search_ranked_indices(&self.playlists, &self.state.playlists_search_term, true);
        if indices.is_empty() {
            return;
        }

        let index = index.min(indices.len() - 1);
        self.state.selected_playlist.select(Some(index));
        self.state.playlists_scroll_state =
            self.state.playlists_scroll_state.content_length(indices.len()).position(index);
    }

    fn cycle_section(&mut self, forwards: bool) {
        let has_lyrics = self.lyrics.as_ref().is_some_and(|(_, l, _)| !l.is_empty());

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
                    self.state.active_section =
                        if has_lyrics { ActiveSection::Lyrics } else { ActiveSection::Queue };
                }
                ActiveSection::Tracks => {
                    self.state.last_section = ActiveSection::Tracks;
                    self.state.active_section =
                        if has_lyrics { ActiveSection::Lyrics } else { ActiveSection::Queue };
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
                            ActiveSection::List => ActiveSection::List,
                            _ => ActiveSection::List,
                        }
                    };
                }
                _ => {}
            },
        }
    }

    fn step_section(&mut self, left: bool) {
        let has_lyrics = self.lyrics.as_ref().is_some_and(|(_, l, _)| !l.is_empty());

        let current = self.state.active_section;

        let next = if has_lyrics {
            if left {
                // List -> Tracks -> Lyrics -> Queue
                match current {
                    ActiveSection::List => ActiveSection::Tracks,
                    ActiveSection::Tracks => ActiveSection::Lyrics,
                    ActiveSection::Lyrics => ActiveSection::Queue,
                    ActiveSection::Queue => ActiveSection::Queue,
                    _ => current,
                }
            } else {
                // Queue -> Lyrics -> Tracks -> List
                match current {
                    ActiveSection::Queue => ActiveSection::Lyrics,
                    ActiveSection::Lyrics => ActiveSection::Tracks,
                    ActiveSection::Tracks => ActiveSection::List,
                    ActiveSection::List => ActiveSection::List,
                    _ => current,
                }
            }
        } else {
            // List -> Tracks -> Queue
            if left {
                match current {
                    ActiveSection::List => ActiveSection::Tracks,
                    ActiveSection::Tracks => ActiveSection::Queue,
                    ActiveSection::Queue => ActiveSection::Queue,
                    _ => current,
                }
            } else {
                match current {
                    ActiveSection::Queue => ActiveSection::Tracks,
                    ActiveSection::Tracks => ActiveSection::List,
                    ActiveSection::List => ActiveSection::List,
                    _ => current,
                }
            }
        };

        if next != current {
            match current {
                ActiveSection::Queue => {
                    self.state.selected_queue_item_manual_override = false;
                }
                ActiveSection::Lyrics => {
                    self.state.selected_lyric_manual_override = false;
                }
                _ => {}
            }
        }

        self.state.active_section = next;
    }

    fn handle_nav_up(&mut self) {
        match self.state.active_section {
            ActiveSection::List => {
                match self.state.active_tab {
                    ActiveTab::Library => {
                        let prev = move_up(self.state.selected_artist.selected());
                        self.artist_select_by_index(prev);
                    }
                    ActiveTab::Albums => {
                        let prev = move_up(self.state.selected_album.selected());
                        self.album_select_by_index(prev);
                    }
                    ActiveTab::Playlists => {
                        let prev = move_up(self.state.selected_playlist.selected());
                        self.playlist_select_by_index(prev);
                    }
                    ActiveTab::Search => {
                        // handle_search_tab_events()
                    }
                }
            }
            ActiveSection::Tracks => match self.state.active_tab {
                ActiveTab::Library => {
                    let prev = move_up(self.state.selected_track.selected());
                    self.track_select_by_index(prev);
                }
                ActiveTab::Albums => {
                    let prev = move_up(self.state.selected_album_track.selected());
                    self.album_track_select_by_index(prev);
                }
                ActiveTab::Playlists => {
                    let prev = move_up(self.state.selected_playlist_track.selected());
                    self.playlist_track_select_by_index(prev);
                }
                _ => {}
            },
            ActiveSection::Queue => {
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
        }
    }

    fn handle_nav_down(&mut self) {
        match self.state.active_section {
            ActiveSection::List => {
                match self.state.active_tab {
                    ActiveTab::Library => {
                        let len = if !self.state.artists_search_term.is_empty() {
                            search_ranked_indices(
                                &self.artists,
                                &self.state.artists_search_term,
                                false,
                            )
                            .len()
                        } else {
                            self.artists.len()
                        };

                        if len == 0 {
                            return;
                        }

                        let next = move_down(self.state.selected_artist.selected(), len);
                        self.artist_select_by_index(next);
                        return;
                    }
                    ActiveTab::Albums => {
                        let len = if !self.state.albums_search_term.is_empty() {
                            search_ranked_indices(
                                &self.albums,
                                &self.state.albums_search_term,
                                false,
                            )
                            .len()
                        } else {
                            self.albums.len()
                        };

                        if len == 0 {
                            return;
                        }

                        let next = move_down(self.state.selected_album.selected(), len);
                        self.album_select_by_index(next);
                        return;
                    }
                    ActiveTab::Playlists => {
                        let len = if !self.state.playlists_search_term.is_empty() {
                            search_ranked_indices(
                                &self.playlists,
                                &self.state.playlists_search_term,
                                false,
                            )
                            .len()
                        } else {
                            self.playlists.len()
                        };

                        if len == 0 {
                            return;
                        }

                        let next = move_down(self.state.selected_playlist.selected(), len);
                        self.playlist_select_by_index(next);
                        return;
                    }
                    ActiveTab::Search => {
                        // handle_search_tab_events()
                    }
                }
            }
            ActiveSection::Tracks => {
                if self.state.active_tab == ActiveTab::Library {
                    let len =
                        search_ranked_indices(&self.tracks, &self.state.tracks_search_term, true)
                            .len();
                    if len == 0 {
                        return;
                    }

                    let next = move_down(self.state.selected_track.selected(), len);
                    self.track_select_by_index(next);
                    return;
                }
                if self.state.active_tab == ActiveTab::Albums {
                    let len = search_ranked_indices(
                        &self.album_tracks,
                        &self.state.album_tracks_search_term,
                        true,
                    )
                    .len();

                    if len == 0 {
                        return;
                    }

                    let next = move_down(self.state.selected_album_track.selected(), len);
                    self.album_track_select_by_index(next);
                    return;
                }
                if self.state.active_tab == ActiveTab::Playlists {
                    let len = search_ranked_indices(
                        &self.playlist_tracks,
                        &self.state.playlist_tracks_search_term,
                        true,
                    )
                    .len();

                    if len == 0 {
                        return;
                    }

                    let next = move_down(self.state.selected_playlist_track.selected(), len);
                    self.playlist_track_select_by_index(next);
                    return;
                }
            }
            ActiveSection::Queue => {
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
        }
    }

    async fn handle_move_item_up(&mut self) {
        match self.state.active_section {
            ActiveSection::Tracks => match self.state.active_tab {
                ActiveTab::Playlists => {
                    self.move_playlist_edit_step(-1);
                }
                _ => {}
            },
            ActiveSection::Queue => {
                self.move_queue_item_up().await;
            }
            _ => {}
        }
    }

    async fn handle_move_item_down(&mut self) {
        match self.state.active_section {
            ActiveSection::Tracks => {
                if self.state.active_tab == ActiveTab::Playlists {
                    self.move_playlist_edit_step(1);
                }
            }
            ActiveSection::Queue => {
                self.move_queue_item_down().await;
            }
            _ => {}
        }
    }

    fn page_up(&mut self) {
        match (self.state.active_section, self.state.active_tab) {
            (ActiveSection::List, ActiveTab::Library) => {
                page_up_list(
                    self.artists.len(),
                    self.left_list_height,
                    &mut self.state.selected_artist,
                    &mut self.state.artists_scroll_state,
                );
            }
            (ActiveSection::List, ActiveTab::Albums) => {
                page_up_list(
                    self.albums.len(),
                    self.left_list_height,
                    &mut self.state.selected_album,
                    &mut self.state.albums_scroll_state,
                );
            }
            (ActiveSection::List, ActiveTab::Playlists) => {
                page_up_list(
                    self.playlists.len(),
                    self.left_list_height,
                    &mut self.state.selected_playlist,
                    &mut self.state.playlists_scroll_state,
                );
            }
            (ActiveSection::Tracks, ActiveTab::Library) => {
                page_up_table(
                    self.tracks.len(),
                    self.track_list_height,
                    &mut self.state.selected_track,
                    &mut self.state.tracks_scroll_state,
                );
            }
            (ActiveSection::Tracks, ActiveTab::Albums) => {
                page_up_table(
                    self.album_tracks.len(),
                    self.track_list_height,
                    &mut self.state.selected_album_track,
                    &mut self.state.album_tracks_scroll_state,
                );
            }
            (ActiveSection::Tracks, ActiveTab::Playlists) => {
                page_up_table(
                    self.playlist_tracks.len(),
                    self.track_list_height,
                    &mut self.state.selected_playlist_track,
                    &mut self.state.playlist_tracks_scroll_state,
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
                    self.artists.len(),
                    self.left_list_height,
                    &mut self.state.selected_artist,
                    &mut self.state.artists_scroll_state,
                );
            }
            (ActiveSection::List, ActiveTab::Albums) => {
                page_down_list(
                    self.albums.len(),
                    self.left_list_height,
                    &mut self.state.selected_album,
                    &mut self.state.albums_scroll_state,
                );
            }
            (ActiveSection::List, ActiveTab::Playlists) => {
                page_down_list(
                    self.playlists.len(),
                    self.left_list_height,
                    &mut self.state.selected_playlist,
                    &mut self.state.playlists_scroll_state,
                );
            }
            (ActiveSection::Tracks, ActiveTab::Library) => {
                page_down_table(
                    self.tracks.len(),
                    self.track_list_height,
                    &mut self.state.selected_track,
                    &mut self.state.tracks_scroll_state,
                );
            }
            (ActiveSection::Tracks, ActiveTab::Albums) => {
                page_down_table(
                    self.album_tracks.len(),
                    self.track_list_height,
                    &mut self.state.selected_album_track,
                    &mut self.state.album_tracks_scroll_state,
                );
            }
            (ActiveSection::Tracks, ActiveTab::Playlists) => {
                page_down_table(
                    self.playlist_tracks.len(),
                    self.track_list_height,
                    &mut self.state.selected_playlist_track,
                    &mut self.state.playlist_tracks_scroll_state,
                );
            }

            _ => {}
        }
        self.dirty = true;
    }

    fn go_first(&mut self) {
        match self.state.active_section {
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
        }
    }

    fn go_last(&mut self) {
        match self.state.active_section {
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
        }
    }

    fn jump_forward(&mut self) {
        match self.state.active_tab {
            ActiveTab::Library => {
                match self.state.active_section {
                    ActiveSection::List => {
                        if self.artists.is_empty() {
                            return;
                        }

                        let indices = if !self.state.artists_search_term.is_empty() {
                            search_ranked_indices(
                                &self.artists,
                                &self.state.artists_search_term,
                                false,
                            )
                        } else {
                            (0..self.artists.len()).collect()
                        };

                        if indices.is_empty() {
                            return;
                        }

                        let selected = self.state.selected_artist.selected().unwrap_or(0);
                        let current_idx = indices[selected];
                        let current_char = sort::strip_article(&self.artists[current_idx].name)
                            .chars()
                            .next()
                            .unwrap_or_default()
                            .to_ascii_lowercase();

                        if let Some((next_pos, _)) =
                            indices.iter().enumerate().skip(selected + 1).find(|(_, &i)| {
                                sort::strip_article(&self.artists[i].name)
                                    .chars()
                                    .next()
                                    .map(|c| c.to_ascii_lowercase())
                                    != Some(current_char)
                            })
                        {
                            self.artist_select_by_index(next_pos);
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

                    let indices = if !self.state.albums_search_term.is_empty() {
                        search_ranked_indices(&self.albums, &self.state.albums_search_term, false)
                    } else {
                        (0..self.albums.len()).collect()
                    };

                    if indices.is_empty() {
                        return;
                    }

                    let selected = self.state.selected_album.selected().unwrap_or(0);
                    let current_idx = indices[selected];
                    let current_char = sort::strip_article(&self.albums[current_idx].name)
                        .chars()
                        .next()
                        .unwrap_or_default()
                        .to_ascii_lowercase();

                    if let Some((next_pos, _)) =
                        indices.iter().enumerate().skip(selected + 1).find(|(_, &i)| {
                            sort::strip_article(&self.albums[i].name)
                                .chars()
                                .next()
                                .map(|c| c.to_ascii_lowercase())
                                != Some(current_char)
                        })
                    {
                        self.album_select_by_index(next_pos);
                    }
                }
            }
            ActiveTab::Playlists => {
                if matches!(self.state.active_section, ActiveSection::List) {
                    if self.playlists.is_empty() {
                        return;
                    }

                    let indices = if !self.state.playlists_search_term.is_empty() {
                        search_ranked_indices(
                            &self.playlists,
                            &self.state.playlists_search_term,
                            false,
                        )
                    } else {
                        (0..self.playlists.len()).collect()
                    };

                    if indices.is_empty() {
                        return;
                    }

                    let selected = self.state.selected_playlist.selected().unwrap_or(0);
                    let current_idx = indices[selected];
                    let current_char = self.playlists[current_idx]
                        .name
                        .chars()
                        .next()
                        .unwrap_or_default()
                        .to_ascii_lowercase();

                    if let Some((next_pos, _)) =
                        indices.iter().enumerate().skip(selected + 1).find(|(_, &i)| {
                            self.playlists[i].name.chars().next().map(|c| c.to_ascii_lowercase())
                                != Some(current_char)
                        })
                    {
                        self.playlist_select_by_index(next_pos);
                    }
                }
            }
            _ => {}
        }
    }

    fn jump_backward(&mut self) {
        match self.state.active_tab {
            ActiveTab::Library => {
                match self.state.active_section {
                    // first artist with previous letter
                    ActiveSection::List => {
                        if self.artists.is_empty() {
                            return;
                        }
                        let indices = if !self.state.artists_search_term.is_empty() {
                            search_ranked_indices(
                                &self.artists,
                                &self.state.artists_search_term,
                                false,
                            )
                        } else {
                            (0..self.artists.len()).collect()
                        };

                        if indices.is_empty() {
                            return;
                        }

                        let selected = self.state.selected_artist.selected().unwrap_or(0);
                        let current_idx = indices[selected];
                        let current_char = sort::strip_article(&self.artists[current_idx].name)
                            .chars()
                            .next()
                            .unwrap_or_default()
                            .to_ascii_lowercase();

                        if let Some((prev_pos, _)) =
                            indices.iter().enumerate().take(selected).rev().find(|(_, &i)| {
                                sort::strip_article(&self.artists[i].name)
                                    .chars()
                                    .next()
                                    .map(|c| c.to_ascii_lowercase())
                                    != Some(current_char)
                            })
                        {
                            self.artist_select_by_index(prev_pos);
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
                            let prev_album =
                                self.tracks.iter().rev().skip(self.tracks.len() - selected).find(
                                    |t| t.album_id != current_album && !t.id.starts_with("_album_"),
                                );

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

                    let indices = if !self.state.albums_search_term.is_empty() {
                        search_ranked_indices(&self.albums, &self.state.albums_search_term, false)
                    } else {
                        (0..self.albums.len()).collect()
                    };

                    if indices.is_empty() {
                        return;
                    }

                    let selected = self.state.selected_album.selected().unwrap_or(0);
                    let current_idx = indices[selected];
                    let current_char = sort::strip_article(&self.albums[current_idx].name)
                        .chars()
                        .next()
                        .unwrap_or_default()
                        .to_ascii_lowercase();

                    if let Some((prev_pos, _)) =
                        indices.iter().enumerate().take(selected).rev().find(|(_, &i)| {
                            sort::strip_article(&self.albums[i].name)
                                .chars()
                                .next()
                                .map(|c| c.to_ascii_lowercase())
                                != Some(current_char)
                        })
                    {
                        self.album_select_by_index(prev_pos);
                    }
                }
            }
            ActiveTab::Playlists => {
                if matches!(self.state.active_section, ActiveSection::List) {
                    if self.playlists.is_empty() {
                        return;
                    }

                    let indices = if !self.state.playlists_search_term.is_empty() {
                        search_ranked_indices(
                            &self.playlists,
                            &self.state.playlists_search_term,
                            false,
                        )
                    } else {
                        (0..self.playlists.len()).collect()
                    };

                    if indices.is_empty() {
                        return;
                    }

                    let selected = self.state.selected_playlist.selected().unwrap_or(0);
                    let current_idx = indices[selected];
                    let current_char = self.playlists[current_idx]
                        .name
                        .chars()
                        .next()
                        .unwrap_or_default()
                        .to_ascii_lowercase();

                    if let Some((prev_pos, _)) =
                        indices.iter().enumerate().take(selected).rev().find(|(_, &i)| {
                            self.playlists[i].name.chars().next().map(|c| c.to_ascii_lowercase())
                                != Some(current_char)
                        })
                    {
                        self.playlist_select_by_index(prev_pos);
                    }
                }
            }
            _ => {}
        }
    }

    fn get_active_tracks_and_selected(&self) -> Option<(Vec<DiscographySong>, usize)> {
        let (indices, selected) = match self.state.active_tab {
            ActiveTab::Library => (
                search_ranked_indices(&self.tracks, &self.state.tracks_search_term, true),
                self.state.selected_track.selected().unwrap_or(0),
            ),
            ActiveTab::Albums => (
                search_ranked_indices(
                    &self.album_tracks,
                    &self.state.album_tracks_search_term,
                    true,
                ),
                self.state.selected_album_track.selected().unwrap_or(0),
            ),
            ActiveTab::Playlists => (
                search_ranked_indices(
                    &self.playlist_tracks,
                    &self.state.playlist_tracks_search_term,
                    true,
                ),
                self.state.selected_playlist_track.selected().unwrap_or(0),
            ),
            _ => return None,
        };

        if indices.is_empty() {
            return None;
        }

        let items: Vec<DiscographySong> = match self.state.active_tab {
            ActiveTab::Library => indices.iter().map(|&i| self.tracks[i].clone()).collect(),
            ActiveTab::Albums => indices.iter().map(|&i| self.album_tracks[i].clone()).collect(),
            ActiveTab::Playlists => {
                indices.iter().map(|&i| self.playlist_tracks[i].clone()).collect()
            }
            _ => return None,
        };

        if items.is_empty() {
            return None;
        }

        Some((items, selected))
    }

    async fn handle_enter(&mut self) {
        match self.state.active_section {
            ActiveSection::List => {
                if self.state.active_tab == ActiveTab::Library {
                    self.state.tracks_search_term = String::from("");
                    self.state.selected_track.select(Some(0));

                    let artists =
                        search_ranked_refs(&self.artists, &self.state.artists_search_term, true);

                    let selected = self.state.selected_artist.selected().unwrap_or(0);
                    let artist_id = artists.get(selected).map(|a| a.id.clone());

                    if let Some(id) = artist_id {
                        self.discography(&id).await;
                    }
                }

                if self.state.active_tab == ActiveTab::Albums {
                    self.state.album_tracks_search_term = String::from("");
                    self.state.selected_album_track.select(Some(0));
                    let albums =
                        search_ranked_refs(&self.albums, &self.state.albums_search_term, true);

                    let selected = self.state.selected_album.selected().unwrap_or(0);
                    let album_id = albums.get(selected).map(|a| a.id.clone());

                    if let Some(id) = album_id {
                        self.album_tracks(&id).await;
                    }
                }

                if self.state.active_tab == ActiveTab::Playlists {
                    self.open_playlist(Some(200)).await;
                }
            }
            ActiveSection::Tracks => {
                if let Some((items, selected)) = self.get_active_tracks_and_selected() {
                    self.initiate_main_queue(&items, selected).await;
                }
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
                            self.mpv_handle.seek(time, SeekFlag::Absolute).await;
                            self.play().await;
                            self.buffering = true;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    async fn handle_cancel(&mut self) {
        if self.show_help {
            self.show_help = false;
            self.dirty_clear = true;
            return;
        }
        let artist_id = self.get_id_of_selected(&self.artists, Selectable::Artist);
        let album_id = self.get_id_of_selected(&self.albums, Selectable::Album);
        let album_track_id = self.get_id_of_selected(&self.album_tracks, Selectable::AlbumTrack);
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

    fn show_help(&mut self) {
        self.show_help = !self.show_help;
        self.dirty_clear = true;
    }

    async fn emplace_temp(&mut self, start: bool) {
        match self.state.active_section {
            ActiveSection::Tracks => {
                if let Some((items, selected)) = self.get_active_tracks_and_selected() {
                    if start {
                        self.push_next_to_temporary_queue(&items, selected).await;
                    } else {
                        self.push_to_temporary_queue(&items, selected, 1).await;
                    }
                }
            }
            _ => {}
        }
    }

    async fn toggle_favorite(&mut self) {
        match self.state.active_section {
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
                                let _ = set_favorite_artist(
                                    &self.db.pool,
                                    &artist.id,
                                    !artist.user_data.is_favorite,
                                )
                                .await;
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

                                let _ = set_favorite_album(
                                    &self.db.pool,
                                    &album.id,
                                    !album.user_data.is_favorite,
                                )
                                .await;
                                album.user_data.is_favorite = !album.user_data.is_favorite;
                                self.reorder_lists();
                                self.reposition_cursor(&id, Selectable::Album);
                            }
                            if let Some(album) =
                                self.tracks.iter_mut().find(|a| a.id == format!("_album_{}", id))
                            {
                                album.user_data.is_favorite = !album.user_data.is_favorite;
                            }
                        }
                        ActiveTab::Playlists => {
                            let id = self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                            if let Some(playlist) =
                                self.original_playlists.iter_mut().find(|a| a.id == id)
                            {
                                let _ = client
                                    .set_favorite(&playlist.id, !playlist.user_data.is_favorite)
                                    .await;
                                let _ = set_favorite_playlist(
                                    &self.db.pool,
                                    &playlist.id,
                                    !playlist.user_data.is_favorite,
                                )
                                .await;
                                playlist.user_data.is_favorite = !playlist.user_data.is_favorite;
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
                                let _ = set_favorite_track(
                                    &self.db.pool,
                                    &track.id,
                                    !track.user_data.is_favorite,
                                )
                                .await;
                                track.user_data.is_favorite = !track.user_data.is_favorite;
                                if let Some(tr) =
                                    self.state.queue.iter_mut().find(|t| t.id == track.id)
                                {
                                    tr.is_favorite = !tr.is_favorite;
                                }
                                if track.id.starts_with("_album_") {
                                    let id = track.id.replace("_album_", "");
                                    if let Some(album) = self.albums.iter_mut().find(|a| a.id == id)
                                    {
                                        album.user_data.is_favorite = !album.user_data.is_favorite;
                                    }
                                    let _ = set_favorite_album(
                                        &self.db.pool,
                                        &id,
                                        !track.user_data.is_favorite,
                                    )
                                    .await;
                                    if let Some(album) =
                                        self.original_albums.iter_mut().find(|a| a.id == id)
                                    {
                                        album.user_data.is_favorite = !album.user_data.is_favorite;
                                    }
                                    self.reorder_lists();
                                }
                            }
                        }
                        ActiveTab::Albums => {
                            let id =
                                self.get_id_of_selected(&self.album_tracks, Selectable::AlbumTrack);
                            if let Some(track) = self.album_tracks.iter_mut().find(|t| t.id == id) {
                                let _ = client
                                    .set_favorite(&track.id, !track.user_data.is_favorite)
                                    .await;
                                let _ = set_favorite_track(
                                    &self.db.pool,
                                    &track.id,
                                    !track.user_data.is_favorite,
                                )
                                .await;
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
                                let _ = set_favorite_track(
                                    &self.db.pool,
                                    &track.id,
                                    !track.user_data.is_favorite,
                                )
                                .await;
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
        }
    }

    async fn download(&mut self, remove: bool) {
        if self.state.active_section != ActiveSection::Tracks {
            return;
        }

        let (id, selectable, playlist_id) = match self.state.active_tab {
            ActiveTab::Library => {
                (self.get_id_of_selected(&self.tracks, Selectable::Track), Selectable::Track, None)
            }
            ActiveTab::Albums => (
                self.get_id_of_selected(&self.album_tracks, Selectable::AlbumTrack),
                Selectable::AlbumTrack,
                None,
            ),
            ActiveTab::Playlists => (
                self.get_id_of_selected(&self.playlist_tracks, Selectable::PlaylistTrack),
                Selectable::PlaylistTrack,
                Some(self.state.current_playlist.id.clone()),
            ),
            _ => return,
        };

        let tracks = match selectable {
            Selectable::Track => &mut self.tracks,
            Selectable::AlbumTrack => &mut self.album_tracks,
            Selectable::PlaylistTrack => &mut self.playlist_tracks,
            _ => unreachable!(),
        };

        if matches!(selectable, Selectable::Track) && id.starts_with("_album_") {
            let album_id = id.replace("_album_", "");

            let album_tracks =
                self.tracks.iter().filter(|t| t.album_id == album_id).cloned().collect::<Vec<_>>();

            if remove {
                let _ = self
                    .db
                    .cmd_tx
                    .send(Command::Remove(RemoveCommand::Tracks { tracks: album_tracks.clone() }))
                    .await;

                if self.client.is_none() {
                    for track in album_tracks {
                        self.tracks.retain(|t| t.id != track.id);
                        self.album_tracks.retain(|t| t.id != track.id);
                        self.playlist_tracks.retain(|t| t.id != track.id);
                        let _ = self.remove_from_queue_by_id(track.id).await;
                    }
                }
            } else {
                let to_download = album_tracks
                    .into_iter()
                    .filter(|t| !matches!(t.download_status, DownloadStatus::Downloaded))
                    .collect::<Vec<_>>();

                let _ = self
                    .db
                    .cmd_tx
                    .send(Command::Download(DownloadCommand::Tracks { tracks: to_download }))
                    .await;
            }

            return;
        }

        // Single track handling
        if let Some(track) = tracks.iter_mut().find(|t| t.id == id) {
            if remove {
                track.download_status = DownloadStatus::NotDownloaded;

                let _ = self
                    .db
                    .cmd_tx
                    .send(Command::Remove(RemoveCommand::Track { track: track.clone() }))
                    .await;

                if self.client.is_none() {
                    self.tracks.retain(|t| t.id != id);
                    self.album_tracks.retain(|t| t.id != id);
                    self.playlist_tracks.retain(|t| t.id != id);
                    let _ = self.remove_from_queue_by_id(id).await;
                }
            } else {
                let _ = self
                    .db
                    .cmd_tx
                    .send(Command::Download(DownloadCommand::Track {
                        track: track.clone(),
                        playlist_id,
                    }))
                    .await;
            }
        }

        // shared cleanup
        let album_order = crate::helpers::extract_album_order(&self.tracks);
        self.group_tracks_into_albums(self.tracks.clone(), Some(album_order));

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

        if self.tracks.is_empty() && self.album_tracks.is_empty() && self.playlist_tracks.is_empty()
        {
            self.state.active_section = ActiveSection::List;
            self.state.active_tab = ActiveTab::Library;
            self.state.selected_artist.select(Some(0));
            self.state.selected_album.select(Some(0));
            self.state.selected_playlist.select(Some(0));
        }
    }

    /// Opens the playlist with the given ID.
    /// limit: if true, the playlist will be opened with a limit on the number of tracks and fetched fully with a delay
    ///
    pub async fn open_playlist(&mut self, limit: Option<usize>) {
        self.state.playlist_tracks_search_term.clear();
        self.state.selected_playlist_track.select(Some(0));

        let playlist_id = if !self.state.playlists_search_term.is_empty() {
            let playlists =
                search_ranked_refs(&self.playlists, &self.state.playlists_search_term, false);

            let selected = self.state.selected_playlist.selected().unwrap_or(0);
            playlists.get(selected).map(|p| p.id.clone())
        } else {
            let selected = self.state.selected_playlist.selected().unwrap_or(0);
            self.playlists.get(selected).map(|p| p.id.clone())
        };

        let Some(id) = playlist_id else {
            return;
        };

        self.playlist(&id, limit).await;

        let _ = self
            .state
            .playlist_tracks_scroll_state
            .content_length(self.playlist_tracks.len().saturating_sub(1));
    }

    pub fn move_playlist_edit_step(&mut self, direction: i32) {
        if self.client.is_none() {
            // this is an online-only feature
            return;
        }
        // make sure we don't let the user edit while a fetch is ongoing
        if self.playlist_incomplete || self.playlist_stale {
            return;
        }
        self.begin_playlist_edit();

        let idx = self.state.selected_playlist_track.selected().unwrap_or(0);

        let new_idx = (idx as i32 + direction) as isize;
        if new_idx < 0 || new_idx >= self.playlist_tracks.len() as isize {
            return;
        }

        let new_idx = new_idx as usize;

        // swap with neighbor (item keeps moving)
        self.playlist_tracks.swap(idx, new_idx);
        self.playlist_track_select_by_index(new_idx);
    }

    fn begin_playlist_edit(&mut self) {
        if self.playlist_editing || !self.state.playlist_tracks_search_term.is_empty() {
            return;
        }

        let idx = self.state.selected_playlist_track.selected().unwrap_or(0);

        self.playlist_editing = true;
        self.playlist_edit_origin_index = Some(idx);
        self.playlist_edit_item_id = Some(self.playlist_tracks[idx].id.clone());
    }

    pub fn cancel_playlist_edit(&mut self) {
        if !self.playlist_editing {
            return;
        }

        let Some(item_id) = self.playlist_edit_item_id.clone() else {
            return;
        };
        let Some(origin) = self.playlist_edit_origin_index else {
            return;
        };

        if let Some(current) = self.playlist_tracks.iter().position(|t| t.id == item_id) {
            let item = self.playlist_tracks.remove(current);
            self.playlist_tracks.insert(origin, item);
            self.playlist_track_select_by_index(origin);
        }

        self.playlist_editing = false;
        self.playlist_edit_item_id = None;
        self.playlist_edit_origin_index = None;
    }

    pub async fn commit_playlist_edit(&mut self) {
        if !self.playlist_editing {
            return;
        }

        let Some(client) = self.client.as_ref() else { return };

        let playlist_id = self.get_id_of_selected(&self.playlists, Selectable::Playlist);
        let item_id = self.playlist_edit_item_id.clone().unwrap();

        let new_index = self.playlist_tracks.iter().position(|t| t.id == item_id).unwrap();

        client.move_playlist_item(&item_id, &playlist_id, new_index).await.ok();

        self.playlist_editing = false;
        self.playlist_edit_item_id = None;
        self.playlist_edit_origin_index = None;
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
                self.state.artists_search_term = String::from("");

                // find the artist in the artists list using .id
                let artist = self.artists.iter().find(|a| a.id == artist_id);

                if let Some(art) = artist {
                    let index = self.artists.iter().position(|a| a.id == art.id).unwrap_or(0);
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
                        let discography =
                            match get_discography(&self.db.pool, &artist.id, self.client.as_ref())
                                .await
                            {
                                Ok(tracks) if !tracks.is_empty() => Some(tracks),
                                _ => {
                                    if let Some(client) = self.client.as_ref() {
                                        if let Ok(tracks) = client.discography(&artist.id).await {
                                            Some(tracks)
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                }
                            };
                        if let Some(discography) = discography {
                            if let Some(_) = discography.iter().find(|t| t.id == album_id) {
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

                self.state.artists_search_term = String::from("");

                let index = self.artists.iter().position(|a| a.id == artist_id).unwrap_or(0);
                self.artist_select_by_index(index);

                let selected = self.state.selected_artist.selected().unwrap_or(0);
                self.discography(&self.artists[selected].id.clone()).await;
                self.track_select_by_index(0);

                // now find the first track that matches this album
                if let Some(track) = self.tracks.iter().find(|t| t.album_id == album_id) {
                    let index = self.tracks.iter().position(|t| t.id == track.id).unwrap_or(0);
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
                        let discography =
                            match get_discography(&self.db.pool, &artist.id, self.client.as_ref())
                                .await
                            {
                                Ok(tracks) if !tracks.is_empty() => Some(tracks),
                                _ => {
                                    if let Some(client) = self.client.as_ref() {
                                        if let Ok(tracks) = client.discography(&artist.id).await {
                                            Some(tracks)
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                }
                            };
                        if let Some(discography) = discography {
                            if let Some(_) = discography.iter().find(|t| t.id == track_id) {
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
                self.state.artists_search_term = String::from("");

                let index = self.artists.iter().position(|a| a.id == artist_id).unwrap_or(0);
                self.artist_select_by_index(index);

                let selected = self.state.selected_artist.selected().unwrap_or(0);
                self.discography(&self.artists[selected].id.clone()).await;
                self.track_select_by_index(0);

                // now find the first track that matches this album
                if let Some(track) = self.tracks.iter().find(|t| t.id == track_id) {
                    let index = self.tracks.iter().position(|t| t.id == track.id).unwrap_or(0);
                    self.track_select_by_index(index);
                }
            }
        }
    }

    async fn global_search_perform(&mut self) {
        let artists = self
            .original_artists
            .iter()
            .filter(|a| a.name.to_lowercase().contains(&self.search_term.to_lowercase()))
            .cloned()
            .collect::<Vec<Artist>>();
        self.search_result_artists = artists;
        self.search_result_artists
            .sort_by(|a: &Artist, b: &Artist| sort::compare(&a.name, &b.name));

        self.state.selected_search_artist.select(Some(0));
        self.state.search_artist_scroll_state =
            self.state.search_artist_scroll_state.content_length(self.search_result_artists.len());

        let albums = self
            .original_albums
            .iter()
            .filter(|a| a.name.to_lowercase().contains(&self.search_term.to_lowercase()))
            .cloned()
            .collect::<Vec<Album>>();
        self.search_result_albums = albums;
        self.search_result_albums.sort_by(|a: &Album, b: &Album| sort::compare(&a.name, &b.name));

        self.state.selected_search_album.select(Some(0));
        self.state.search_album_scroll_state =
            self.state.search_album_scroll_state.content_length(self.search_result_albums.len());

        let tracks = match &self.client {
            Some(client) => client.search_tracks(self.search_term.clone()).await,
            None => Ok(get_tracks(&self.db.pool, &self.search_term).await.unwrap_or_default()),
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
        self.search_term_last = self.search_term.clone();
        self.search_term = String::from("");

        self.searching = false;
    }
}

fn page_up_list(
    len: usize,
    step: usize,
    state: &mut ratatui::widgets::ListState,
    scroll: &mut ratatui::widgets::ScrollbarState,
) {
    if len == 0 {
        return;
    }
    let cur = state.selected().unwrap_or(0);
    let new = cur.saturating_sub(step.max(1));
    state.select(Some(new));
    for _ in 0..step {
        scroll.prev();
    }
}

fn page_down_list(
    len: usize,
    step: usize,
    state: &mut ratatui::widgets::ListState,
    scroll: &mut ratatui::widgets::ScrollbarState,
) {
    if len == 0 {
        return;
    }
    let cur = state.selected().unwrap_or(0);
    let new = (cur + step.max(1)).min(len.saturating_sub(1));
    state.select(Some(new));
    for _ in 0..step {
        scroll.next();
    }
}

fn page_up_table(
    len: usize,
    step: usize,
    state: &mut ratatui::widgets::TableState,
    scroll: &mut ratatui::widgets::ScrollbarState,
) {
    if len == 0 {
        return;
    }
    let cur = state.selected().unwrap_or(0);
    let new = cur.saturating_sub(step.max(1));
    state.select(Some(new));
    for _ in 0..step {
        scroll.prev();
    }
}

fn page_down_table(
    len: usize,
    step: usize,
    state: &mut ratatui::widgets::TableState,
    scroll: &mut ratatui::widgets::ScrollbarState,
) {
    if len == 0 {
        return;
    }
    let cur = state.selected().unwrap_or(0);
    let new = (cur + step.max(1)).min(len.saturating_sub(1));
    state.select(Some(new));
    for _ in 0..step {
        scroll.next();
    }
}

fn move_down(selected: Option<usize>, len: usize) -> usize {
    let sel = selected.unwrap_or(len.saturating_sub(1));
    if sel + 1 >= len {
        sel
    } else {
        sel + 1
    }
}

fn move_up(selected: Option<usize>) -> usize {
    selected.unwrap_or(0).saturating_sub(1)
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
