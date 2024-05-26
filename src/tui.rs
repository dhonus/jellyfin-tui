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

#[derive(Debug, Default)]
pub struct App {
    pub percentage: f64,
    pub exit: bool,
    pub artists: Vec<Artist>,
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

        frame.render_widget(
            Paragraph::new("Artist / Album").block(Block::new().borders(Borders::ALL)),
            left[0],
        );

        // render all artists as a list here in left[0]
        let items = self
            .artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<&str>>();
        let list = List::new(items)
            .block(Block::bordered().title("Artists"))
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

        frame.render_widget(
            Paragraph::new("Track").block(Block::new().borders(Borders::ALL)),
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

        frame.render_widget(
            Paragraph::new("Queue").block(Block::new().borders(Borders::ALL)),
            right[0],
        );
        frame.render_widget(
            Paragraph::new("Metadata").block(Block::new().borders(Borders::ALL)),
            right[1],
        );

        let items = ["Item 1", "Item 2", "Item 3"];
        let list = List::new(items)
            .block(Block::bordered().title("List"))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true)
            .direction(ListDirection::BottomToTop);

        frame.render_widget(list, right[0]);
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
            " Seek+5 ".into(),
            "<S>".blue().bold(),
            " Seek-5 ".into(),
            "<R>".blue().bold(),
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
