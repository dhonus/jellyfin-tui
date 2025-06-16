use dirs::data_dir;
use ratatui::widgets::{ListState, ScrollbarState, TableState};
use std::fs::OpenOptions;

use crate::{
    client::{Album, Artist, Playlist},
    keyboard::{ActiveSection, ActiveTab, SearchSection},
    popup::PopupMenu,
    tui::{Filter, MpvPlaybackState, Repeat, Song, Sort},
};

pub fn find_all_subsequences(needle: &str, haystack: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut needle_chars = needle.chars();
    let mut current_needle_char = needle_chars.next();

    let mut current_byte_index = 0;

    for haystack_char in haystack.chars() {
        if let Some(needle_char) = current_needle_char {
            if haystack_char == needle_char {
                ranges.push((
                    current_byte_index,
                    current_byte_index + haystack_char.len_utf8(),
                ));
                current_needle_char = needle_chars.next();
            }
        }
        current_byte_index += haystack_char.len_utf8();
    }

    if current_needle_char.is_none() {
        ranges
    } else {
        Vec::new()
    }
}

/// This struct should contain all the values that should **PERSIST** when the app is closed and reopened.
/// This is PER SERVER, so if you have multiple servers, each will have its own state.
///
#[derive(serde::Serialize, serde::Deserialize)]
pub struct State {
    // (URL, Title, Artist, Album)
    #[serde(default)]
    pub queue: Vec<Song>,
    // Music - active section (Artists, Tracks, Queue)
    #[serde(default)]
    pub active_section: ActiveSection, // current active section (Artists, Tracks, Queue)
    #[serde(default)]
    pub last_section: ActiveSection, // last active section
    // Search - active section (Artists, Albums, Tracks)
    #[serde(default)]
    pub search_section: SearchSection, // current active section (Artists, Albums, Tracks)

    // active tab (Music, Search)
    #[serde(default)]
    pub active_tab: ActiveTab,
    #[serde(default)]
    pub current_artist: Artist,
    #[serde(default)]
    pub current_album: Album,
    #[serde(default)]
    pub current_playlist: Playlist,

    // ratatui list indexes
    #[serde(default)]
    pub selected_artist: ListState,
    #[serde(default)]
    pub selected_track: TableState,
    #[serde(default)]
    pub selected_album: ListState,
    #[serde(default)]
    pub selected_album_track: TableState,
    #[serde(default)]
    pub selected_playlist_track: TableState,
    #[serde(default)]
    pub selected_playlist: ListState,
    #[serde(default)]
    pub artists_scroll_state: ScrollbarState,
    #[serde(default)]
    pub tracks_scroll_state: ScrollbarState,
    #[serde(default)]
    pub albums_scroll_state: ScrollbarState,
    #[serde(default)]
    pub album_tracks_scroll_state: ScrollbarState,
    #[serde(default)]
    pub playlists_scroll_state: ScrollbarState,
    #[serde(default)]
    pub playlist_tracks_scroll_state: ScrollbarState,
    #[serde(default)]
    pub selected_queue_item: ListState,
    #[serde(default)]
    pub selected_queue_item_manual_override: bool,
    #[serde(default)]
    pub selected_lyric: ListState,
    #[serde(default)]
    pub selected_lyric_manual_override: bool,
    #[serde(default)]
    pub current_lyric: usize,
    #[serde(default)]
    pub selected_search_artist: ListState,
    #[serde(default)]
    pub selected_search_album: ListState,
    #[serde(default)]
    pub selected_search_track: ListState,

    #[serde(default)]
    pub artists_search_term: String,
    #[serde(default)]
    pub albums_search_term: String,
    #[serde(default)]
    pub album_tracks_search_term: String,
    #[serde(default)]
    pub tracks_search_term: String,
    #[serde(default)]
    pub playlist_tracks_search_term: String,
    #[serde(default)]
    pub playlists_search_term: String,

    // scrollbars for search results
    #[serde(default)]
    pub search_artist_scroll_state: ScrollbarState,
    #[serde(default)]
    pub search_album_scroll_state: ScrollbarState,
    #[serde(default)]
    pub search_track_scroll_state: ScrollbarState,

    #[serde(default)]
    pub shuffle: bool,

    #[serde(default)]
    pub current_playback_state: MpvPlaybackState,
}

