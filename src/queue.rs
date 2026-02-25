use crate::client::{Client, Transcoding};
use crate::database::database::{Command, UpdateCommand};
use crate::keyboard::{search_ranked_refs, ActiveSection};
use crate::mpv::LoadFileFlag;
use crate::{
    client::DiscographySong,
    database::extension::DownloadStatus,
    helpers,
    tui::{App, Song},
};
use rand::seq::SliceRandom;
use std::collections::HashMap;
/// This file has all the queue control functions
/// the basic idea is keeping our queue in sync with mpv and doing some basic operations
///
use std::sync::Arc;

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
                format!(
                    "{}",
                    downloads_dir
                        .join(&track.server_id)
                        .join(&track.album_id)
                        .join(&track.id)
                        .to_string_lossy()
                )
            }
            _ => match &client {
                Some(client) => client.song_url_sync(&track.id, Some(transcoding)),
                None => "".to_string(),
            },
        },
        name: track.name.clone(),
        artist: track.album_artist.clone(),
        artists: track.artists.clone(),
        album_artists: track.album_artists.clone(),
        album: track.album.clone(),
        album_id: track.album_id.clone(),
        // parent_id: track.parent_id.clone(),
        production_year: track.production_year,
        is_in_queue,
        is_transcoded: transcoding.enabled
            && !matches!(track.download_status, DownloadStatus::Downloaded),
        is_favorite: track.user_data.is_favorite,
        original_index: 0,
        run_time_ticks: track.run_time_ticks,
        disliked: track.disliked,
    }
}

impl App {
    /// This is the main queue control function. It basically initiates a new queue when we play a song without modifiers
    ///
    pub async fn initiate_main_queue(&mut self, tracks: &[DiscographySong], skip: usize) {
        if tracks.is_empty() {
            return;
        }
        let selected_is_album = tracks.get(skip).is_some_and(|t| t.id.starts_with("_album_"));

        // the playlist MPV will be getting
        self.state.queue = tracks
            .iter()
            .enumerate()
            .skip(skip)
            .filter(|(i, track)| if *i == skip { true } else { !track.disliked })
            // if selected is an album, this will filter out all the tracks that are not part of the album
            .filter(|(_, track)| {
                !selected_is_album
                    || track.parent_id == tracks.get(skip + 1).map_or("", |t| &t.parent_id)
            })
            .filter(|(_, track)| !track.id.starts_with("_album_")) // and then we filter out the album itself
            .map(|(_, track)| {
                make_track(
                    self.client.as_ref(),
                    &self.downloads_dir,
                    track,
                    false,
                    &self.transcoding,
                )
            })
            .collect();

        for (i, s) in self.state.queue.iter_mut().enumerate() {
            s.original_index = i as i64;
        }

        if let Err(e) = self.start_new_queue().await {
            log::error!("Failed to start playlist: {}", e);
            self.set_generic_message("Failed to start playlist", &e.to_string());
            return;
        }
        if self.state.shuffle {
            self.do_shuffle(true).await;
            // select the first song in the queue
            self.mpv_handle.play_index(0).await;
            self.state.selected_queue_item.select(Some(0));
        }

        let _ = self
            .db
            .cmd_tx
            .send(Command::Update(UpdateCommand::SongPlayed {
                track_id: self.state.queue[0].id.clone(),
            }))
            .await;
    }

    pub async fn start_new_queue(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let songs = self.state.queue.clone();
        let mut urls = Vec::with_capacity(songs.len());

        for song in &songs {
            match helpers::normalize_mpvsafe_url(&song.url) {
                Ok(safe_url) => {
                    urls.push(safe_url);
                }
                Err(e) => {
                    log::error!("Failed to normalize URL '{}': {:?}", song.url, e);

                    if e.to_string().contains("No such file or directory") {
                        let _ = self
                            .db
                            .cmd_tx
                            .send(Command::Update(UpdateCommand::OfflineRepair))
                            .await;
                    }
                }
            }
        }

        self.mpv_handle.stop().await;
        self.mpv_handle.load_files(urls, LoadFileFlag::AppendPlay, None).await;
        self.mpv_handle.play().await;

        self.stopped = false;
        self.paused = false;
        self.song_changed = true;

        Ok(())
    }

