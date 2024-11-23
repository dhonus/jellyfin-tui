/// This file has all the queue control functions
/// the basic idea is keeping our queue in sync with mpv and doing some basic operations
/// 

use crate::tui::{App, Song};

impl App {
    /// This is the main queue control function. It basically initiates a new queue when we play a song without modifiers
    /// 
    pub async fn replace_queue(&mut self) {
        let selected = self.selected_track.selected().unwrap_or(0);
        if let Some(client) = &self.client {
            if let Ok(mut mpv) = self.mpv_state.lock() {
                mpv.should_stop = true;
            }

            let results = self.track_search_results();

            let skip = match self.tracks_search_term.len() {
                0 => selected,
                _ => self.tracks.iter().position(|t| t.id == results[selected]).unwrap_or(0),
            };

            // the playlist MPV will be getting
            self.playlist = self
                .tracks
                .iter()
                .skip(skip)
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

            let _ = self.mpv_start_playlist(); // TODO: inform user of error
        }
    }

    /// Append the selected track to the end of the queue
    /// 
    pub async fn push_to_queue(&mut self) {
        // if self.playlist.len() == 0 {
        //     self.replace_queue().await;
        //     return;
        // }
        if let Some(client) = &self.client {
            let selected = self.selected_track.selected().unwrap_or(0);
            // if we shift click we only appned the selected track to the playlist
            let track = &self.tracks[selected];
            let song = Song {
                id: track.id.clone(),
                url: client.song_url_sync(track.id.clone()),
                name: track.name.clone(),
                artist: track.album_artist.clone(),
                artist_items: track.artist_items.clone(),
                album: track.album.clone(),
                parent_id: track.parent_id.clone(),
                production_year: track.production_year,
            };
            let url = song.url.clone();
            self.playlist.push(song);

            // if mpv is all good we append to queue
            let mpv = self.mpv_state.lock().unwrap();
            mpv.mpv
                .command("loadfile", &[url.as_str(), "append"])
                .map_err(|e| format!("Failed to load playlist: {:?}", e)).ok();
        }
    }

    /// Add a new song right aftter the currently playing song
    /// 
    pub async fn push_next_to_queue(&mut self) {
        // if self.playlist.len() == 0 {
        //     self.replace_queue().await;
        //     return;
        // }
        if let Some(client) = &self.client {
            let selected = self.selected_track.selected().unwrap_or(0);
            let selected_queue_item = self.selected_queue_item.selected().unwrap_or(0);
            // if we shift click we only appned the selected track to the playlist
            let track = &self.tracks[selected];
            let song = Song {
                id: track.id.clone(),
                url: client.song_url_sync(track.id.clone()),
                name: track.name.clone(),
                artist: track.album_artist.clone(),
                artist_items: track.artist_items.clone(),
                album: track.album.clone(),
                parent_id: track.parent_id.clone(),
                production_year: track.production_year,
            };
            let url = song.url.clone();
            self.playlist.insert(selected_queue_item + 1, song);

            // if mpv is all good we append to queue
            let mpv = self.mpv_state.lock().unwrap ();
            mpv.mpv
                .command("loadfile", &[url.as_str(), "insert-next"])
                .map_err(|e| format!("Failed to load playlist: {:?}", e)).ok();

            // get the track-list
            // let count: i64 = mpv.mpv.get_property("playlist/count").unwrap_or(0);
            // let track_list: Vec<MpvNode> = Vec::with_capacity(count as usize);
            // println!("{:?}", count);

            // let second: String = mpv.mpv.get_property("playlist/1/filename").unwrap_or("".to_string());
            // println!("So these wont be the same sad sad {second}{:?}", self.playlist.get(1).unwrap().url);
            // // compare the strings
            // println!("{:?}", self.playlist.get(1).unwrap().url == second);

        }
    }

