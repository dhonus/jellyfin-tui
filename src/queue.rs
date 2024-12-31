/// This file has all the queue control functions
/// the basic idea is keeping our queue in sync with mpv and doing some basic operations
///

use crate::{client::DiscographySong, tui::{App, Song}};

impl App {
    /// This is the main queue control function. It basically initiates a new queue when we play a song without modifiers
    ///
    pub fn replace_queue(&mut self, tracks: &Vec<DiscographySong>, skip: usize) {
        if tracks.is_empty() {
            return;
        }
        if let Some(client) = &self.client {

            let selected_is_album = tracks.get(skip).map_or(false, |t| t.id == "_album_");

            // the playlist MPV will be getting
            self.queue = tracks
                .iter()
                .skip(skip)
                // if selected is an album, this will filter out all the tracks that are not part of the album   
                .filter(|track| !selected_is_album || track.parent_id == tracks.get(skip + 1).map_or("", |t| &t.parent_id))
                .filter(|track| track.id != "_album_") // and then we filter out the album itself
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
                        is_in_queue: false,
                        is_transcoded: client.transcoding.enabled,
                        is_favorite: track.user_data.is_favorite,
                    }
                })
                .collect();

            let _ = self.mpv_start_playlist(); // TODO: inform user of error
        }
    }

    fn replace_queue_one_track(&mut self, tracks: &Vec<DiscographySong>, skip: usize) {
        if tracks.is_empty() { 
            return;
        }

        if let Some(client) = &self.client {
            let track = &tracks[skip];
            if track.id == "_album_" {
                return;
            }
            let song = Song {
                id: track.id.clone(),
                url: client.song_url_sync(track.id.clone()),
                name: track.name.clone(),
                artist: track.album_artist.clone(),
                artist_items: track.artist_items.clone(),
                album: track.album.clone(),
                parent_id: track.parent_id.clone(),
                production_year: track.production_year,
                is_in_queue: false,
                is_transcoded: client.transcoding.enabled,
                is_favorite: track.user_data.is_favorite,
            };

            self.queue = vec![song];

            let _ = self.mpv_start_playlist(); // TODO: inform user of error
        }
    }

    /// Append the tracks to the end of the queue
    ///
    pub async fn append_to_queue(&mut self, tracks: &Vec<DiscographySong>, skip: usize) {
        if self.queue.is_empty() {
            self.replace_queue(tracks, skip);
            return;
        }
        if let Some(client) = &self.client {
            let mut new_queue: Vec<Song> = Vec::new();
            for track in tracks.iter().skip(skip) {
                if track.id == "_album_" {
                    continue;
                }
                let song = Song {
                    id: track.id.clone(),
                    url: client.song_url_sync(track.id.clone()),
                    name: track.name.clone(),
                    artist: track.album_artist.clone(),
                    artist_items: track.artist_items.clone(),
                    album: track.album.clone(),
                    parent_id: track.parent_id.clone(),
                    production_year: track.production_year,
                    is_in_queue: false,
                    is_transcoded: client.transcoding.enabled,
                    is_favorite: track.user_data.is_favorite,
                };
                new_queue.push(song);
            }

            if let Ok(mpv) = self.mpv_state.lock() {
                for song in new_queue.iter() {
                    let _ = mpv.mpv.command("loadfile", &[song.url.as_str(), "append"]);
                }
            }

            self.queue.extend(new_queue);
        }
    }

    /// Append the selected track to the end of the queue
    ///
    pub async fn push_to_queue(&mut self, tracks: &Vec<DiscographySong>, skip: usize) {
        if self.queue.is_empty() || tracks.is_empty() {
            self.replace_queue_one_track(tracks, skip);
            return;
        }
        if let Some(client) = &self.client {

            // if we shift click we only appned the selected track to the playlist
            let track = &tracks[skip];
            if track.id == "_album_" {
                self.push_album_to_queue(false).await;
                return;
            }
            let song = Song {
                id: track.id.clone(),
                url: client.song_url_sync(track.id.clone()),
                name: track.name.clone(),
                artist: track.album_artist.clone(),
                artist_items: track.artist_items.clone(),
                album: track.album.clone(),
                parent_id: track.parent_id.clone(),
                production_year: track.production_year,
                is_in_queue: true,
                is_transcoded: client.transcoding.enabled,
                is_favorite: track.user_data.is_favorite,
            };
            let url = song.url.clone();

            let mut selected_queue_item = -1;
            for (i, song) in self.queue.iter().enumerate() {
                if song.is_in_queue {
                    selected_queue_item = i as i64;
                }
            }

            if selected_queue_item == -1 {
                selected_queue_item = self.selected_queue_item.selected().unwrap_or(0) as i64;
            }

            let mpv = match self.mpv_state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };

            if let Ok(_) = mpv.mpv.command("loadfile", &[url.as_str(), "insert-at", (selected_queue_item + 1).to_string().as_str()]) {
                self.queue.insert((selected_queue_item + 1) as usize, song);
            }
        }
    }

    async fn push_album_to_queue(&mut self, start: bool) {
        let selected = self.selected_track.selected().unwrap_or(0);
        if let Some(client) = &self.client {
            let album_id = self.tracks[selected].parent_id.clone();
            let album = self.tracks[selected].album.clone();
            let album_artist = self.tracks[selected].album_artist.clone();
            let tracks = self.tracks.iter().skip(selected + 1).take_while(|t| t.parent_id == album_id).collect::<Vec<_>>();

            let mut selected_queue_item = -1;
            for (i, song) in self.queue.iter().enumerate() {
                if song.is_in_queue && !start {
                    selected_queue_item = i as i64;
                }
            }

            if selected_queue_item == -1 {
                selected_queue_item = self.selected_queue_item.selected().unwrap_or(0) as i64;
            }

            let mpv = match self.mpv_state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };

            for track in tracks.iter().rev() {
                let song = Song {
                    id: track.id.clone(),
                    url: client.song_url_sync(track.id.clone()),
                    name: track.name.clone(),
                    artist: album_artist.clone(),
                    artist_items: track.artist_items.clone(),
                    album: album.clone(),
                    parent_id: album_id.clone(),
                    production_year: track.production_year,
                    is_in_queue: true,
                    is_transcoded: client.transcoding.enabled,
                    is_favorite: track.user_data.is_favorite,
                };

                if let Ok(_) = mpv.mpv.command("loadfile", &[song.url.as_str(), "insert-at", (selected_queue_item + 1).to_string().as_str()]) {
                    self.queue.insert((selected_queue_item + 1) as usize, song);
                }
            }
        }
    }

    /// Add a new song right aftter the currently playing song
    ///
    pub async fn push_next_to_queue(&mut self, tracks: &Vec<DiscographySong>, skip: usize) {
        if self.queue.is_empty() {
            self.replace_queue_one_track(tracks, skip);
            return;
        }
        if let Some(client) = &self.client {
            let selected_queue_item = self.selected_queue_item.selected().unwrap_or(0);
            // if we shift click we only appned the selected track to the playlist
            let track = &tracks[skip];
            if track.id == "_album_" {
                self.push_album_to_queue(true).await;
                return;
            }
            let song = Song {
                id: track.id.clone(),
                url: client.song_url_sync(track.id.clone()),
                name: track.name.clone(),
                artist: track.album_artist.clone(),
                artist_items: track.artist_items.clone(),
                album: track.album.clone(),
                parent_id: track.parent_id.clone(),
                production_year: track.production_year,
                is_in_queue: true,
                is_transcoded: client.transcoding.enabled,
                is_favorite: track.user_data.is_favorite,
            };

            let mpv = match self.mpv_state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };

            if let Ok(_) = mpv.mpv.command("loadfile", &[song.url.as_str(), "insert-next"]) {
                self.queue.insert(selected_queue_item as usize + 1, song);
            }

            // get the track-list
            // let count: i64 = mpv.mpv.get_property("playlist/count").unwrap_or(0);
            // let track_list: Vec<MpvNode> = Vec::with_capacity(count as usize);
            // println!("{:?}", count);

            // let second: String = mpv.mpv.get_property("playlist/1/filename").unwrap_or("".to_string());
            // println!("So these wont be the same sad sad {second}{:?}", self.queue.get(1).unwrap().url);
            // // compare the strings
            // println!("{:?}", self.queue.get(1).unwrap().url == second);

        }
    }

    /// Remove the *selected* song from the queue
    ///
    pub async fn pop_from_queue(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        let selected_queue_item = match self.selected_queue_item.selected() {
            Some(item) => item,
            None => return,
        };

        let mpv = match self.mpv_state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };

        if let Ok(_) = mpv.mpv.command("playlist-remove", &[selected_queue_item.to_string().as_str()]) {
            self.queue.remove(selected_queue_item);
        }
    }

    /// Clear the queue
    /// 
    pub async fn clear_queue(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        if let Ok(mpv) = self.mpv_state.lock() {
            for i in (0..self.queue.len()).rev() {
                if !self.queue[i].is_in_queue {
                    continue;
                }
                if let Ok(_) = mpv.mpv.command("playlist-remove", &[i.to_string().as_str()]) {
                    self.queue.remove(i);
                }
            }
        }
    }

    /// Essentially, because of the queue itself being temporary we need to handle interactions differently
    /// If we play a song *outside* the queue, we MOVE the queue to that new position (remove, insert there, play selected)
    /// If we play a song *inside* the queue, we just play it
    ///
    pub async fn relocate_queue_and_play(&mut self) {
        if let Ok(mpv) = self.mpv_state.lock() {
            // get a list of all the songs in the queue
            let mut queue: Vec<Song> = self.queue.iter().filter(|s| s.is_in_queue).cloned().collect();
            let queue_len = queue.len();

            let mut index = self.selected_queue_item.selected().unwrap_or(0);
            let after: bool = index >= self.current_playback_state.current_index as usize;

            // early return in case we're within queue bounds
            if self.queue[index].is_in_queue {
                let _ = mpv.mpv.command("playlist-play-index", &[&index.to_string()]);
                if self.paused {
                    let _ = mpv.mpv.set_property("pause", false);
                    self.paused = false;
                }
                self.selected_queue_item.select(Some(index));
                return;
            }

            // Delete all songs before the selected song
            for i in (0..self.queue.len()).rev() {
                if let Some(song) = self.queue.get(i as usize) {
                    if song.is_in_queue {
                        self.queue.remove(i as usize);
                        mpv.mpv.command("playlist_remove", &[&i.to_string()]).ok();
                    }
                }
            }

            if after {
                index -= queue_len;
            }
            self.selected_queue_item.select(Some(index));

            // to put them back in the queue in the correct order
            queue.reverse();

            for song in queue {
                if (index + 1) > self.queue.len() {
                    let _ = mpv.mpv.command("loadfile", &[song.url.as_str(), "append"]);
                    self.queue.push(song);
                } else {
                    let _ = mpv.mpv.command("loadfile", &[song.url.as_str(), "insert-at", (index + 1).to_string().as_str()]);
                    self.queue.insert(index + 1, song);
                }
            }

            let _ = mpv.mpv.command("playlist-play-index", &[&index.to_string()]);
            if self.paused {
                let _ = mpv.mpv.set_property("pause", false);
                self.paused = false;
            }
        }
    }

    /// Swap the selected song with the one above it
    ///
    pub async fn move_queue_item_up(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        if let Some(selected_queue_item) = self.selected_queue_item.selected() {
            if selected_queue_item == 0 {
                return;
            }

            if let Some(src) = self.queue.get(selected_queue_item) {
                if let Some(dst) = self.queue.get(selected_queue_item - 1) {
                    if src.is_in_queue != dst.is_in_queue {
                        return;
                    }
                }
            }

            // i don't think i've ever disliked an API more
            if let Ok(mpv) = self.mpv_state.lock() {
                let _ = mpv.mpv.command("playlist-move", &[
                    selected_queue_item.to_string().as_str(),
                    (selected_queue_item - 1).to_string().as_str()
                ]).map_err(|e| format!("Failed to move playlist item: {:?}", e));
            }
            self.selected_queue_item.select(Some(selected_queue_item - 1));

            self.queue.swap(selected_queue_item, selected_queue_item - 1);

            // if we moved the current song either directly or by moving the song above it
            // we need to update the current index
            if self.current_playback_state.current_index == selected_queue_item as i64 {
                self.current_playback_state.current_index -= 1;
            } else if self.current_playback_state.current_index == (selected_queue_item - 1) as i64 {
                self.current_playback_state.current_index += 1;
            }

            // discard next poll
            let _ = self.receiver.try_recv();

            #[cfg(debug_assertions)] { self.__debug_error_corrector_tm(); }
        }
    }

    /// Swap the selected song with the one below it
    ///
    pub async fn move_queue_item_down(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        if let Some(selected_queue_item) = self.selected_queue_item.selected() {
            if selected_queue_item == self.queue.len() - 1 {
                return;
            }

            if let Some(src) = self.queue.get(selected_queue_item) {
                if let Some(dst) = self.queue.get(selected_queue_item + 1) {
                    if src.is_in_queue != dst.is_in_queue {
                        return;
                    }
                }
            }

            if let Ok(mpv) = self.mpv_state.lock() {
                let _ = mpv.mpv.command("playlist-move", &[
                    (selected_queue_item + 1).to_string().as_str(),
                    selected_queue_item.to_string().as_str(),
                ]).map_err(|e| format!("Failed to move playlist item: {:?}", e));
            }

            self.queue.swap(selected_queue_item, selected_queue_item + 1);

            // if we moved the current song either directly or by moving the song above it
            // we need to update the current index
            if self.current_playback_state.current_index == selected_queue_item as i64 {
                self.current_playback_state.current_index += 1;
            } else if self.current_playback_state.current_index == (selected_queue_item + 1) as i64 {
                self.current_playback_state.current_index -= 1;
            }

            self.selected_queue_item.select(Some(selected_queue_item + 1));

            // discard next poll
            let _ = self.receiver.try_recv();

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
            for _ in self.queue.iter() {
                let mpv_url = mpv.mpv.get_property(format!("playlist/{}/filename", i).as_str()).unwrap_or("".to_string());
                mpv_playlist.push(mpv_url);
                i += 1;
            }
            let mut new_queue = Vec::new();
            for mpv_url in mpv_playlist.iter() {
                for song in self.queue.iter() {
                    if &song.url == mpv_url {
                        new_queue.push(song.clone());
                        break;
                    }
                }
            }
            for (i, song) in self.queue.iter().enumerate() {
                if song.url != mpv_playlist[i] {
                    println!("[##] position changed {} != {}", song.url, mpv_playlist[i]);
                }
            }
            self.queue = new_queue;
        }
    }
}
