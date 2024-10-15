use crate::client::{self, report_progress, Album, Artist, Client, DiscographySong, ProgressReport, Lyric};
use crate::keyboard::{*};
use crate::mpris;

use libmpv::{*};

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

pub struct MpvPlaybackState {
    pub percentage: f64,
    pub duration: f64,
    pub current_index: i64,
    pub last_index: i64,
    pub volume: i64,
}

/// Internal song representation. Used in the queue and passed to MPV
#[derive(Clone, Default)]
pub struct Song {
    pub id: String,
    pub url: String,
    pub name: String,
    pub artist: String,
    pub artist_items: Vec<Artist>,
    pub album: String,
    pub parent_id: String,
    pub production_year: u64,
}

pub struct App {
    pub exit: bool,

    pub artists: Vec<Artist>, // all artists
    pub tracks: Vec<DiscographySong>, // current artist's tracks
    pub lyrics: Option<(String, Vec<Lyric>, bool)>, // ID, lyrics, time_synced
    pub playlist: Vec<Song>, // (URL, Title, Artist, Album)
    pub active_song_id: String,

    pub metadata: Option<client::MediaStream>,
    pub cover_art: Option<Box<dyn StatefulProtocol>>,
    cover_art_dir: String,
    picker: Option<Picker>,

    pub paused: bool,
    pub buffering: i8, // 0 = not buffering, 1 = requested to buffer, 2 = buffering

    pub spinner: usize, // spinner for buffering
    spinner_skipped: u8,
    pub spinner_stages: Vec<&'static str>,

    // Music - active section (Artists, Tracks, Queue)
    pub active_section: ActiveSection, // current active section (Artists, Tracks, Queue)
    pub last_section: ActiveSection, // last active section

    // Search - active section (Artists, Albums, Tracks)
    pub search_section: SearchSection, // current active section (Artists, Albums, Tracks)

    // active tab (Music, Search)
    pub active_tab: ActiveTab,
    pub searching: bool,
    pub search_term: String,

    pub locally_searching: bool,
    pub artists_search_term: String,
    pub tracks_search_term: String,

    pub search_result_artists: Vec<Artist>,
    pub search_result_albums: Vec<Album>,
    pub search_result_tracks: Vec<DiscographySong>,

    // ratatui list indexes
    pub selected_artist: ListState,
    pub selected_track: ListState,
    pub selected_queue_item: ListState,
    pub selected_lyric: ListState,
    pub selected_lyric_manual_override: bool,

    pub selected_search_artist: ListState,
    pub selected_search_album: ListState,
    pub selected_search_track: ListState,

    pub client: Option<Client>, // jellyfin http client

    // mpv is run in a separate thread, this is the handle
    mpv_thread: Option<thread::JoinHandle<()>>,
    pub mpv_state: Arc<Mutex<MpvState>>, // shared mutex for controlling mpv

    // every second, we get the playback state from the mpv thread
    sender: Sender<MpvPlaybackState>,
    receiver: Receiver<MpvPlaybackState>,
    pub current_playback_state: MpvPlaybackState,
    old_percentage: f64,
    scrobble_this: (String, u64), // an id of the previous song we want to scrobble when it ends
    pub controls: Option<MediaControls>,
}

impl Default for App {
    fn default() -> Self {
        let mut picker = match Picker::from_termios() {
            Ok(picker) => {
                picker
            }
            Err(_e) => {
                let picker = Picker::new((8, 12));
                picker
            }
        };
        picker.guess_protocol();

        let (sender, receiver) = channel();

        let controls = match mpris::mpris() {
            Ok(controls) => Some(controls),
            Err(_) => None,
        };

        App {
            exit: false,
            artists: vec![],
            tracks: vec![],
            lyrics: None,
            metadata: None,
            playlist: vec![],
            active_song_id: String::from(""),
            cover_art: None,
            cover_art_dir: match cache_dir() {
                Some(dir) => dir,
                None => PathBuf::from("./"),
            }.join("jellyfin-tui").join("covers").to_str().unwrap_or("").to_string(),
            picker: Some(picker),
            paused: true,

            buffering: 0,
            spinner: 0,
            spinner_skipped: 0,
            spinner_stages: vec![
                "◰", "◳", "◲", "◱"
            ],

            active_section: ActiveSection::default(),
            last_section: ActiveSection::default(),

            search_section: SearchSection::default(),

            active_tab: ActiveTab::default(),
            searching: false,
            search_term: String::from(""),

            locally_searching: false,
            artists_search_term: String::from(""),
            tracks_search_term: String::from(""),

            search_result_artists: vec![],
            search_result_albums: vec![],
            search_result_tracks: vec![],

            selected_artist: ListState::default(),
            selected_track: ListState::default(),
            selected_queue_item: ListState::default(),
            selected_lyric: ListState::default(),
            selected_lyric_manual_override: false,

            selected_search_artist: ListState::default(),
            selected_search_album: ListState::default(),
            selected_search_track: ListState::default(),
            client: None,
            mpv_thread: None,
            mpv_state: Arc::new(Mutex::new(MpvState::new())),
            sender,
            receiver,
            current_playback_state: MpvPlaybackState {
                percentage: 0.0,
                duration: 0.0,
                current_index: 0,
                last_index: -1,
                volume: 100,
            },
            old_percentage: 0.0,
            scrobble_this: (String::from(""), 0),
            controls,
        }
    }
}

