use crate::tui::App;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc::Sender;
use std::thread;
use tokio::sync::oneshot;

// Command IDs
const CMD_PLAY: u8 = 0x00;
const CMD_PAUSE: u8 = 0x01;
const CMD_TOGGLE: u8 = 0x02;
const CMD_STOP: u8 = 0x03;
const CMD_NEXT: u8 = 0x04;
const CMD_PREVIOUS: u8 = 0x05;
const CMD_SEEK_REL: u8 = 0x06;
const CMD_SEEK_ABS: u8 = 0x07;
const CMD_VOLUME_GET: u8 = 0x10;
const CMD_VOLUME_SET: u8 = 0x11;
const CMD_VOLUME_ADJ: u8 = 0x12;
const CMD_STATUS: u8 = 0x20;
const CMD_SEARCH: u8 = 0x30;
const CMD_RES_PLAY: u8 = 0x31;
const CMD_RES_ENQ: u8 = 0x32;
const CMD_Q_LIST: u8 = 0x40;
const CMD_Q_CLEAR: u8 = 0x41;
const CMD_Q_IDX_PLAY: u8 = 0x42;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SocketError {
    Success = 0x00,
    UnknownCommand = 0x01,
    InvalidPayload = 0x02,
    PlayerStopped = 0x03,
    IndexOutOfBounds = 0x04,
    SearchFailed = 0x05,
    NoSearchResults = 0x06,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SearchCategory {
    Artists = 0,
    Albums = 1,
    Tracks = 2,
}

impl TryFrom<u8> for SearchCategory {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(SearchCategory::Artists),
            1 => Ok(SearchCategory::Albums),
            2 => Ok(SearchCategory::Tracks),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PlaybackState {
    Stopped = 0,
    Playing = 1,
    Paused = 2,
}

#[derive(Debug, Clone)]
pub enum SocketCommand {
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    SeekRel { offset_ms: i32 },
    SeekAbs { position_ms: u32 },
    VolumeGet,
    VolumeSet { volume: u8 },
    VolumeAdj { delta: i8 },
    Status,
    Search { term: String },
    ResPlay { category: SearchCategory, index: u16 },
    ResEnq { category: SearchCategory, index: u16 },
    QList,
    QClear,
    QIdxPlay { index: u16 },
}

#[derive(Debug, Clone)]
pub struct SearchResultEntry {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct SearchResults {
    pub artists: Vec<SearchResultEntry>,
    pub albums: Vec<SearchResultEntry>,
    pub tracks: Vec<SearchResultEntry>,
}

#[derive(Debug, Clone)]
pub struct StatusInfo {
    pub state: PlaybackState,
    pub volume: u8,
    pub position_ms: u32,
    pub duration_ms: u32,
    pub track_name: String,
}

#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub id: String,
    pub name: String,
    pub artist: String,
}

#[derive(Debug, Clone)]
pub enum SocketResponse {
    Ok,
    Toggle { playing: bool },
    Volume { volume: u8 },
    Status(StatusInfo),
    Search(SearchResults),
    QList(Vec<QueueEntry>),
    Error(SocketError),
}

pub fn parse_message(data: &[u8]) -> Result<SocketCommand, SocketError> {
    if data.len() < 3 {
        return Err(SocketError::InvalidPayload);
    }

    let cmd = data[0];
    let len = u16::from_be_bytes([data[1], data[2]]) as usize;

    if data.len() < 3 + len {
        return Err(SocketError::InvalidPayload);
    }

    let payload = &data[3..3 + len];

    match cmd {
        CMD_PLAY => Ok(SocketCommand::Play),
        CMD_PAUSE => Ok(SocketCommand::Pause),
        CMD_TOGGLE => Ok(SocketCommand::Toggle),
        CMD_STOP => Ok(SocketCommand::Stop),
        CMD_NEXT => Ok(SocketCommand::Next),
        CMD_PREVIOUS => Ok(SocketCommand::Previous),
        CMD_SEEK_REL => {
            if payload.len() < 4 {
                return Err(SocketError::InvalidPayload);
            }
            let offset_ms = i32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
            Ok(SocketCommand::SeekRel { offset_ms })
        }
        CMD_SEEK_ABS => {
            if payload.len() < 4 {
                return Err(SocketError::InvalidPayload);
            }
            let position_ms = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
            Ok(SocketCommand::SeekAbs { position_ms })
        }
        CMD_VOLUME_GET => Ok(SocketCommand::VolumeGet),
        CMD_VOLUME_SET => {
            if payload.is_empty() {
                return Err(SocketError::InvalidPayload);
            }
            Ok(SocketCommand::VolumeSet { volume: payload[0] })
        }
        CMD_VOLUME_ADJ => {
            if payload.is_empty() {
                return Err(SocketError::InvalidPayload);
            }
            Ok(SocketCommand::VolumeAdj { delta: payload[0] as i8 })
        }
        CMD_STATUS => Ok(SocketCommand::Status),
        CMD_SEARCH => {
            if payload.len() < 2 {
                return Err(SocketError::InvalidPayload);
            }
            let term_len = u16::from_be_bytes([payload[0], payload[1]]) as usize;
            if payload.len() < 2 + term_len {
                return Err(SocketError::InvalidPayload);
            }
            let term = String::from_utf8_lossy(&payload[2..2 + term_len]).to_string();
            Ok(SocketCommand::Search { term })
        }
        CMD_RES_PLAY => {
            if payload.len() < 3 {
                return Err(SocketError::InvalidPayload);
            }
            let category = SearchCategory::try_from(payload[0])
                .map_err(|_| SocketError::InvalidPayload)?;
            let index = u16::from_be_bytes([payload[1], payload[2]]);
            Ok(SocketCommand::ResPlay { category, index })
        }
        CMD_RES_ENQ => {
            if payload.len() < 3 {
                return Err(SocketError::InvalidPayload);
            }
            let category = SearchCategory::try_from(payload[0])
                .map_err(|_| SocketError::InvalidPayload)?;
            let index = u16::from_be_bytes([payload[1], payload[2]]);
            Ok(SocketCommand::ResEnq { category, index })
        }
        CMD_Q_LIST => Ok(SocketCommand::QList),
        CMD_Q_CLEAR => Ok(SocketCommand::QClear),
        CMD_Q_IDX_PLAY => {
            if payload.len() < 2 {
                return Err(SocketError::InvalidPayload);
            }
            let index = u16::from_be_bytes([payload[0], payload[1]]);
            Ok(SocketCommand::QIdxPlay { index })
        }
        _ => Err(SocketError::UnknownCommand),
    }
}

pub fn encode_response(resp: &SocketResponse) -> Vec<u8> {
    match resp {
        SocketResponse::Ok => vec![SocketError::Success as u8, 0x00, 0x00],
        SocketResponse::Toggle { playing } => {
            vec![SocketError::Success as u8, 0x00, 0x01, if *playing { 1 } else { 0 }]
        }
        SocketResponse::Volume { volume } => {
            vec![SocketError::Success as u8, 0x00, 0x01, *volume]
        }
        SocketResponse::Status(info) => {
            let track_bytes = info.track_name.as_bytes();
            let track_len = track_bytes.len().min(u16::MAX as usize);
            let payload_len = 1 + 1 + 4 + 4 + 2 + track_len;
            let mut buf = Vec::with_capacity(3 + payload_len);
            buf.push(SocketError::Success as u8);
            buf.extend_from_slice(&(payload_len as u16).to_be_bytes());
            buf.push(info.state as u8);
            buf.push(info.volume);
            buf.extend_from_slice(&info.position_ms.to_be_bytes());
            buf.extend_from_slice(&info.duration_ms.to_be_bytes());
            buf.extend_from_slice(&(track_len as u16).to_be_bytes());
            buf.extend_from_slice(&track_bytes[..track_len]);
            buf
        }
        SocketResponse::Search(results) => {
            let mut payload = Vec::new();
            payload.extend_from_slice(&(results.artists.len() as u16).to_be_bytes());
            payload.extend_from_slice(&(results.albums.len() as u16).to_be_bytes());
            payload.extend_from_slice(&(results.tracks.len() as u16).to_be_bytes());

            for entry in &results.artists {
                encode_search_entry(&mut payload, entry);
            }
            for entry in &results.albums {
                encode_search_entry(&mut payload, entry);
            }
            for entry in &results.tracks {
                encode_search_entry(&mut payload, entry);
            }

            let mut buf = Vec::with_capacity(3 + payload.len());
            buf.push(SocketError::Success as u8);
            buf.extend_from_slice(&(payload.len() as u16).to_be_bytes());
            buf.extend(payload);
            buf
        }
        SocketResponse::QList(entries) => {
            let mut payload = Vec::new();
            payload.extend_from_slice(&(entries.len() as u16).to_be_bytes());

            for entry in entries {
                let id_bytes = entry.id.as_bytes();
                let name_bytes = entry.name.as_bytes();
                let artist_bytes = entry.artist.as_bytes();
                payload.extend_from_slice(&(id_bytes.len() as u16).to_be_bytes());
                payload.extend_from_slice(id_bytes);
                payload.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
                payload.extend_from_slice(name_bytes);
                payload.extend_from_slice(&(artist_bytes.len() as u16).to_be_bytes());
                payload.extend_from_slice(artist_bytes);
            }

            let mut buf = Vec::with_capacity(3 + payload.len());
            buf.push(SocketError::Success as u8);
            buf.extend_from_slice(&(payload.len() as u16).to_be_bytes());
            buf.extend(payload);
            buf
        }
        SocketResponse::Error(err) => vec![*err as u8, 0x00, 0x00],
    }
}

fn encode_search_entry(buf: &mut Vec<u8>, entry: &SearchResultEntry) {
    let id_bytes = entry.id.as_bytes();
    let name_bytes = entry.name.as_bytes();
    buf.extend_from_slice(&(id_bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(id_bytes);
    buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(name_bytes);
}

pub fn get_socket_path() -> std::path::PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        std::path::PathBuf::from(runtime_dir).join("jellyfin-tui.sock")
    } else {
        let suffix = std::env::var("USER").unwrap_or_else(|_| "default".to_string());
        std::path::PathBuf::from(format!("/tmp/jellyfin-tui-{}.sock", suffix))
    }
}

pub fn t_socket(tx: Sender<(SocketCommand, oneshot::Sender<SocketResponse>)>) {
    let socket_path = get_socket_path();
    let _ = std::fs::remove_file(&socket_path);

    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            log::error!("Failed to bind socket at {:?}: {}", socket_path, e);
            return;
        }
    };

    log::info!("Socket listening at {:?}", socket_path);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let tx = tx.clone();
                thread::spawn(move || {
                    handle_connection(stream, tx);
                });
            }
            Err(e) => {
                log::error!("Socket accept error: {}", e);
            }
        }
    }
}

