/* --------------------------
The main struct of the program. Holds the state and main logic.
    - Gets created in main.rs and the run() function is called in the main loop.
Notable fields:
    - state = main persistent state object (gets deserialized and loaded when you reopen the app)
    - client = HTTP client (client.rs)
    - mpv_thread = MPV thread handle. We use MPV for audio playback.
    - mpv_state = Shared state for controlling MPV. We update this state every frame using a channel from the MPV thread.
        - sender = Sender for the MPV channel.
        - receiver = Receiver for the MPV channel.
    - controls = MPRIS controls. We use MPRIS for media controls.
-------------------------- */

use crate::client::{self, report_progress, Album, Artist, Client, DiscographySong, Lyric, Playlist, ProgressReport};
use crate::keyboard::{*};
use crate::mpris;
use crate::popup::PopupState;

use libmpv2::{*};
use serde::{Deserialize, Serialize};

use core::panic;
use std::io::Stdout;

use souvlaki::{MediaControlEvent, MediaControls};

use dirs::cache_dir;
use std::path::PathBuf;

use ratatui::{
    Terminal,
    Frame,
    prelude::*,
    widgets::*,
};

use ratatui_image::{picker::Picker, protocol::StatefulProtocol};

use std::time::Duration;

/// A type alias for the terminal type used in this application
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use std::thread;

/// This represents the playback state of MPV
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MpvPlaybackState {
    pub percentage: f64,
    pub duration: f64,
    pub current_index: i64,
    pub last_index: i64,
    pub volume: i64,
    pub audio_bitrate: i64,
    pub file_format: String,
}

/// Internal song representation. Used in the queue and passed to MPV
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Song {
    pub id: String,
    pub url: String,
    pub name: String,
    pub artist: String,
    pub artist_items: Vec<Artist>,
    pub album: String,
    pub parent_id: String,
    pub production_year: u64,
    pub is_in_queue: bool,
    pub is_transcoded: bool,
    pub is_favorite: bool,
}
#[derive(PartialEq, Serialize, Deserialize)]
pub enum Repeat {
    None,
    One,
    All,
}
pub struct App {
    pub exit: bool,
    pub dirty: bool, // dirty flag for rendering
    pub dirty_clear: bool, // dirty flag for clearing the screen
    
    pub state: State, // main persistent state

    pub primary_color: Color, // primary color
    pub config: Option<serde_json::Value>, // config
    pub auto_color: bool, // grab color from cover art (coolest feature ever omg)
    
    pub artists: Vec<Artist>, // all artists
    pub tracks: Vec<DiscographySong>, // current artist's tracks
    pub tracks_playlist: Vec<DiscographySong>, // current playlist tracks
    pub playlists: Vec<Playlist>, // playlists
    pub lyrics: Option<(String, Vec<Lyric>, bool)>, // ID, lyrics, time_synced
    pub active_song_id: String,

    pub metadata: Option<client::MediaStream>,
    pub cover_art: Option<Box<StatefulProtocol>>,
    pub cover_art_path: String,
    cover_art_dir: String,
    picker: Option<Picker>,

    pub paused: bool,
    pending_seek: Option<f64>, // pending seek
    pub buffering: bool, // buffering state (spinner)
    
    pub spinner: usize, // spinner for buffering
    spinner_skipped: u8,
    pub spinner_stages: Vec<&'static str>,

    pub searching: bool,
    pub show_help: bool,
    pub search_term: String,

    pub locally_searching: bool,

    pub search_result_artists: Vec<Artist>,
    pub search_result_albums: Vec<Album>,
    pub search_result_tracks: Vec<DiscographySong>,
    
    pub popup: PopupState,

    pub client: Option<Client>, // jellyfin http client

    // mpv is run in a separate thread, this is the handle
    mpv_thread: Option<thread::JoinHandle<()>>,
    pub mpv_state: Arc<Mutex<MpvState>>, // shared mutex for controlling mpv
    pub song_changed: bool,

    pub mpris_paused: bool,
    pub mpris_active_song_id: String,

    // every second, we get the playback state from the mpv thread
    sender: Sender<MpvPlaybackState>,
    pub receiver: Receiver<MpvPlaybackState>,
    old_percentage: f64,
    scrobble_this: (String, u64), // an id of the previous song we want to scrobble when it ends
    pub controls: Option<MediaControls>,
}

