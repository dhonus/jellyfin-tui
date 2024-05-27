use crate::client::{self, Artist, Client, DiscographySong};
use libmpv::{events::*, *};

use std::io::{self, stdout, Stdout};

use crossterm::{execute, terminal::*};
use ratatui::{prelude::*, widgets::*};
use ratatui::widgets::block::Title;
use ratatui::widgets::Borders;
use ratatui::widgets::{block::Position, Block, Paragraph};
use ratatui::symbols::border;

use std::time::Duration;

/// A type alias for the terminal type used in this application
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, Receiver};

use std::thread;

use crossterm::event::{self, Event, KeyEvent};
use crossterm::{
    event::{KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

/// Initialize the terminal
pub fn init() -> io::Result<Tui> {
    execute!(stdout(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    Terminal::new(CrosstermBackend::new(stdout()))
}

/// Restore the terminal to its original state
pub fn restore() -> io::Result<()> {
    execute!(stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

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

pub struct App {
    pub percentage: f64,
    pub exit: bool,
    pub artists: Vec<Artist>,
    pub tracks: Vec<DiscographySong>,
    pub active_section: ActiveSection,
    pub selected_artist: ListState,
    pub selected_track: ListState,
    pub selected_queue_item: ListState,
    pub client: Option<Client>,
    mpv_thread: Option<thread::JoinHandle<()>>,
    mpv_state: Arc<Mutex<MpvState>>,
    sender: Sender<f64>,
    receiver: Receiver<f64>,
}

impl Default for App {
    fn default() -> Self {
        let (sender, receiver) = channel();

        App {
            percentage: 0.0,
            exit: false,
            artists: vec![],
            tracks: vec![],
            active_section: ActiveSection::Artists,
            selected_artist: ListState::default(),
            selected_track: ListState::default(),
            selected_queue_item: ListState::default(),
            client: None,
            mpv_thread: None,
            mpv_state: Arc::new(Mutex::new(MpvState::new())),
            sender,
            receiver,
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

        let mut ev_ctx = mpv.create_event_context();
            ev_ctx.disable_deprecated_events().unwrap();
            ev_ctx.observe_property("volume", Format::Int64, 0).unwrap();
            ev_ctx
                .observe_property("demuxer-cache-state", Format::Node, 0)
                .unwrap();
        MpvState { mpv, should_stop: false }
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
        // get playback state
        match self.receiver.try_recv() {
            Ok(percentage) => {
                self.percentage = percentage;
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
    }

    fn toggle_section(&mut self, forwards: bool) {
        match forwards {
            true => match self.active_section {
                ActiveSection::Artists => self.active_section = ActiveSection::Tracks,
                ActiveSection::Tracks => self.active_section = ActiveSection::Queue,
                ActiveSection::Queue => self.active_section = ActiveSection::Artists,
            },
            false => match self.active_section {
                ActiveSection::Artists => self.active_section = ActiveSection::Queue,
                ActiveSection::Tracks => self.active_section = ActiveSection::Artists,
                ActiveSection::Queue => self.active_section = ActiveSection::Tracks,
            },
        }
    }

    pub fn render_frame(&mut self, frame: &mut Frame) {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(22),
                Constraint::Percentage(56),
                Constraint::Percentage(22),
            ])
            .split(frame.size());

        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(outer_layout[0]);

        let center = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(90), Constraint::Percentage(10)])
            .split(outer_layout[1]);

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(outer_layout[2]);

        let artist_block = match self.active_section {
            ActiveSection::Artists => Block::new().borders(Borders::ALL).border_style(style::Color::Blue),
            _ => Block::new().borders(Borders::ALL).border_style(style::Color::White),
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

        frame.render_widget(
            Paragraph::new("Cover art").block(Block::new().borders(Borders::ALL)),
            left[1],
        );

        let track_block = match self.active_section {
            ActiveSection::Tracks => Block::new().borders(Borders::ALL).border_style(style::Color::Blue),
            _ => Block::new().borders(Borders::ALL).border_style(style::Color::White),
        };

        let items = self
            .tracks
            .iter()
            .map(|track| format!("{} - {}", track.album.as_str(),track.name.as_str()))
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

        frame.render_widget(
            Paragraph::new("Controls2")
                .set_style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .block(
                    Block::new()
                        .borders(Borders::ALL)
                        .border_style(style::Color::White),
                ),
            center[1],
        );

        // render controls
        frame.render_widget(
            &Controls {},
            Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Percentage(100)])
                .split(center[0])[0],
        );

        frame.render_widget(
            LineGauge::default()
                .block(Block::bordered().title("Progress"))
                .gauge_style(
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
                .line_set(symbols::line::THICK)
                .ratio(self.percentage / 100 as f64),
            center[1],
        );

        let queue_block = match self.active_section {
            ActiveSection::Queue => Block::new().borders(Borders::ALL).border_style(style::Color::Blue),
            _ => Block::new().borders(Borders::ALL).border_style(style::Color::White),
        };

        let items = ["Item 1", "Item 2", "Item 3"];
        let list = List::new(items)
            .block(queue_block.title("Lyrics / Queue"))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true)
            .direction(ListDirection::BottomToTop);

        frame.render_stateful_widget(list, right[0], &mut self.selected_queue_item);

        frame.render_widget(
            Paragraph::new("Metadata").block(Block::new().borders(Borders::ALL)),
            right[1],
        );

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
            KeyCode::Left => {
                let mpv = self.mpv_state.lock().unwrap();
                let _ = mpv.mpv.seek_backward(5.0);
            }
            KeyCode::Right => {
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
            KeyCode::Down | KeyCode::Char('j') => {
                match self.active_section {
                    ActiveSection::Artists => {
                        let selected = self.selected_artist.selected().unwrap_or(self.artists.len() - 1);
                        if selected == self.artists.len() - 1 {
                            self.selected_artist.select(Some(selected));
                            return;
                        }
                        self.selected_artist.select(Some(selected + 1));
                    }
                    ActiveSection::Tracks => {
                        let selected = self.selected_track.selected().unwrap_or(self.tracks.len() - 1);
                        if selected == self.tracks.len() - 1 {
                            self.selected_track.select(Some(selected));
                            return;
                        }
                        self.selected_track.select(Some(selected + 1));
                        
                    }
                    ActiveSection::Queue => {
                        *self.selected_queue_item.offset_mut() += 1;
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                match self.active_section {
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
                }
            }
            KeyCode::Char('g') => {
                match self.active_section {
                    ActiveSection::Artists => {
                        self.selected_artist.select(Some(0));
                    }
                    ActiveSection::Tracks => {
                        self.selected_track.select(Some(0));
                    }
                    ActiveSection::Queue => {
                        self.selected_queue_item.select(Some(0));
                    }
                }
            }
            KeyCode::Char('G') => {
                match self.active_section {
                    ActiveSection::Artists => {
                        self.selected_artist.select(Some(self.artists.len() - 1));
                    }
                    ActiveSection::Tracks => {
                        self.selected_track.select(Some(0));
                    }
                    ActiveSection::Queue => {
                        self.selected_queue_item.select(Some(0));
                    }
                }
            }
            KeyCode::Enter => {
                match self.active_section {
                    ActiveSection::Artists => {
                        let selected = self.selected_artist.selected().unwrap_or(0);
                        // println!("Selected artist: {:?}", self.artists[selected]);
                        self.discography(&self.artists[selected].id.clone()).await;
                        self.selected_track.select(Some(0));
                    }
                    ActiveSection::Tracks => {
                        let selected = self.selected_track.selected().unwrap_or(0);
                        // println!("Selected track: {:?}", selected);
                        match self.client {
                            Some(ref client) => {
                                let song = &self.tracks[selected];
                                let url = client.song_url(song.id.clone()).await;
                                match url {
                                    Ok(url) => {
                                        // stop mpv
                                        let lock = self.mpv_state.clone();
                                        let mut mpv = lock.lock().unwrap();
                                        mpv.should_stop = true;
                                        self.play_song(&url);
                                    }
                                    Err(e) => {
                                        // println!("Failed to get song url: {:?}", e);
                                    }
                                }
                            }
                            None => {
                                println!("No client");
                            }
                        }
                    }
                    ActiveSection::Queue => {
                        let selected = self.selected_queue_item.selected().unwrap_or(0);
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

    fn play_song(&mut self, song: &str) {
        let _ = {
            self.mpv_state = Arc::new(Mutex::new(MpvState::new())); // Shared state for controlling MPV
            let mpv_state = self.mpv_state.clone();
            let sender = self.sender.clone();
            let song = song.to_string();

            self.mpv_thread = Some(thread::spawn(move || {
                Self::t_play(song, mpv_state, sender);
            }));
        };
    }

    fn t_play(song: String, mpv_state: Arc<Mutex<MpvState>>, sender: Sender<f64>) {
        {
            let path = String::from(song);
            let lock = mpv_state.clone();
            let mpv = lock.lock().unwrap();

            mpv.mpv.playlist_load_files(&[(&path, FileState::AppendPlay, None)])
                .unwrap();

            drop (mpv);

            loop { // main mpv loop
                let lock = mpv_state.clone();
                let mpv = lock.lock().unwrap();
                if mpv.should_stop {
                    return;
                }
                let percentage = mpv.mpv.get_property("percent-pos").unwrap_or(0.0);
                drop(mpv);
                sender.send(percentage).unwrap();
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