fn handle_connection(mut stream: UnixStream, tx: Sender<(SocketCommand, oneshot::Sender<SocketResponse>)>) {
    let mut buf = [0u8; 4096];

    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => n,
            Err(e) => {
                log::error!("Socket read error: {}", e);
                return;
            }
        };

        let cmd = match parse_message(&buf[..n]) {
            Ok(cmd) => cmd,
            Err(err) => {
                let resp = encode_response(&SocketResponse::Error(err));
                let _ = stream.write_all(&resp);
                continue;
            }
        };

        let (resp_tx, resp_rx) = oneshot::channel();
        if tx.send((cmd, resp_tx)).is_err() {
            log::error!("Socket command channel closed");
            return;
        }

        let resp = match resp_rx.blocking_recv() {
            Ok(resp) => resp,
            Err(_) => SocketResponse::Error(SocketError::UnknownCommand),
        };

        let encoded = encode_response(&resp);
        if stream.write_all(&encoded).is_err() {
            return;
        }
    }
}

pub fn cleanup_socket() {
    let socket_path = get_socket_path();
    let _ = std::fs::remove_file(&socket_path);
    log::info!("Removed socket file {:?}", socket_path);
}

impl App {
    pub async fn handle_socket_events(&mut self) {
        while let Ok((cmd, resp_tx)) = self.socket_rx.try_recv() {
            let resp = self.handle_socket_command(cmd).await;
            let _ = resp_tx.send(resp);
        }
    }

