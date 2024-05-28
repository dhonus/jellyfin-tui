use crate::client::{self, Artist, Client, DiscographySong};
use layout::Flex;
use libmpv::{*};

use std::io::{self, Stdout};

use ratatui::symbols::border;
use ratatui::widgets::block::Title;
use ratatui::widgets::Borders;
use ratatui::widgets::{block::Position, Block, Paragraph};
use ratatui::{prelude::*, widgets::*};

use std::time::Duration;

/// A type alias for the terminal type used in this application
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use std::thread;

use crossterm::event::{self, Event, KeyEvent};
use crossterm::event::KeyCode;

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
    pub current_index: i64,
}

pub struct App {
    pub exit: bool,

    artists: Vec<Artist>, // all artists
    tracks: Vec<DiscographySong>, // current artist's tracks
    lyrics: (String, Vec<String>),
    playlist: Vec<(String, String, String, String)>, // (URL, Title, Artist, Album)
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
    
    // every second, we get the playback state from the mpv thread
    sender: Sender<MpvPlaybackState>, 
    receiver: Receiver<MpvPlaybackState>,
    current_playback_state: MpvPlaybackState,   
}

impl Default for App {
    fn default() -> Self {
        let (sender, receiver) = channel();

        App {
            exit: false,
            artists: vec![],
            tracks: vec![],
            lyrics: (String::from(""), vec![]),
            playlist: vec![],
            active_section: ActiveSection::Artists,
            last_section: ActiveSection::Artists,
            selected_artist: ListState::default(),
            selected_track: ListState::default(),
            selected_queue_item: ListState::default(),
            client: None,
            mpv_thread: None,
            mpv_state: Arc::new(Mutex::new(MpvState::new())),
            sender,
            receiver,
            current_playback_state: MpvPlaybackState {
                percentage: 0.0,
                current_index: 0,
            },
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
        mpv.set_property("volume", 50).unwrap();
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
        let client = client::Client::new("https://jelly.danielhonus.com").await;
        if client.access_token.is_empty() {
            println!("Failed to authenticate. Exiting...");
            return;
        }
        self.client = Some(client);
        self.artists = artists;
        self.active_section = ActiveSection::Artists;
        self.selected_artist.select(Some(0));
    }

    pub async fn run(&mut self, terminal: &mut Tui) {
        // get playback state from the mpv thread
        match self.receiver.try_recv() {
            Ok(state) => {
                self.current_playback_state.percentage = state.percentage;
                self.current_playback_state.current_index = state.current_index;

                // Queue position
                self.selected_queue_item
                    .select(Some(state.current_index as usize));

                // if id is different, fetch lyrics
                match self.tracks.get(state.current_index as usize) {
                    Some(track) => {
                        if track.id != self.lyrics.0 {
                            match self.client {
                                Some(ref client) => {
                                    let lyrics = client.lyrics(track.id.clone()).await;
                                    match lyrics {
                                        Ok(lyrics) => {
                                            self.lyrics = (track.id.clone(), lyrics);
                                        }
                                        Err(e) => {
                                            println!("Failed to get lyrics: {:?}", e);
                                        }
                                    }
                                }
                                None => {}
                            }
                        }
                    }
                    None => {}
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

        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(outer_layout[0]);

        let center = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(85), Constraint::Min(7)])
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

        // render all artists as a list here in left[0]
        let items = self
            .artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<&str>>();

        let list = List::new(items)
            .block(artist_block.title("Artist / Album"))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            )
            .repeat_highlight_symbol(true);

        frame.render_stateful_widget(list, left[0], &mut self.selected_artist);

        let p = (
            Paragraph::new("Cover art").block(Block::new().borders(Borders::ALL)),
            left[1],
        );

        frame.render_widget(p.0, p.1);

        let track_block = match self.active_section {
            ActiveSection::Tracks => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::Blue),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::White),
        };

        let items = self
            .tracks
            .iter()
            .map(|track| format!("{} - {}", track.album.as_str(), track.name.as_str()))
            .collect::<Vec<String>>();
        let list = List::new(items)
            .block(track_block.title("Track"))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
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
            Some(song) => format!("{} - {} - {}", song.2, song.1, song.3),
            None => String::from("No song playing"),
        };

        let bottom = Block::default()
            .borders(Borders::ALL)
            .padding(Padding::vertical(1));
        let inner = bottom.inner(center[1]);
        frame.render_widget(bottom, center[1]);

        let layout = Layout::vertical(vec![
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(inner);

        // current song
        frame.render_widget(
            Paragraph::new(current_song).block(
                Block::bordered()
                    .borders(Borders::NONE)
                    .padding(Padding::horizontal(2)),
            ),
            layout[0],
        );

        let progress_bar_area = Layout::default()
            .direction(Direction::Horizontal)
            .flex(Flex::Center)
            .constraints(vec![
                Constraint::Percentage(8),
                Constraint::Percentage(84),
                Constraint::Percentage(8),
            ])
            .split(layout[1]);

        frame.render_widget(
            LineGauge::default()
                .block(Block::bordered().borders(Borders::NONE))
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
                        .block(Block::new().title("Lyrics").borders(Borders::ALL)),
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
            .map(|song| song.1.as_str())
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
            KeyCode::Char(' ') => {
                // get the current state of mpv
                let mpv = self.mpv_state.lock().unwrap();
                let paused = mpv.mpv.get_property("pause").unwrap_or(false);
                if paused {
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
                    self.selected_track.select(Some(0));
                }
                ActiveSection::Queue => {
                    self.selected_queue_item.select(Some(0));
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
                                        (
                                            client.song_url_sync(track.id.clone()),
                                            track.name.clone(),
                                            track.album_artist.clone(),
                                            track.album.clone(),
                                        )
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
                Self::t_playlist(songs, mpv_state, sender);
            }));
        };
    }

    fn t_playlist(
        songs: Vec<(String, String, String, String)>,
        mpv_state: Arc<Mutex<MpvState>>,
        sender: Sender<MpvPlaybackState>,
    ) {
        {
            let lock = mpv_state.clone();
            let mpv = lock.lock().unwrap();

            mpv.mpv.playlist_clear().unwrap();

            mpv.mpv
                .playlist_load_files(
                    &songs
                        .iter()
                        .map(|song| (song.0.as_str(), FileState::AppendPlay, None))
                        .collect::<Vec<(&str, FileState, Option<&str>)>>()
                        .as_slice(),
                )
                .unwrap();

            drop(mpv);

            loop {
                // main mpv loop
                let lock = mpv_state.clone();
                let mpv = lock.lock().unwrap();
                if mpv.should_stop {
                    return;
                }
                let percentage = mpv.mpv.get_property("percent-pos").unwrap_or(0.0);
                let current_index: i64 = mpv.mpv.get_property("playlist-pos").unwrap_or(0);

                // println!("Playlist pos: {:?}", pos);
                drop(mpv);
                sender
                    .send({
                        MpvPlaybackState {
                            percentage,
                            current_index,
                        }
                    })
                    .unwrap();
                thread::sleep(Duration::from_secs_f32(0.1));
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
