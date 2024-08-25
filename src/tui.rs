use crate::client::{self, report_progress, Album, Artist, Client, DiscographySong, ProgressReport, Lyric};
use crate::keyboard::{*};
use crate::mpris;
use layout::Flex;
use libmpv::{*};

use std::io::Stdout;

use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata};

use ratatui::{
    Terminal,
    Frame,
    symbols::border,
    widgets::{
        Block,
        block::Title,
        block::Position,
        Borders,
        Paragraph
    },
    prelude::*,
    widgets::*,
};

use ratatui_image::{picker::Picker, StatefulImage, protocol::StatefulProtocol, Resize};

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
#[derive(Clone)]
pub struct Song {
    pub id: String,
    pub url: String,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub parent_id: String,
    pub production_year: u64,
}

pub struct App {
    pub exit: bool,

    pub artists: Vec<Artist>, // all artists
    pub tracks: Vec<DiscographySong>, // current artist's tracks
    pub lyrics: (String, Vec<Lyric>, bool), // ID, lyrics, time_synced
    pub playlist: Vec<Song>, // (URL, Title, Artist, Album)
    pub active_song_id: String,

    metadata: Option<client::MediaStream>,
    cover_art: Option<Box<dyn StatefulProtocol>>,
    picker: Option<Picker>,

    pub paused: bool,
    pub buffering: i8, // 0 = not buffering, 1 = requested to buffer, 2 = buffering

    spinner: usize, // spinner for buffering
    spinner_skipped: u8,
    spinner_stages: Vec<&'static str>,
    
    // Music - active section (Artists, Tracks, Queue)
    pub active_section: ActiveSection, // current active section (Artists, Tracks, Queue)
    pub last_section: ActiveSection, // last active section

    // Search - active section (Artists, Albums, Tracks)
    pub search_section: SearchSection, // current active section (Artists, Albums, Tracks)

    // active tab (Music, Search)
    pub active_tab: ActiveTab,
    pub searching: bool,
    pub search_term: String,

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
    pub controls: MediaControls,
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

        let controls = mpris::mpris();