    async fn handle_socket_command(&mut self, cmd: SocketCommand) -> SocketResponse {
        match cmd {
            SocketCommand::Play => {
                self.play().await;
                SocketResponse::Ok
            }
            SocketCommand::Pause => {
                self.pause().await;
                SocketResponse::Ok
            }
            SocketCommand::Toggle => {
                if self.paused {
                    self.play().await;
                    SocketResponse::Toggle { playing: true }
                } else {
                    self.pause().await;
                    SocketResponse::Toggle { playing: false }
                }
            }
            SocketCommand::Stop => {
                self.stop().await;
                SocketResponse::Ok
            }
            SocketCommand::Next => {
                if self.stopped {
                    return SocketResponse::Error(SocketError::PlayerStopped);
                }
                self.next().await;
                SocketResponse::Ok
            }
            SocketCommand::Previous => {
                if self.stopped {
                    return SocketResponse::Error(SocketError::PlayerStopped);
                }
                self.previous().await;
                SocketResponse::Ok
            }
            SocketCommand::SeekRel { offset_ms } => {
                if self.stopped {
                    return SocketResponse::Error(SocketError::PlayerStopped);
                }
                let offset_secs = offset_ms as f64 / 1000.0;
                self.mpv_handle.seek(offset_secs, crate::mpv::SeekFlag::Relative).await;
                SocketResponse::Ok
            }
            SocketCommand::SeekAbs { position_ms } => {
                if self.stopped {
                    return SocketResponse::Error(SocketError::PlayerStopped);
                }
                let position_secs = position_ms as f64 / 1000.0;
                self.mpv_handle.seek(position_secs, crate::mpv::SeekFlag::Absolute).await;
                SocketResponse::Ok
            }
            SocketCommand::VolumeGet => {
                let volume = (self.state.current_playback_state.volume as u8).min(100);
                SocketResponse::Volume { volume }
            }
            SocketCommand::VolumeSet { volume } => {
                let vol = (volume as i64).min(100);
                self.mpv_handle.set_volume(vol).await;
                SocketResponse::Ok
            }
            SocketCommand::VolumeAdj { delta } => {
                let current = self.state.current_playback_state.volume;
                let new_vol = (current + delta as i64).clamp(0, 100);
                self.mpv_handle.set_volume(new_vol).await;
                SocketResponse::Volume { volume: new_vol as u8 }
            }
            SocketCommand::Status => {
                let state = if self.stopped {
                    PlaybackState::Stopped
                } else if self.paused {
                    PlaybackState::Paused
                } else {
                    PlaybackState::Playing
                };

                let track_name = self.state.queue
                    .get(self.state.current_playback_state.current_index)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();

                SocketResponse::Status(StatusInfo {
                    state,
                    volume: (self.state.current_playback_state.volume as u8).min(100),
                    position_ms: (self.state.current_playback_state.position * 1000.0) as u32,
                    duration_ms: (self.state.current_playback_state.duration * 1000.0) as u32,
                    track_name,
                })
            }
            SocketCommand::Search { term } => {
                let results = self.perform_search(&term).await;
                SocketResponse::Search(results)
            }
            SocketCommand::ResPlay { category, index } => {
                match self.play_search_result(category, index as usize).await {
                    Ok(()) => SocketResponse::Ok,
                    Err(err) => SocketResponse::Error(err),
                }
            }
            SocketCommand::ResEnq { category, index } => {
                match self.enqueue_search_result(category, index as usize).await {
                    Ok(()) => SocketResponse::Ok,
                    Err(err) => SocketResponse::Error(err),
                }
            }
            SocketCommand::QList => {
                let entries: Vec<QueueEntry> = self.state.queue
                    .iter()
                    .map(|s| QueueEntry {
                        id: s.id.clone(),
                        name: s.name.clone(),
                        artist: s.artist.clone(),
                    })
                    .collect();
                SocketResponse::QList(entries)
            }
            SocketCommand::QClear => {
                self.clear_queue().await;
                SocketResponse::Ok
            }
            SocketCommand::QIdxPlay { index } => {
                if (index as usize) >= self.state.queue.len() {
                    return SocketResponse::Error(SocketError::IndexOutOfBounds);
                }
                self.mpv_handle.play_index(index as usize).await;
                self.play().await;
                SocketResponse::Ok
            }
        }
    }

