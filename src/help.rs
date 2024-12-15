/* --------------------------
Help page rendering functions
    - Pressing '?' in any tab should show the help page in its place
    - should of an equivalent layout
-------------------------- */

use ratatui::{
    Frame,
    prelude::*,
    widgets::*,
};

impl crate::tui::App {
    pub fn render_home_help(&mut self, app_container: Rect, frame: &mut Frame) {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(20),
                Constraint::Percentage(56),
                Constraint::Percentage(24),
            ])
            .split(app_container);

        let left = outer_layout[0];

        let center = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(86), Constraint::Min(10)])
            .split(outer_layout[1]);

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(32), Constraint::Percentage(68)])
            .split(outer_layout[2]);

        let artist_block = Block::new()
            .borders(Borders::ALL)
            .border_style(style::Color::White);

        let artist_help_text = vec![
            Line::from("This is a list of all artists sorted alphabetically."),
            Line::from(""),
            Line::from("Usage:").underlined(),
            Line::from(vec![
                "  - Use ".white(),
                "<↑/↓>".fg(self.primary_color).bold(),
                " (j/k) to navigate".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "<Enter>".fg(self.primary_color).bold(),
                " to select".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "Tab".fg(self.primary_color).bold(),
                " to switch to Tracks".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "Shift + Tab".fg(self.primary_color).bold(),
                " to switch to Lyrics".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "a".fg(self.primary_color).bold(),
                " to skip to alphabetically next artist".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "A".fg(self.primary_color).bold(),
                " to skip to alphabetically previous artist".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "g".fg(self.primary_color).bold(),
                " to skip to the top of the list".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "G".fg(self.primary_color).bold(),
                " to skip to the bottom of the list".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "f".fg(self.primary_color).bold(),
                " to favorite an artist".white(),
            ]),
            Line::from(""),
            Line::from("Searching:").underlined(),
            Line::from(vec![
                "  - Use ".white(),
                "/".fg(self.primary_color).bold(),
                " to start searching".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "Esc".fg(self.primary_color).bold(),
                " to clear search".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "Enter".fg(self.primary_color).bold(),
                " to confirm search".white(),
            ]),
        ];

        let artist_help = Paragraph::new(artist_help_text)
            .block(artist_block.title("Artists"))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(artist_help, left);


        let track_block = Block::new()
            .borders(Borders::ALL)
            .border_style(style::Color::White);

        let track_help_text = vec![
            Line::from(""),
            Line::from("jellyfin-tui Library help").centered(),
            Line::from("Here is a table of all tracks."),
            Line::from(""),
            Line::from("Usage:").underlined(),
            Line::from(vec![
                "  - Use ".white(),
                "<↑/↓>".fg(self.primary_color).bold(),
                " (j/k) to navigate".white(),
            ]),
            // "  - Use Enter to play a song",
            Line::from(vec![
                "  - Use ".white(),
                "<Enter>".fg(self.primary_color).bold(),
                " to play a song".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "Tab".fg(self.primary_color).bold(),
                " to switch to Artists".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "Shift + Tab".fg(self.primary_color).bold(),
                " to switch to Lyrics".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "g".fg(self.primary_color).bold(),
                " to skip to the top of the list".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "G".fg(self.primary_color).bold(),
                " to skip to the bottom of the list".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "a".fg(self.primary_color).bold(),
                " to skip to alphabetically next artist".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "A".fg(self.primary_color).bold(),
                " to skip to alphabetically previous artist".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "f".fg(self.primary_color).bold(),
                " to favorite a song".white(),
            ]),
            Line::from(""),
            Line::from("Searching:").underlined(),
            Line::from(vec![
                "  - Use ".white(),
                "/".fg(self.primary_color).bold(),
                " to start searching".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "Esc".fg(self.primary_color).bold(),
                " to clear search".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "Enter".fg(self.primary_color).bold(),
                " to confirm search".white(),
            ]),
            Line::from(""),
            Line::from("Queue:").underlined(),
            Line::from("  - jellyfin-tui has a double queue system. A general queue and temporary queue"),
            Line::from(vec![
                "  - Playing a song with ".white(),
                "<Enter>".fg(self.primary_color).bold(),
                " will create a new general queue".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "e".fg(self.primary_color).bold(),
                ", or ".white(),
                "shift + Enter".fg(self.primary_color).bold(),
                " to enqueue a song (temporary queue)".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "ctrl + e".fg(self.primary_color).bold(),
                ", or ".white(),
                "ctrl + Enter".fg(self.primary_color).bold(),
                " play next in the queue (temporary queue)".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "E".fg(self.primary_color).bold(),
                " to clear the temporary queue".white(),
            ]),
            Line::from(""),
            Line::from("General").underlined(),
            Line::from(vec![
                "  - Use ".white(),
                "?".fg(self.primary_color).bold(),
                " to show this help".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "F1..FX".fg(self.primary_color).bold(),
                " to switch tabs".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "q".fg(self.primary_color).bold(),
                " or ".white(),
                "ctrl + c".fg(self.primary_color).bold(),
                " to quit".white(),
            ]),
        ];

        let track_help = Paragraph::new(track_help_text )
            .block(track_block.title("Tracks"))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(track_help, center[0]);

        let queue_block = Block::new()
            .borders(Borders::ALL)
            .border_style(style::Color::White);

        let queue_help_text = vec![
            Line::from("This is the queue."),
            Line::from(""),
            Line::from("Usage:").underlined(),
            Line::from(vec![
                "  - Use ".white(),
                "<↑/↓>".fg(self.primary_color).bold(),
                " (j/k) to navigate".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "Shift + <↑/↓>".fg(self.primary_color).bold(),
                " (J/K) to change order".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "<Enter>".fg(self.primary_color).bold(),
                " to play a song".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "d".fg(self.primary_color).bold(),
                " to remove a song from the queue".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "x".fg(self.primary_color).bold(),
                " to clear the queue and stop playback".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "f".fg(self.primary_color).bold(),
                " to favorite a song".white(),
            ]),
        ];

        let queue_help = Paragraph::new(queue_help_text)
            .block(queue_block.title("Queue"))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(queue_help, right[1]);

        let bottom = Block::default()
            .borders(Borders::ALL)
            .padding(Padding::new(0, 0, 0, 0));

        // let inner = bottom.inner(center[1]);

        frame.render_widget(bottom, center[1]);

        // lyrics area
        let lyrics_block = Block::new()
            .borders(Borders::ALL)
            .border_style(style::Color::White);

        let lyrics_help_text = vec![
            Line::from("This is the lyrics area."),
            Line::from(""),
            Line::from("Usage:").underlined(),
            Line::from(vec![
                "  - Use ".white(),
                "<↑/↓>".fg(self.primary_color).bold(),
                " (j/k) to navigate".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "<Enter>".fg(self.primary_color).bold(),
                " to jump to the current lyric".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "Tab".fg(self.primary_color).bold(),
                " to switch to previous Pane".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "Shift + Tab".fg(self.primary_color).bold(),
                " to switch to Queue".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "g".fg(self.primary_color).bold(),
                " to select the first lyric".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "G".fg(self.primary_color).bold(),
                " to select the last lyric".white(),
            ]),
            Line::from(""),
        ];

        let lyrics_help = Paragraph::new(lyrics_help_text)
            .block(lyrics_block.title("Lyrics"))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(lyrics_help, right[0]);

        // player area
        let player_block = Block::new()
            .borders(Borders::ALL)
            .border_style(style::Color::White);

        let player_help_text = vec![
            Line::from("This is the player area."),
            Line::from(""),
            Line::from("Usage:").underlined(),
            Line::from(vec![
                "  - Use ".white(),
                "Space".fg(self.primary_color).bold(),
                " to play/pause".white(),
                "\t".into(),
                "  - Use ".white(),
                "r".fg(self.primary_color).bold(),
                " to toggle Replay None->All(*)->One(1)".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "←/→".fg(self.primary_color).bold(),
                " to seek".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "+/-".fg(self.primary_color).bold(),
                " to change volume".white(),
            ]),
            Line::from(vec![
                "  - Use ".white(),
                "m".fg(self.primary_color).bold(),
                " to mute/unmute".white(),
            ]),
        ];

        let player_help = Paragraph::new(player_help_text)
            .block(player_block.title("Player"))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(player_help, center[1]);
    }
}