        App {
            exit: false,
            artists: vec![],
            tracks: vec![],
            lyrics: (String::from(""), vec![], false),
            metadata: None,
            playlist: vec![],
            active_song_id: String::from(""),
            cover_art: None,
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

    pub async fn run<'a>(&mut self, terminal: &'a mut Tui) {
        // get playback state from the mpv thread
        match self.receiver.try_recv() {
            Ok(state) => {
                self.current_playback_state.percentage = state.percentage;
                self.current_playback_state.current_index = state.current_index;
                self.current_playback_state.duration = state.duration;
                self.current_playback_state.volume = state.volume;

                // Queue position
                self.selected_queue_item
                    .select(Some(state.current_index as usize));

                let song = match self.playlist.get(state.current_index as usize) {
                    Some(song) => song.clone(),
                    None => Song {
                        id: String::from(""),
                        url: String::from(""),
                        name: String::from(""),
                        artist: String::from(""),
                        album: String::from(""),
                        parent_id: String::from(""),
                        production_year: 0,
                    },
                };
                let song_id = song.id.clone();

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
                    self.scrobble_this = (song_id.clone(), (self.current_playback_state.duration * self.current_playback_state.percentage * 100000.0) as u64);

                    let client = self.client.as_ref().unwrap();
                    
                    let runit = report_progress(
                        client.base_url.clone(), client.access_token.clone(), ProgressReport {
                        volume_level: 100,
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
                if song_id != self.active_song_id {
                    self.selected_lyric_manual_override = false;
                    self.active_song_id = song_id;
                    // fetch lyrics
                    match self.client {
                        Some(ref client) => {
                            let lyrics = client.lyrics(self.active_song_id.clone()).await;
                            let metadata = match client.metadata(self.active_song_id.clone()).await {
                                Ok(metadata) => Some(metadata),
                                _ => {
                                    None
                                }
                            };
                            let cover_image = match client.download_cover_art(song.parent_id).await {
                                Ok(cover_image) => {
                                    if cover_image != "" {
                                        Some(cover_image)
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };
                            // force log the song, then panic
                            match lyrics {
                                Ok(lyrics) => {
                                    let time_synced = lyrics.iter().all(|l| l.start != 0);
                                    self.lyrics = (self.active_song_id.clone(), lyrics, time_synced);
                                }
                                _ => {
                                    self.lyrics = (String::from(""), vec![], false);
                                }
                            }
                            self.selected_lyric.select(None);

                            match metadata {
                                Some(metadata) => {
                                    self.metadata = Some(metadata);
                                }
                                _ => {
                                    self.metadata = None;
                                }
                            }
                            match cover_image {
                                Some(cover_image) => {
                                    let p = format!("./covers/{}", cover_image);
                                    let _ = match image::ImageReader::open(p) {
                                        Ok(reader) => {
                                            match reader.decode() {
                                                Ok(img) => {
                                                    match self.picker {
                                                        Some(ref mut picker) => {
                                                            let image_fit_state = picker.new_resize_protocol(img.clone());
                                                            self.cover_art = Some(image_fit_state);
                                                        }
                                                        None => {}
                                                    }
                                                }
                                                Err(_e) => {
                                                    //self.cover_art = String::from("");
                                                    return;
                                                }
                                            }
                                        }
                                        Err(_e) => {
                                            //self.cover_art = String::from("");
                                            return;
                                        }
                                    };
                                }
                                None => {
                                    self.cover_art = None;
                                }
                            }

                            if self.scrobble_this.0 != "" {
                                let _ = client.stopped(
                                    self.scrobble_this.0.clone(),
                                    self.scrobble_this.1,
                                ).await;
                                self.scrobble_this = (String::from(""), 0);
                            }

                            let _ = client.playing(self.active_song_id.clone()).await;
                        }
                        None => {}
                    }
                }
            }
            Err(_) => {}
        }

        // let the rats take over
        terminal
            .draw(|frame: &mut Frame| {
                self.render_frame(frame);
            })
            .unwrap();

        self.handle_events().await.unwrap();

        self.handle_mpris_events();

        // ratatui is an immediate mode tui which is cute, but it will be heavy on the cpu
        // later maybe make a thread that sends refresh signals
        // ok for now, but will cause some user input jank
        let fps = 60;
        thread::sleep(Duration::from_millis(1000 / fps));
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
        Paragraph::new(volume)
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Right)
            .wrap(Wrap { trim: false })
            .render(tabs_layout[1], buf);
    }

    fn render_search(&mut self, app_container: Rect, frame: &mut Frame) {
        // search bar up top, results in 3 lists. Artists, Albums, Tracks
        // split the app container into 2 parts
        let search_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Min(3),
                Constraint::Percentage(95),
            ])
            .split(app_container);

        let search_area = search_layout[0];
        let results_area = search_layout[1];


        // render search bar
        if self.searching {
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Search")
                    .border_style(style::Color::Blue),
                search_area,
            );
        } else {
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Search")
                    .border_style(style::Color::Gray),
                search_area,
            );
        };

        // search term
        let search_term = Paragraph::new(self.search_term.clone())
            .block(Block::default().borders(Borders::ALL).title("Search Term"))
            .wrap(Wrap { trim: false });
        frame.render_widget(search_term, search_area);

        let instructions = if self.searching {
            Title::from(Line::from(vec![
                " Search ".white().into(),
                "<Enter>".blue().bold(),
                " Clear search ".white().into(),
                "<Delete>".blue().bold(),
                " Cancel ".white().into(),
                "<Esc> ".blue().bold(),
            ]))
        } else {
            Title::from(Line::from(vec![
                " Go ".white().into(),
                "<Enter>".blue().bold(),
                " Search ".white().into(),
                "< / > <F2>".blue().bold(),
                " Next Section ".white().into(),
                "<Tab>".blue().bold(),
                " Previous Section ".white().into(),
                "<Shift+Tab> ".blue().bold(),
            ]))
        };

        Block::default()
            .title("Search")
            .title(
                instructions
                    .alignment(Alignment::Center)
                    .position(Position::Bottom),
            )
            .borders(Borders::ALL)
            .border_set(border::THICK)
            .render(search_area, frame.buffer_mut());

        // split results area into 3 parts
        let results_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(results_area);

        // render search results
        // 3 lists, artists, albums, tracks
        let artists = self
            .search_result_artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<&str>>();

