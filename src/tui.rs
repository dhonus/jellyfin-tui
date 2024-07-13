use crate::client::{self, Artist, Client, DiscographySong, ProgressReport, report_progress};
use layout::Flex;
use libmpv::{*};
use mpris_server::{LocalServer, PlaybackRate};

use std::io::{self, Stdout};

use ratatui::symbols::border;
use ratatui::widgets::block::Title;
use ratatui::widgets::Borders;
use ratatui::widgets::{block::Position, Block, Paragraph};
use ratatui::{prelude::*, widgets::*};

use ratatui::{Terminal, terminal::Frame};
use ratatui_image::{picker::Picker, StatefulImage, protocol::StatefulProtocol, Resize};

use std::time::Duration;

/// A type alias for the terminal type used in this application
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use std::thread;

use crossterm::event::{self, Event, KeyEvent};
use crossterm::event::KeyCode;

use futures::executor::block_on;
use mpris_server::{zbus::Result, Player, Time, Metadata};

#[derive(Debug)]
pub enum ActiveSection {
    Artists,
    Tracks,
    Queue,
}
impl Default for ActiveSection {
    fn default() -> Self {
        ActiveSection::Artists
    }
}

pub struct MpvPlaybackState {
    pub percentage: f64,
    pub duration: f64,
    pub current_index: i64,
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

#[derive(Clone)]
pub struct MprisExchange {
    pub play_pause: bool,
    pub stop: bool,
    pub next: bool,
    pub previous: bool,
    pub volume: f64, // volume level change as +-%
    // pub position: f64,
    pub metadata: Song,
}

pub struct App {
    pub exit: bool,

    artists: Vec<Artist>, // all artists
    tracks: Vec<DiscographySong>, // current artist's tracks
    lyrics: (String, Vec<String>),
    metadata: Option<client::MediaStream>,
    playlist: Vec<Song>, // (URL, Title, Artist, Album)
    active_song_id: String,
    cover_art: Option<Box<dyn StatefulProtocol>>,
    picker: Option<Picker>,
    paused: bool,
    active_section: ActiveSection, // current active section (Artists, Tracks, Queue)
    last_section: ActiveSection, // last active section
    
    // ratatui list indexes
    selected_artist: ListState,
    selected_track: ListState,
    selected_queue_item: ListState,
    
    client: Option<Client>, // jellyfin http client
    
    // mpv is run in a separate thread, this is the handle
    mpv_thread: Option<thread::JoinHandle<()>>,
    mpv_state: Arc<Mutex<MpvState>>, // shared mutex for controlling mpv
    
    dbus_state: Arc<Mutex<MprisExchange>>, // shared mutex for controlling dbus
    
    // every second, we get the playback state from the mpv thread
    sender: Sender<MpvPlaybackState>, 
    receiver: Receiver<MpvPlaybackState>,
    current_playback_state: MpvPlaybackState,
    old_percentage: f64,
    scrobble_this: (String, u64), // an id of the previous song we want to scrobble when it ends
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

        App {
            exit: false,
            artists: vec![],
            tracks: vec![],
            lyrics: (String::from(""), vec![]),
            metadata: None,
            playlist: vec![],
            active_song_id: String::from(""),
            cover_art: None,
            picker: Some(picker),
            paused: true,
            active_section: ActiveSection::Artists,
            last_section: ActiveSection::Artists,
            selected_artist: ListState::default(),
            selected_track: ListState::default(),
            selected_queue_item: ListState::default(),
            client: None,
            mpv_thread: None,
            mpv_state: Arc::new(Mutex::new(MpvState::new())),
            dbus_state: Arc::new(Mutex::new(MprisExchange {
                play_pause: false,
                stop: false,
                next: false,
                previous: false,
                volume: 0.0,
                metadata: Song {
                    id: String::from(""),
                    url: String::from(""),
                    name: String::from(""),
                    artist: String::from(""),
                    album: String::from(""),
                    parent_id: String::from(""),
                    production_year: 0,
                },
            })),
            sender,
            receiver,
            current_playback_state: MpvPlaybackState {
                percentage: 0.0,
                duration: 0.0,
                current_index: 0,
            },
            old_percentage: 0.0,
            scrobble_this: (String::from(""), 0),
        }
    }
}

