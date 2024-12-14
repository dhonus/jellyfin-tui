use std::fs::OpenOptions;

use crate::{client::Artist, tui::Song};
use dirs::cache_dir;

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

use ratatui::widgets::TableState;
use serde::{Serialize, Deserialize};
#[derive(Serialize, Deserialize)]
pub struct State {
    pub selected_artist: Option<Artist>,
    pub selected_track: Option<TableState>,
    pub queue: Option<Vec<Song>>, // (URL, Title, Artist, Album)
    pub current_song: Option<Song>,
    pub position: Option<f64>,
    pub current_index: Option<i64>,
}

impl State {
    pub fn new() -> State {
        State {
            selected_artist: None,
            selected_track: None,
            queue: None,
            current_song: None,
            position: None,
            current_index: None,
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

    pub fn from_saved_state() -> Result<State, Box<dyn std::error::Error>> {
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
                    let state: State = serde_json::from_reader(file)?;
                    Ok(state)
                }
                Err(_) => {
                    Ok(State::new())
                }
            }
    }

}