    pub async fn perform_search(&mut self, term: &str) -> SearchResults {
        let term_lower = term.to_lowercase();

        let artist_results: Vec<_> = self.original_artists
            .iter()
            .filter(|a| a.name.to_lowercase().contains(&term_lower))
            .cloned()
            .collect();
        self.search_result_artists = artist_results;

        let artists: Vec<SearchResultEntry> = self.search_result_artists
            .iter()
            .map(|a| SearchResultEntry {
                id: a.id.clone(),
                name: a.name.clone(),
            })
            .collect();

        let album_results: Vec<_> = self.original_albums
            .iter()
            .filter(|a| a.name.to_lowercase().contains(&term_lower))
            .cloned()
            .collect();
        self.search_result_albums = album_results;

        let albums: Vec<SearchResultEntry> = self.search_result_albums
            .iter()
            .map(|a| SearchResultEntry {
                id: a.id.clone(),
                name: a.name.clone(),
            })
            .collect();

        let track_results = match &self.client {
            Some(client) => client.search_tracks(term.to_string()).await.unwrap_or_default(),
            None => {
                use crate::database::extension::get_tracks;
                get_tracks(&self.db.pool, term).await.unwrap_or_default()
            }
        };
        self.search_result_tracks = track_results;

        let tracks: Vec<SearchResultEntry> = self.search_result_tracks
            .iter()
            .map(|t| SearchResultEntry {
                id: t.id.clone(),
                name: t.name.clone(),
            })
            .collect();

        SearchResults { artists, albums, tracks }
    }