impl Default for App {
    fn default() -> Self {
        let config = match crate::config::get_config() {
            Ok(config) => Some(config),
            Err(_) => None,
        };

        let primary_color = crate::config::get_primary_color();

        let is_art_enabled = config.as_ref().and_then(|c| c.get("art")).and_then(|a| a.as_bool()).unwrap_or(true);
        let picker = if is_art_enabled {
            match Picker::from_query_stdio() {
                Ok(picker) => Some(picker),
                Err(_) => {
                    let picker = Picker::from_fontsize((8, 12));
                    Some(picker)
                }
            }
        } else {
            None
        };

        let (sender, receiver) = channel();

        let controls = match mpris::mpris() {
            Ok(controls) => Some(controls),
            Err(_) => None,
        };

        App {
            exit: false,
            dirty: true,
            dirty_clear: false,
            state: State::new(),
            primary_color,
            config: config.clone(),
            auto_color: config.as_ref().and_then(|c| c.get("auto_color")).and_then(|a| a.as_bool()).unwrap_or(true),

            artists: vec![],
            tracks: vec![],
            tracks_playlist: vec![],
            playlists: vec![],
            lyrics: None,
            metadata: None,
            active_song_id: String::from(""),
            cover_art: None,
            cover_art_path: String::from(""),
            cover_art_dir: match cache_dir() {
                Some(dir) => dir,
                None => PathBuf::from("./"),
            }.join("jellyfin-tui").join("covers").to_str().unwrap_or("").to_string(),
            picker,
            paused: true,

            pending_seek: None,
            buffering: false,
            spinner: 0,
            spinner_skipped: 0,
            spinner_stages: vec![
                "◰", "◳", "◲", "◱"
            ],
            searching: false,
            show_help: false,
            search_term: String::from(""),

            locally_searching: false,

            search_result_artists: vec![],
            search_result_albums: vec![],
            search_result_tracks: vec![],
            
            popup: PopupState::default(),

            client: None,
            mpv_thread: None,
            mpris_paused: true,
            mpris_active_song_id: String::from(""),
            mpv_state: Arc::new(Mutex::new(MpvState::new(&config))),
            song_changed: false,

            sender,
            receiver,

            old_percentage: 0.0,
            scrobble_this: (String::from(""), 0),
            controls,
        }
    }
}

/// This struct should contain all the values that should **PERSIST** when the app is closed and reopened.
/// 
#[derive(serde::Serialize, serde::Deserialize)]
pub struct State {
    // (URL, Title, Artist, Album)
    pub queue: Vec<Song>,
    // Music - active section (Artists, Tracks, Queue)
    pub active_section: ActiveSection, // current active section (Artists, Tracks, Queue)
    pub last_section: ActiveSection, // last active section
    // Search - active section (Artists, Albums, Tracks)
    pub search_section: SearchSection, // current active section (Artists, Albums, Tracks)
    
    // active tab (Music, Search)
    pub active_tab: ActiveTab,
    pub current_artist: Artist,
    pub current_playlist: Playlist,

    // ratatui list indexes
    pub selected_artist: ListState,
    pub selected_track: TableState,
    pub selected_playlist_track: TableState,
    pub selected_playlist: ListState,
    pub tracks_scroll_state: ScrollbarState,
    pub artists_scroll_state: ScrollbarState,
    pub playlists_scroll_state: ScrollbarState,
    pub playlist_tracks_scroll_state: ScrollbarState,
    pub selected_queue_item: ListState,
    pub selected_queue_item_manual_override: bool,
    pub selected_lyric: ListState,
    pub selected_lyric_manual_override: bool,
    pub current_lyric: usize,
    pub selected_search_artist: ListState,
    pub selected_search_album: ListState,
    pub selected_search_track: ListState,

    pub artists_search_term: String,
    pub tracks_search_term: String,
    pub playlist_tracks_search_term: String,
    pub playlists_search_term: String,

    // scrollbars for search results
    pub search_artist_scroll_state: ScrollbarState,
    pub search_album_scroll_state: ScrollbarState,
    pub search_track_scroll_state: ScrollbarState,

    // repeat mode
    pub repeat: Repeat,

    pub current_playback_state: MpvPlaybackState,
}

pub struct MpvState {
    pub mpris_events: Vec<MediaControlEvent>,
    pub mpv: Mpv,
}