struct MpvState {
    mpv: Mpv,
    should_stop: bool,
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

        // spawn the mprising thread
        // let _ = {
        //     let mpris_state = self.dbus_state.clone();
        //     let mpv_state = self.mpv_state.clone();
        //     thread::spawn(move || block_on(
        //         Self::t_mpris(mpris_state, mpv_state)
        //     ));
        // };

        // let player = Player::builder("com.tui.jellyfin")
        //     .can_play(true)
        //     .can_pause(true)
        //     .build()
        //     .await;

        // match player {
        //     Ok(player) => {
        //         println!("MPRIS server started");
        //         player.connect_play_pause(|_player| {
        //             println!("PlayPause");
        //         });
        //         player.set_metadata(
        //             Metadata::builder()
        //                 .title("Title")
        //                 .artist(["Artist"])
        //                 .album("Album")
        //                 .build(),
        //         ).await;

        //         player.run().await;
        //     }
        //     Err(e) => {
        //         println!("Failed to start MPRIS server: {:?}", e);
        //     }
        // }
    }

    pub async fn run(&mut self, terminal: &mut Tui) {
        // get playback state from the mpv thread
        match self.receiver.try_recv() {
            Ok(state) => {
                self.current_playback_state.percentage = state.percentage;
                self.current_playback_state.current_index = state.current_index;
                self.current_playback_state.duration = state.duration;

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

                if song_id != self.active_song_id {
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
                                    self.lyrics = (self.active_song_id.clone(), lyrics);
                                }
                                _ => {
                                    self.lyrics = (String::from(""), vec![]);
                                }
                            }
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
                                    let _ = match image::io::Reader::open(p) {
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
            .draw(|frame| {
                self.render_frame(frame);
            })
            .unwrap();

        self.handle_events().await.unwrap();

        // ratatui is an immediate mode tui which is cute, but it will be heavy on the cpu
        // later maybe make a thread that sends refresh signals
        // ok for now, but will cause some user input jank
        let fps = 60;
        thread::sleep(Duration::from_millis(1000 / fps));
    }

    fn toggle_section(&mut self, forwards: bool) {
        match forwards {
            true => match self.active_section {
                ActiveSection::Artists => self.active_section = ActiveSection::Tracks,
                ActiveSection::Tracks => self.active_section = ActiveSection::Artists,
                ActiveSection::Queue => match self.last_section {
                    ActiveSection::Artists => self.active_section = ActiveSection::Artists,
                    ActiveSection::Tracks => self.active_section = ActiveSection::Tracks,
                    ActiveSection::Queue => self.active_section = ActiveSection::Artists,
                },
            },
            false => match self.active_section {
                ActiveSection::Artists => {
                    self.last_section = ActiveSection::Artists;
                    self.active_section = ActiveSection::Queue;
                }
                ActiveSection::Tracks => {
                    self.last_section = ActiveSection::Tracks;
                    self.active_section = ActiveSection::Queue;
                }
                ActiveSection::Queue => match self.last_section {
                    ActiveSection::Artists => self.active_section = ActiveSection::Artists,
                    ActiveSection::Tracks => self.active_section = ActiveSection::Tracks,
                    ActiveSection::Queue => self.active_section = ActiveSection::Artists,
                },
            },
        }
    }

    /// This is the main render function for rataui. It's called every frame.
    /// TODO: optimize this
    pub fn render_frame(&mut self, frame: &mut Frame) {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(18),
                Constraint::Percentage(58),
                Constraint::Percentage(24),
            ])
            .split(frame.size());

        let left = outer_layout[0];

        // create a wrapper, to get the width. After that create the inner 'left' and split it
        let center = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(86), Constraint::Min(8)])
            .split(outer_layout[1]);

        let right = match self.lyrics.1.len() {
            0 => Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Percentage(10), Constraint::Percentage(90)])
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
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED),
            _ => Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        };

        // render all artists as a list here in left[0]
        let items = self
            .artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<&str>>();

        let list = List::new(items)
            .block(artist_block.title("Artist / Album"))
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
            .repeat_highlight_symbol(true);
        frame.render_stateful_widget(list, center[0], &mut self.selected_track);

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
                .block(Block::bordered().padding(Padding::zero()).borders(Borders::NONE))
                .gauge_style(
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::DarkGray)
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

        match self.paused {
            true => {
                frame.render_widget(
                    Paragraph::new("⏸︎").left_aligned().block(
                        Block::bordered()
                            .borders(Borders::NONE)
                            .padding(Padding::zero()),
                    ),
                    progress_bar_area[0],
                );
            }
            false => {
                frame.render_widget(
                    Paragraph::new("►").left_aligned().block(
                        Block::bordered()
                            .borders(Borders::NONE)
                            .padding(Padding::zero()),
                    ),
                    progress_bar_area[0],
                );
            }
        }

        match self.current_playback_state.duration {
            0.0 => {
                frame.render_widget(
                    Paragraph::new("0:00 / 0:00").centered().block(
                        Block::bordered()
                            .borders(Borders::NONE)
                            .padding(Padding::zero()),
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
                            .padding(Padding::zero()),
                    ),
                    progress_bar_area[2],
                );
            }
        }

        match self.lyrics.1.len() {
            0 => {
                let lyrics = "No lyrics available";
                frame.render_widget(
                    Paragraph::new(lyrics).block(Block::new().borders(Borders::ALL)),
                    right[0],
                );
            }
            _ => {
                let lyrics = self.lyrics.1.join("\n");
                frame.render_widget(
                    Paragraph::new(lyrics)
                        .block(Block::new().title("Lyrics")
                            .borders(Borders::ALL).padding(Padding::horizontal(1))
                        ).wrap(Wrap { trim: false }),
                    right[0],
                );
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
            .repeat_highlight_symbol(true);

        frame.render_stateful_widget(list, right[1], &mut self.selected_queue_item);
    }

    async fn handle_events(&mut self) -> io::Result<()> {
        while event::poll(Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(key_event) => {
                    self.handle_key_event(key_event).await;
                }
                Event::Mouse(mouse_event) => {
                    self.handle_mouse_event(mouse_event);
                }
                _ => {}
            }
        }
        Ok(())
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
    async fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            KeyCode::Left | KeyCode::Char('r')  => {
                let mpv = self.mpv_state.lock().unwrap();
                let _ = mpv.mpv.seek_backward(5.0);
            }
            KeyCode::Right | KeyCode::Char('s') => {
                let mpv = self.mpv_state.lock().unwrap();
                let _ = mpv.mpv.seek_forward(5.0);
            }
            KeyCode::Char('n') => {
                let client = self.client.as_ref().unwrap();
                let _ = client.stopped(
                    self.active_song_id.clone(),
                    // position ticks
                    (self.current_playback_state.duration * self.current_playback_state.percentage * 100000.0) as u64,
                ).await;
                let mpv = self.mpv_state.lock().unwrap();
                let _ = mpv.mpv.playlist_next_force();
            }
            KeyCode::Char('p') => {
                let mpv = self.mpv_state.lock().unwrap();
                let _ = mpv.mpv.playlist_previous_force();
            }
            KeyCode::Char(' ') => {
                // get the current state of mpv
                let mpv = self.mpv_state.lock().unwrap();
                self.paused = mpv.mpv.get_property("pause").unwrap_or(false);
                if self.paused {
                    let _ = mpv.mpv.unpause();
                } else {
                    let _ = mpv.mpv.pause();
                }
            }
            KeyCode::Tab => {
                self.toggle_section(true);
            }
            KeyCode::BackTab => {
                self.toggle_section(false);
            }
            KeyCode::Down | KeyCode::Char('j') => match self.active_section {
                ActiveSection::Artists => {
                    let selected = self
                        .selected_artist
                        .selected()
                        .unwrap_or(self.artists.len() - 1);
                    if selected == self.artists.len() - 1 {
                        self.selected_artist.select(Some(selected));
                        return;
                    }
                    self.selected_artist.select(Some(selected + 1));
                }
                ActiveSection::Tracks => {
                    let selected = self
                        .selected_track
                        .selected()
                        .unwrap_or(self.tracks.len() - 1);
                    if selected == self.tracks.len() - 1 {
                        self.selected_track.select(Some(selected));
                        return;
                    }
                    self.selected_track.select(Some(selected + 1));
                }
                ActiveSection::Queue => {
                    *self.selected_queue_item.offset_mut() += 1;
                }
            },
            KeyCode::Up | KeyCode::Char('k') => match self.active_section {
                ActiveSection::Artists => {
                    let selected = self.selected_artist.selected().unwrap_or(0);
                    if selected == 0 {
                        self.selected_artist.select(Some(selected));
                        return;
                    }
                    self.selected_artist.select(Some(selected - 1));
                }
                ActiveSection::Tracks => {
                    let selected = self.selected_track.selected().unwrap_or(0);
                    if selected == 0 {
                        self.selected_track.select(Some(selected));
                        return;
                    }
                    self.selected_track.select(Some(selected - 1));
                }
                ActiveSection::Queue => {
                    let lvalue = self.selected_queue_item.offset_mut();
                    if *lvalue == 0 {
                        return;
                    }
                    *lvalue -= 1;
                }
            },
            KeyCode::Char('g') => match self.active_section {
                ActiveSection::Artists => {
                    self.selected_artist.select(Some(0));
                }
                ActiveSection::Tracks => {
                    self.selected_track.select(Some(0));
                }
                ActiveSection::Queue => {
                    self.selected_queue_item.select(Some(0));
                }
            },
            KeyCode::Char('G') => match self.active_section {
                ActiveSection::Artists => {
                    self.selected_artist.select(Some(self.artists.len() - 1));
                }
                ActiveSection::Tracks => {
                    self.selected_track.select(Some(self.tracks.len() - 1));
                }
                ActiveSection::Queue => {
                    self.selected_queue_item.select(Some(self.playlist.len() - 1));
                }
            },
            KeyCode::Enter => {
                match self.active_section {
                    ActiveSection::Artists => {
                        let selected = self.selected_artist.selected().unwrap_or(0);
                        self.discography(&self.artists[selected].id.clone()).await;
                        self.selected_track.select(Some(0));
                    }
                    ActiveSection::Tracks => {
                        let selected = self.selected_track.selected().unwrap_or(0);
                        match self.client {
                            Some(ref client) => {
                                let lock = self.mpv_state.clone();
                                let mut mpv = lock.lock().unwrap();
                                mpv.should_stop = true;

                                // the playlist MPV will be getting
                                self.playlist = self
                                    .tracks
                                    .iter()
                                    .skip(selected)
                                    .map(|track| {
                                        Song {
                                            id: track.id.clone(),
                                            url: client.song_url_sync(track.id.clone()),
                                            name: track.name.clone(),
                                            artist: track.album_artist.clone(),
                                            album: track.album.clone(),
                                            parent_id: track.parent_id.clone(),
                                            production_year: track.production_year,
                                        }
                                    })
                                    .collect();
                                self.replace_playlist();
                            }
                            None => {
                                println!("No client");
                            }
                        }
                    }
                    ActiveSection::Queue => {
                        let _ = self.selected_queue_item.selected().unwrap_or(0);
                        // println!("Selected queue item: {:?}", selected);
                    }
                }
            }
            _ => {}
        }
    }

    /// Fetch the discography of an artist
    /// This will change the active section to tracks
    async fn discography(&mut self, id: &str) {
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

    fn replace_playlist(&mut self) {
        let _ = {
            self.mpv_state = Arc::new(Mutex::new(MpvState::new())); // Shared state for controlling MPV
            let mpv_state = self.mpv_state.clone();
            let sender = self.sender.clone();
            let songs = self.playlist.clone();
            // println!("Playing playlist: {:?}", songs);

            self.mpv_thread = Some(thread::spawn(move || {
                Self::t_playlist(songs, mpv_state, sender)
            }));
        };
    }
    /// Thread function for mpris control
    async fn t_mpris(
        _mpris_state: Arc<Mutex<MprisExchange>>,
        _mpv_state: Arc<Mutex<MpvState>>,
    ) {
        let player = Player::builder("com.tui.jellyfin")
            .can_play(true)
            .can_pause(true)
            .build()
            .await;

        match player {
            Ok(player) => {
                println!("MPRIS server started");
                player.connect_play_pause(move |_player| {
                    // get the lock and set the play_pause state
                    let mut state = _mpris_state.lock().unwrap();
                    state.play_pause = true;
                    drop(state);
                });

                let _ = player.set_metadata(
                    Metadata::builder()
                        .title("Title")
                        .artist(["Artist"])
                        .album("Album")
                        .build(),
                ).await;

                // replace with a loop
                // player.run().await;
                // run as a task

                // arc it
                // let player = player;
                let player_arc = Arc::new(player);
                let player_arc_clone = player_arc.clone();

                // tokio::task::spawn_local(async move {
                //     player.run();
                // });

                let local_set = tokio::task::LocalSet::new();

                // player_arc.clone().run().await;
                let l = local_set.spawn_local(async move {
                    player_arc.run().await;
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    println!("MPRIS thread finished");
                    std::future::pending::<()>().await;
                });

                // player.run().await;

                // start l
                // local_set.run_until(l).await;
                // start but don't block
                // local_set.spawn_local(l);

                // set metadata to something else to test if this works
                
                // let _ = player_arc_clone.set_metadata(
                //     Metadata::builder()
                //         .title("Title")
                //         .artist(["Artist"])
                //         .album("Album")
                //         .build(),
                // ).await;

                player_arc_clone.connect_play_pause(|_player| {
                    println!("PlayPause");
                });

                // sleep forever
                loop {
                    thread::sleep(Duration::from_secs(1));
                    println!("MPRIS thread running");
                    // check l value
                    // if l.is_finished() {
                    //     println!("MPRIS thread finished");
                    // }
                //     let _ = player.set_metadata(
                //     Metadata::builder()
                //         .title("Title2")
                //         .artist(["Artist"])
                //         .album("Album")
                //         .build(),
                // ).await;
                }
            }
            Err(e) => {
                println!("Failed to start MPRIS server: {:?}", e);
            }
        }
        println!("MPRIS thread ended");
    }

    fn t_playlist(
        songs: Vec<Song>,
        mpv_state: Arc<Mutex<MpvState>>,
        sender: Sender<MpvPlaybackState>,
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

                // println!("Playlist pos: {:?}", pos);
                drop(mpv);
                sender
                    .send({
                        MpvPlaybackState {
                            percentage,
                            duration,
                            current_index,
                        }
                    })
                    .unwrap();

                thread::sleep(Duration::from_secs_f32(0.2));
            }
        }
    }

    fn handle_mouse_event(&mut self, _mouse_event: crossterm::event::MouseEvent) {
        // println!("Mouse event: {:?}", _mouse_event);
    }
    fn exit(&mut self) {
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