    /// Append the tracks to the end of the queue
    ///
    pub async fn append_to_main_queue(&mut self, tracks: &[DiscographySong], skip: usize) {
        if self.state.queue.is_empty() {
            self.initiate_main_queue(tracks, skip).await;
            return;
        }
        let mut new_queue: Vec<Song> = Vec::new();
        for (i, track) in tracks.iter().enumerate().skip(skip) {
            if track.id.starts_with("_album_") {
                continue;
            }
            if i != skip && track.disliked {
                continue;
            }
            new_queue.push(make_track(
                self.client.as_ref(),
                &self.downloads_dir,
                track,
                false,
                &self.transcoding,
            ));
        }

        let max_original_index =
            self.state.queue.iter().map(|s| s.original_index).max().unwrap_or(0);
        for (i, s) in new_queue.iter_mut().enumerate() {
            s.original_index = max_original_index + 1 + i as i64;
        }

        for song in &new_queue {
            match helpers::normalize_mpvsafe_url(&song.url) {
                Ok(safe_url) => {
                    self.mpv_handle.load_files(vec![safe_url], LoadFileFlag::Append, None).await;
                }
                Err(e) => {
                    log::error!("Failed to normalize URL '{}': {:?}", song.url, e);
                    if e.to_string().contains("No such file or directory") {
                        let _ = self
                            .db
                            .cmd_tx
                            .send(Command::Update(UpdateCommand::OfflineRepair))
                            .await;
                    }
                }
            }
        }

        self.state.queue.extend(new_queue);
    }