    async fn play_search_result(&mut self, category: SearchCategory, index: usize) -> Result<(), SocketError> {
        match category {
            SearchCategory::Tracks => {
                if index >= self.search_result_tracks.len() {
                    return Err(SocketError::IndexOutOfBounds);
                }
                let tracks = self.search_result_tracks.clone();
                self.initiate_main_queue(&tracks, index).await;
                Ok(())
            }
            SearchCategory::Albums => {
                if index >= self.search_result_albums.len() {
                    return Err(SocketError::IndexOutOfBounds);
                }
                let album = &self.search_result_albums[index];
                let tracks = self.fetch_album_tracks(&album.id).await?;
                if tracks.is_empty() {
                    return Err(SocketError::NoSearchResults);
                }
                self.initiate_main_queue(&tracks, 0).await;
                Ok(())
            }
            SearchCategory::Artists => {
                if index >= self.search_result_artists.len() {
                    return Err(SocketError::IndexOutOfBounds);
                }
                let artist = &self.search_result_artists[index];
                let tracks = self.fetch_artist_tracks(&artist.id).await?;
                if tracks.is_empty() {
                    return Err(SocketError::NoSearchResults);
                }
                self.initiate_main_queue(&tracks, 0).await;
                Ok(())
            }
        }
    }

    async fn enqueue_search_result(&mut self, category: SearchCategory, index: usize) -> Result<(), SocketError> {
        match category {
            SearchCategory::Tracks => {
                if index >= self.search_result_tracks.len() {
                    return Err(SocketError::IndexOutOfBounds);
                }
                let tracks = self.search_result_tracks.clone();
                self.push_to_temporary_queue(&tracks, index, 1).await;
                Ok(())
            }
            SearchCategory::Albums => {
                if index >= self.search_result_albums.len() {
                    return Err(SocketError::IndexOutOfBounds);
                }
                let album = &self.search_result_albums[index];
                let tracks = self.fetch_album_tracks(&album.id).await?;
                if tracks.is_empty() {
                    return Err(SocketError::NoSearchResults);
                }
                for (i, _) in tracks.iter().enumerate() {
                    self.push_to_temporary_queue(&tracks, i, 1).await;
                }
                Ok(())
            }
            SearchCategory::Artists => {
                if index >= self.search_result_artists.len() {
                    return Err(SocketError::IndexOutOfBounds);
                }
                let artist = &self.search_result_artists[index];
                let tracks = self.fetch_artist_tracks(&artist.id).await?;
                if tracks.is_empty() {
                    return Err(SocketError::NoSearchResults);
                }
                for (i, _) in tracks.iter().enumerate() {
                    self.push_to_temporary_queue(&tracks, i, 1).await;
                }
                Ok(())
            }
        }
    }

    async fn fetch_album_tracks(&self, album_id: &str) -> Result<Vec<crate::client::DiscographySong>, SocketError> {
        use crate::database::extension::get_album_tracks;
        get_album_tracks(&self.db.pool, album_id, self.client.as_ref())
            .await
            .map_err(|_| SocketError::SearchFailed)
    }

    async fn fetch_artist_tracks(&self, artist_id: &str) -> Result<Vec<crate::client::DiscographySong>, SocketError> {
        use crate::database::extension::get_discography;
        get_discography(&self.db.pool, artist_id, self.client.as_ref())
            .await
            .map_err(|_| SocketError::SearchFailed)
    }
}
