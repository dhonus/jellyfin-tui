use crate::tui::App;
use crate::keyboard::{*};

use souvlaki::MediaMetadata;
use ratatui_image::{StatefulImage, Resize};
use layout::Flex;
use ratatui::{
    Frame,
    widgets::{
        Block,
        block::Title,
        block::Position,
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
            .constraints(if show_lyrics {
                vec![Constraint::Percentage(68), Constraint::Percentage(32)]
            } else {
                vec![Constraint::Min(3), Constraint::Percentage(100)]
            })
            .split(outer_layout[2]);
    
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
                artist.name.to_lowercase().contains(&self.artists_search_term.to_lowercase())
            })
            .map(|artist| {
                if self.playlist.iter().map(|song| song.artist_items.clone()).flatten().any(|a| a.id == artist.id) {
                    return ListItem::new(artist.name.as_str())
                        .style(Style::default().fg(Color::Blue))
                } else {
                    let mut item = Text::default();
                    item.push_span(Span::styled(artist.name.as_str(), Style::default().fg(Color::White)));
                    if artist.jellyfintui_recently_added {
                        item.push_span(Span::styled(" ★", Style::default().fg(Color::Yellow)));
                    }
                    return ListItem::new(item)
                }
            })
            .collect::<Vec<ListItem>>();
    
        let list = List::new(items)
            .block(if self.artists_search_term.is_empty() {
                artist_block.title("Artists")
            } else {
                artist_block.title(format!("Artists matching: {}", self.artists_search_term))
            })
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
            // if search_term is not empty we filter the tracks
            .filter(|track| {
                if self.tracks_search_term.is_empty() {
                    return true;
                }
                track.name.to_lowercase().contains(&self.tracks_search_term.to_lowercase()) && track.id != "_album_"
            })
            .map(|track| {
                if track.id == "_album_" {
                    // this is the dummy that symbolizes the name of the album
                    return ListItem::new(track.name.as_str())
                        .style(Style::default().fg(Color::White)
                        .add_modifier(Modifier::BOLD));
                }
                let title = format!("{}", track.name);
                // track.run_time_ticks is in microseconds
                let seconds = (track.run_time_ticks / 1_000_0000) % 60;
                let minutes = (track.run_time_ticks / 1_000_0000 / 60) % 60;
                let hours = (track.run_time_ticks / 1_000_0000 / 60) / 60;
                let hours_optional_text = match hours {
                    0 => String::from(""),
                    _ => format!("{}:", hours),
                };
    
                let mut time_span_text = format!("  {}{:02}:{:02}", hours_optional_text, minutes, seconds);
                // push track.parent_index_number as CD1, CD2, etc
                if track.parent_index_number > 0 {
                    time_span_text.push_str(
                        format!(" CD{}", track.parent_index_number).as_str()
                    );
                }
                if track.has_lyrics{
                    time_span_text.push_str(" (l)");
                }
                let index = Span::styled(
                    format!("{}. ", track.index_number),
                        Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                );
                if track.id == self.active_song_id {
                    let mut time: Text = Text::default();
                    time.push_span(
                        Span::styled(
                            format!("{}{}", index, title),
                            Style::default().fg(Color::Blue),
                        )
                    );
                    time.push_span(
                        Span::styled(
                            time_span_text,
                            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                        )
                    );
                    ListItem::new(time)
                        .style(Style::default().fg(Color::Blue))
    
                } else {
                    let mut time: Text = Text::from(index);
                    time.push_span(
                        Span::styled(
                            title,
                            Style::default().fg(Color::White),
                        )
                    );
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
        let track_instructions = Title::from(Line::from(vec![
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
        let list = List::new(items)
            .block(
                track_block
                    .title(if self.tracks_search_term.is_empty() {
                        format!("Tracks")
                    } else {
                        format!("Tracks matching: {}", self.tracks_search_term)
                    })
                    .title(track_instructions.alignment(Alignment::Center).position(Position::Bottom)),
            )
            .highlight_symbol(">>")
            .highlight_style(
                track_highlight_style
            )
            .scroll_padding(10)
            .repeat_highlight_symbol(true);
    
        if self.tracks.len() == 0 {
            let message_paragraph = Paragraph::new("jellyfin-tui")
                .block(
                    Block::default().borders(Borders::ALL).title("Tracks").padding(Padding::new(
                        0, 0, center[0].height / 2, 0,
                    )),
                )
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Center);
            frame.render_widget(message_paragraph, center[0]);
        } else {
            frame.render_widget(Clear, center[0]);
            frame.render_stateful_widget(list, center[0], &mut self.selected_track);
        }
    
        // change section Title to 'Searching: TERM' if locally searching
        if self.locally_searching {
            let searching_instructions = Title::from(Line::from(vec![
                " Confirm ".white().into(),
                "<Enter>".blue().bold(),
                " Clear and keep selection ".white().into(),
                "<Esc> ".blue().bold(),
            ]));
            if self.active_section == ActiveSection::Tracks {
                frame.render_widget(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Searching: {}", self.tracks_search_term))
                        .title(searching_instructions.alignment(Alignment::Center).position(Position::Bottom))
                        .border_style(style::Color::Blue),
                    center[0],
                );
            }
            if self.active_section == ActiveSection::Artists {
                frame.render_widget(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Searching: {}", self.artists_search_term))
                        .border_style(style::Color::Blue),
                    left,
                );
            }
        }
    
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
    
        // update mpris metadata
        if self.current_playback_state.current_index != self.current_playback_state.last_index {
            let metadata = match self
                .playlist
                .get(self.current_playback_state.current_index as usize)
            {
                Some(song) => {
                    let metadata = MediaMetadata {
                        title: Some(song.name.as_str()),
                        artist: Some(song.artist.as_str()),
                        album: Some(song.album.as_str()),
                        cover_url: None,
                        duration: None,
                    };
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
            match self.controls {
                Some(ref mut controls) =>
                    match controls.set_metadata(metadata) {
                    _ => {}
                },
                None => {}
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
                            .fg(Color::LightBlue)
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
                .border_style(style::Color::Blue)
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
        } else if let Some(lyrics) = &self.lyrics {
            // this will show the lyrics in a scrolling list
            let items = lyrics
                .1
                .iter()
                .map(|lyric| {
                    let width = right[0].width as usize;
                    if lyric.text.len() > (width - 5) {
                        // word wrap
                        let mut lines = vec![];
                        let mut line = String::new();
                        for word in lyric.text.split_whitespace() {
                            if line.len() + word.len() + 1 < width - 5 {
                                line.push_str(word);
                                line.push_str(" ");
                            } else {
                                lines.push(line.clone());
                                line.clear();
                                line.push_str(word);
                                line.push_str(" ");
                            }
                        }
                        lines.push(line);
                        // assemble into string separated by newlines
                        lines.join("\n")
                    } else {
                        lyric.text.clone()
                    }
                })
                .collect::<Vec<String>>();
    
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
            if lyrics.2 && !self.selected_lyric_manual_override {
                let current_time = self.current_playback_state.duration * self.current_playback_state.percentage / 100.0;
                let current_time_microseconds = (current_time * 10_000_000.0) as u64;
                for (i, lyric) in lyrics.1.iter().enumerate() {
                    if lyric.start >= current_time_microseconds {
                        let index = i - 1;
                        if index >= lyrics.1.len() {
                            self.selected_lyric.select(Some(0));
                        } else {
                            self.selected_lyric.select(Some(index));
                        }
                        break;
                    }
                }
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
            .scroll_padding(10)
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