impl MpvState {
    fn new(config: &Option<serde_json::Value>) -> Self {
        let mpv = Mpv::with_initializer(|mpv| {
            mpv.set_option("msg-level", "ffmpeg/demuxer=no").unwrap();
            Ok(())
        }).expect("[XX] Failed to initiate mpv context");
        mpv.set_property("vo", "null").unwrap();
        mpv.set_property("volume", 100).unwrap();
        mpv.set_property("prefetch-playlist", "yes").unwrap(); // gapless playback

        // no console output (it shifts the tui around)
        // TODO: can we catch this and show it in a proper area?
        mpv.set_property("quiet", "yes").ok(); 
        mpv.set_property("really-quiet", "yes").ok(); 

        // optional mpv options (hah...)
        if let Some(config) = config {
            if let Some(mpv_config) = config.get("mpv") {
                if let Some(mpv_config) = mpv_config.as_object() {
                    for (key, value) in mpv_config {
                        if let Some(value) = value.as_str() {
                            mpv.set_property(key, value).unwrap_or_else(|e| {
                                panic!("[XX] Failed to set mpv property {key}: {:?}", e)
                            });
                        }
                    }
                }
            }
        }

        let ev_ctx = events::EventContext::new(mpv.ctx);
        ev_ctx.disable_deprecated_events().unwrap();
        ev_ctx.observe_property("volume", Format::Int64, 0).unwrap();
        ev_ctx
            .observe_property("demuxer-cache-state", Format::Node, 0)
            .unwrap();
        MpvState {
            mpris_events: vec![],
            mpv,
        }
    }
}

impl App {
    pub async fn init(&mut self, artists: Vec<Artist>) {
        let client = client::Client::new(true).await;
        if client.access_token.is_empty() {
            panic!("[XX] Failed to authenticate. Exiting...");
        }
        self.client = Some(client);
        self.artists = artists;
        self.state.artists_scroll_state = ScrollbarState::new(self.artists.len() - 1);
        self.state.active_section = ActiveSection::Artists;
        self.state.selected_artist.select(Some(0));
        self.state.selected_playlist.select(Some(0));

        if let Some(client) = &self.client {
            if let Ok(playlists) = client.playlists(String::from("")).await {
                self.playlists = playlists;
                self.state.playlists_scroll_state = ScrollbarState::new(self.playlists.len() - 1);
            }
        }

        self.register_controls(self.mpv_state.clone());

        let persist = self.config.as_ref().and_then(|c| c.get("persist")).and_then(|a| a.as_bool()).unwrap_or(true);
        if persist {
            let _ = self.load_state().await;
        }
        #[cfg(target_os = "linux")]
        {
            if let Some(ref mut controls) = self.controls {
                let _ = controls.set_volume(self.state.current_playback_state.volume as f64 / 100.0);
            }
        }
            
    }

    pub async fn run<'a>(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {

        if let Some (seek) = self.pending_seek {
            if let Ok(mpv) = self.mpv_state.lock() {
                let paused_for_cache = mpv.mpv.get_property("seekable").unwrap_or(false);
                if paused_for_cache {
                    let _ = mpv.mpv.command("seek", &[&seek.to_string(), "absolute"]);
                    self.pending_seek = None;
                }
            }
            self.dirty = true;
        }

        // get playback state from the mpv thread
        let state = self.receiver.try_recv()?;

        self.dirty = true;

        self.state.current_playback_state.percentage = state.percentage;
        self.state.current_playback_state.current_index = state.current_index;
        self.state.current_playback_state.duration = state.duration;
        self.state.current_playback_state.volume = state.volume;
        if !state.file_format.is_empty() {
            self.state.current_playback_state.file_format = state.file_format;
        }
        if let Some(client) = &self.client {
            if let Some(metadata) = self.metadata.as_mut() {
                if client.transcoding.enabled 
                    && state.audio_bitrate > 0 
                    && self.state.queue.get(state.current_index as usize)
                        .and_then(|s| Some(s.is_transcoded)).unwrap_or(false) 
                {
                    metadata.bit_rate = state.audio_bitrate as u64;
                }
            }
        }

        // Queue position
        if !self.state.selected_queue_item_manual_override {
            self.state.selected_queue_item
                .select(Some(state.current_index as usize));
        }