pub struct MpvState {
    pub mpris_events: Vec<MediaControlEvent>,
    pub mpv: Mpv,
    pub should_stop: bool,
}

impl MpvState {
    fn new() -> Self {
        let mpv = Mpv::new().unwrap();
        mpv.set_property("vo", "null").unwrap();
        mpv.set_property("volume", 100).unwrap();
        mpv.set_property("prefetch-playlist", "yes").unwrap(); // gapless playback

        let ev_ctx = mpv.create_event_context();
        ev_ctx.disable_deprecated_events().unwrap();
        ev_ctx.observe_property("volume", Format::Int64, 0).unwrap();
        ev_ctx
            .observe_property("demuxer-cache-state", Format::Node, 0)
            .unwrap();
        MpvState {
            mpris_events: vec![],
            mpv,
            should_stop: false,
        }
    }
}

impl App {
    pub async fn init(&mut self, artists: Vec<Artist>) {
        let client = client::Client::new().await;
        if client.access_token.is_empty() {
            println!("Failed to authenticate. Exiting...");
            return;
        }
        self.client = Some(client);
        self.artists = artists;
        self.active_section = ActiveSection::Artists;
        self.selected_artist.select(Some(0));

        self.register_controls(self.mpv_state.clone());
    }

    pub async fn run<'a>(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // get playback state from the mpv thread
        let state = self.receiver.try_recv()?;

        self.current_playback_state.percentage = state.percentage;
        self.current_playback_state.current_index = state.current_index;
        self.current_playback_state.duration = state.duration;
        self.current_playback_state.volume = state.volume;

        // Queue position
        self.selected_queue_item
            .select(Some(state.current_index as usize));

        let song = self.playlist.get(state.current_index as usize).cloned().unwrap_or_default();

        if self.current_playback_state.percentage > self.old_percentage {
            if self.buffering == 1 {
                self.buffering = 2;
            }
            else if self.buffering == 2 {
                self.buffering = 0;
            }
        }

        if (self.old_percentage + 2.0) < self.current_playback_state.percentage {
            self.old_percentage = self.current_playback_state.percentage;

            // if % > 0.5, report progress
            self.scrobble_this = (song.id.clone(), (self.current_playback_state.duration * self.current_playback_state.percentage * 100000.0) as u64);

            let client = self.client.as_ref().ok_or("[!!] No client")?;

            let runit = report_progress(
                client.base_url.clone(), client.access_token.clone(), ProgressReport {
                volume_level: self.current_playback_state.volume as u64,
                is_paused: self.paused,
                // take into account duratio, percentage and *10000
                position_ticks: (self.current_playback_state.duration * self.current_playback_state.percentage * 100000.0) as u64,
                media_source_id: self.active_song_id.clone(),
                playback_start_time_ticks: 0,
                can_seek: false, // TODO
                item_id: self.active_song_id.clone(),
                event_name: "timeupdate".to_string(),
            });
            tokio::spawn(runit);

        } else if self.old_percentage > self.current_playback_state.percentage {
            self.old_percentage = self.current_playback_state.percentage;
        }

