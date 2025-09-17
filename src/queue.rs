use std::sync::Arc;
/// This file has all the queue control functions
/// the basic idea is keeping our queue in sync with mpv and doing some basic operations
///
use crate::{client::DiscographySong, database::extension::DownloadStatus, helpers, tui::{App, Song}};
use rand::seq::SliceRandom;
use crate::client::{Client, Transcoding};
use crate::database::database::{Command, UpdateCommand};

fn make_track(
    client: Option<&Arc<Client>>,
    downloads_dir: &std::path::PathBuf,
    track: &DiscographySong,
    is_in_queue: bool,
    transcoding: &Transcoding,
) -> Song {
    Song {
        id: track.id.clone(),
        url: match track.download_status {
            DownloadStatus::Downloaded => {
                format!("{}", downloads_dir
                    .join(&track.server_id).join(&track.album_id).join(&track.id)
                    .to_string_lossy()
                )
            }
            _ => match &client {
                Some(client) => client.song_url_sync(&track.id, transcoding),
                None => "".to_string(),
            },
        },
        name: track.name.clone(),
        artist: track.album_artist.clone(),
        artist_items: track.album_artists.clone(),
        album: track.album.clone(),
        album_id: track.album_id.clone(),
        // parent_id: track.parent_id.clone(),
        production_year: track.production_year,
        is_in_queue,
        is_transcoded: transcoding.enabled && !matches!(track.download_status, DownloadStatus::Downloaded),
        is_favorite: track.user_data.is_favorite,
        original_index: 0,
        run_time_ticks: track.run_time_ticks,
    }
}

impl App {
    /// This is the main queue control function. It basically initiates a new queue when we play a song without modifiers
    ///
    pub async fn initiate_main_queue(&mut self, tracks: &[DiscographySong], skip: usize) {
        if tracks.is_empty() {
            return;
        }
        let selected_is_album = tracks
            .get(skip)
            .is_some_and(|t| t.id.starts_with("_album_"));

        // the playlist MPV will be getting
        self.state.queue = tracks
            .iter()
            .skip(skip)
            // if selected is an album, this will filter out all the tracks that are not part of the album
            .filter(|track| {
                !selected_is_album
                    || track.parent_id == tracks.get(skip + 1).map_or("", |t| &t.parent_id)
            })
            .filter(|track| !track.id.starts_with("_album_")) // and then we filter out the album itself
            .map(|track| make_track(self.client.as_ref(), &self.downloads_dir, track, false, &self.transcoding))
            .collect();

        if let Err(e) = self.mpv_start_playlist().await {
            log::error!("Failed to start playlist: {}", e);
            self.set_generic_message(
                "Failed to start playlist", &e.to_string(),
            );
            return;
        }
        if self.state.shuffle {
            self.do_shuffle(true).await;
            // select the first song in the queue
            if let Ok(mpv) = self.mpv_state.lock() {
                let _ = mpv.mpv.command("playlist-play-index", &["0"]);
                self.state.selected_queue_item.select(Some(0));
            }
        }

        let _ = self.db.cmd_tx
            .send(Command::Update(UpdateCommand::SongPlayed {
                track_id: self.state.queue[0].id.clone(),
            }))
            .await;
    }

    async fn initiate_main_queue_one_track(&mut self, tracks: &[DiscographySong], skip: usize) {
        if tracks.is_empty() {
            return;
        }

        let track = &tracks[skip];
        if track.id.starts_with("_album_") {
            return;
        }

        self.state.queue = vec![
            make_track(
                self.client.as_ref(), &self.downloads_dir, track, false, &self.transcoding
            )
        ];

        if let Err(e) = self.mpv_start_playlist().await {
            log::error!("Failed to start playlist: {}", e);
            self.set_generic_message(
                "Failed to start playlist", &e.to_string(),
            );
        }
    }