impl State {
    pub fn new() -> State {
        State {
            queue: vec![],
            active_section: ActiveSection::default(),
            last_section: ActiveSection::default(),
            search_section: SearchSection::default(),
            active_tab: ActiveTab::default(),
            current_artist: Artist::default(),
            current_album: Album::default(),
            current_playlist: Playlist::default(),
            selected_artist: ListState::default(),
            selected_track: TableState::default(),
            selected_album: ListState::default(),
            selected_album_track: TableState::default(),
            selected_playlist_track: TableState::default(),
            selected_playlist: ListState::default(),
            tracks_scroll_state: ScrollbarState::default(),
            albums_scroll_state: ScrollbarState::default(),
            album_tracks_scroll_state: ScrollbarState::default(),
            artists_scroll_state: ScrollbarState::default(),
            playlists_scroll_state: ScrollbarState::default(),
            playlist_tracks_scroll_state: ScrollbarState::default(),
            selected_queue_item: ListState::default(),
            selected_queue_item_manual_override: false,
            selected_lyric: ListState::default(),
            selected_lyric_manual_override: false,
            current_lyric: 0,

            selected_search_artist: ListState::default(),
            selected_search_album: ListState::default(),
            selected_search_track: ListState::default(),

            artists_search_term: String::from(""),
            albums_search_term: String::from(""),
            album_tracks_search_term: String::from(""),
            tracks_search_term: String::from(""),
            playlist_tracks_search_term: String::from(""),
            playlists_search_term: String::from(""),

            search_artist_scroll_state: ScrollbarState::default(),
            search_album_scroll_state: ScrollbarState::default(),
            search_track_scroll_state: ScrollbarState::default(),

            shuffle: false,

            current_playback_state: MpvPlaybackState {
                percentage: 0.0,
                duration: 0.0,
                current_index: 0,
                last_index: -1,
                volume: 100,
                audio_bitrate: 0,
                audio_samplerate: 0,
                file_format: String::from(""),
                hr_channels: String::from(""),
            },
        }
    }

    /// Save the current state to a file. We keep separate files for offline and online states.
    /// 
    pub fn save(&self, server_id: &String, offline: bool) -> Result<(), Box<dyn std::error::Error>> {
        let data_dir = data_dir().unwrap();
        let states_dir = data_dir.join("jellyfin-tui").join("states");
        match OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .append(false)
            .open(states_dir
                .join(if offline { format!("offline_{}.json", server_id) } else { format!("{}.json", server_id) })
            )
        {
            Ok(file) => {
                serde_json::to_writer(file, &self)?;
            }
            Err(_) => {
                return Err("Could not open state file".into());
            }
        }
        Ok(())
    }

    /// Load the state from a file. We keep separate files for offline and online states.
    /// 
    pub fn load(server_id: &String, is_offline: bool) -> Result<State, Box<dyn std::error::Error>> {
        let data_dir = data_dir().unwrap();
        let states_dir = data_dir.join("jellyfin-tui").join("states");
        match OpenOptions::new()
            .read(true)
            .open(states_dir
                .join(if is_offline { format!("offline_{}.json", server_id) } else { format!("{}.json", server_id) })
            )
        {
            Ok(file) => {
                let state: State = serde_json::from_reader(file)?;
                Ok(state)
            }
            Err(_) => Ok(State::new()),
        }
    }
}


/// This one is similar, but it's preferences independent of the server. Applies to ALL servers.
/// 
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Preferences {
    // repeat mode
    #[serde(default)]
    pub repeat: Repeat,
    #[serde(default)]
    pub large_art: bool,
    
    #[serde(default)]
    pub transcoding: bool,

    #[serde(default)]
    pub artist_filter: Filter,
    #[serde(default)]
    pub artist_sort: Sort,
    #[serde(default)]
    pub album_filter: Filter,
    #[serde(default)]
    pub album_sort: Sort,
    #[serde(default)]
    pub playlist_filter: Filter,
    #[serde(default)]
    pub playlist_sort: Sort,

    #[serde(default)]
    pub preferred_global_shuffle: Option<PopupMenu>,
}

impl Preferences {
    pub fn new() -> Preferences {
        Preferences {
            repeat: Repeat::All,
            large_art: false,
            
            transcoding: false,

            artist_filter: Filter::default(),
            artist_sort: Sort::default(),
            album_filter: Filter::default(),
            album_sort: Sort::default(),
            playlist_filter: Filter::default(),
            playlist_sort: Sort::default(),

            preferred_global_shuffle: Some(PopupMenu::GlobalShuffle {
                tracks_n: 100,
                only_played: true,
                only_unplayed: false,
            }),
        }
    }

    /// Save the current state to a file. We keep separate files for offline and online states.
    /// 
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let data_dir = data_dir().unwrap();
        let states_dir = data_dir.join("jellyfin-tui");
        match OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .append(false)
            .open(states_dir.join("preferences.json"))
        {
            Ok(file) => {
                serde_json::to_writer(file, &self)?;
            }
            Err(_) => {
                return Err("Could not open state file".into());
            }
        }
        Ok(())
    }

    /// Load the state from a file. We keep separate files for offline and online states.
    /// 
    pub fn load() -> Result<Preferences, Box<dyn std::error::Error>> {
        let data_dir = data_dir().unwrap();
        let states_dir = data_dir.join("jellyfin-tui");
        match OpenOptions::new()
            .read(true)
            .open(states_dir.join("preferences.json"))
        {
            Ok(file) => {
                let prefs: Preferences = serde_json::from_reader(file)?;
                Ok(prefs)
            }
            Err(_) => Ok(Preferences::new()),
        }
    }
}