    /// Append the provided n tracks to the end of the queue
    ///
    pub async fn push_to_temporary_queue(
        &mut self,
        tracks: &[DiscographySong],
        skip: usize,
        n: usize,
    ) {
        if self.state.queue.is_empty() || tracks.is_empty() {
            // self.initiate_main_queue_one_track(tracks, skip).await;
            self.initiate_main_queue(tracks, skip).await;
            return;
        }

        let mut songs: Vec<Song> = Vec::new();
        for i in 0..n {
            let track = &tracks[skip + i];
            if i != 0 && track.disliked {
                continue;
            }
            if track.id.starts_with("_album_") {
                self.push_album_to_temporary_queue(false).await;
                return;
            }
            let song = make_track(
                self.client.as_ref(),
                &self.downloads_dir,
                track,
                true,
                &self.transcoding,
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

        for song in songs.iter().rev() {
            match helpers::normalize_mpvsafe_url(&song.url) {
                Ok(safe_url) => {
                    self.mpv_handle
                        .load_files(
                            vec![safe_url],
                            LoadFileFlag::InsertAt,
                            Some(selected_queue_item + 1),
                        )
                        .await;
                    self.state.queue.insert((selected_queue_item + 1) as usize, song.clone());
                }
                Err(e) => {
                    log::error!("Failed to normalize URL '{}': {:?}", song.url, e);
                    if e.to_string().contains("No such file or directory") {
                        let _ = self
                            .db
                            .cmd_tx
                            .send(Command::Update(UpdateCommand::OfflineRepair))
                            .await;
                    }
                }
            }
        }
    }

    /// Add a new song right after the currently playing song
    ///
    pub async fn push_next_to_temporary_queue(
        &mut self,
        tracks: &Vec<DiscographySong>,
        skip: usize,
    ) {
        if self.state.queue.is_empty() || tracks.is_empty() {
            self.initiate_main_queue(tracks, skip).await;
            return;
        }
        let selected_queue_item = self.state.selected_queue_item.selected().unwrap_or(0);
        // if we shift click we only appned the selected track to the playlist
        let track = &tracks[skip];
        if track.id.starts_with("_album_") {
            self.push_album_to_temporary_queue(true).await;
            return;
        }

        let song =
            make_track(self.client.as_ref(), &self.downloads_dir, track, true, &self.transcoding);

        match helpers::normalize_mpvsafe_url(&song.url) {
            Ok(safe_url) => {
                self.mpv_handle.load_files(vec![safe_url], LoadFileFlag::InsertNext, None).await;
                self.state.queue.insert(selected_queue_item + 1, song);
            }
            Err(e) => {
                log::error!("Failed to normalize URL '{}': {:?}", song.url, e);
                if e.to_string().contains("No such file or directory") {
                    let _ =
                        self.db.cmd_tx.send(Command::Update(UpdateCommand::OfflineRepair)).await;
                }
            }
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
        let refs = search_ranked_refs(&self.tracks, &self.state.tracks_search_term, true);

        let Some(parent) = refs.get(selected) else {
            return;
        };
        let album_id = &parent.parent_id;

        let tracks = refs
            .iter()
            .skip(selected + 1)
            .take_while(|t| t.parent_id == *album_id)
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

        for track in tracks.iter().rev() {
            let song = make_track(
                self.client.as_ref(),
                &self.downloads_dir,
                track,
                true,
                &self.transcoding,
            );
            self.mpv_handle
                .load_files(
                    vec![song.url.clone()],
                    LoadFileFlag::InsertAt,
                    Some(selected_queue_item + 1),
                )
                .await;

            self.state.queue.insert((selected_queue_item + 1) as usize, song);
        }
    }

    /// Remove the *selected* song from the queue
    ///
    pub async fn pop_from_queue(&mut self) {
        if self.state.queue.is_empty() {
            return;
        }

        if self.state.active_section != ActiveSection::Queue {
            return;
        }

        let selected_queue_item = match self.state.selected_queue_item.selected() {
            Some(item) => item,
            None => return,
        };

        self.mpv_handle.playlist_remove(selected_queue_item).await;
        self.state.queue.remove(selected_queue_item);
    }

    pub async fn remove_from_queue_by_id(&mut self, id: String) {
        if self.state.queue.is_empty() {
            return;
        }

        let mut to_remove = Vec::new();
        for (i, song) in self.state.queue.iter().enumerate() {
            if song.id == id {
                to_remove.push(i);
            }
        }
        for i in to_remove.iter().rev() {
            self.mpv_handle.playlist_remove(*i).await;
            self.state.queue.remove(*i);
        }
    }

    /// Clear the queue
    ///
    pub async fn clear_temporary_queue(&mut self) {
        if self.state.queue.is_empty() {
            return;
        }

        for i in (0..self.state.queue.len()).rev() {
            if self.state.queue[i].is_in_queue {
                self.mpv_handle.playlist_remove(i).await;
                self.state.queue.remove(i);
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

        // collect the queue songs (those marked is_in_queue)
        let mut queue: Vec<Song> =
            self.state.queue.iter().filter(|s| s.is_in_queue).cloned().collect();

        let queue_len = queue.len();

        let mut index = self.state.selected_queue_item.selected().unwrap_or(0);
        let after = index >= self.state.current_playback_state.current_index;

        // early return: selected item already in queue
        if self.state.queue[index].is_in_queue {
            self.stopped = false;
            self.mpv_handle.play_index(index).await;
            self.play().await;
            self.state.selected_queue_item.select(Some(index));
            return;
        }

        // remove all queued songs from mpv + local queue
        for i in (0..self.state.queue.len()).rev() {
            if let Some(song) = self.state.queue.get(i) {
                if song.is_in_queue {
                    self.state.queue.remove(i);
                    self.mpv_handle.playlist_remove(i).await;
                }
            }
        }

        // adjust index if selection was after the removed queue block
        if after {
            index -= queue_len;
        }
        self.state.selected_queue_item.select(Some(index));

        let insert_pos = index + 1;
        queue.reverse();

        for song in queue {
            let safe_url = match helpers::normalize_mpvsafe_url(&song.url) {
                Ok(u) => u,
                Err(e) => {
                    log::error!("Failed to normalize URL '{}': {:?}", song.url, e);
                    continue;
                }
            };

            let (flag, pos) = if insert_pos >= self.state.queue.len() {
                (LoadFileFlag::Append, None)
            } else {
                (LoadFileFlag::InsertAt, Some(insert_pos as i64))
            };

            self.mpv_handle.load_files(vec![safe_url], flag, pos).await;

            if pos.is_some() {
                self.state.queue.insert(insert_pos, song);
            } else {
                self.state.queue.push(song);
            }
        }

        // finally play selected item
        self.stopped = false;
        self.mpv_handle.play_index(index).await;
        self.play().await;
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

            self.mpv_handle.playlist_move(selected_queue_item, selected_queue_item - 1).await;

            self.state.selected_queue_item.select(Some(selected_queue_item - 1));

            self.state.queue.swap(selected_queue_item, selected_queue_item - 1);

            // if we moved the current song either directly or by moving the song above it
            // we need to update the current index
            if self.state.current_playback_state.current_index == selected_queue_item {
                self.state.current_playback_state.current_index -= 1;
            } else if self.state.current_playback_state.current_index == (selected_queue_item - 1) {
                self.state.current_playback_state.current_index += 1;
            }

            // discard next poll
            let _ = self.receiver.try_recv();
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

            self.mpv_handle.playlist_move(selected_queue_item + 1, selected_queue_item).await;

            self.state.queue.swap(selected_queue_item, selected_queue_item + 1);

            // if we moved the current song either directly or by moving the song above it
            // we need to update the current index
            if self.state.current_playback_state.current_index == selected_queue_item {
                self.state.current_playback_state.current_index += 1;
            } else if self.state.current_playback_state.current_index == (selected_queue_item + 1) {
                self.state.current_playback_state.current_index -= 1;
            }

            self.state.selected_queue_item.select(Some(selected_queue_item + 1));

            // discard next poll
            let _ = self.receiver.try_recv();
        }
    }

    /// Shuffles the queue
    ///
    ///
    pub async fn do_shuffle(&mut self, include_current: bool) {
        let len = self.state.queue.len();
        if len <= 1 {
            return;
        }

        let ci = match usize::try_from(self.state.current_playback_state.current_index) {
            Ok(i) if i < len => i,
            _ => 0,
        };

        let start = if include_current { ci } else { ci.saturating_add(1) };
        if start >= len {
            return;
        }

        let mut temp: Vec<Song> = Vec::new();
        let mut rest: Vec<Song> = Vec::new();
        for s in &self.state.queue[start..] {
            if s.is_in_queue {
                temp.push(s.clone());
            } else {
                rest.push(s.clone());
            }
        }

        let mut normalized_tail = Vec::with_capacity(len - start);
        normalized_tail.extend(temp.iter().cloned());
        normalized_tail.extend(rest.iter().cloned());

        // index map for current positions in [start..]
        let mut index_by_id: HashMap<String, usize> = HashMap::with_capacity(len - start);
        for i in start..len {
            index_by_id.insert(self.state.queue[i].id.clone(), i);
        }

        // normalize temp-first ordering
        for (i, target) in normalized_tail.iter().enumerate() {
            let g = start + i;

            if self.state.queue[g].id != target.id {
                if let Some(&from) = index_by_id.get(&target.id) {
                    let to = g;

                    self.mpv_handle.playlist_move_nowait(from, to);

                    let moved = self.state.queue.remove(from);
                    self.state.queue.insert(to, moved);

                    // update index map for affected range
                    let (lo, hi) = if from < to { (from, to) } else { (to, from) };
                    for idx in lo..=hi {
                        index_by_id.insert(self.state.queue[idx].id.clone(), idx);
                    }
                }
            }
        }

        let temp_count = temp.len();
        let shuffle_from = (start + temp_count).min(len);
        if shuffle_from >= len.saturating_sub(1) {
            return;
        }

        // shuffle the rest
        let mut local_current: Vec<Song> = self.state.queue[shuffle_from..].to_vec();
        let mut desired_order = local_current.clone();
        desired_order.shuffle(&mut rand::rng());

        for i in 0..desired_order.len() {
            if let Some(j) = local_current.iter().position(|s| s.id == desired_order[i].id) {
                if j != i {
                    let from = shuffle_from + j;
                    let to = shuffle_from + i;

                    self.mpv_handle.playlist_move_nowait(from, to);

                    let item = local_current.remove(j);
                    local_current.insert(i, item);
                }
            }
        }

        for (i, song) in local_current.into_iter().enumerate() {
            self.state.queue[shuffle_from + i] = song;
        }

        self.mpv_handle.await_reply().await;
    }

    /// Attempts to unshuffle the queue
    ///
    pub async fn do_unshuffle(&mut self) {
        let len = self.state.queue.len();
        if len <= 1 {
            return;
        }

        let ci = match usize::try_from(self.state.current_playback_state.current_index) {
            Ok(i) if i < len => i,
            _ => 0,
        };

        let start = ci.saturating_add(1).min(len);
        if start >= len {
            return;
        }

        let mut temp: Vec<Song> = Vec::new();
        let mut rest: Vec<Song> = Vec::new();
        for s in &self.state.queue[start..] {
            if s.is_in_queue {
                temp.push(s.clone());
            } else {
                rest.push(s.clone());
            }
        }

        let mut normalized_tail = Vec::with_capacity(len - start);
        normalized_tail.extend(temp.iter().cloned());
        normalized_tail.extend(rest.iter().cloned());

        let mut index_by_id: HashMap<String, usize> = HashMap::with_capacity(len - start);
        for i in start..len {
            index_by_id.insert(self.state.queue[i].id.clone(), i);
        }

        for (i, target) in normalized_tail.iter().enumerate() {
            let g = start + i;
            if self.state.queue[g].id != target.id {
                if let Some(&from) = index_by_id.get(&target.id) {
                    let to = g;

                    self.mpv_handle.playlist_move_nowait(from, to);

                    let moved = self.state.queue.remove(from);
                    self.state.queue.insert(to, moved);

                    let (lo, hi) = if from < to { (from, to) } else { (to, from) };
                    for idx in lo..=hi {
                        index_by_id.insert(self.state.queue[idx].id.clone(), idx);
                    }
                }
            }
        }

        let temp_count = temp.len();
        let sort_from = (start + temp_count).min(len);
        if sort_from >= len.saturating_sub(1) {
            return;
        }

        let mut desired_rest = self.state.queue[sort_from..].to_vec();
        desired_rest.sort_by_key(|s| s.original_index);

        index_by_id.clear();
        for i in sort_from..len {
            index_by_id.insert(self.state.queue[i].id.clone(), i);
        }

        for i in 0..desired_rest.len() {
            let target_id = &desired_rest[i].id;
            let g = sort_from + i;

            if self.state.queue[g].id != *target_id {
                if let Some(&from) = index_by_id.get(target_id) {
                    let to = g;

                    self.mpv_handle.playlist_move_nowait(from, to);

                    let moved = self.state.queue.remove(from);
                    self.state.queue.insert(to, moved);

                    let (lo, hi) = if from < to { (from, to) } else { (to, from) };
                    for idx in lo..=hi {
                        index_by_id.insert(self.state.queue[idx].id.clone(), idx);
                    }
                }
            }
        }

        self.mpv_handle.await_reply().await;
    }
}
