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

use crate::client::{Album, Artist, DiscographySong};
use crate::database::extension::DownloadStatus;
use crate::{helpers, keyboard::*};
use crate::tui::{App, Repeat};

use layout::Flex;
use ratatui::{
    prelude::*,
    widgets::*,
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use ratatui_image::{Resize, StatefulImage};
use souvlaki::{MediaMetadata, MediaPosition};
use std::time::Duration;

impl App {
    pub fn render_home(&mut self, app_container: Rect, frame: &mut Frame) {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(22),
                Constraint::Percentage(56),
                Constraint::Percentage(22),
            ])
            .split(app_container);

        // create a wrapper, to get the width. After that create the inner 'left' and split it
        let center = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(100), Constraint::Length(8)])
            .split(outer_layout[1]);

        let show_lyrics = self
            .lyrics
            .as_ref()
            .is_some_and(|(_, lyrics, _)| !lyrics.is_empty());
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                if show_lyrics
                    && !self
                        .lyrics
                        .as_ref()
                        .map_or(true, |(_, lyrics, _)| lyrics.len() == 1)
                {
                    vec![Constraint::Percentage(68), Constraint::Percentage(32)]
                } else {
                    vec![Constraint::Min(3), Constraint::Percentage(100)]
                },
            )
            .split(outer_layout[2]);

        // update mpris metadata
        if self.active_song_id != self.mpris_active_song_id
            && self.state.current_playback_state.current_index
                != self.state.current_playback_state.last_index
            && self.state.current_playback_state.duration > 0.0
        {
            self.mpris_active_song_id = self.active_song_id.clone();
            let cover_url = format!("file://{}", self.cover_art_path);
            let metadata = match self
                .state
                .queue
                .get(self.state.current_playback_state.current_index as usize)
            {
                Some(song) => {
                    let metadata = MediaMetadata {
                        title: Some(song.name.as_str()),
                        artist: Some(song.artist.as_str()),
                        album: Some(song.album.as_str()),
                        cover_url: Some(cover_url.as_str()),
                        duration: Some(Duration::from_secs(
                            (self.state.current_playback_state.duration) as u64,
                        )),
                    };
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
        if self.paused != self.mpris_paused && self.state.current_playback_state.duration > 0.0 {
            self.mpris_paused = self.paused;
            if let Some(ref mut controls) = self.controls {
                let progress = self.state.current_playback_state.duration
                    * self.state.current_playback_state.percentage
                    / 100.0;
                let _ = controls.set_playback(if self.paused {
                    souvlaki::MediaPlayback::Paused {
                        progress: Some(MediaPosition(Duration::from_secs_f64(progress))),
                    }
                } else {
                    souvlaki::MediaPlayback::Playing {
                        progress: Some(MediaPosition(Duration::from_secs_f64(progress))),
                    }
                });
            }
        }

        self.render_library_left(frame, outer_layout);
        self.render_library_center(frame, &center);
        self.render_player(frame, &center);
        self.render_library_right(frame, right);
        self.create_popup(frame);
    }

    fn render_library_left(&mut self, frame: &mut Frame, outer_layout: std::rc::Rc<[Rect]>) {
        // LEFT sidebar construct. large_art flag determines the split
        let left = if self.state.large_art {
            if let Some(cover_art) = self.cover_art.as_mut() {
                let outer_area = outer_layout[0];
                let block = Block::default()
                    .borders(Borders::ALL)
                    .title("Cover art")
                    .white()
                    .border_style(style::Color::White);

                let chunk_area = block.inner(outer_area);
                let resize = Resize::Scale(None);
                let img_area = cover_art.size_for(&resize, chunk_area);

                let block_total_height = img_area.height + 2;
                let top_height = outer_area.height.saturating_sub(block_total_height);

                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(vec![
                        Constraint::Length(top_height),         // artist list area
                        Constraint::Length(block_total_height), // image area
                    ])
                    .split(outer_area);

                frame.render_widget(block, layout[1]);

                let inner_area = layout[1].inner(Margin {
                    vertical: 1,
                    horizontal: 1,
                });

                let final_centered = Rect {
                    x: inner_area.x + (inner_area.width.saturating_sub(img_area.width)) / 2,
                    y: inner_area.y + (inner_area.height.saturating_sub(img_area.height)) / 2,
                    width: img_area.width,
                    height: img_area.height,
                };

                let image = StatefulImage::default().resize(Resize::Scale(None));
                frame.render_stateful_widget(image, final_centered, cover_art);

                layout
            } else {
                Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(vec![Constraint::Percentage(100)])
                    .split(outer_layout[0])
            }
            // these two should be the same
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Percentage(100)])
                .split(outer_layout[0])
        };

        match self.state.active_tab {
            ActiveTab::Library => {
                self.render_library_artists(frame, left);
            }
            ActiveTab::Albums => {
                self.render_library_albums(frame, left);
            }
            _ => {}
        }
    }

    fn render_library_artists(&mut self, frame: &mut Frame, left: std::rc::Rc<[Rect]>) {
        let artist_block = match self.state.active_section {
            ActiveSection::List => Block::new()
                .borders(Borders::ALL)
                .border_style(self.primary_color),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::White),
        };

        let selected_artist = self.get_id_of_selected(&self.artists, Selectable::Artist);

        let mut artist_highlight_style = match self.state.active_section {
            ActiveSection::List => Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::White)
                .fg(Color::Indexed(232)),
            _ => Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Indexed(236))
                .fg(Color::White),
        };

        if let Some(song) = self
            .state
            .queue
            .get(self.state.current_playback_state.current_index as usize)
        {
            if song.artist_items.iter().any(|a| a.id == selected_artist) {
                artist_highlight_style = artist_highlight_style.add_modifier(Modifier::ITALIC);
            }
        }

        let artists = search_results(&self.artists, &self.state.artists_search_term, true)
            .iter()
            .map(|id| self.artists.iter().find(|artist| artist.id == *id).unwrap())
            .collect::<Vec<&Artist>>();

        let terminal_height = frame.area().height as usize;
        let selection = self.state.selected_artist.selected().unwrap_or(0);

        // render all artists as a list here in left[0]
        let items = artists
            .iter()
            .enumerate()
            .map(|(i, artist)| {
                if i < selection.saturating_sub(terminal_height)
                    || i > selection + terminal_height
                {
                    return ListItem::new(Text::raw(""));
                }
                let color = if let Some(song) = self
                    .state
                    .queue
                    .get(self.state.current_playback_state.current_index as usize)
                {
                    if song.artist_items.iter().any(|a| a.id == artist.id) {
                        self.primary_color
                    } else {
                        Color::White
                    }
                } else {
                    Color::White
                };

                // underline the matching search subsequence ranges
                let mut item = Text::default();
                let mut last_end = 0;
                let all_subsequences = helpers::find_all_subsequences(
                    &self.state.artists_search_term.to_lowercase(),
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
                        Style::default().fg(color).underlined(),
                    ));

                    last_end = end;
                }

                if last_end < artist.name.len() {
                    item.push_span(Span::styled(
                        &artist.name[last_end..],
                        Style::default().fg(color),
                    ));
                }

                if artist.user_data.is_favorite {
                    item.push_span(Span::styled(" ♥", Style::default().fg(self.primary_color)));
                }

                if artist.jellyfintui_recently_added {
                    item.push_span(Span::styled(" ★", Style::default().fg(Color::Yellow)));
                }
                ListItem::new(item)
            })
            .collect::<Vec<ListItem>>();

        let items_len = items.len();
        let list = List::new(items)
            .block(if self.state.artists_search_term.is_empty() {
                artist_block
                    .title_alignment(Alignment::Right)
                    .title_top(Line::from("All").left_aligned())
                    .title_top(format!("({} artists)", self.artists.len()))
                    .title_bottom(
                        if self.artists_stale {
                            Line::from("Outdated, press <y> to refresh").left_aligned()
                        } else {
                            Line::from("")
                        },
                    )
                    .title_position(block::Position::Bottom)
            } else {
                artist_block
                    .title_alignment(Alignment::Right)
                    .title_top(
                        Line::from(format!("Matching: {}", self.state.artists_search_term))
                            .left_aligned(),
                    )
                    .title_top(format!("({} artists)", items_len))
                    .title_bottom(
                        if self.artists_stale {
                            Line::from("Outdated, press <y> to refresh").left_aligned()
                        } else {
                            Line::from("")
                        },
                    )
                    .title_position(block::Position::Bottom)
            })
            .highlight_symbol(">>")
            .highlight_style(artist_highlight_style)
            .scroll_padding(10)
            .repeat_highlight_symbol(true);

        frame.render_stateful_widget(list, left[0], &mut self.state.selected_artist);

        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"))
                .track_style(Style::default().fg(Color::DarkGray))
                .thumb_style(Style::default().fg(Color::Gray)),
            left[0].inner(Margin {
                vertical: 1,
                horizontal: 1,
            }),
            &mut self.state.artists_scroll_state,
        );

        if self.locally_searching && self.state.active_section == ActiveSection::List {
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Searching: {}", self.state.artists_search_term))
                    .border_style(self.primary_color),
                left[0],
            );
        }
    }

    fn render_library_albums(&mut self, frame: &mut Frame, left: std::rc::Rc<[Rect]>) {
        let album_block = match self.state.active_section {
            ActiveSection::List => Block::new()
                .borders(Borders::ALL)
                .border_style(self.primary_color),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(Color::White),
        };

        let selected_album = self.get_id_of_selected(&self.albums, Selectable::Album);

        let mut album_highlight_style = match self.state.active_section {
            ActiveSection::List => Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::White)
                .fg(Color::Indexed(232)),
            _ => Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Indexed(236))
                .fg(Color::White),
        };

        if let Some(song) = self
            .state
            .queue
            .get(self.state.current_playback_state.current_index as usize)
        {
            if song.parent_id == selected_album {
                album_highlight_style = album_highlight_style.add_modifier(Modifier::ITALIC);
            }
        }

        let albums = search_results(&self.albums, &self.state.albums_search_term, true)
            .iter()
            .map(|id| self.albums.iter().find(|album| album.id == *id).unwrap())
            .collect::<Vec<&Album>>();

        let terminal_height = frame.area().height as usize;
        let selection = self.state.selected_album.selected().unwrap_or(0);

        let items = albums
            .iter()
            .enumerate()
            .map(|(i, album)| {
                if i < selection.saturating_sub(terminal_height)
                    || i > selection + terminal_height
                {
                    return ListItem::new(Text::raw(""));
                }

                let color = if let Some(song) = self
                    .state
                    .queue
                    .get(self.state.current_playback_state.current_index as usize)
                {
                    if song.parent_id == album.id {
                        self.primary_color
                    } else {
                        Color::White
                    }
                } else {
                    Color::White
                };

                // underline the matching search subsequence ranges
                let mut item = Text::default();
                let mut last_end = 0;
                let all_subsequences = helpers::find_all_subsequences(
                    &self.state.albums_search_term.to_lowercase(),
                    &album.name.to_lowercase(),
                );
                for (start, end) in all_subsequences {
                    if last_end < start {
                        item.push_span(Span::styled(
                            &album.name[last_end..start],
                            Style::default().fg(color),
                        ));
                    }

                    item.push_span(Span::styled(
                        &album.name[start..end],
                        Style::default().fg(color).underlined(),
                    ));

                    last_end = end;
                }

                if last_end < album.name.len() {
                    item.push_span(Span::styled(
                        &album.name[last_end..],
                        Style::default().fg(color),
                    ));
                }

                if album.user_data.is_favorite {
                    item.push_span(Span::styled(" ♥", Style::default().fg(self.primary_color)));
                }
                
                item.push_span(Span::styled(
                    format!(" - {}", album.album_artists.iter().map(|a| a.name.as_str()).collect::<Vec<&str>>().join(", ")),
                    Style::default().fg(Color::DarkGray),
                ));

                ListItem::new(item)
            })
            .collect::<Vec<ListItem>>();

        let items_len = items.len();

        let list = List::new(items)
            .block(if self.state.albums_search_term.is_empty() {
                album_block
                    .title_alignment(Alignment::Right)
                    .title_top(Line::from("All").left_aligned())
                    .title_top(format!("({} albums)", self.albums.len()))
                    .title_bottom(
                        if self.albums_stale {
                            Line::from("Outdated, press <y> to refresh").left_aligned()
                        } else {
                            Line::from("")
                        },
                    )
                    .title_position(block::Position::Bottom)
            } else {
                album_block
                    .title_alignment(Alignment::Right)
                    .title_top(
                        Line::from(format!("Matching: {}", self.state.albums_search_term))
                            .left_aligned(),
                    )
                    .title_top(format!("({} albums)", items_len))
                    .title_bottom(
                        if self.albums_stale {
                            Line::from("Outdated, press <y> to refresh").left_aligned()
                        } else {
                            Line::from("")
                        },
                    )
                    .title_position(block::Position::Bottom)
            })
            .highlight_symbol(">>")
            .highlight_style(album_highlight_style)
            .scroll_padding(10)
            .repeat_highlight_symbol(true);

        frame.render_stateful_widget(list, left[0], &mut self.state.selected_album);

        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"))
                .track_style(Style::default().fg(Color::DarkGray))
                .thumb_style(Style::default().fg(Color::Gray)),
            left[0].inner(Margin {
                vertical: 1,
                horizontal: 1,
            }),
            &mut self.state.albums_scroll_state,
        );

        if self.locally_searching && self.state.active_section == ActiveSection::List {
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Searching: {}", self.state.albums_search_term))
                    .border_style(self.primary_color),
                left[0],
            );
        }
    }

    /// Individual widget rendering functions
    pub fn render_library_right(&mut self, frame: &mut Frame, right: std::rc::Rc<[Rect]>) {
        let show_lyrics = self
            .lyrics
            .as_ref()
            .is_some_and(|(_, lyrics, _)| !lyrics.is_empty());
        let lyrics_block = match self.state.active_section {
            ActiveSection::Lyrics => Block::new()
                .borders(Borders::ALL)
                .border_style(self.primary_color),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(Color::White),
        };

        if !show_lyrics {
            let message_paragraph = Paragraph::new("No lyrics available")
                .block(lyrics_block.title("Lyrics"))
                .white()
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Center);

            frame.render_widget(message_paragraph, right[0]);
        } else if let Some((_, lyrics, time_synced)) = &self.lyrics {
            // this will show the lyrics in a scrolling list
            let items = lyrics
                .iter()
                .enumerate()
                .map(|(index, lyric)| {
                    let style = if (index == self.state.current_lyric)
                        && (index != self.state.selected_lyric.selected().unwrap_or(0))
                    {
                        Style::default().fg(self.primary_color)
                    } else {
                        Style::default().white()
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
                        .bg(Color::White)
                        .fg(Color::Indexed(232)),
                )
                .repeat_highlight_symbol(false)
                .scroll_padding(10);
            frame.render_stateful_widget(list, right[0], &mut self.state.selected_lyric);

            // if lyrics are time synced, we will scroll to the current lyric
            if *time_synced {
                let current_time = self.state.current_playback_state.duration
                    * self.state.current_playback_state.percentage
                    / 100.0;
                let current_time_microseconds = (current_time * 10_000_000.0) as u64;
                for (i, lyric) in lyrics.iter().enumerate() {
                    if lyric.start >= current_time_microseconds {
                        let index = if i == 0 { 0 } else { i - 1 };
                        if self.state.selected_lyric_manual_override {
                            self.state.current_lyric = index;
                            break;
                        }
                        if index >= lyrics.len() {
                            self.state.selected_lyric.select(Some(0));
                            self.state.current_lyric = 0;
                        } else {
                            self.state.selected_lyric.select(Some(index));
                            self.state.current_lyric = index;
                        }
                        break;
                    }
                }
            }
        }
        let queue_block = match self.state.active_section {
            ActiveSection::Queue => Block::new()
                .borders(Borders::ALL)
                .border_style(self.primary_color),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(Color::White),
        };

        let items = self
            .state
            .queue
            .iter()
            .enumerate()
            .map(|(index, song)| {
                // skip previously played songs
                let mut item = Text::default();
                if song.is_in_queue {
                    item.push_span(Span::styled("+ ", Style::default().fg(self.primary_color)));
                }
                if index == self.state.current_playback_state.current_index as usize {
                    item.push_span(Span::styled(
                        song.name.as_str(),
                        Style::default().fg(self.primary_color),
                    ));
                    if song.is_favorite {
                        item.push_span(Span::styled(" ♥", Style::default().fg(self.primary_color)));
                    }
                    return ListItem::new(item);
                }
                item.push_span(Span::styled(
                    song.name.as_str(),
                    Style::default().fg(if self.state.repeat == Repeat::One {
                        Color::DarkGray
                    } else {
                        Color::White
                    }),
                ));
                if song.is_favorite {
                    item.push_span(Span::styled(" ♥", Style::default().fg(self.primary_color)));
                }
                item.push_span(Span::styled(
                    " - ",
                    Style::default().fg(if self.state.repeat == Repeat::One {
                        Color::DarkGray
                    } else {
                        Color::White
                    }),
                ));
                item.push_span(Span::styled(
                    song.artist.as_str(),
                    Style::default().fg(Color::DarkGray),
                ));
                ListItem::new(item)
            })
            .collect::<Vec<ListItem>>();
        let list = List::new(items)
            .block(
                queue_block
                    .title_alignment(Alignment::Right)
                    .title_top(Line::from("Queue").left_aligned())
                    .title_top(if self.state.queue.is_empty() {
                        String::from("")
                    } else {
                        format!(
                            "({}/{})",
                            self.state.current_playback_state.current_index + 1,
                            self.state.queue.len()
                        )
                    })
                    .title_position(block::Position::Bottom)
                    .title_bottom(if self.state.shuffle {
                        Line::from("(shuffle)").right_aligned()
                    } else {
                        Line::from("")
                    }),
            )
            .highlight_symbol(">>")
            .highlight_style(
                Style::default()
                    .bold()
                    .fg(Color::Indexed(232))
                    .bg(Color::White),
            )
            .scroll_padding(5)
            .repeat_highlight_symbol(true);

        frame.render_stateful_widget(list, right[1], &mut self.state.selected_queue_item);
    }

    fn render_library_center(&mut self, frame: &mut Frame, center: &std::rc::Rc<[Rect]>) {
        let track_block = match self.state.active_section {
            ActiveSection::Tracks => Block::new()
                .borders(Borders::ALL)
                .border_style(self.primary_color),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(style::Color::White),
        };

        let current_track = self
            .state
            .queue
            .get(self.state.current_playback_state.current_index as usize);

        let mut track_highlight_style = match self.state.active_section {
            ActiveSection::Tracks => Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Indexed(232))
                .bg(Color::White),
            _ => Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Indexed(236))
                .fg(Color::White),
        };

        // let selected_track = self.get_id_of_selected(&self.tracks, Selectable::Track);
        let selected_track = match self.state.active_tab {
            ActiveTab::Library => self.get_id_of_selected(&self.tracks, Selectable::Track),
            ActiveTab::Albums => self.get_id_of_selected(&self.album_tracks, Selectable::Track),
            _ => return,
        };
        if current_track.is_some() && current_track.unwrap().id == selected_track {
            track_highlight_style = track_highlight_style.add_modifier(Modifier::ITALIC);
        }

        match self.state.active_tab {
            ActiveTab::Library => {
                self.render_library_tracks_table(frame, center, track_block, track_highlight_style);
            }
            ActiveTab::Albums => {
                self.render_album_tracks_table(frame, center, track_block, track_highlight_style);
            }
            _ => {}
        }

        // change section Title to 'Searching: TERM' if locally searching
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
                        .title(format!(
                            "Searching: {}",
                            if self.state.active_tab == ActiveTab::Library {
                                self.state.tracks_search_term.clone()
                            } else {
                                self.state.album_tracks_search_term.clone()
                            }
                        ))
                        .title_bottom(searching_instructions.alignment(Alignment::Center))
                        .border_style(self.primary_color),
                    center[0],
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
            &mut self.state.tracks_scroll_state,
        );
    }

    /// These are split into two basically the same functions because the tracks are rendered differently
    ///
    fn render_library_tracks_table(
        &mut self,
        frame: &mut Frame,
        center: &std::rc::Rc<[Rect]>,
        track_block: Block,
        track_highlight_style: Style,
    ) {
        let tracks = search_results(&self.tracks, &self.state.tracks_search_term, true)
            .iter()
            .map(|id| self.tracks.iter().find(|t| t.id == *id).unwrap())
            .collect::<Vec<&DiscographySong>>();

        let terminal_height = frame.area().height as usize;
        let selection = self.state.selected_track.selected().unwrap_or(0);

        let items = tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                if i < selection.saturating_sub(terminal_height)
                    || i > selection + terminal_height
                {
                    return Row::default();
                }
                let title = track.name.to_string();

                if track.id.starts_with("_album_") {
                    let total_time = track.run_time_ticks / 10_000_000;
                    let seconds = total_time % 60;
                    let minutes = (total_time / 60) % 60;
                    let hours = total_time / 60 / 60;
                    let hours_optional_text = match hours {
                        0 => String::from(""),
                        _ => format!("{}:", hours),
                    };
                    let duration = format!("{}{:02}:{:02}", hours_optional_text, minutes, seconds);
                    // this is the dummy that symbolizes the name of the album
                    return Row::new(vec![
                        Cell::from(">>"),
                        Cell::from(title),
                        Cell::from(""),
                        Cell::from(""),
                        Cell::from(if track.user_data.is_favorite {
                            "♥".to_string()
                        } else {
                            "".to_string()
                        })
                        .style(Style::default().fg(self.primary_color)),
                        Cell::from(""),
                        Cell::from(""),
                        Cell::from(""),
                        Cell::from(duration),
                    ])
                    .style(Style::default().fg(Color::White))
                    .bold();
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
                    &self.state.tracks_search_term.to_lowercase(),
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
                        Style::default().fg(color).underlined(),
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
                    Cell::from(format!("{}.", track.index_number)).style(
                        if track.id == self.active_song_id {
                            Style::default().fg(color)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                    Cell::from(if all_subsequences.is_empty() {
                        track.name.to_string().into()
                    } else {
                        Line::from(title)
                    }),
                    Cell::from(track.album.clone()),
                    Cell::from(match track.download_status {
                        DownloadStatus::Downloaded => Line::from("⇊"),
                        DownloadStatus::Queued => Line::from("◴"),
                        DownloadStatus::Downloading => Line::from(self.spinner_stages[self.spinner]),
                        DownloadStatus::NotDownloaded => Line::from(""),
                    }),
                    Cell::from(if track.user_data.is_favorite {
                        "♥".to_string()
                    } else {
                        "".to_string()
                    })
                    .style(Style::default().fg(self.primary_color)),
                    Cell::from(format!("{}", track.user_data.play_count)),
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
                    Cell::from(format!(
                        "{}{:02}:{:02}",
                        hours_optional_text, minutes, seconds
                    )),
                ])
                .style(if track.id == self.active_song_id {
                    Style::default().fg(self.primary_color).italic()
                } else {
                    Style::default().fg(Color::White)
                })
            })
            .collect::<Vec<Row>>();

        let track_instructions = Line::from(vec![
            " Help ".white(),
            "<?>".fg(self.primary_color).bold(),
            " Quit ".white(),
            "<Q> ".fg(self.primary_color).bold(),
        ]);

        let widths = [
            Constraint::Length(items.len().to_string().len() as u16 + 1),
            Constraint::Percentage(75), // title and track even width
            Constraint::Percentage(25),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(10),
        ];

        if self.tracks.is_empty() {
            let message_paragraph = Paragraph::new("jellyfin-tui")
                .block(
                    track_block
                        .title("Tracks")
                        .padding(Padding::new(0, 0, center[0].height / 2, 0))
                        .title_bottom(track_instructions.alignment(Alignment::Center)),
                )
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Center);
            frame.render_widget(message_paragraph, center[0]);
            return;
        }

        let items_len = items.len();
        let totaltime = self
            .tracks
            .iter()
            .filter(|t| !t.id.starts_with("_album_"))
            .map(|t| t.run_time_ticks / 10_000_000)
            .sum::<u64>();
        let seconds = totaltime % 60;
        let minutes = (totaltime / 60) % 60;
        let hours = totaltime / 60 / 60;
        let hours_optional_text = match hours {
            0 => String::from(""),
            _ => format!("{}:", hours),
        };
        let duration = format!("{}{:02}:{:02}", hours_optional_text, minutes, seconds);
        let table = Table::new(items, widths)
            .block(
                if self.state.tracks_search_term.is_empty()
                    && !self.state.current_artist.name.is_empty()
                {
                    track_block
                        .title(format!("{}", self.state.current_artist.name))
                        .title_top(
                            Line::from(format!(
                                "({} tracks - {})",
                                self.tracks
                                    .iter()
                                    .filter(|t| !t.id.starts_with("_album_"))
                                    .count(),
                                duration
                            ))
                            .right_aligned(),
                        )
                        .title_bottom(track_instructions.alignment(Alignment::Center))
                } else {
                    track_block
                        .title(format!("Matching: {}", self.state.tracks_search_term))
                        .title_top(Line::from(format!("({} tracks)", items_len)).right_aligned())
                        .title_bottom(track_instructions.alignment(Alignment::Center))
                },
            )
            .row_highlight_style(track_highlight_style)
            .highlight_symbol(">>")
            .style(Style::default().bg(Color::Reset))
            .header(
                Row::new(vec![
                    "#", "Title", "Album", "⇊", "♥", "Plays", "Disc", "Lrc", "Duration",
                ])
                .style(Style::new().bold().white())
                .bottom_margin(0),
            );

        frame.render_widget(Clear, center[0]);
        frame.render_stateful_widget(table, center[0], &mut self.state.selected_track);
    }

    fn render_album_tracks_table(
        &mut self,
        frame: &mut Frame,
        center: &std::rc::Rc<[Rect]>,
        track_block: Block,
        track_highlight_style: Style,
    ) {
        let tracks = search_results(
            &self.album_tracks,
            &self.state.album_tracks_search_term,
            true,
        )
        .iter()
        .map(|id| self.album_tracks.iter().find(|t| t.id == *id).unwrap())
        .collect::<Vec<&DiscographySong>>();

        let terminal_height = frame.area().height as usize;
        let selection = self.state.selected_album_track.selected().unwrap_or(0);

        let items = tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                if i < selection.saturating_sub(terminal_height)
                    || i > selection + terminal_height
                {
                    return Row::default();
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
                    &self.state.album_tracks_search_term.to_lowercase(),
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
                        Style::default().fg(color).underlined(),
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
                    Cell::from(format!("{}.", track.index_number)).style(
                        if track.id == self.active_song_id {
                            Style::default().fg(color)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                    Cell::from(if all_subsequences.is_empty() {
                        track.name.to_string().into()
                    } else {
                        Line::from(title)
                    }),
                    Cell::from(if track.user_data.is_favorite {
                        "♥".to_string()
                    } else {
                        "".to_string()
                    })
                    .style(Style::default().fg(self.primary_color)),
                    Cell::from(format!("{}", track.user_data.play_count)),
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
                    Cell::from(format!(
                        "{}{:02}:{:02}",
                        hours_optional_text, minutes, seconds
                    )),
                ])
                .style(if track.id == self.active_song_id {
                    Style::default().fg(self.primary_color).italic()
                } else {
                    Style::default().fg(Color::White)
                })
            })
            .collect::<Vec<Row>>();

        let track_instructions = Line::from(vec![
            " Help ".white(),
            "<?>".fg(self.primary_color).bold(),
            " Quit ".white(),
            "<Q> ".fg(self.primary_color).bold(),
        ]);

        let widths = [
            Constraint::Length(items.len().to_string().len() as u16 + 1),
            Constraint::Percentage(100), // title and track even width
            Constraint::Length(2),
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(10),
        ];

        if self.album_tracks.is_empty() {
            let message_paragraph = Paragraph::new("jellyfin-tui")
                .block(
                    track_block
                        .title("Tracks")
                        .padding(Padding::new(0, 0, center[0].height / 2, 0))
                        .title_bottom(track_instructions.alignment(Alignment::Center)),
                )
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Center);
            frame.render_widget(message_paragraph, center[0]);
            return;
        }

        let items_len = items.len();
        let totaltime = self
            .album_tracks
            .iter()
            .map(|t| t.run_time_ticks)
            .sum::<u64>()
            / 10_000_000;
        let seconds = totaltime % 60;
        let minutes = (totaltime / 60) % 60;
        let hours = totaltime / 60 / 60;
        let hours_optional_text = match hours {
            0 => String::from(""),
            _ => format!("{}:", hours),
        };
        let duration = format!("{}{:02}:{:02}", hours_optional_text, minutes, seconds);
        let table = Table::new(items, widths)
            .block(
                if self.state.album_tracks_search_term.is_empty()
                    && !self.state.current_album.name.is_empty()
                {
                    track_block
                        .title(format!("{} ({})", self.state.current_album.name, self.state.current_album.album_artists.iter().map(|a| a.name.as_str()).collect::<Vec<&str>>().join(", ")))
                        .title_top(
                            Line::from(format!(
                                "({} tracks - {})",
                                self.album_tracks
                                    .iter()
                                    .filter(|t| !t.id.starts_with("_album_"))
                                    .count(),
                                duration
                            ))
                            .right_aligned(),
                        )
                        .title_bottom(track_instructions.alignment(Alignment::Center))
                } else {
                    track_block
                        .title(format!("Matching: {}", self.state.album_tracks_search_term))
                        .title_top(Line::from(format!("({} tracks)", items_len)).right_aligned())
                        .title_bottom(track_instructions.alignment(Alignment::Center))
                },
            )
            .row_highlight_style(track_highlight_style)
            .highlight_symbol(">>")
            .style(Style::default().bg(Color::Reset))
            .header(
                Row::new(vec![
                    "#", "Title", "♥", "Plays", "Disc", "Lyr", "Duration",
                ])
                .style(Style::new().bold().white())
                .bottom_margin(0),
            );

        frame.render_widget(Clear, center[0]);
        frame.render_stateful_widget(table, center[0], &mut self.state.selected_album_track);
    }

    pub fn render_player(&mut self, frame: &mut Frame, center: &std::rc::Rc<[Rect]>) {
        let current_song = match self
            .state
            .queue
            .get(self.state.current_playback_state.current_index as usize)
        {
            Some(song) => {
                let str = format!("{} - {} - {}", song.name, song.artist, song.album);
                if song.production_year > 0 {
                    format!("{} ({})", str, song.production_year)
                } else {
                    str
                }
            }
            None => String::from("No track playing"),
        };

        let bottom = Block::default()
            .borders(Borders::ALL)
            .fg(Color::White)
            .padding(Padding::new(0, 0, 0, 0));

        let inner = bottom.inner(center[1]);
        frame.render_widget(bottom, center[1]);

        // split the bottom into two parts
        let bottom_split = Layout::default()
            .flex(Flex::SpaceAround)
            .direction(Direction::Horizontal)
            .constraints(if self.cover_art.is_some() && !self.state.large_art {
                vec![
                    Constraint::Percentage(2),
                    Constraint::Length((center[1].height) * 2 + 1),
                    Constraint::Percentage(0),
                    Constraint::Percentage(93),
                    Constraint::Percentage(2),
                ]
            } else {
                vec![
                    Constraint::Percentage(2),
                    Constraint::Percentage(0),
                    Constraint::Percentage(0),
                    Constraint::Percentage(93),
                    Constraint::Percentage(2),
                ]
            })
            .split(inner);

        if self.cover_art.is_some() && !self.state.large_art {
            let image = StatefulImage::default();
            frame.render_stateful_widget(image, bottom_split[1], self.cover_art.as_mut().unwrap());
        }

        let duration = match self.state.current_playback_state.duration {
            0.0 => "0:00 / 0:00".to_string(),
            _ => {
                let current_time = self.state.current_playback_state.duration
                    * self.state.current_playback_state.percentage
                    / 100.0;
                let total_seconds = self.state.current_playback_state.duration;
                let duration = format!(
                    "{}:{:02} / {}:{:02}",
                    current_time as u32 / 60,
                    current_time as u32 % 60,
                    total_seconds as u32 / 60,
                    total_seconds as u32 % 60
                );
                duration
            }
        };

        let layout = Layout::vertical(vec![Constraint::Length(3), Constraint::Length(3)])
            .split(bottom_split[3]);

        // current song
        frame.render_widget(
            Paragraph::new(current_song)
                .block(
                    Block::bordered()
                        .borders(Borders::NONE)
                        .padding(Padding::new(0, 0, 1, 0)),
                )
                .style(Style::default().fg(Color::White)),
            layout[0],
        );

        let progress_bar_area = Layout::default()
            .direction(Direction::Horizontal)
            .flex(Flex::Center)
            .constraints(vec![
                Constraint::Fill(100),
                Constraint::Min(duration.len() as u16 + 5),
            ])
            .split(layout[1]);

        frame.render_widget(
            LineGauge::default()
                .block(Block::bordered().borders(Borders::NONE))
                .filled_style(if self.buffering {
                    Style::default()
                        .fg(self.primary_color)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                })
                .unfilled_style(
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
                .style(Style::default().fg(Color::White))
                .line_set(symbols::line::ROUNDED)
                .ratio(self.state.current_playback_state.percentage / 100_f64)
                .label(Line::from(format!(
                    "{}   {:.0}% ",
                    if self.buffering {
                        self.spinner_stages[self.spinner]
                    } else if self.paused {
                        "⏸︎"
                    } else {
                        "►"
                    },
                    self.state.current_playback_state.percentage
                ))),
            progress_bar_area[0],
        );

        let metadata = match self.metadata {
            Some(ref metadata) => {
                let mut transcoding_text = String::from("");
                let current_song = self
                    .state
                    .queue
                    .get(self.state.current_playback_state.current_index as usize);
                if let Some(song) = current_song {
                    if song.is_transcoded {
                        transcoding_text =
                            format!("- {} kbps [transcoding]", metadata.bit_rate / 1000);
                    } else {
                        transcoding_text = format!("- {} kbps", metadata.bit_rate / 1000);
                    }
                    if song.url.contains("jellyfin-tui/downloads") {
                        transcoding_text += " [local]";
                    }
                }
                let ret = format!(
                    "{} - {} Hz - {} channels {}",
                    // metadata.codec.as_str(),
                    self.state.current_playback_state.file_format,
                    metadata.sample_rate,
                    metadata.channels,
                    transcoding_text
                );
                ret
            }
            None => String::from("No metadata available"),
        };

        frame.render_widget(
            Paragraph::new(metadata).centered().block(
                Block::bordered()
                    .borders(Borders::NONE)
                    .padding(Padding::new(0, 0, 1, 0)),
            ),
            if self.state.large_art { layout[1] } else { progress_bar_area[0] },
        );

        frame.render_widget(
            Paragraph::new(duration)
                .centered()
                .block(
                    Block::bordered()
                        .borders(Borders::NONE)
                        .padding(Padding::ZERO),
                )
                .style(Style::default().fg(Color::White)),
            progress_bar_area[1],
        );
    }

    // pub fn centered_rect(&self, r: Rect, percent_x: u16, percent_y: u16) -> Rect {
    //     let popup_layout = Layout::default()
    //       .direction(Direction::Vertical)
    //       .constraints([
    //         Constraint::Percentage((100 - percent_y) / 2),
    //         Constraint::Percentage(percent_y),
    //         Constraint::Percentage((100 - percent_y) / 2),
    //       ])
    //       .split(r);

    //     Layout::default()
    //       .direction(Direction::Horizontal)
    //       .constraints([
    //         Constraint::Percentage((100 - percent_x) / 2),
    //         Constraint::Percentage(percent_x),
    //         Constraint::Percentage((100 - percent_x) / 2),
    //       ])
    //       .split(popup_layout[1])[1]
    // }
}