        let albums = self
            .search_result_albums
            .iter()
            .map(|album| album.name.as_str())
            .collect::<Vec<&str>>();
        let tracks = self
            .search_result_tracks
            .iter()
            .map(|track| {
                let title = format!("{} - {}", track.name, track.album);
                // track.run_time_ticks is in microseconds
                let seconds = (track.run_time_ticks / 1_000_0000) % 60;
                let minutes = (track.run_time_ticks / 1_000_0000 / 60) % 60;
                let hours = (track.run_time_ticks / 1_000_0000 / 60) / 60;
                let hours_optional_text = match hours {
                    0 => String::from(""),
                    _ => format!("{}:", hours),
                };

                let mut time_span_text = format!("  {}{:02}:{:02}", hours_optional_text, minutes, seconds);
                if track.has_lyrics{
                    time_span_text.push_str(" (l)");
                }
                if track.id == self.active_song_id {
                    let mut time: Text = Text::from(title);
                    time.push_span(
                        Span::styled(
                            time_span_text,
                            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                        )
                    );
                    ListItem::new(time)
                        .style(Style::default().fg(Color::Blue))
                } else {
                    let mut time: Text = Text::from(title);
                    time.push_span(
                        Span::styled(
                            time_span_text,
                            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                        )
                    );
                    ListItem::new(time)
                }
            })
            .collect::<Vec<ListItem>>();