    /// Append the tracks to the end of the queue
    ///
    pub async fn append_to_main_queue(&mut self, tracks: &[DiscographySong], skip: usize) {
        if self.state.queue.is_empty() {
            self.initiate_main_queue(tracks, skip).await;
            return;
        }
        let mut new_queue: Vec<Song> = Vec::new();
        for track in tracks.iter().skip(skip) {
            if track.id.starts_with("_album_") {
                continue;
            }
            let song = make_track(
                self.client.as_ref(),
                &self.downloads_dir,
                track,
                false,
                &self.transcoding
            );
            new_queue.push(song);
        }

        if let Ok(mpv) = self.mpv_state.lock() {
            for song in &new_queue {
                match helpers::normalize_mpvsafe_url(&song.url) {
                    Ok(safe_url) => {
                        let _ = mpv.mpv.command("loadfile", &[safe_url.as_str(), "append"]);
                    }
                    Err(e) => {
                        log::error!("Failed to normalize URL '{}': {:?}", song.url, e);
                        if e.to_string().contains("No such file or directory") {
                            let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::OfflineRepair)).await;
                        }
                    },
                }
            }
        }

        self.state.queue.extend(new_queue);
    }

    /// Append the provided n tracks to the end of the queue
    ///
    pub async fn push_to_temporary_queue(&mut self, tracks: &[DiscographySong], skip: usize, n: usize) {
        if self.state.queue.is_empty() || tracks.is_empty() {
            self.initiate_main_queue_one_track(tracks, skip).await;
            return;
        }

        let mut songs: Vec<Song> = Vec::new();
        for i in 0..n {
            let track = &tracks[skip + i];
            if track.id.starts_with("_album_") {
                self.push_album_to_temporary_queue(false).await;
                return;
            }
            let song = make_track(
                self.client.as_ref(),
                &self.downloads_dir,
                track,
                true,
                &self.transcoding
            );

            songs.push(song);
        }

        let mut selected_queue_item = -1;
        for (i, song) in self.state.queue.iter().enumerate() {
            if song.is_in_queue {
                selected_queue_item = i as i64;
            }
        }

        if selected_queue_item == -1 {
            selected_queue_item = self.state.selected_queue_item.selected().unwrap_or(0) as i64;
        }

        let mpv = match self.mpv_state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };

        for song in songs.iter().rev() {
            match helpers::normalize_mpvsafe_url(&song.url) {
                Ok(safe_url) => {
                    if let Ok(_) = mpv.mpv.command(
                        "loadfile",
                        &[
                            safe_url.as_str(),
                            "insert-at",
                            (selected_queue_item + 1).to_string().as_str(),
                        ],
                    ) {
                        self.state.queue.insert((selected_queue_item + 1) as usize, song.clone());
                    }
                }
                Err(e) => {
                    log::error!("Failed to normalize URL '{}': {:?}", song.url, e);
                    if e.to_string().contains("No such file or directory") {
                        let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::OfflineRepair)).await;
                    }
                },
            }
        }
    }

    /// Add a new song right after the currently playing song
    ///
    pub async fn push_next_to_temporary_queue(&mut self, tracks: &Vec<DiscographySong>, skip: usize) {
        if self.state.queue.is_empty() || tracks.is_empty() {
            self.initiate_main_queue_one_track(tracks, skip).await;
            return;
        }
        let selected_queue_item = self.state.selected_queue_item.selected().unwrap_or(0);
        // if we shift click we only appned the selected track to the playlist
        let track = &tracks[skip];
        if track.id.starts_with("_album_") {
            self.push_album_to_temporary_queue(true).await;
            return;
        }

        let song = make_track(
            self.client.as_ref(),
            &self.downloads_dir,
            track,
            true,
            &self.transcoding
        );

        let mpv = match self.mpv_state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };

        match helpers::normalize_mpvsafe_url(&song.url) {
            Ok(safe_url) => {
                if let Ok(_) = mpv.mpv.command("loadfile", &[safe_url.as_str(), "insert-next"]) {
                    self.state.queue.insert(selected_queue_item + 1, song);
                }
            }
            Err(e) => {
                log::error!("Failed to normalize URL '{}': {:?}", song.url, e);
                if e.to_string().contains("No such file or directory") {
                    let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::OfflineRepair)).await;
                }
            },
        }

        // get the track-list
        // let count: i64 = mpv.mpv.get_property("playlist/count").unwrap_or(0);
        // let track_list: Vec<MpvNode> = Vec::with_capacity(count as usize);
        // println!("{:?}", count);

        // let second: String = mpv.mpv.get_property("playlist/1/filename").unwrap_or("".to_string());
        // println!("So these wont be the same sad sad {second}{:?}", self.state.queue.get(1).unwrap().url);
        // // compare the strings
        // println!("{:?}", self.state.queue.get(1).unwrap().url == second);
    }

    async fn push_album_to_temporary_queue(&mut self, start: bool) {
        let selected = self.state.selected_track.selected().unwrap_or(0);
        let album_id = self.tracks[selected].parent_id.clone();
        let tracks = self
            .tracks
            .iter()
            .skip(selected + 1)
            .take_while(|t| t.parent_id == album_id)
            .collect::<Vec<_>>();

        let mut selected_queue_item = -1;
        for (i, song) in self.state.queue.iter().enumerate() {
            if song.is_in_queue && !start {
                selected_queue_item = i as i64;
            }
        }

        if selected_queue_item == -1 {
            selected_queue_item = self.state.selected_queue_item.selected().unwrap_or(0) as i64;
        }

        let mpv = match self.mpv_state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };

        for track in tracks.iter().rev() {
            let song = make_track(
                self.client.as_ref(),
                &self.downloads_dir,
                track,
                true,
                &self.transcoding
            );

            if let Ok(_) = mpv.mpv.command(
                "loadfile",
                &[
                    song.url.as_str(),
                    "insert-at",
                    (selected_queue_item + 1).to_string().as_str(),
                ],
            ) {
                self.state
                    .queue
                    .insert((selected_queue_item + 1) as usize, song);
            }
        }
    }

    /// Remove the *selected* song from the queue
    ///
    pub async fn pop_from_queue(&mut self) {
        if self.state.queue.is_empty() {
            return;
        }

        let selected_queue_item = match self.state.selected_queue_item.selected() {
            Some(item) => item,
            None => return,
        };

        let mpv = match self.mpv_state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };

        if let Ok(_) = mpv.mpv.command(
            "playlist-remove",
            &[selected_queue_item.to_string().as_str()],
        ) {
            self.state.queue.remove(selected_queue_item);
        }
    }

    pub async fn remove_from_queue_by_id(
        &mut self,
        id: String,
    ) {
        if self.state.queue.is_empty() {
            return;
        }

        let mpv = match self.mpv_state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };

        let mut to_remove = Vec::new();
        for (i, song) in self.state.queue.iter().enumerate() {
            if song.id == id {
                to_remove.push(i);
            }
        }
        for i in to_remove.iter().rev() {
            if let Ok(_) = mpv.mpv.command("playlist-remove", &[i.to_string().as_str()]) {
                self.state.queue.remove(*i);
            }
        }
    }

    /// Clear the queue
    ///
    pub async fn clear_queue(&mut self) {
        if self.state.queue.is_empty() {
            return;
        }
        if let Ok(mpv) = self.mpv_state.lock() {
            for i in (0..self.state.queue.len()).rev() {
                if !self.state.queue[i].is_in_queue {
                    continue;
                }
                if let Ok(_) = mpv
                    .mpv
                    .command("playlist-remove", &[i.to_string().as_str()])
                {
                    self.state.queue.remove(i);
                }
            }
        }
    }

    /// Essentially, because of the queue itself being temporary we need to handle interactions differently
    /// If we play a song *outside* the queue, we MOVE the queue to that new position (remove, insert there, play selected)
    /// If we play a song *inside* the queue, we just play it
    ///
    pub async fn relocate_queue_and_play(&mut self) {
        if self.state.queue.is_empty() {
            return;
        }
        if let Ok(mpv) = self.mpv_state.lock() {
            // get a list of all the songs in the queue
            let mut queue: Vec<Song> = self
                .state
                .queue
                .iter()
                .filter(|s| s.is_in_queue)
                .cloned()
                .collect();
            let queue_len = queue.len();

            let mut index = self.state.selected_queue_item.selected().unwrap_or(0);
            let after: bool = index >= self.state.current_playback_state.current_index as usize;

            // early return in case we're within queue bounds
            if self.state.queue[index].is_in_queue {
                let _ = mpv
                    .mpv
                    .command("playlist-play-index", &[&index.to_string()]);
                if self.paused {
                    let _ = mpv.mpv.set_property("pause", false);
                    self.paused = false;
                }
                self.state.selected_queue_item.select(Some(index));
                return;
            }

            // Delete all songs before the selected song
            for i in (0..self.state.queue.len()).rev() {
                if let Some(song) = self.state.queue.get(i) {
                    if song.is_in_queue {
                        self.state.queue.remove(i);
                        mpv.mpv.command("playlist_remove", &[&i.to_string()]).ok();
                    }
                }
            }

            if after {
                index -= queue_len;
            }
            self.state.selected_queue_item.select(Some(index));

            // to put them back in the queue in the correct order
            queue.reverse();

            for song in queue {
                match helpers::normalize_mpvsafe_url(&song.url) {
                    Ok(safe_url) => {
                        if (index + 1) > self.state.queue.len() {
                            let _ = mpv.mpv.command("loadfile", &[safe_url.as_str(), "append"]);
                            self.state.queue.push(song);
                        } else {
                            let _ = mpv.mpv.command(
                                "loadfile",
                                &[
                                    safe_url.as_str(),
                                    "insert-at",
                                    (index + 1).to_string().as_str(),
                                ],
                            );
                            self.state.queue.insert(index + 1, song);
                        }
                    }
                    Err(e) => log::error!("Failed to normalize URL '{}': {:?}", song.url, e),
                }
            }

            let _ = mpv
                .mpv
                .command("playlist-play-index", &[&index.to_string()]);
            if self.paused {
                let _ = mpv.mpv.set_property("pause", false);
                self.paused = false;
            }
        }
    }

    /// Swap the selected song with the one above it
    ///
    pub async fn move_queue_item_up(&mut self) {
        if self.state.queue.is_empty() {
            return;
        }
        if let Some(selected_queue_item) = self.state.selected_queue_item.selected() {
            if selected_queue_item == 0 {
                return;
            }

            if let Some(src) = self.state.queue.get(selected_queue_item) {
                if let Some(dst) = self.state.queue.get(selected_queue_item - 1) {
                    if src.is_in_queue != dst.is_in_queue {
                        return;
                    }
                }
            }

            // i don't think i've ever disliked an API more
            if let Ok(mpv) = self.mpv_state.lock() {
                let _ = mpv
                    .mpv
                    .command(
                        "playlist-move",
                        &[
                            selected_queue_item.to_string().as_str(),
                            (selected_queue_item - 1).to_string().as_str(),
                        ],
                    )
                    .map_err(|e| format!("Failed to move playlist item: {:?}", e));
            }
            self.state
                .selected_queue_item
                .select(Some(selected_queue_item - 1));

            self.state
                .queue
                .swap(selected_queue_item, selected_queue_item - 1);

            // if we moved the current song either directly or by moving the song above it
            // we need to update the current index
            if self.state.current_playback_state.current_index == selected_queue_item as i64 {
                self.state.current_playback_state.current_index -= 1;
            } else if self.state.current_playback_state.current_index
                == (selected_queue_item - 1) as i64
            {
                self.state.current_playback_state.current_index += 1;
            }

            // discard next poll
            let _ = self.receiver.try_recv();

            #[cfg(debug_assertions)]
            {
                self.__debug_error_corrector_tm();
            }
        }
    }

    /// Swap the selected song with the one below it
    ///
    pub async fn move_queue_item_down(&mut self) {
        if self.state.queue.is_empty() {
            return;
        }
        if let Some(selected_queue_item) = self.state.selected_queue_item.selected() {
            if selected_queue_item == self.state.queue.len() - 1 {
                return;
            }

            if let Some(src) = self.state.queue.get(selected_queue_item) {
                if let Some(dst) = self.state.queue.get(selected_queue_item + 1) {
                    if src.is_in_queue != dst.is_in_queue {
                        return;
                    }
                }
            }

            if let Ok(mpv) = self.mpv_state.lock() {
                let _ = mpv
                    .mpv
                    .command(
                        "playlist-move",
                        &[
                            (selected_queue_item + 1).to_string().as_str(),
                            selected_queue_item.to_string().as_str(),
                        ],
                    )
                    .map_err(|e| format!("Failed to move playlist item: {:?}", e));
            }

            self.state
                .queue
                .swap(selected_queue_item, selected_queue_item + 1);

            // if we moved the current song either directly or by moving the song above it
            // we need to update the current index
            if self.state.current_playback_state.current_index == selected_queue_item as i64 {
                self.state.current_playback_state.current_index += 1;
            } else if self.state.current_playback_state.current_index
                == (selected_queue_item + 1) as i64
            {
                self.state.current_playback_state.current_index -= 1;
            }

            self.state
                .selected_queue_item
                .select(Some(selected_queue_item + 1));

            // discard next poll
            let _ = self.receiver.try_recv();

            #[cfg(debug_assertions)]
            {
                self.__debug_error_corrector_tm();
            }
        }
    }

    /// Get a safe current index
    fn safe_current_index(&self, raw: i64, len: usize) -> usize {
        if len == 0 { return 0; }
        usize::try_from(raw).ok().map(|i| i.min(len - 1)).unwrap_or(0)
    }

    /// Get the index after which we can shuffle
    fn get_shuffle_tail_index(&self, queue: &[Song], current_index: usize, include_current: bool) -> usize {
        let protected_tail = queue[current_index..]
            .iter()
            .take_while(|s| s.is_in_queue)
            .count();

        let mut after = current_index + protected_tail + 1;
        if queue[current_index].is_in_queue { after = after.saturating_sub(1); }
        if include_current { after = after.saturating_sub(1); }
        after.min(queue.len())
    }

    /// Shuffles the queue
    ///
    pub async fn do_shuffle(&mut self, include_current: bool) {
        if let Ok(mpv) = self.mpv_state.lock() {
            if self.state.queue.is_empty() {
                return;
            }

            let ci = self.safe_current_index(self.state.current_playback_state.current_index, self.state.queue.len());
            let shuffle_after = self.get_shuffle_tail_index(&self.state.queue, ci, include_current);

            if shuffle_after >= self.state.queue.len().saturating_sub(1) {
                return;
            }

            // write original_index for when unshuffle is called later
            for (i, s) in self.state.queue.iter_mut().enumerate() {
                s.original_index = i as i64;
            }

            let mut local_current: Vec<Song> = self.state.queue[shuffle_after..].to_vec();
            let mut desired_order = local_current.clone();
            desired_order.shuffle(&mut rand::rng());

            for (i, _) in desired_order.iter().enumerate() {
                let target_id = &desired_order[i].id;
                if let Some(j) = local_current.iter().position(|s| &s.id == target_id) {
                    if j != i {
                        let from = shuffle_after + j;
                        let to   = shuffle_after + i;
                        let _ = mpv.mpv.command("playlist-move", &[&from.to_string(), &to.to_string()]);
                        let item = local_current.remove(j);
                        local_current.insert(i, item);
                    }
                }
            }

            for (i, song) in local_current.into_iter().enumerate() {
                self.state.queue[shuffle_after + i] = song;
            }
        }
    }

    /// Attempts to unshuffle the queue
    ///
    pub async fn do_unshuffle(&mut self) {
        if let Ok(mpv) = self.mpv_state.lock() {
            if self.state.queue.is_empty() {
                return;
            }

            let current_index = self.safe_current_index(self.state.current_playback_state.current_index, self.state.queue.len());
            let shuffle_after = self.get_shuffle_tail_index(&self.state.queue, current_index, false);

            if shuffle_after >= self.state.queue.len().saturating_sub(1) {
                return;
            }

            let mut local_current: Vec<Song> = self.state.queue[shuffle_after..].to_vec();

            local_current.sort_by_key(|s| s.original_index);

            for (i, song) in local_current.iter().enumerate() {
                if self.state.queue[shuffle_after + i].id != song.id {
                    if let Some(j) = (shuffle_after..self.state.queue.len())
                        .position(|k| self.state.queue[k].id == song.id)
                        .map(|rel| shuffle_after + rel)
                    {
                        let _ = mpv.mpv.command("playlist-move", &[&j.to_string(), &(shuffle_after + i).to_string()]);
                        let moved = self.state.queue.remove(j);
                        self.state.queue.insert(shuffle_after + i, moved);
                    }
                }
            }
        }
    }

    /// (debug) Sync the queue with mpv and scream about it.
    /// It is a patently stupid function that should not exist, but the mpv api is not great
    /// Can be removed from well tested code
    ///
    fn __debug_error_corrector_tm(&mut self) {
        let mut mpv_playlist = Vec::new();

        if let Ok(mpv) = self.mpv_state.lock() {
            for (i, _) in self.state.queue.iter().enumerate() {
                let mpv_url = mpv
                    .mpv
                    .get_property(format!("playlist/{}/filename", i).as_str())
                    .unwrap_or("".to_string());
                mpv_playlist.push(mpv_url);
            }
            let mut new_queue = Vec::new();
            for mpv_url in mpv_playlist.iter() {
                for song in self.state.queue.iter() {
                    if &song.url == mpv_url {
                        new_queue.push(song.clone());
                        break;
                    }
                }
            }
            for (i, song) in self.state.queue.iter().enumerate() {
                if song.url != mpv_playlist[i] {
                    println!("[##] position changed {} != {}", song.url, mpv_playlist[i]);
                }
            }
            self.state.queue = new_queue;
        }
    }
}