        // song has changed
        if &song.id != &self.active_song_id {
            self.selected_lyric_manual_override = false;
            self.active_song_id = song.id.clone();

            // fetch lyrics
            let client = self.client.as_ref().ok_or("[!!] No client")?;
            let lyrics = client.lyrics(self.active_song_id.clone()).await;
            self.metadata = client.metadata(self.active_song_id.clone()).await.ok();

            self.lyrics = lyrics.map(|lyrics| {
                let time_synced = lyrics.iter().all(|l| l.start != 0);
                ( self.active_song_id.clone(), lyrics, time_synced )
            }).ok();

            self.selected_lyric.select(None);

            self.cover_art = None;
            let cover_image = client.download_cover_art(song.parent_id).await.unwrap_or_default();
            
            if !cover_image.is_empty() && !self.cover_art_dir.is_empty() {
                // let p = format!("./covers/{}", cover_image);
                let p = format!("{}/{}", self.cover_art_dir, cover_image);
                if let Ok(reader) = image::ImageReader::open(p) {
                    if let Ok(img) = reader.decode() {
                        if let Some(ref mut picker) = self.picker {
                            let image_fit_state = picker.new_resize_protocol(img.clone());
                            self.cover_art = Some(image_fit_state);
                        }
                    }
                }
            };

            if self.scrobble_this.0 != "" {
                let _ = client.stopped(
                    self.scrobble_this.0.clone(),
                    self.scrobble_this.1,
                ).await;
                self.scrobble_this = (String::from(""), 0);
            }

            let _ = client.playing(self.active_song_id.clone()).await;
        }
        Ok(())
    }

    pub async fn draw<'a>(&mut self, terminal: &'a mut Tui) -> std::result::Result<(), Box<dyn std::error::Error>> {

        // let the rats take over
        terminal
            .draw(|frame: &mut Frame| {
                self.render_frame(frame);
            })
            .unwrap();

        self.handle_events().await.unwrap();

        self.handle_mpris_events().await;

        // ratatui is an immediate mode tui which is cute, but it will be heavy on the cpu
        // later maybe make a thread that sends refresh signals
        // ok for now, but will cause some user input jank
        // let fps = 60;
        // thread::sleep(Duration::from_millis(1000 / fps));

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

        match self.active_tab {
            ActiveTab::Library => {
                self.render_home(app_container[1], frame);
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
                Constraint::Percentage(80),
                Constraint::Percentage(20),
            ])
            .split(area);
        Tabs::new(vec!["Library", "Search"])
            .style(Style::default().white())
            .highlight_style(Style::default().blue())
            .select(self.active_tab as usize)
            .divider(symbols::DOT)
            .padding(" ", " ")
            .render(tabs_layout[0], buf);

        // Volume: X%
        let volume = format!("Volume: {}% ", self.current_playback_state.volume);
        let volume_color = if self.current_playback_state.volume <= 100 {
            Color::White
        } else {
            Color::Yellow
        };
        Paragraph::new(volume)
            .style(Style::default().fg(volume_color))
            .alignment(Alignment::Right)
            .wrap(Wrap { trim: false })
            .render(tabs_layout[1], buf);
    }

    /// Fetch the discography of an artist
    /// This will change the active section to tracks
    pub async fn discography(&mut self, id: &str) {
        match self.client {
            Some(ref client) => {
                let artist = client.discography(id).await;
                match artist {
                    Ok(artist) => {
                        self.active_section = ActiveSection::Tracks;
                        self.tracks = artist.items;
                    }
                    Err(e) => {
                        println!("Failed to get discography: {:?}", e);
                    }
                }
            }
            None => {} // this would be bad
        }
    }

    pub fn replace_playlist(&mut self) {
        let _ = {
            if self.mpv_thread.is_some() {
                let alive = match self.mpv_thread {
                    Some(ref thread) => thread.is_finished(),
                    None => false,
                };
                if !alive {
                    self.mpv_thread = None;
                } else {
                    // self.mpv_thread.take().unwrap().join().unwrap();
                    match self.mpv_thread.take() {
                        Some(thread) => {
                            let _ = thread.join();
                        }
                        None => {}
                    }
                }
            }
            self.mpv_state = Arc::new(Mutex::new(MpvState::new())); // Shared state for controlling MPV
            let mpv_state = self.mpv_state.clone();
            let sender = self.sender.clone();
            let songs = self.playlist.clone();
            // println!("Playing playlist: {:?}", songs);

            let state: MpvPlaybackState = MpvPlaybackState {
                percentage: 0.0,
                duration: 0.0,
                current_index: 0,
                last_index: -1,
                volume: self.current_playback_state.volume,
            };
            match self.controls {
                Some(ref mut controls) => {
                    if match controls.detach() {
                        Ok(_) => true,
                        Err(_) => false,
                    } {
                        self.register_controls(mpv_state.clone());
                    }
                }
                None => {}
            }

            self.mpv_thread = Some(thread::spawn(move || {
                Self::t_playlist(songs, mpv_state, sender, state);
            }));

            self.paused = false;
        };
    }

    fn t_playlist(
        songs: Vec<Song>,
        mpv_state: Arc<Mutex<MpvState>>,
        sender: Sender<MpvPlaybackState>,
        state: MpvPlaybackState,
    ) {
        {
            let lock = mpv_state.clone();
            let mpv = match lock.lock() {
                Ok(mpv) => mpv,
                Err(_) => {
                    return;
                }
            };

            match mpv.mpv.playlist_clear() {
                Ok(_) => {}
                Err(_) => {}
            }

            mpv.mpv
                .playlist_load_files(
                    &songs
                        .iter()
                        .map(|song| (song.url.as_str(), FileState::AppendPlay, None))
                        .collect::<Vec<(&str, FileState, Option<&str>)>>()
                        .as_slice(),
                )
                .unwrap();

            mpv.mpv.set_property("volume", state.volume).unwrap();

            drop(mpv);

            loop {
                // main mpv loop
                let lock = mpv_state.clone();
                let mpv = match lock.lock() {
                    Ok(mpv) => mpv,
                    Err(_) => {
                        return;
                    }
                };
                if mpv.should_stop {
                    return;
                }
                let percentage = mpv.mpv.get_property("percent-pos").unwrap_or(0.0);
                let current_index: i64 = mpv.mpv.get_property("playlist-pos").unwrap_or(0);
                let duration = mpv.mpv.get_property("duration").unwrap_or(0.0);
                let volume = mpv.mpv.get_property("volume").unwrap_or(0);
                drop(mpv);

                let _ = sender
                    .send({
                        MpvPlaybackState {
                            percentage,
                            duration,
                            current_index,
                            last_index: state.last_index,
                            volume: volume as i64,
                        }
                    });

                thread::sleep(Duration::from_secs_f32(0.2));
            }
        }
    }

    pub fn exit(&mut self) {
        self.exit = true;
    }
}