    /// Remove the *selected* song from the queue
    /// 
    pub async fn pop_from_queue(&mut self) {
        if self.playlist.is_empty() {
            return;
        }
        if let Some(selected_queue_item) = self.selected_queue_item.selected() {
            if let Ok(mpv) = self.mpv_state.lock() {
                self.playlist.remove(selected_queue_item);
                mpv.mpv
                    .command("playlist-remove", &[selected_queue_item.to_string().as_str()])
                    .map_err(|e| format!("Failed to remove from playlist: {:?}", e)).ok();
            }
        }
    }

    /// Swap the selected song with the one above it
    /// 
    pub async fn move_queue_item_up(&mut self) {
        if self.playlist.is_empty() {
            return;
        }
        if let Some(selected_queue_item) = self.selected_queue_item.selected() {
            if selected_queue_item == 0 {
                return;
            }

            // i don't think i've ever disliked an API more
            if let Ok(mpv) = self.mpv_state.lock() {
                mpv.mpv.command("playlist-move", &[
                    selected_queue_item.to_string().as_str(),
                    (selected_queue_item - 1).to_string().as_str()
                ]).map_err(|e| format!("Failed to move playlist item: {:?}", e)).ok();
            }
            self.selected_queue_item.select(Some(selected_queue_item - 1));

            self.playlist.swap(selected_queue_item, selected_queue_item - 1);

            // if we moved the current song either directly or by moving the song above it
            // we need to update the current index
            if self.current_playback_state.current_index == selected_queue_item as i64 {
                self.current_playback_state.current_index -= 1;
            } else if self.current_playback_state.current_index == (selected_queue_item - 1) as i64 {
                self.current_playback_state.current_index += 1;
            }

            // discard next poll
            self.receiver.try_recv().ok();

            #[cfg(debug_assertions)] { self.__debug_error_corrector_tm(); }
        }
    }

    /// Swap the selected song with the one below it
    /// 
    pub async fn move_queue_item_down(&mut self) {
        if self.playlist.is_empty() {
            return;
        }
        if let Some(selected_queue_item) = self.selected_queue_item.selected() {
            if selected_queue_item == self.playlist.len() - 1 {
                return;
            }

            if let Ok(mpv) = self.mpv_state.lock() {
                mpv.mpv.command("playlist-move", &[
                    (selected_queue_item + 1).to_string().as_str(),
                    selected_queue_item.to_string().as_str(),
                ]).map_err(|e| format!("Failed to move playlist item: {:?}", e)).ok();
            }

            self.playlist.swap(selected_queue_item, selected_queue_item + 1);

            // if we moved the current song either directly or by moving the song above it
            // we need to update the current index
            if self.current_playback_state.current_index == selected_queue_item as i64 {
                self.current_playback_state.current_index += 1;
            } else if self.current_playback_state.current_index == (selected_queue_item + 1) as i64 {
                self.current_playback_state.current_index -= 1;
            }

            self.selected_queue_item.select(Some(selected_queue_item + 1));

            // discard next poll
            self.receiver.try_recv().ok();

            #[cfg(debug_assertions)] { self.__debug_error_corrector_tm(); }
        }
    }

    /// (debug) Sync the queue with mpv and scream about it. 
    /// It is a patently stupid function that should not exist, but the mpv api is not great
    /// Can be removed from well tested code
    /// 
    fn __debug_error_corrector_tm(&mut self) {

        let mut mpv_playlist = Vec::new();

        if let Ok(mpv) = self.mpv_state.lock() {
            let mut i = 0;
            for _ in self.playlist.iter() {
                let mpv_url = mpv.mpv.get_property(format!("playlist/{}/filename", i).as_str()).unwrap_or("".to_string());
                mpv_playlist.push(mpv_url);
                i += 1;
            }
            let mut new_playlist = Vec::new();
            for mpv_url in mpv_playlist.iter() {
                for song in self.playlist.iter() {
                    if &song.url == mpv_url {
                        new_playlist.push(song.clone());
                        break;
                    }
                }
            }
            for (i, song) in self.playlist.iter().enumerate() {
                if song.url != mpv_playlist[i] {
                    println!("[##] position changed {} != {}", song.url, mpv_playlist[i]);
                }
            }
            self.playlist = new_playlist;
        }
    }
}
