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
}

impl App {
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
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true)
            .direction(ListDirection::BottomToTop);

        frame.render_widget(list, left[0]);

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

        frame.render_widget(list, right[0]);

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
            _ => {}
        }
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