        // wipe played queue items (done here because mpv state)
        if let Ok(mpv) = self.mpv_state.lock() {
            for i in (0..state.current_index).rev() {
                if let Some(song) = self.state.queue.get(i as usize) {
                    if song.is_in_queue {
                        self.state.queue.remove(i as usize);
                        mpv.mpv.command("playlist_remove", &[&i.to_string()]).ok();

                        // move down the selected queue item if it's above the current index
                        if let Some(selected) = self.state.selected_queue_item.selected() {
                            self.state.selected_queue_item.select(Some(selected - 1));
                            self.state.current_playback_state.current_index -= 1;
                        }
                    }
                }
            }
        }
        let song = self.state.queue.get(self.state.current_playback_state.current_index as usize).cloned().unwrap_or_default();

        if let Ok(mpv) = self.mpv_state.lock() {
            let paused_for_cache = mpv.mpv.get_property("paused-for-cache").unwrap_or(false);
            let seeking = mpv.mpv.get_property("seeking").unwrap_or(false);
            self.buffering = paused_for_cache || seeking;
        }

        if (self.old_percentage + 2.0) < self.state.current_playback_state.percentage {
            self.old_percentage = self.state.current_playback_state.percentage;

            // if % > 0.5, report progress
            self.scrobble_this = (song.id.clone(), (self.state.current_playback_state.duration * self.state.current_playback_state.percentage * 100000.0) as u64);

            let client = self.client.as_ref().ok_or(" ! No client")?;

            let runit = report_progress(
                client.base_url.clone(), client.access_token.clone(), ProgressReport {
                volume_level: self.state.current_playback_state.volume as u64,
                is_paused: self.paused,
                // take into account duratio, percentage and *10000
                position_ticks: (self.state.current_playback_state.duration * self.state.current_playback_state.percentage * 100000.0) as u64,
                media_source_id: self.active_song_id.clone(),
                playback_start_time_ticks: 0,
                can_seek: false, // TODO
                item_id: self.active_song_id.clone(),
                event_name: "timeupdate".to_string(),
            });
            tokio::spawn(runit);
            
        } else if self.old_percentage > self.state.current_playback_state.percentage {
            self.old_percentage = self.state.current_playback_state.percentage;
        }
        
