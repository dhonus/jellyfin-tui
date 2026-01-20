/* --------------------------
Help page rendering functions
    - Pressing '?' in any tab should show the help page in its place
    - should of an equivalent layout
-------------------------- */
use ratatui::{prelude::*, widgets::*, Frame};

impl crate::tui::App {
    pub fn render_home_help(&mut self, app_container: Rect, frame: &mut Frame) {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(self.preferences.constraint_width_percentages_music.0),
                Constraint::Percentage(self.preferences.constraint_width_percentages_music.1),
                Constraint::Percentage(self.preferences.constraint_width_percentages_music.2),
            ])
            .split(app_container);

        let left = outer_layout[0];

        let center = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(100), Constraint::Length(13)])
            .split(outer_layout[1]);

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(32), Constraint::Percentage(68)])
            .split(outer_layout[2]);

        let artist_block = Block::new()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(self.theme.resolve(&self.theme.border));

        let artist_help_text = vec![
            Line::from("This is a list of all artists sorted alphabetically.")
                .fg(self.theme.resolve(&self.theme.foreground)),
            Line::from(""),
            Line::from("Usage:").fg(self.theme.resolve(&self.theme.foreground)).underlined(),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)).bold(),
                "<↑/↓>".fg(self.theme.primary_color).bold(),
                " (j/k) to navigate".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "<Enter>".fg(self.theme.primary_color).bold(),
                " to select".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Tab".fg(self.theme.primary_color).bold(),
                " to switch to Tracks".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Shift + Tab".fg(self.theme.primary_color).bold(),
                " to switch to Lyrics".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "a".fg(self.theme.primary_color).bold(),
                " to skip to next album".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "A".fg(self.theme.primary_color).bold(),
                " to skip to previous album".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "g".fg(self.theme.primary_color).bold(),
                " to skip to the top of the list".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "G".fg(self.theme.primary_color).bold(),
                " to skip to the bottom of the list".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "f".fg(self.theme.primary_color).bold(),
                " to favorite an artist".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(""),
            Line::from("Searching:").fg(self.theme.resolve(&self.theme.foreground)).underlined(),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "/".fg(self.theme.primary_color).bold(),
                " to start searching".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Esc".fg(self.theme.primary_color).bold(),
                " to clear search".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Enter".fg(self.theme.primary_color).bold(),
                " to confirm search".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
        ];

        let artist_help = Paragraph::new(artist_help_text)
            .block(artist_block.title("Artists").fg(self.theme.resolve(&self.theme.section_title)))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(artist_help, left);

        let track_block = Block::new()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(self.theme.resolve(&self.theme.border));

        let track_help_text = vec![
            Line::from(""),
            Line::from("jellyfin-tui Library help")
                .centered()
                .fg(self.theme.resolve(&self.theme.foreground)),
            Line::from("Here is a table of all tracks.")
                .fg(self.theme.resolve(&self.theme.foreground)),
            Line::from(""),
            Line::from("Usage:").fg(self.theme.resolve(&self.theme.foreground)).underlined(),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "<↑/↓>".fg(self.theme.primary_color).bold(),
                " (j/k) to navigate".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            // "  - Use Enter to play a song",
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "<Enter>".fg(self.theme.primary_color).bold(),
                " to play a song".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Tab".fg(self.theme.primary_color).bold(),
                " to switch to Artists".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Shift + Tab".fg(self.theme.primary_color).bold(),
                " to switch to Lyrics".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "g".fg(self.theme.primary_color).bold(),
                " to skip to the top of the list".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "G".fg(self.theme.primary_color).bold(),
                " to skip to the bottom of the list".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "a".fg(self.theme.primary_color).bold(),
                " to jump to next album".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "A".fg(self.theme.primary_color).bold(),
                " to jump to previous album, or start of current"
                    .fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "f".fg(self.theme.primary_color).bold(),
                " to favorite a song".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "d".fg(self.theme.primary_color).bold(),
                " to download a song or album, press again to delete download"
                    .fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(""),
            Line::from("Searching:").fg(self.theme.resolve(&self.theme.foreground)).underlined(),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "/".fg(self.theme.primary_color).bold(),
                " to start searching".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Esc".fg(self.theme.primary_color).bold(),
                " to clear search".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Enter".fg(self.theme.primary_color).bold(),
                " to confirm search".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(""),
            Line::from("General").underlined().fg(self.theme.resolve(&self.theme.foreground)),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "?".fg(self.theme.primary_color).bold(),
                " to show this help".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "F1..FX".fg(self.theme.primary_color).bold(),
                " or ".fg(self.theme.resolve(&self.theme.foreground)),
                "1..9".fg(self.theme.primary_color).bold(),
                " to switch tabs".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "q".fg(self.theme.primary_color).bold(),
                " or ".fg(self.theme.resolve(&self.theme.foreground)),
                "ctrl + c".fg(self.theme.primary_color).bold(),
                " to quit".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
        ];

        let track_help = Paragraph::new(track_help_text)
            .block(track_block.title("Tracks").fg(self.theme.resolve(&self.theme.section_title)))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(track_help, center[0]);

        let queue_block = Block::new()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(self.theme.resolve(&self.theme.border));

        let queue_help_text = vec![
            Line::from("This is the queue.").fg(self.theme.resolve(&self.theme.foreground)),
            Line::from(""),
            Line::from("Usage:").fg(self.theme.resolve(&self.theme.foreground)).underlined(),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "<↑/↓>".fg(self.theme.primary_color).bold(),
                " (j/k) to navigate".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Shift + <↑/↓>".fg(self.theme.primary_color).bold(),
                " (J/K) to change order".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "<Enter>".fg(self.theme.primary_color).bold(),
                " to play a song".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Delete".fg(self.theme.primary_color).bold(),
                " to remove a song from the queue".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "x".fg(self.theme.primary_color).bold(),
                " to clear the queue and stop playback"
                    .fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "X".fg(self.theme.primary_color).bold(),
                " to clear the queue and also unselect everything"
                    .fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "f".fg(self.theme.primary_color).bold(),
                " to favorite a song".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "g".fg(self.theme.primary_color).bold(),
                " to skip to the top of the list".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "G".fg(self.theme.primary_color).bold(),
                " to skip to the bottom of the list".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from("Creation:").fg(self.theme.resolve(&self.theme.foreground)).underlined(),
            Line::from(
                "  - jellyfin-tui has a double queue system. A main queue and temporary queue",
            )
            .fg(self.theme.resolve(&self.theme.foreground)),
            Line::from(""),
            Line::from(vec![
                "  - Playing a song with ".fg(self.theme.resolve(&self.theme.foreground)),
                "<Enter>".fg(self.theme.primary_color).bold(),
                " will create a new main queue".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "e".fg(self.theme.primary_color).bold(),
                ", or ".fg(self.theme.resolve(&self.theme.foreground)),
                "shift + Enter".fg(self.theme.primary_color).bold(),
                " to enqueue a song (temporary queue)"
                    .fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "ctrl + e".fg(self.theme.primary_color).bold(),
                ", or ".fg(self.theme.resolve(&self.theme.foreground)),
                "ctrl + Enter".fg(self.theme.primary_color).bold(),
                " play next in the queue (temporary queue)"
                    .fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "E".fg(self.theme.primary_color).bold(),
                " to clear the temporary queue".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
        ];

        let queue_help = Paragraph::new(queue_help_text)
            .block(queue_block.title("Queue").fg(self.theme.resolve(&self.theme.section_title)))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(queue_help, right[1]);

        let bottom = Block::default().borders(Borders::ALL).padding(Padding::new(0, 0, 0, 0));

        // let inner = bottom.inner(center[1]);

        frame.render_widget(bottom, center[1]);

        // lyrics area
        let lyrics_block = Block::new()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(self.theme.resolve(&self.theme.border));

        let lyrics_help_text = vec![
            Line::from("This is the lyrics area.").fg(self.theme.resolve(&self.theme.foreground)),
            Line::from(""),
            Line::from("Usage:").fg(self.theme.resolve(&self.theme.foreground)).underlined(),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "<↑/↓>".fg(self.theme.primary_color).bold(),
                " (j/k) to navigate".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "<Enter>".fg(self.theme.primary_color).bold(),
                " to jump to the current lyric".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Tab".fg(self.theme.primary_color).bold(),
                " to switch to previous Pane".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Shift + Tab".fg(self.theme.primary_color).bold(),
                " to switch to Queue".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "g".fg(self.theme.primary_color).bold(),
                " to select the first lyric".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "G".fg(self.theme.primary_color).bold(),
                " to select the last lyric".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(""),
        ];

        let lyrics_help = Paragraph::new(lyrics_help_text)
            .block(lyrics_block.title("Lyrics").fg(self.theme.resolve(&self.theme.section_title)))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(lyrics_help, right[0]);

        // player area
        let player_block = Block::new()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(self.theme.resolve(&self.theme.border));

        let player_help_text = vec![
            Line::from("This is the player area.").fg(self.theme.resolve(&self.theme.foreground)),
            Line::from(""),
            Line::from("Usage:").fg(self.theme.resolve(&self.theme.foreground)).underlined(),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Space".fg(self.theme.primary_color).bold(),
                " to play/pause".fg(self.theme.resolve(&self.theme.foreground)),
                "\t".into(),
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "r".fg(self.theme.primary_color).bold(),
                " to toggle Replay None->All(*)->One(1)"
                    .fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "←/→".fg(self.theme.primary_color).bold(),
                " to seek 5s bck/fwd".fg(self.theme.resolve(&self.theme.foreground)),
                "\t".into(),
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "p".fg(self.theme.primary_color).bold(),
                " to open the command menu".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                ",/.".fg(self.theme.primary_color).bold(),
                " to seek 1m bck/fwd".fg(self.theme.resolve(&self.theme.foreground)),
                "\t".into(),
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "P".fg(self.theme.primary_color).bold(),
                " to open the GLOBAL command menu".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "+/-".fg(self.theme.primary_color).bold(),
                " to change volume".fg(self.theme.resolve(&self.theme.foreground)),
                "\t".into(),
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "R".fg(self.theme.primary_color).bold(),
                " to toggle repeat".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "s".fg(self.theme.primary_color).bold(),
                " to toggle shuffle".fg(self.theme.resolve(&self.theme.foreground)),
                "\t".into(),
                " - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Ctrl+(Left/h)".fg(self.theme.primary_color).bold(),
                " shrink current section".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Ctrl+s".fg(self.theme.primary_color).bold(),
                " to shuffle globally".fg(self.theme.resolve(&self.theme.foreground)),
                "\t".into(),
                " - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Ctrl+(Right/l)".fg(self.theme.primary_color).bold(),
                " expand current section".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "T".fg(self.theme.primary_color).bold(),
                " to toggle transcoding".fg(self.theme.resolve(&self.theme.foreground)),
                "\t".into(),
            ]),
        ];

        let player_help = Paragraph::new(player_help_text)
            .block(player_block.title("Player").fg(self.theme.resolve(&self.theme.section_title)))
            .fg(self.theme.resolve(&self.theme.foreground))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(player_help, center[1]);
    }

    pub fn render_playlists_help(&mut self, app_container: Rect, frame: &mut Frame) {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(self.preferences.constraint_width_percentages_music.0),
                Constraint::Percentage(self.preferences.constraint_width_percentages_music.1),
                Constraint::Percentage(self.preferences.constraint_width_percentages_music.2),
            ])
            .split(app_container);

        let left = outer_layout[0];

        let center = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(100), Constraint::Length(10)])
            .split(outer_layout[1]);

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(32), Constraint::Percentage(68)])
            .split(outer_layout[2]);

        let artist_block = Block::new()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(self.theme.resolve(&self.theme.border));

        let artist_help_text = vec![
            Line::from("This is a list of all playlists sorted alphabetically.")
                .fg(self.theme.resolve(&self.theme.foreground)),
            Line::from(""),
            Line::from("Usage:").fg(self.theme.resolve(&self.theme.foreground)).underlined(),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "<↑/↓>".fg(self.theme.primary_color).bold(),
                " (j/k) to navigate".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "<Enter>".fg(self.theme.primary_color).bold(),
                " to select".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Tab".fg(self.theme.primary_color).bold(),
                " to switch to Tracks".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Shift + Tab".fg(self.theme.primary_color).bold(),
                " to switch to Lyrics".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "a".fg(self.theme.primary_color).bold(),
                " to skip to alphabetically next playlist"
                    .fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "A".fg(self.theme.primary_color).bold(),
                " to skip to alphabetically previous playlist"
                    .fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "g".fg(self.theme.primary_color).bold(),
                " to skip to the top of the list".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "G".fg(self.theme.primary_color).bold(),
                " to skip to the bottom of the list".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "f".fg(self.theme.primary_color).bold(),
                " to favorite a playlist".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(""),
            Line::from("Searching:").underlined(),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "/".fg(self.theme.primary_color).bold(),
                " to start searching".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Esc".fg(self.theme.primary_color).bold(),
                " to clear search".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "Enter".fg(self.theme.primary_color).bold(),
                " to confirm search".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
        ];

        let artist_help = Paragraph::new(artist_help_text)
            .block(artist_block.title("Artists").fg(self.theme.resolve(&self.theme.section_title)))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(artist_help, left);

        let track_block = Block::new()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(self.theme.resolve(&self.theme.border));

        let track_help_text = vec![
            Line::from(""),
            Line::from("jellyfin-tui Playlists help").centered().fg(self.theme.resolve(&self.theme.foreground)),
            Line::from("").centered(),
            Line::from("Here is a table of all tracks of a playlist. The controls are the same as for the Artists tab.").fg(self.theme.resolve(&self.theme.foreground)),
            Line::from(""),
            Line::from(concat!(r#"Most controls for playlists or their tracks are in the command menu."#,
                r#"You can rename, delete, or play a playlist from there."#,
                r#"The command menu you will see depends on which section you are in."#)).fg(self.theme.resolve(&self.theme.foreground)),
            Line::from(""),
            Line::from("Usage:").fg(self.theme.resolve(&self.theme.foreground)).underlined(),
            Line::from(vec![
                "  - Use ".fg(self.theme.resolve(&self.theme.foreground)),
                "p".fg(self.theme.primary_color).bold(),
                " to open a menu with commands to use".fg(self.theme.resolve(&self.theme.foreground)),
            ]),
        ];

        let track_help = Paragraph::new(track_help_text)
            .block(track_block.title("Tracks").fg(self.theme.resolve(&self.theme.section_title)))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(track_help, center[0]);

        let queue_block = Block::new()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(self.theme.resolve(&self.theme.border));

        let queue_help = Paragraph::new("")
            .block(queue_block.title("Queue").fg(self.theme.resolve(&self.theme.section_title)))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(queue_help, right[1]);

        let bottom = Block::default().borders(Borders::ALL).padding(Padding::new(0, 0, 0, 0));

        frame.render_widget(bottom, center[1]);

        // lyrics area
        let lyrics_block = Block::new()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(self.theme.resolve(&self.theme.border));

        let lyrics_help = Paragraph::new("")
            .block(lyrics_block.title("Lyrics").fg(self.theme.resolve(&self.theme.section_title)))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(lyrics_help, right[0]);

        // player area
        let player_block = Block::new()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(self.theme.resolve(&self.theme.border));

        let player_help = Paragraph::new("")
            .block(player_block.title("Player").fg(self.theme.resolve(&self.theme.section_title)))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(player_help, center[1]);
    }
}
