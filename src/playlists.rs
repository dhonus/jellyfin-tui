/* --------------------------
The playlists tab is rendered here.
-------------------------- */

use crate::tui::App;
use crate::keyboard::{*};

use ratatui::{
    Frame,
    widgets::{
        Block,
        Borders,
    },
    prelude::*,
    widgets::*,
};

impl App {
    pub fn render_playlists(&mut self, app_container: Rect, frame: &mut Frame) {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(22),
                Constraint::Percentage(56),
                Constraint::Percentage(22),
            ])
            .split(app_container);
    
        let left = outer_layout[0];

        // create a wrapper, to get the width. After that create the inner 'left' and split it
        let center = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(86), Constraint::Min(8)])
            .split(outer_layout[1]);
        
        let show_lyrics = self.lyrics.as_ref().is_some_and(|(_, lyrics, _)| !lyrics.is_empty());
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints(if show_lyrics && !self.lyrics.as_ref().map_or(true, |(_, lyrics, _)| lyrics.len() == 1) {
                vec![Constraint::Percentage(68), Constraint::Percentage(32)]
            } else {
                vec![Constraint::Min(3), Constraint::Percentage(100)]
            })
            .split(outer_layout[2]);

        let playlist_block = match self.state.active_section {
            ActiveSection::Artists => Block::new()
                .borders(Borders::ALL)
                .border_style(self.primary_color),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::White),
        };
        
        let selected_playlist = self.get_id_of_selected(&self.playlists, Selectable::Playlist);
        let mut playlist_highlight_style = match self.state.active_section {
            ActiveSection::Artists => Style::default()
                .bg(Color::White)
                .fg(Color::Indexed(232))
                .add_modifier(Modifier::BOLD),
            _ => Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        };

        if self.state.current_playlist.id == selected_playlist {
            playlist_highlight_style = playlist_highlight_style.add_modifier(Modifier::ITALIC);
        }
    
        let items = self
            .playlists
            .iter()
            .filter(|playlist| {
                if self.state.playlists_search_term.is_empty() {
                    return true;
                }
                !crate::helpers::find_all_subsequences(
                    &self.state.playlists_search_term.to_lowercase(), &playlist.name.to_lowercase()
                ).is_empty()
            })
            .map(|playlist| {
                let color = if playlist.id == self.state.current_playlist.id {
                    self.primary_color
                } else {
                    Color::White
                };

                // underline the matching search subsequence ranges
                let mut item = Text::default();
                let mut last_end = 0;
                let all_subsequences = crate::helpers::find_all_subsequences(
                    &self.state.playlists_search_term.to_lowercase(),
                    &playlist.name.to_lowercase(),
                );
                for (start, end) in all_subsequences {
                    if last_end < start {
                        item.push_span(Span::styled(
                            &playlist.name[last_end..start],
                            Style::default().fg(color),
                        ));
                    }

                    item.push_span(Span::styled(
                        &playlist.name[start..end],
                        Style::default().fg(color).underlined()
                    ));

                    last_end = end;
                }

                if last_end < playlist.name.len() {
                    item.push_span(Span::styled(
                        &playlist.name[last_end..],
                        Style::default().fg(color),
                    ));
                }
                if playlist.user_data.is_favorite {
                    item.push_span(Span::styled(" ♥", Style::default().fg(self.primary_color)));
                }
                ListItem::new(item)
            })
            .collect::<Vec<ListItem>>();

        let items_len = items.len();
        let list = List::new(items)
            .block(if self.state.playlists_search_term.is_empty() {
                playlist_block
                    .title_alignment(Alignment::Right)
                    .title_top(Line::from("All").left_aligned())
                    .title_top(format!("({} playlists)", self.playlists.len())).title_position(block::Position::Bottom)
            } else {
                playlist_block
                    .title_alignment(Alignment::Right)
                    .title_top(Line::from(
                        format!("Matching {}", self.state.playlists_search_term)
                    ).left_aligned())
                    .title_top(format!("({} playlists)", items_len)).title_position(block::Position::Bottom)
            })
            .highlight_symbol(">>")
            .highlight_style(
                playlist_highlight_style
            )
            .scroll_padding(10)
            .repeat_highlight_symbol(true);
    
        frame.render_stateful_widget(list, left, &mut self.state.selected_playlist);

        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"))
                .track_style(Style::default().fg(Color::DarkGray))
                .thumb_style(Style::default().fg(Color::Gray)),
            left.inner(Margin {
                vertical: 1,
                horizontal: 1,
            }),
            &mut self.state.playlists_scroll_state,
        );

        let track_block = match self.state.active_section {
            ActiveSection::Tracks => Block::new()
                .borders(Borders::ALL)
                .border_style(self.primary_color),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::White),
        };
    
        let track_highlight_style = match self.state.active_section {
            ActiveSection::Tracks => Style::default()
                .bg(Color::White)
                .fg(Color::Indexed(232))
                .add_modifier(Modifier::BOLD),
            _ => Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        };

        let items = self
            .tracks_playlist
            .iter()
            // if search_term is not empty we filter the tracks
            .filter(|track: &&crate::client::DiscographySong| {
                if self.state.playlist_tracks_search_term.is_empty() {
                    return true;
                }
                !crate::helpers::find_all_subsequences(
                    &self.state.playlist_tracks_search_term.to_lowercase(), &track.name.to_lowercase()
                ).is_empty() && track.id != "_album_"
            })
            .enumerate()
            .map(|(index, track)| {
                let title = track.name.to_string();

                if track.id == "_album_" {
                    // this is the dummy that symbolizes the name of the album
                    return Row::new(vec![
                        Cell::from(">>"),
                        Cell::from(title),
                        Cell::from(""),
                        Cell::from(""),
                        Cell::from(""),
                        Cell::from(""),
                    ]).style(Style::default().fg(Color::White)).bold();
                }

                // track.run_time_ticks is in microseconds
                let seconds = (track.run_time_ticks / 10_000_000) % 60;
                let minutes = (track.run_time_ticks / 10_000_000 / 60) % 60;
                let hours = (track.run_time_ticks / 10_000_000 / 60) / 60;
                let hours_optional_text = match hours {
                    0 => String::from(""),
                    _ => format!("{}:", hours),
                };

                let all_subsequences = crate::helpers::find_all_subsequences(
                    &self.state.playlist_tracks_search_term.to_lowercase(),
                    &track.name.to_lowercase(),
                );

                let mut title = vec![];
                let mut last_end = 0;
                let color = if track.id == self.active_song_id {
                    self.primary_color
                } else {
                    Color::White
                };
                for (start, end) in &all_subsequences {
                    if &last_end < start {
                        title.push(Span::styled(
                            &track.name[last_end..*start],
                            Style::default().fg(color),
                        ));
                    }

                    title.push(Span::styled(
                        &track.name[*start..*end],
                        Style::default().fg(color).underlined()
                    ));

                    last_end = *end;
                }

                if last_end < track.name.len() {
                    title.push(Span::styled(
                        &track.name[last_end..],
                        Style::default().fg(color),
                    ));
                }
    
                Row::new(vec![
                    Cell::from(format!("{}.", index + 1)).style(if track.id == self.active_song_id {
                        Style::default().fg(color)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }),
                    Cell::from(if all_subsequences.is_empty() {
                        track.name.to_string().into()
                    } else {
                        Line::from(title)
                    }),
                    Cell::from(track.artist_items.iter().map(|artist| artist.name.clone()).collect::<Vec<String>>().join(", ")),
                    Cell::from(track.album.clone()),
                    Cell::from(if track.user_data.is_favorite {
                        "♥".to_string()
                    } else {
                        "".to_string()
                    }).style(Style::default().fg(self.primary_color)),
                    Cell::from(format!("{}", track.user_data.play_count)),
                    Cell::from(if track.has_lyrics {
                        "✓".to_string()
                    } else {
                        "".to_string()
                    }),
                    Cell::from(format!("{}{:02}:{:02}", hours_optional_text, minutes, seconds)),
                ]).style(if track.id == self.active_song_id {
                    Style::default().fg(self.primary_color).italic()
                } else {
                    Style::default().fg(Color::White)
                })
            }).collect::<Vec<Row>>();

        let track_instructions = Line::from(vec![
           " Help ".white(),
           "<?>".fg(self.primary_color).bold(),
           " Quit ".white(),
           "<Q> ".fg(self.primary_color).bold(),
        ]);
        let widths = [
            Constraint::Length(items.len().to_string().len() as u16 + 1),
            Constraint::Percentage(50), // title and track even width
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Length(2),
            Constraint::Length(5),
            Constraint::Length(6),
            Constraint::Length(10),
        ];

        if self.tracks_playlist.is_empty() {
            let message_paragraph = Paragraph::new(if self.state.current_playlist.id.is_empty() {
                "jellyfin-tui".to_string()
            } else {
                "No tracks in the current playlist".to_string()
            })
                .block(
                    track_block.title("Tracks").padding(Padding::new(
                        0, 0, center[0].height / 2, 0,
                    )).title_bottom(track_instructions.alignment(Alignment::Center))
                )
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Center);
            frame.render_widget(message_paragraph, center[0]);
        } else {
            let items_len = items.len();
            let totaltime = self.state.current_playlist.run_time_ticks / 10_000_000;
            let seconds = totaltime % 60;
            let minutes = (totaltime / 60) % 60;
            let hours = totaltime / 60 / 60;
            let hours_optional_text = match hours {
                0 => String::from(""),
                _ => format!("{}:", hours),
            };
            let duration = format!("{}{:02}:{:02}", hours_optional_text, minutes, seconds);

            let table = Table::new(items, widths)
                .block(if self.state.playlist_tracks_search_term.is_empty() && !self.state.current_playlist.name.is_empty() {
                    track_block
                        .title(self.state.current_playlist.name.to_string())
                        .title_top(Line::from(format!("({} tracks - {})", self.tracks_playlist.len(), duration)).right_aligned())
                        .title_bottom(track_instructions.alignment(Alignment::Center))
                } else {
                    track_block
                        .title(format!("Matching: {}", self.state.playlist_tracks_search_term))
                        .title_top(Line::from(format!("({} tracks)", items_len)).right_aligned())
                        .title_bottom(track_instructions.alignment(Alignment::Center))
                })
                .row_highlight_style(track_highlight_style)
                .highlight_symbol(">>")
                .style(
                    Style::default().bg(Color::Reset)
                )
                .header(
                    Row::new(vec!["#", "Title", "Artist", "Album", "♥", "Plays", "Lyrics", "Duration"])
                    .style(Style::new().bold().white())
                        .bottom_margin(0),
                );
            frame.render_widget(Clear, center[0]);
            frame.render_stateful_widget(table, center[0], &mut self.state.selected_playlist_track);
        }

        if self.locally_searching {
            let searching_instructions = Line::from(vec![
                " Confirm ".white(),
                "<Enter>".fg(self.primary_color).bold(),
                " Clear and keep selection ".white(),
                "<Esc> ".fg(self.primary_color).bold(),
            ]);
            if self.state.active_section == ActiveSection::Tracks {
                frame.render_widget(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Searching: {}", self.state.playlist_tracks_search_term))
                        .title_bottom(searching_instructions.alignment(Alignment::Center))
                        .border_style(self.primary_color),
                        center[0],
                );
            }
            if self.state.active_section == ActiveSection::Artists {
                frame.render_widget(
                    Block::default()
                    .borders(Borders::ALL)
                        .title(format!("Searching: {}", self.state.playlists_search_term))
                        .border_style(self.primary_color),
                    left,
                );
            }
        }

        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"))
                .track_style(Style::default().fg(Color::DarkGray))
                .thumb_style(Style::default().fg(Color::Gray)),
            center[0].inner(Margin {
                vertical: 1,
                horizontal: 1,
            }),
            &mut self.state.playlist_tracks_scroll_state,
        );

        self.render_player(frame, center);
        self.render_library_right(frame, right);

        self.create_popup(frame);
    }
}
