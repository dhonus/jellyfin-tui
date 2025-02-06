use std::fs::OpenOptions;
use dirs::cache_dir;
use ratatui::widgets::{ListState, ScrollbarState, TableState};

use crate::{
    client::{Artist, Playlist}, keyboard::{ActiveSection, ActiveTab, SearchSection}, popup::PopupMenu, tui::{Filter, MpvPlaybackState, Repeat, Sort}
};

pub fn find_all_subsequences(needle: &str, haystack: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut needle_chars = needle.chars();
    let mut current_needle_char = needle_chars.next();

    let mut current_byte_index = 0;

    for haystack_char in haystack.chars() {
        if let Some(needle_char) = current_needle_char {
            if haystack_char == needle_char {
                ranges.push(
                    (current_byte_index, current_byte_index + haystack_char.len_utf8())
                );
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


impl crate::tui::State {
    pub fn new() -> crate::tui::State {
        crate::tui::State {
            queue: vec![],
            active_section: ActiveSection::default(),
            last_section: ActiveSection::default(),
            search_section: SearchSection::default(),
            active_tab: ActiveTab::default(),
            current_artist: Artist::default(),
            current_playlist: Playlist::default(),
            selected_artist: ListState::default(),
            selected_track: TableState::default(),
            selected_playlist_track: TableState::default(),
            selected_playlist: ListState::default(),
            tracks_scroll_state: ScrollbarState::default(),
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
            tracks_search_term: String::from(""),
            playlist_tracks_search_term: String::from(""),
            playlists_search_term: String::from(""),

            search_artist_scroll_state: ScrollbarState::default(),
            search_album_scroll_state: ScrollbarState::default(),
            search_track_scroll_state: ScrollbarState::default(),

            repeat: Repeat::All,
            shuffle: false,
            large_art: false, 

            artist_filter: Filter::default(),
            artist_sort: Sort::default(),
            playlist_filter: Filter::default(),
            playlist_sort: Sort::default(),

            preffered_global_shuffle: PopupMenu::GlobalShuffle { tracks_n: 100, only_played: true, only_unplayed: false },

            current_playback_state: MpvPlaybackState {
                percentage: 0.0,
                duration: 0.0,
                current_index: 0,
                last_index: -1,
                volume: 100,
                audio_bitrate: 0,
                file_format: String::from(""),
            },
        }
    }

    pub fn save_state(&self) -> Result<(), Box<dyn std::error::Error>> {
        let cache_dir = match cache_dir() {
            Some(dir) => dir,
            None => {
                return Err("Could not find cache directory".into());
            }
        };
        match OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .append(false)
            .open(cache_dir.join("jellyfin-tui").join("state.json")) {
                Ok(file) => {
                    serde_json::to_writer(file, &self)?;
                }
                Err(_) => {
                    return Err("Could not open state file".into());
                }
            }
        Ok(())
    }

    pub fn load_state() -> Result<crate::tui::State, Box<dyn std::error::Error>> {
        let cache_dir = match cache_dir() {
            Some(dir) => dir,
            None => {
                return Err("Could not find cache directory".into());
            }
        };
        match OpenOptions::new()
            .read(true)
            .open(cache_dir.join("jellyfin-tui").join("state.json")) {
                Ok(file) => {
                    let state: crate::tui::State = serde_json::from_reader(file)?;
                    Ok(state)
                }
                Err(_) => {
                    Ok(crate::tui::State::new())
                }
            }
    }

}