        let artists_list = match self.search_section {
            SearchSection::Artists => List::new(artists)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(style::Color::Blue)
                        .title("Artists")
                )
                .highlight_symbol(">>")
                .highlight_style(
                    Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED)
                )
                .scroll_padding(10)
                .repeat_highlight_symbol(true),
            _ => List::new(artists)
                .block(Block::default().borders(Borders::ALL).title("Artists"))
                .highlight_symbol(">>")
                .highlight_style(
                    Style::default()
                    .add_modifier(Modifier::BOLD)
                    .bg(Color::DarkGray)
                    .fg(Color::Black)
                )
                .scroll_padding(10)
                .repeat_highlight_symbol(true),
        };

        let albums_list = match self.search_section {
            SearchSection::Albums => List::new(albums)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(style::Color::Blue)
                        .title("Albums")
                )
                .highlight_symbol(">>")
                .highlight_style(
                    Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED)
                )
                .repeat_highlight_symbol(true),
            _ => List::new(albums)
                .block(Block::default().borders(Borders::ALL).title("Albums"))
                .highlight_symbol(">>")
                .highlight_style(
                    Style::default()
                    .add_modifier(Modifier::BOLD)
                    .bg(Color::DarkGray)
                    .fg(Color::Black)
                )
                .repeat_highlight_symbol(true),
        };

        let tracks_list = match self.search_section {
            SearchSection::Tracks => List::new(tracks)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(style::Color::Blue)
                        .title("Tracks")
                )
                .highlight_symbol(">>")
                .highlight_style(
                    Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED)
                )
                .repeat_highlight_symbol(true),
            _ => List::new(tracks)
                .block(Block::default().borders(Borders::ALL).title("Tracks"))
                .highlight_symbol(">>")
                .highlight_style(
                    Style::default()
                    .add_modifier(Modifier::BOLD)
                    .bg(Color::DarkGray)
                    .fg(Color::Black)
                )
                .repeat_highlight_symbol(true),
        };

        // frame.render_widget(artists_list, results_layout[0]);
        frame.render_stateful_widget(artists_list, results_layout[0], &mut self.selected_search_artist);
        frame.render_stateful_widget(albums_list, results_layout[1], &mut self.selected_search_album);
        frame.render_stateful_widget(tracks_list, results_layout[2], &mut self.selected_search_track);

        // render search results
    }

    /// TODO: optimize this
    fn render_home(&mut self, app_container: Rect, frame: &mut Frame) {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(18),
                Constraint::Percentage(58),
                Constraint::Percentage(24),
            ])
            .split(app_container);

        let left = outer_layout[0];

        // create a wrapper, to get the width. After that create the inner 'left' and split it
        let center = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(86), Constraint::Min(8)])
            .split(outer_layout[1]);

        let right = match self.lyrics.1.len() {
            0 => Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Min(3), Constraint::Percentage(100)])
                .split(outer_layout[2]),
            _ => Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Percentage(68), Constraint::Percentage(32)])
                .split(outer_layout[2]),
        };

        let artist_block = match self.active_section {
            ActiveSection::Artists => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::Blue),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::White),
        };

        let artist_highlight_style = match self.active_section {
            ActiveSection::Artists => Style::default()
                .bg(Color::White)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            _ => Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        };

        // currently playing song name. We can get this easily, we have the playlist and the current index
        let current_artist = match self
            .playlist
            .get(self.current_playback_state.current_index as usize)
        {
            Some(song) => {
                song.artist.clone()
            }
            None => String::from(""),
        };

        // render all artists as a list here in left[0]
        let items = self
            .artists
            .iter()
            .map(|artist| {
                if artist.name == current_artist {
                    return ListItem::new(artist.name.as_str())
                        .style(Style::default().fg(Color::Blue))
                } else {
                    return ListItem::new(artist.name.as_str())
                }
            })
            .collect::<Vec<ListItem>>();
            // .collect::<Vec<&str>>();

        let list = List::new(items)
            .block(artist_block.title("Artist"))
            .highlight_symbol(">>")
            .highlight_style(
                artist_highlight_style
            )
            .repeat_highlight_symbol(true);

        frame.render_stateful_widget(list, left, &mut self.selected_artist);

        let track_block = match self.active_section {
            ActiveSection::Tracks => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::Blue),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::White),
        };
        
        let track_highlight_style = match self.active_section {
            ActiveSection::Tracks => Style::default()
                .bg(Color::White)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            _ => Style::default()
                .bg(Color::DarkGray)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        };
        let items = self
            .tracks
            .iter()
            .map(|track| {
                let title = format!("{} - {}", track.album, track.name);
                // track.run_time_ticks is in microseconds
                let seconds = (track.run_time_ticks / 1_000_0000) % 60;
                let minutes = (track.run_time_ticks / 1_000_0000 / 60) % 60;
                let hours = (track.run_time_ticks / 1_000_0000 / 60) / 60;
                let hours_optional_text = match hours {
                    0 => String::from(""),
                    _ => format!("{}:", hours),
                };

                let mut time_span_text = format!("  {}{:02}:{:02}", hours_optional_text, minutes, seconds);
                if track.has_lyrics{
                    time_span_text.push_str(" (l)");
                }
                if track.id == self.active_song_id {
                    let mut time: Text = Text::from(title);
                    time.push_span(
                        Span::styled(
                            time_span_text,
                            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                        )
                    );
                    ListItem::new(time)
                        .style(Style::default().fg(Color::Blue))
                } else {
                    let mut time: Text = Text::from(title);
                    time.push_span(
                        Span::styled(
                            time_span_text,
                            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                        )
                    );
                    ListItem::new(time)
                }
            })
            .collect::<Vec<ListItem>>();
        let list = List::new(items)
            .block(track_block.title("Track"))
            .highlight_symbol(">>")
            .highlight_style(
                track_highlight_style
            )
            .scroll_padding(10)
            .repeat_highlight_symbol(true);

        if self.tracks.len() == 0 {
            let message_paragraph = Paragraph::new("jellyfin-tui")
                .block(
                    Block::default().borders(Borders::ALL).title("Track").padding(Padding::new(
                        0, 0, center[0].height / 2, 0,
                    )),
                )
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Center);
            frame.render_widget(message_paragraph, center[0]);
        } else {
            frame.render_stateful_widget(list, center[0], &mut self.selected_track);
        }

        // render controls
        frame.render_widget(
            &Controls {},
            Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Percentage(100)])
                .split(center[0])[0],
        );

        // currently playing song name. We can get this easily, we have the playlist and the current index
        let current_song = match self
            .playlist
            .get(self.current_playback_state.current_index as usize)
        {
            Some(song) => {
                let str = format!("{} - {} - {}", song.name, song.artist, song.album);
                if song.production_year > 0 {
                    format!("{} ({})", str, song.production_year)
                } else {
                    str
                }
            }
            None => String::from("No song playing"),
        };

        // update mpris metadata
        if self.current_playback_state.current_index != self.current_playback_state.last_index {
            let metadata = match self
                .playlist
                .get(self.current_playback_state.current_index as usize)
            {
                Some(song) => {
                    let metadata = MediaMetadata {
                        title: Some(song.name.as_str()),
                        artist: Some(song.artist.as_str()),
                        album: Some(song.album.as_str()),
                        cover_url: None,
                        duration: None,
                    };
                    // if let Some(ref cover_art) = self.cover_art {
                    //     metadata.cover_url = Some(cover_art
                    // }
                    metadata
                }
                None => MediaMetadata {
                    title: None,
                    artist: None,
                    album: None,
                    cover_url: None,
                    duration: None,
                },
            };
            match self.controls.set_metadata(metadata) {
                _ => {}
            }
        }
        let bottom = Block::default()
            .borders(Borders::ALL)
            .padding(Padding::new(0, 0, 0, 0));
        let inner = bottom.inner(center[1]);
        frame.render_widget(bottom, center[1]);

        // split the bottom into two parts
        let bottom_split = Layout::default()
            .flex(Flex::SpaceAround)
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(15), Constraint::Percentage(85)])
            .split(inner);

        if self.cover_art.is_some() {
            let image = StatefulImage::new(None).resize(Resize::Fit(None));
            frame.render_stateful_widget(image, self.centered_rect(bottom_split[0], 80, 100), self.cover_art.as_mut().unwrap());
        } else {
            self.cover_art = None;
        }
        

        let layout = Layout::vertical(vec![
            Constraint::Percentage(55),
            Constraint::Percentage(45),
        ])
        .split(bottom_split[1]);

        // current song
        frame.render_widget(
            Paragraph::new(current_song).block(
                Block::bordered()
                    .borders(Borders::NONE)
                    .padding(Padding::new(2, 2, 1, 0)),
            ),
            layout[0],
        );

        let progress_bar_area = Layout::default()
            .direction(Direction::Horizontal)
            .flex(Flex::Center)
            .constraints(vec![
                Constraint::Percentage(5),
                Constraint::Fill(93),
                Constraint::Min(20),
            ])
            .split(layout[1]);

        frame.render_widget(
            LineGauge::default()
                .block(Block::bordered().padding(Padding::ZERO).borders(Borders::NONE))
                .filled_style(
                    if self.buffering != 0 {
                        Style::default()
                            .fg(Color::LightBlue)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    }
                )
                .unfilled_style(
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
                .line_set(symbols::line::ROUNDED)
                .ratio(self.current_playback_state.percentage / 100 as f64),
            progress_bar_area[1],
        );

        let metadata = match self.metadata {
            Some(ref metadata) => format!(
                "{} - {} Hz - {} channels - {} kbps",
                metadata.codec.as_str(),
                metadata.sample_rate,
                metadata.channels,
                metadata.bit_rate / 1000,
            ),
            None => String::from("No metadata available"),
        };

        frame.render_widget(
            Paragraph::new(metadata).centered().block(
                Block::bordered()
                    .borders(Borders::NONE)
                    .padding(Padding::new(
                        1,
                        1,
                        1,
                        0,
                    )),
            ),
            progress_bar_area[1],
        );

        if self.buffering != 0 {
            frame.render_widget(
                Paragraph::new(self.spinner_stages[self.spinner]).left_aligned().block(
                    Block::bordered()
                        .borders(Borders::NONE)
                        .padding(Padding::ZERO),
                ),
                progress_bar_area[0],
            );
        } else {
            match self.paused {
                true => {
                    frame.render_widget(
                        Paragraph::new("⏸︎").left_aligned().block(
                            Block::bordered()
                                .borders(Borders::NONE)
                                .padding(Padding::ZERO),
                        ),
                        progress_bar_area[0],
                    );
                }
                false => {
                    frame.render_widget(
                        Paragraph::new("►").left_aligned().block(
                            Block::bordered()
                                .borders(Borders::NONE)
                                .padding(Padding::ZERO),
                        ),
                        progress_bar_area[0],
                    );
                }
            }
        }

        match self.current_playback_state.duration {
            0.0 => {
                frame.render_widget(
                    Paragraph::new("0:00 / 0:00").centered().block(
                        Block::bordered()
                            .borders(Borders::NONE)
                            .padding(Padding::ZERO),
                    ),
                    progress_bar_area[2],
                );
            }
            _ => {
                let current_time = self.current_playback_state.duration * self.current_playback_state.percentage / 100.0;
                let total_seconds = self.current_playback_state.duration;
                let duration = format!(
                    "{}:{:02} / {}:{:02}",
                    current_time as u32 / 60,
                    current_time as u32 % 60,
                    total_seconds as u32 / 60,
                    total_seconds as u32 % 60
                );
                
                frame.render_widget(
                    Paragraph::new(duration).centered().block(
                        Block::bordered()
                            .borders(Borders::NONE)
                            .padding(Padding::ZERO),
                    ),
                    progress_bar_area[2],
                );
            }
        }

        let lyrics_block = match self.active_section {
            ActiveSection::Lyrics => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::Blue)
                ,
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::White),
        };

        match self.lyrics.1.len() {
            0 => {
                let message_paragraph = Paragraph::new("No lyrics available")
                .block(
                    lyrics_block.title("Lyrics"),
                )
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Center);

                frame.render_widget(
                    message_paragraph, right[0],
                );
            }
            _ => {
                // this will show the lyrics in a scrolling list
                let items = self
                    .lyrics
                    .1
                    .iter()
                    .map(|lyric| {
                        let width = right[0].width as usize;
                        if lyric.text.len() > (width - 5) {
                            // word wrap
                            let mut lines = vec![];
                            let mut line = String::new();
                            for word in lyric.text.split_whitespace() {
                                if line.len() + word.len() + 1 < width - 5 {
                                    line.push_str(word);
                                    line.push_str(" ");
                                } else {
                                    lines.push(line.clone());
                                    line.clear();
                                    line.push_str(word);
                                    line.push_str(" ");
                                }
                            }
                            lines.push(line);
                            // assemble into string separated by newlines
                            lines.join("\n")
                        } else {
                            lyric.text.clone()
                        }
                    })
                    .collect::<Vec<String>>();

                let list = List::new(items)
                    .block(lyrics_block.title("Lyrics"))
                    .highlight_symbol(">>")
                    .highlight_style(
                        Style::default()
                        .add_modifier(Modifier::BOLD)
                        .add_modifier(Modifier::REVERSED)
                    )
                    .repeat_highlight_symbol(false)
                    .scroll_padding(10);
                frame.render_stateful_widget(list, right[0], &mut self.selected_lyric);
                
                // if lyrics are time synced, we will scroll to the current lyric
                if self.lyrics.2 && !self.selected_lyric_manual_override {
                    let current_time = self.current_playback_state.duration * self.current_playback_state.percentage / 100.0;
                    let current_time_microseconds = (current_time * 10_000_000.0) as u64;
                    for (i, lyric) in self.lyrics.1.iter().enumerate() {
                        if lyric.start >= current_time_microseconds {
                            let index = i - 1;
                            if index >= self.lyrics.1.len() {
                                self.selected_lyric.select(Some(0));
                            } else {
                                self.selected_lyric.select(Some(index));
                            }
                            break;
                        }
                    }
                }
            }
        }

        let queue_block = match self.active_section {
            ActiveSection::Queue => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::Blue),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::White),
        };

        let items = self
            .playlist
            .iter()
            .map(|song| song.name.as_str())
            .collect::<Vec<&str>>();
        let list = List::new(items)
            .block(queue_block.title("Queue"))
            .highlight_symbol(">>")
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            )
            .scroll_padding(10)
            .repeat_highlight_symbol(true);

        frame.render_stateful_widget(list, right[1], &mut self.selected_queue_item);
    }

    pub fn centered_rect(&self, r: Rect, percent_x: u16, percent_y: u16) -> Rect {
        let popup_layout = Layout::default()
          .direction(Direction::Vertical)
          .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
          ])
          .split(r);
      
        Layout::default()
          .direction(Direction::Horizontal)
          .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
          ])
          .split(popup_layout[1])[1]
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

            if match self.controls.detach() {
                Ok(_) => true,
                Err(_) => false,
            } {
                self.register_controls(mpv_state.clone());
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

                // println!("Playlist pos: {:?}", pos);
                drop(mpv);
                sender
                    .send({
                        MpvPlaybackState {
                            percentage,
                            duration,
                            current_index,
                            last_index: state.last_index,
                            volume: volume as i64,
                        }
                    })
                    .unwrap();

                thread::sleep(Duration::from_secs_f32(0.2));
            }
        }
    }

    pub fn exit(&mut self) {
        self.exit = true;
    }
}

struct Controls {}
impl Widget for &Controls {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let instructions = Title::from(Line::from(vec![
            " Play/Pause ".white().into(),
            "<Space>".blue().bold(),
            " Seek+5s ".white().into(),
            "<S>".blue().bold(),
            " Seek-5s ".white().into(),
            "<R>".blue().bold(),
            " Next Section ".white().into(),
            "<Tab>".blue().bold(),
            " Quit ".white().into(),
            "<Q> ".blue().bold(),
        ]));
        Block::default()
            .title("Track")
            .title(
                instructions
                    .alignment(Alignment::Center)
                    .position(Position::Bottom),
            )
            .borders(Borders::ALL)
            .border_set(border::THICK)
            .render(area, buf);

    }
}