        // song has changed
        self.song_changed = self.song_changed || song.id != self.active_song_id;
        if self.song_changed {
            self.song_changed = false;
            self.state.selected_lyric_manual_override = false;
            self.state.selected_lyric.select(None);
            self.state.current_lyric = 0;

            self.active_song_id = song.id.clone();

            // fetch lyrics
            let client = self.client.as_ref().ok_or(" ! No client")?;
            let lyrics = client.lyrics(&self.active_song_id).await;
            self.metadata = client.metadata(&self.active_song_id).await.ok();

            self.lyrics = lyrics.map(|lyrics| {
                let time_synced = lyrics.iter().all(|l| l.start != 0);
                ( self.active_song_id.clone(), lyrics, time_synced )
            }).ok();

            self.state.selected_lyric.select(None);

            self.cover_art = None;
            self.cover_art_path = String::from("");
            let cover_image = client.download_cover_art(song.parent_id).await.unwrap_or_default();

            if !cover_image.is_empty() && !self.cover_art_dir.is_empty() {
                // let p = format!("./covers/{}", cover_image);
                let p = format!("{}/{}", self.cover_art_dir, cover_image);
                if let Ok(reader) = image::ImageReader::open(&p) {
                    if let Ok(img) = reader.decode() {
                        if let Some(ref mut picker) = self.picker {
                            let image_fit_state = picker.new_resize_protocol(img.clone());
                            self.cover_art = Some(Box::new(image_fit_state));
                            self.cover_art_path = p.clone();
                        }
                        if self.auto_color {
                            self.grab_primary_color(&p);
                        }
                    }
                }
            };

            let client = self.client.as_ref().ok_or(" ! No client")?;
            // Scrobble. The way to do scrobbling in jellyfin is using the last.fm jellyfin plugin. 
            // Essentially, this event should be sent either way, the scrobbling is purely server side and not something we need to worry about.
            if !self.scrobble_this.0.is_empty() {
                let _ = client.stopped(
                    &self.scrobble_this.0,
                    self.scrobble_this.1,
                ).await;
                self.scrobble_this = (String::from(""), 0);
            }

            let _ = client.playing(&self.active_song_id).await;
        }
        Ok(())
    }

    pub async fn draw<'a>(&mut self, terminal: &'a mut Tui) -> std::result::Result<(), Box<dyn std::error::Error>> {

        // let the rats take over
        if self.dirty {
            terminal
                .draw(|frame: &mut Frame| {
                    self.render_frame(frame);
                })?;
            self.dirty = false;
        }

        if self.dirty_clear {
            terminal.clear()?;
            self.dirty_clear = false;
        }

        self.handle_events().await?;

        self.handle_mpris_events().await;

        // ratatui is an immediate mode tui which is cute, but it will be heavy on the cpu
        // we use a dirty draw flag and thread::sleep to throttle the bool check a bit
    
        thread::sleep(Duration::from_millis(2));

        Ok(())
    }

    /// This is the main render function for rataui. It's called every frame.
    pub fn render_frame<'a>(&mut self, frame: &'a mut Frame) {

        let app_container = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Min(1),
                Constraint::Percentage(100),
            ])
            .split(frame.area());

        // render tabs
        self.render_tabs(app_container[0], frame.buffer_mut());

        match self.state.active_tab {
            ActiveTab::Library => {
                if self.show_help {
                    self.render_home_help(app_container[1], frame);
                } else {
                    self.render_home(app_container[1], frame);
                }
            }
            ActiveTab::Playlists => {
                if self.show_help {
                    self.render_playlists_help(app_container[1], frame);
                } else {
                    self.render_playlists(app_container[1], frame);
                }
            }
            ActiveTab::Search => {
                self.render_search(app_container[1], frame);
            }
        }

        self.spinner_skipped += 1;
        if self.spinner_skipped > 5 {
            self.spinner_skipped = 0;
            self.spinner += 1;
            if self.spinner > self.spinner_stages.len() - 1 {
                self.spinner = 0;
            }
        }
    }

    fn render_tabs(&self, area: Rect, buf: &mut Buffer) {
        // split the area into left and right
        let tabs_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(70),
                Constraint::Percentage(30),
                Constraint::Min(15),
            ])
            .split(area);
        Tabs::new(vec!["Artists", "Playlists", "Search"])
            .style(Style::default().white().dim())
            .highlight_style(Style::default().white().not_dim())
            .select(self.state.active_tab as usize)
            .divider(symbols::DOT)
            .padding(" ", " ")
            .render(tabs_layout[0], buf);

        let repeat_icon = match self.state.repeat {
            Repeat::None => "",
            Repeat::One => "R1",
            Repeat::All => "R*",
        };
        let transcoding = if let Some(client) = self.client.as_ref() {
            if client.transcoding.enabled {
                format!("[{}@{}]", client.transcoding.container, client.transcoding.bitrate)
            } else {
                "".to_string()
            }
        } else {
            "".to_string()
        };
        let info = [repeat_icon, &transcoding].join(" ");
        let volume_color = match self.state.current_playback_state.volume {
            0..=100 => Color::White,
            101..=120 => Color::Yellow,
            _ => Color::Red,
        };
        
        Paragraph::new(info)
            .style(Style::default().fg(volume_color))
            .alignment(Alignment::Right)
            .wrap(Wrap { trim: false })
            .render(tabs_layout[1], buf);

        LineGauge::default()
            .block(
                Block::default()
                    .padding(Padding::horizontal(1))
            )
            .filled_style(
                Style::default()
                    .fg(volume_color)
                    .add_modifier(Modifier::BOLD)
            )
            .label(Line::from(
                    format!("{}%", self.state.current_playback_state.volume)
                ).style(Style::default().fg(volume_color))
            )
            .unfilled_style(
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .line_set(symbols::line::ROUNDED)
            .ratio((self.state.current_playback_state.volume as f64 / 100_f64).min(1.0))
            .render(tabs_layout[2], buf);
    }

    /// Fetch the discography of an artist
    /// This will change the active section to tracks
    pub async fn discography(&mut self, id: &str) {
        if id.is_empty() {
            return;
        }
        let recently_added = self.artists.iter()
            .any(|a| a.id == id && a.jellyfintui_recently_added);
        if let Some(client) = self.client.as_ref() {
            if let Ok(artist) = client.discography(id, recently_added).await {
                self.state.active_section = ActiveSection::Tracks;
                self.tracks = artist.items;
                self.state.tracks_scroll_state = ScrollbarState::new(
                    std::cmp::max(0, self.tracks.len() as i32 - 1) as usize
                );
                self.state.current_artist = self.artists.iter()
                    .find(|a| a.id == id)
                    .cloned()
                    .unwrap_or_default();
            }
        }
    }

    pub async fn playlist(&mut self, id: &String) {
        if id.is_empty() {
            return;
        }
        if let Some(client) = self.client.as_ref() {
            if let Ok(playlist) = client.playlist(id).await {
                self.state.active_section = ActiveSection::Tracks;
                self.tracks_playlist = playlist.items;
                self.state.playlist_tracks_scroll_state = ScrollbarState::new(
                    std::cmp::max(0, self.tracks_playlist.len() as i32 - 1) as usize
                );
                self.state.current_playlist = self.playlists.iter()
                    .find(|a| a.id == *id)
                    .cloned().unwrap_or_default();
            }
        }
    }

    pub fn mpv_start_playlist(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let sender = self.sender.clone();
        let songs = self.state.queue.clone();

        let state: MpvPlaybackState = MpvPlaybackState {
            percentage: 0.0,
            duration: 0.0,
            current_index: self.state.current_playback_state.current_index,
            last_index: -1,
            volume: self.state.current_playback_state.volume,
            audio_bitrate: 0,
            file_format: String::from(""),
        };

        if self.mpv_thread.is_some() {
            if let Ok(mpv) = self.mpv_state.lock() {
                let _ = mpv.mpv.command("stop", &[]);
                for song in &songs  {
                    mpv.mpv
                    .command("loadfile", &[&[song.url.as_str(), "append-play"].join(" ")])
                    .map_err(|e| format!("Failed to load playlist: {:?}", e))?;
                }
                let _ = mpv.mpv.set_property("pause", false);
                self.paused = false;
                self.song_changed = true;
            }
            return Ok(());
        }

        let mpv_state = self.mpv_state.clone();
        if let Some(ref mut controls) = self.controls {
            if controls.detach().is_ok() {
                self.register_controls(mpv_state.clone());
            }
        }

        self.mpv_thread = Some(thread::spawn(move || {
            if let Err(e) = Self::t_playlist(songs, mpv_state, sender, state) {
                eprintln!("Error in playlist thread: {:?}", e);
            }
        }));

        self.paused = false;

        Ok(())
    }

    /// The thread that keeps in sync with the mpv thread
    fn t_playlist(
        songs: Vec<Song>,
        mpv_state: Arc<Mutex<MpvState>>,
        sender: Sender<MpvPlaybackState>,
        state: MpvPlaybackState,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mpv = mpv_state.lock().map_err(|e| format!("Failed to lock mpv_state: {:?}", e))?;

        let _ = mpv.mpv.command("playlist_clear", &["force"]);

        for song in songs  {
            mpv.mpv
            .command("loadfile", &[&[song.url.as_str(), "append-play"].join(" ")])
            .map_err(|e| format!("Failed to load playlist: {:?}", e))?;
        }

        mpv.mpv.set_property("volume", state.volume)?;
        mpv.mpv.set_property("playlist-pos", state.current_index)?;

        drop(mpv);

        loop {
            // main mpv loop
            let mpv = mpv_state.lock().map_err(|e| format!("Failed to lock mpv_state: {:?}", e))?;

            let percentage = mpv.mpv.get_property("percent-pos").unwrap_or(0.0);
            let current_index: i64 = mpv.mpv.get_property("playlist-pos").unwrap_or(0);
            let duration = mpv.mpv.get_property("duration").unwrap_or(0.0);
            let volume = mpv.mpv.get_property("volume").unwrap_or(0);
            let audio_bitrate = mpv.mpv.get_property("audio-bitrate").unwrap_or(0);
            let file_format = mpv.mpv.get_property("file-format").unwrap_or(String::from(""));
            drop(mpv);

            let _ = sender
                .send({
                    MpvPlaybackState {
                        percentage,
                        duration,
                        current_index,
                        last_index: state.last_index,
                        volume,
                        audio_bitrate,
                        file_format: file_format.to_string(),
                    }
                });

            thread::sleep(Duration::from_secs_f32(0.2));
        }
    }

    pub fn get_image_buffer(img: image::DynamicImage) -> (Vec<u8>, color_thief::ColorFormat) {
        match img {
            image::DynamicImage::ImageRgb8(buffer) => {
                (buffer.to_vec(), color_thief::ColorFormat::Rgb)
            }
            image::DynamicImage::ImageRgba8(buffer) => {
                (buffer.to_vec(), color_thief::ColorFormat::Rgba)
            }
            _ => unreachable!(),
        }
    }

    fn grab_primary_color(&mut self, p: &str) {
        let img = match image::open(p) {
            Ok(img) => img,
            Err(_) => {
                return;
            }
        };
        let (buffer, color_type) = Self::get_image_buffer(img);
        if let Ok(colors) = color_thief::get_palette(&buffer, color_type, 10, 4) {
            let prominent_color = &colors
                .iter()
                .filter(|&color| {
                    // filter out too dark or light colors
                    let brightness = 0.299 * color.r as f32 + 0.587 * color.g as f32 + 0.114 * color.b as f32;
                    brightness > 50.0 && brightness < 200.0
                })
                .max_by_key(|color| {
                    let max = color.iter().max().unwrap();
                    let min = color.iter().min().unwrap();
                    max - min
                })
                .unwrap_or(&colors[0]);
            
            let max = prominent_color.iter().max().unwrap();
            let scale = 255.0 / max as f32;
            let mut primary_color = prominent_color.iter().map(|c| (c as f32 * scale) as u8).collect::<Vec<u8>>();

            // enhance contrast against black and white
            let brightness = 0.299 * primary_color[0] as f32
                + 0.587 * primary_color[1] as f32
                + 0.114 * primary_color[2] as f32;

            if brightness < 80.0 {
                primary_color = primary_color
                    .iter()
                    .map(|c| (c + 50).min(255))
                    .collect::<Vec<u8>>();
            } else if brightness > 200.0 {
                primary_color = primary_color
                    .iter()
                    .map(|c| (*c as i32 - 50).max(0) as u8)
                    .collect::<Vec<u8>>();
            }

            self.primary_color = Color::Rgb(primary_color[0], primary_color[1], primary_color[2]);
        }
    }

    pub async fn refresh(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        self.dirty = true;
        // this will now re-pull the artist list from the server
        if let Some(client) = &self.client {
            let artists = match client.artists(String::from("")).await {
                Ok(artists) => artists,
                Err(e) => {
                    return Err(Box::new(e));
                }
            };
            // let current_artist_id = self.get_id_of_selected(&self.artists, Selectable::Artist);
            self.artists = artists;
            self.state.artists_scroll_state = self.state.artists_scroll_state.content_length(self.artists.len() - 1);

            let playlists = match client.playlists(String::from("")).await {
                Ok(playlists) => playlists,
                Err(e) => {
                    return Err(Box::new(e));
                }
            };
            self.playlists = playlists;
            self.state.playlists_scroll_state = self.state.playlists_scroll_state.content_length(self.playlists.len() - 1);
        }

        Ok(())
    }

    pub fn save_state(&self) {
        let persist = self.config.as_ref().and_then(|c| c.get("persist")).and_then(|a| a.as_bool()).unwrap_or(true);
        if !persist {
            return;
        }
        if let Err(e) = self.state.save_state() {
            eprintln!("[XX] Failed to save state This is most likely a bug: {:?}", e);
        }
    }

    async fn load_state(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        self.state = State::load_state()?;

        let position = Some(self.state.current_playback_state.duration * (self.state.current_playback_state.percentage / 100.0));
        self.buffering = true;

        let current_artist_id = self.state.current_artist.id.clone();
        let current_playlist_id = self.state.current_playlist.id.clone();

        self.discography(&current_artist_id).await;
        self.playlist(&current_playlist_id).await;

        // the number of items may have changed. Let's use the IDs to look them up again to be sure
        let artists = search_results(&self.artists, &self.state.artists_search_term, true);
        let index = artists.iter().position(|f| f == &current_artist_id);
        self.state.selected_artist.select(Some(index.unwrap_or(0)));

        let playlists = search_results(&self.playlists, &self.state.playlists_search_term, true);
        let index = playlists.iter().position(|p| p == &current_playlist_id);
        self.state.selected_playlist.select(Some(index.unwrap_or(0)));

        // handle expired session token in urls
        if let Some(client) = self.client.as_mut() {
            for song in &mut self.state.queue {
                song.url = client.song_url_sync(song.id.clone());
            }
        }

        let _ = self.mpv_start_playlist();

        if let Ok(mpv) = self.mpv_state.lock() {
            self.song_changed = true;
            let _ = mpv.mpv.set_property("pause", true);
            self.paused = true;
        }
        self.pending_seek = position; // we seek after the song gets loaded
        println!(" - Restored previous session.");
        Ok(())
    }

    pub fn exit(&mut self) {
        self.save_state();
        self.exit = true;
    }
}
