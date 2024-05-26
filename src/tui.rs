use std::io::{self, stdout, Stdout};

use crossterm::{execute, terminal::*};
use ratatui::{prelude::*, widgets::*};

use crossterm::event::{self, Event, KeyEvent};
use libmpv::{events::*, *};
use ratatui::symbols::border;
use ratatui::widgets::block::Title;
use ratatui::widgets::Borders;
use ratatui::widgets::{block::Position, Block, Paragraph};

use std::time::Duration;
/// A type alias for the terminal type used in this application
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

use crate::client::Artist;

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

#[derive(Debug, Default)]
pub struct App {
    pub percentage: f64,
    pub exit: bool,
    pub artists: Vec<Artist>,
    pub active_section: ActiveSection,
    pub selected_artist: ListState,
    pub selected_track: ListState,
    pub selected_queue_item: ListState,
}

impl App {

    pub fn init(&mut self, artists: Vec<Artist>) {
        self.artists = artists;
        self.active_section = ActiveSection::Artists;
        self.selected_artist.select(Some(0));
    }

    pub fn run(&mut self, terminal: &mut Tui, mut mpv: &Mpv) {
        terminal
            .draw(|frame| {
                self.render_frame(frame);
            })
            .unwrap();
        self.handle_events(&mut mpv).unwrap();
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
                Constraint::Percentage(18),
                Constraint::Percentage(55),
                Constraint::Percentage(27),
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

        frame.render_widget(
            Paragraph::new("Track").block(track_block),
            center[0],
        );

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
            .block(queue_block.title("List"))
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

    fn handle_events(&mut self, mut mpv: &Mpv) -> io::Result<()> {
        while event::poll(Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(key_event) => {
                    self.handle_key_event(key_event, &mut mpv);
                }
                Event::Mouse(mouse_event) => {
                    self.handle_mouse_event(mouse_event);
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn handle_key_event(&mut self, key_event: KeyEvent, mut mpv: &Mpv) {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            KeyCode::Left => {
                let _ = mpv.seek_backward(5.0);
            }
            KeyCode::Right => {
                let _ = mpv.seek_forward(5.0);
            }
            KeyCode::Char(' ') => {
                let paused = mpv.get_property("pause").unwrap_or(true);
                if paused {
                    let _ = mpv.unpause();
                } else {
                    let _ = mpv.pause();
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
                        let lvalue = self.selected_track.offset_mut();
                        if *lvalue == 0 {
                            return;
                        }
                        *lvalue -= 1;
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
                        println!("Selected artist: {:?}", self.artists[selected].name);
                    }
                    ActiveSection::Tracks => {
                        let selected = self.selected_track.selected().unwrap_or(0);
                        println!("Selected track: {:?}", selected);
                    }
                    ActiveSection::Queue => {
                        let selected = self.selected_queue_item.selected().unwrap_or(0);
                        println!("Selected queue item: {:?}", selected);
                    }
                }
            }
            _ => {}
        }
    }
    fn handle_mouse_event(&mut self, _mouse_event: crossterm::event::MouseEvent) {
        println!("Mouse event: {:?}", _mouse_event);
    }
    fn exit(&mut self) {
        self.exit = true;
    }
}

struct Controls {}
impl Widget for &Controls {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let instructions = Title::from(Line::from(vec![
            " Play/Pause ".into(),
            "<Space>".blue().bold(),
            " Seek+5s ".into(),
            "<S>".blue().bold(),
            " Seek-5s ".into(),
            "<R>".blue().bold(),
            " Next Section ".into(),
            "<Tab>".blue().bold(),
            " Quit ".into(),
            "<Q> ".blue().bold(),
        ]));
        let block = Block::default()
            .title(
                instructions
                    .alignment(Alignment::Center)
                    .position(Position::Bottom),
            )
            .borders(Borders::ALL)
            .border_set(border::THICK);

        Paragraph::new("hi")
            .alignment(Alignment::Center)
            .block(block)
            .render(area, buf);
    }
}

// impl Widget for &App {
//     fn render(self, area: Rect, buf: &mut Buffer) {}
// }
