/* --------------------------
Main Library tab
    - This file defines the Library tab. The render_home function is called on every frame and generates all the widgets for the Library tab.
    - Layout is as such:
        outer_layout[0]: left - Artists list
        outer_layout[1]: center:
            center[0]: Tracks list
            center[1]: Bottom section with current song, progress bar, metadata, etc.
        outer_layout[2]: right:
            right[0]: Lyrics list
            right[1]: Queue list
-------------------------- */

use crate::helpers;
use crate::tui::App;
use crate::keyboard::{*};

use souvlaki::{MediaMetadata, MediaPosition};
use ratatui_image::{StatefulImage, Resize};
use std::time::Duration;
use layout::Flex;
use ratatui::{
    Frame,
    widgets::{
        Block,
        Borders,
        Paragraph
    },
    prelude::*,
    widgets::*,
};

impl App {
    pub fn render_home(&mut self, app_container: Rect, frame: &mut Frame) {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(20),
                Constraint::Percentage(56),
                Constraint::Percentage(24),
            ])
            .split(app_container);
    
        let left = outer_layout[0];

        // create a wrapper, to get the width. After that create the inner 'left' and split it
        let center = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(86), Constraint::Min(8)])
            .split(outer_layout[1]);
        
        let show_lyrics = self.lyrics.as_ref().map_or(false, |(_, lyrics, _)| !lyrics.is_empty());
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints(if show_lyrics && !self.lyrics.as_ref().map_or(true, |(_, lyrics, _)| lyrics.len() == 1) {
                vec![Constraint::Percentage(68), Constraint::Percentage(32)]
            } else {
                vec![Constraint::Min(3), Constraint::Percentage(100)]
            })
            .split(outer_layout[2]);
    
        let artist_block = match self.active_section {
            ActiveSection::Artists => Block::new()
                .borders(Borders::ALL)
                .border_style(self.primary_color),
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
    
        // render all artists as a list here in left[0]
        let items = self
            .artists
            .iter()
            .filter(|artist| {
                if self.artists_search_term.is_empty() {
                    return true;
                }
                helpers::find_all_subsequences(
                    &self.artists_search_term.to_lowercase(), &artist.name.to_lowercase()
                ).len() > 0
            })
            .map(|artist| {
                // we color all artists that have songs in the playlist :)
                let color = if self.queue.iter().map(|song| song.artist_items.clone()).flatten().any(|a| a.id == artist.id) {
                    self.primary_color
                } else { Color::White };

                // underline the matching search subsequence ranges
                let mut item = Text::default();
                let mut last_end = 0;
                let all_subsequences = helpers::find_all_subsequences(
                    &self.artists_search_term.to_lowercase(),
                    &artist.name.to_lowercase(),
                );
                for (start, end) in all_subsequences {
                    if last_end < start {
                        item.push_span(Span::styled(
                            &artist.name[last_end..start],
                            Style::default().fg(color),
                        ));
                    }

                    item.push_span(Span::styled(
                        &artist.name[start..end],
                        Style::default().fg(color).underlined()
                    ));

                    last_end = end;
                }

                if last_end < artist.name.len() {
                    item.push_span(Span::styled(
                        &artist.name[last_end..],
                        Style::default().fg(color),
                    ));
                }

                if artist.jellyfintui_recently_added {
                    item.push_span(Span::styled(" ★", Style::default().fg(Color::Yellow)));
                }
                ListItem::new(item)
            })
            .collect::<Vec<ListItem>>();
    
        let list = List::new(items)
            .block(if self.artists_search_term.is_empty() {
                artist_block.title(format!("Artists ({})", self.artists.len()))
            } else {
                artist_block.title(format!("Artists matching: {}", self.artists_search_term))
            })
            .highlight_symbol(">>")
            .highlight_style(
                artist_highlight_style
            )
            .scroll_padding(10)
            .repeat_highlight_symbol(true);
    
        frame.render_stateful_widget(list, left, &mut self.selected_artist);

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
            &mut self.artists_scroll_state,
        );
    
        let track_block = match self.active_section {
            ActiveSection::Tracks => Block::new()
                .borders(Borders::ALL)
                .border_style(self.primary_color),
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
            // if search_term is not empty we filter the tracks
            .filter(|track| {
                if self.tracks_search_term.is_empty() {
                    return true;
                }
                helpers::find_all_subsequences(
                    &self.tracks_search_term.to_lowercase(), &track.name.to_lowercase()
                ).len() > 0 && track.id != "_album_"
            })
            .map(|track| {
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

                let all_subsequences = helpers::find_all_subsequences(
                    &self.tracks_search_term.to_lowercase(),
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
                    Cell::from(format!("{}.", track.index_number)).style(if track.id == self.active_song_id {
                        Style::default().fg(color)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }),
                    Cell::from(if all_subsequences.is_empty() {
                        track.name.to_string().into()
                    } else {
                        Line::from(title)
                    }),
                    Cell::from(track.album.clone()),
                    Cell::from(if track.parent_index_number > 0 {
                        format!("{}", track.parent_index_number)
                    } else {
                        String::from("1")
                    }),
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
            Constraint::Length(3),
            Constraint::Percentage(50), // title and track even width
            Constraint::Percentage(50),
            Constraint::Length(5),
            Constraint::Length(6),
            Constraint::Length(10),
        ];

        if self.tracks.is_empty() {
            let message_paragraph = Paragraph::new("jellyfin-tui")
                .block(
                    track_block.title("Tracks").padding(Padding::new(
                        0, 0, center[0].height / 2, 0,
                    )).title_bottom(track_instructions.alignment(Alignment::Center))
                )
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Center);
            frame.render_widget(message_paragraph, center[0]);
        } else {
            let table = Table::new(items, widths)
                .block(
                    track_block
                    .title(if self.tracks_search_term.is_empty() && !self.current_artist_name.is_empty() {
                            format!("{} ({})", self.current_artist_name, self.tracks.len())
                        } else {
                            format!("Tracks matching: {}", self.tracks_search_term)
                        })
                        .title_bottom(track_instructions.alignment(Alignment::Center))
                )
                .row_highlight_style(track_highlight_style)
                .highlight_symbol(">>")
                .style(
                    Style::default().bg(Color::Reset)
                )
                .header(
                    Row::new(vec!["#", "Title", "Album", "Disc", "Lyrics", "Duration"])
                    .style(Style::new().bold())
                        .bottom_margin(0),
                );
            frame.render_widget(Clear, center[0]);
            frame.render_stateful_widget(table, center[0], &mut self.selected_track);
        }

        // change section Title to 'Searching: TERM' if locally searching
        if self.locally_searching {
            let searching_instructions = Line::from(vec![
                " Confirm ".white(),
                "<Enter>".fg(self.primary_color).bold(),
                " Clear and keep selection ".white(),
                "<Esc> ".fg(self.primary_color).bold(),
            ]);
            if self.active_section == ActiveSection::Tracks {
                frame.render_widget(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Searching: {}", self.tracks_search_term))
                        .title_bottom(searching_instructions.alignment(Alignment::Center))
                        .border_style(self.primary_color),
                        center[0],
                );
            }
            if self.active_section == ActiveSection::Artists {
                frame.render_widget(
                    Block::default()
                    .borders(Borders::ALL)
                        .title(format!("Searching: {}", self.artists_search_term))
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
            &mut self.tracks_scroll_state,
        );
    
        // currently playing song name. We can get this easily, we have the playlist and the current index
        let current_song = match self
            .queue
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
        if self.active_song_id != self.mpris_active_song_id && self.current_playback_state.current_index != self.current_playback_state.last_index && self.current_playback_state.duration > 0.0 {
            self.mpris_active_song_id = self.active_song_id.clone();
            let metadata = match self
                .queue
                .get(self.current_playback_state.current_index as usize)
            {
                Some(song) => {
                    let metadata = MediaMetadata {
                        title: Some(song.name.as_str()),
                        artist: Some(song.artist.as_str()),
                        album: Some(song.album.as_str()),
                        cover_url: None,
                        duration: Some(Duration::from_secs((self.current_playback_state.duration) as u64)),
                    };
                    // TODO add cover art to mpris
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

            if let Some(ref mut controls) = self.controls {
                let _ = controls.set_metadata(metadata);
            }
        }
        if self.paused != self.mpris_paused && self.current_playback_state.duration > 0.0 {
            self.mpris_paused = self.paused;
            if let Some(ref mut controls) = self.controls {
                let progress = self.current_playback_state.duration * self.current_playback_state.percentage / 100.0;
                let _ = controls.set_playback(if self.paused { souvlaki::MediaPlayback::Paused { progress: Some(MediaPosition(Duration::from_secs_f64(progress))) } } else { souvlaki::MediaPlayback::Playing { progress: Some(MediaPosition(Duration::from_secs_f64(progress))) } });
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
            .constraints(
                if self.cover_art.is_some() {
                    vec![Constraint::Percentage(15), Constraint::Percentage(85)]
                } else {
                    vec![Constraint::Percentage(2), Constraint::Percentage(100)]
                }
            )
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
                            .fg(self.primary_color)
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
                .ratio(self.current_playback_state.percentage / 100_f64),
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
                .border_style(self.primary_color)
                ,
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::White),
        };

        if !show_lyrics {
            let message_paragraph = Paragraph::new("No lyrics available")
            .block(
                lyrics_block.title("Lyrics"),
            )
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Center);
    
            frame.render_widget(
                message_paragraph, right[0],
            );
        } else if let Some((_, lyrics, time_synced)) = &self.lyrics {
            // this will show the lyrics in a scrolling list
            let items = lyrics
                .iter()
                .enumerate()
                .map(|(index, lyric)| {

                    let style = if (index == self.current_lyric) && (index != self.selected_lyric.selected().unwrap_or(0)) {
                        Style::default().fg(self.primary_color)
                    } else {
                        Style::default()
                    };

                    let width = right[0].width as usize;
                    if lyric.text.len() > (width - 5) {
                        // word wrap
                        let mut lines = vec![];
                        let mut line = String::new();
                        for word in lyric.text.split_whitespace() {
                            if line.len() + word.len() + 1 < width - 5 {
                                line.push_str(word);
                                line.push(' ');
                            } else {
                                lines.push(line.clone());
                                line.clear();
                                line.push_str(word);
                                line.push(' ');
                            }
                        }
                        lines.push(line);
                        ListItem::new(Text::from(lines.join("\n"))).style(style)
                    } else {
                        ListItem::new(Text::from(lyric.text.clone())).style(style)
                    }
                })
                .collect::<Vec<ListItem>>();

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
            if *time_synced {
                let current_time = self.current_playback_state.duration * self.current_playback_state.percentage / 100.0;
                let current_time_microseconds = (current_time * 10_000_000.0) as u64;
                for (i, lyric) in lyrics.iter().enumerate() {
                    if lyric.start >= current_time_microseconds {
                        let index = if i == 0 { 0 } else { i - 1 };
                        if self.selected_lyric_manual_override {
                            self.current_lyric = index;
                            break;
                        }
                        if index >= lyrics.len() {
                            self.selected_lyric.select(Some(0));
                            self.current_lyric = 0;
                        } else {
                            self.selected_lyric.select(Some(index));
                            self.current_lyric = index;
                        }
                        break;
                    }
                }
            }
        }
    
        let queue_block = match self.active_section {
            ActiveSection::Queue => Block::new()
                .borders(Borders::ALL)
                .border_style(self.primary_color),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::White),
        };
    
        let items = self
            .queue
            .iter()
            .enumerate()
            .map(|(index, song)| {
                // skip previously played songs
                let mut item = Text::default();
                if song.is_in_queue {
                    item.push_span(Span::styled("+ ", Style::default().fg(self.primary_color)));
                }
                if index == self.current_playback_state.current_index as usize {
                    item.push_span(Span::styled(song.name.as_str(), Style::default().fg(self.primary_color)));
                    return ListItem::new(item)
                }
                item.push_span(Span::styled(song.name.as_str(), Style::default().fg(Color::White)));
                item.push_span(Span::styled(" - ", Style::default().fg(Color::White)));
                item.push_span(Span::styled(song.artist.as_str(), Style::default().fg(Color::DarkGray)));
                ListItem::new(item)
            })
            .collect::<Vec<ListItem>>();
        let list = List::new(items)
            .block(queue_block.title("Queue"))
            .highlight_symbol(">>")
            .highlight_style(
                Style::default()
                    .bold()
                    .fg(Color::Black)
                    .bg(Color::White),
            )
            .scroll_padding(5)
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
}
