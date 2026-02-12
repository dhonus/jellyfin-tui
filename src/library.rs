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

use crate::database::extension::DownloadStatus;
use crate::tui::App;
use crate::{helpers, keyboard::*};

use crate::config::LyricsVisibility;
use crate::helpers::format_release_date;
use layout::Flex;
use ratatui::{
    prelude::*,
    widgets::*,
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use ratatui_image::{Resize, StatefulImage};

impl App {
    pub fn render_home(&mut self, app_container: Rect, frame: &mut Frame) {
        let outer_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(self.preferences.constraint_width_percentages_music.0),
                Constraint::Percentage(self.preferences.constraint_width_percentages_music.1),
                Constraint::Percentage(self.preferences.constraint_width_percentages_music.2),
            ])
            .split(app_container);

        // create a wrapper, to get the width. After that create the inner 'left' and split it
        let center = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Percentage(100),
                Constraint::Length(if self.preferences.large_art { 7 } else { 8 }),
            ])
            .split(outer_layout[1]);

        let has_lyrics = self.lyrics.as_ref().is_some_and(|(_, l, _)| !l.is_empty());

        let show_panel = match self.lyrics_visibility {
            LyricsVisibility::Auto => has_lyrics,
            LyricsVisibility::Always => true,
            LyricsVisibility::Never => false,
        };

        let lyrics_slot_constraints = if show_panel {
            if has_lyrics && !self.lyrics.as_ref().map_or(true, |(_, l, _)| l.len() == 1) {
                vec![
                    Constraint::Percentage(68),
                    Constraint::Percentage(32),
                    Constraint::Min(if self.download_item.is_some() { 3 } else { 0 }),
                ]
            } else {
                vec![
                    Constraint::Min(3),
                    Constraint::Percentage(100),
                    Constraint::Min(if self.download_item.is_some() { 3 } else { 0 }),
                ]
            }
        } else {
            vec![
                Constraint::Min(0),
                Constraint::Percentage(100),
                Constraint::Min(if self.download_item.is_some() { 3 } else { 0 }),
            ]
        };

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints(lyrics_slot_constraints)
            .split(outer_layout[2]);

        self.render_library_left(frame, outer_layout);
        self.render_library_center(frame, &center);
        self.render_player(frame, &center);
        self.render_library_right(frame, right);
        self.create_popup(frame);
    }

    fn render_library_left(&mut self, frame: &mut Frame, outer_layout: std::rc::Rc<[Rect]>) {
        // LEFT sidebar construct. large_art flag determines the split
        let left = if self.preferences.large_art {
            if let Some(cover_art) = self.cover_art.as_mut() {
                let outer_area = outer_layout[0];
                let block = Block::default()
                    .borders(Borders::ALL)
                    .title(Line::from("Artwork").fg(self.theme.resolve(&self.theme.section_title)))
                    .border_type(self.border_type)
                    .border_style(self.theme.resolve(&self.theme.border));

                let chunk_area = block.inner(outer_area);
                let img_area = cover_art.size_for(Resize::Scale(None), chunk_area);

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

                let inner_area = layout[1].inner(Margin { vertical: 1, horizontal: 1 });

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
                .border_style(self.theme.resolve(&self.theme.border_focused)),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(self.theme.resolve(&self.theme.border)),
        }
        .border_type(self.border_type);

        let selected_artist = self.get_id_of_selected(&self.artists, Selectable::Artist);

        let mut artist_highlight_style = match self.state.active_section {
            ActiveSection::List => Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(self.theme.resolve(&self.theme.selected_active_background))
                .fg(self.theme.resolve(&self.theme.selected_active_foreground)),
            _ => Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(self.theme.resolve(&self.theme.selected_inactive_background))
                .fg(self.theme.resolve(&self.theme.selected_inactive_foreground)),
        };

        if let Some(song) = self.state.queue.get(self.state.current_playback_state.current_index) {
            if song.artist_items.iter().any(|a| a.id == selected_artist) {
                artist_highlight_style = artist_highlight_style.add_modifier(Modifier::ITALIC);
            }
        }

        let artists = search_ranked_refs(&self.artists, &self.state.artists_search_term, true);

        let terminal_height = frame.area().height as usize;
        let selection = self.state.selected_artist.selected().unwrap_or(0);

        // dynamic pageup/down height calc
        let playlist_block_inner_h = artist_block.inner(left[0]).height as usize;
        self.left_list_height = playlist_block_inner_h.max(1);

        // render all artists as a list here in left[0]
        let items = artists
            .iter()
            .enumerate()
            .map(|(i, artist)| {
                if i < selection.saturating_sub(terminal_height) || i > selection + terminal_height
                {
                    return ListItem::new(Text::raw(""));
                }
                let color = if let Some(song) =
                    self.state.queue.get(self.state.current_playback_state.current_index)
                {
                    if song.artist_items.iter().any(|a| a.id == artist.id)
                        || song.artist_items.iter().any(|a| a.name == artist.name)
                    {
                        self.theme.primary_color
                    } else {
                        self.theme.resolve(&self.theme.foreground)
                    }
                } else {
                    self.theme.resolve(&self.theme.foreground)
                };

                // underline the matching search subsequence ranges
                let mut item = Text::default();
                let mut last_end = 0;

                if artist.user_data.is_favorite {
                    item.push_span(Span::styled(
                        "♥ ",
                        Style::default().fg(self.theme.primary_color),
                    ));
                }

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

                ListItem::new(item)
            })
            .collect::<Vec<ListItem>>();

        let artists_title_color = match self.state.active_section {
            ActiveSection::List => self.theme.resolve(&self.theme.border_focused),
            _ => self.theme.resolve(&self.theme.section_title),
        };

        let items_len = items.len();
        let list = List::new(items)
            .block(if self.state.artists_search_term.is_empty() {
                artist_block
                    .title_alignment(Alignment::Right)
                    .title_top(Line::from("All").fg(artists_title_color).left_aligned())
                    .title_top(
                        Line::from(format!("({} artists)", self.artists.len()))
                            .fg(artists_title_color)
                            .right_aligned(),
                    )
                    .title_position(TitlePosition::Bottom)
            } else {
                artist_block
                    .title_alignment(Alignment::Right)
                    .title_top(
                        Line::from(format!("Matching: {}", self.state.artists_search_term))
                            .fg(artists_title_color)
                            .left_aligned(),
                    )
                    .title_top(
                        Line::from(format!("({} artists)", items_len))
                            .fg(artists_title_color)
                            .right_aligned(),
                    )
                    .title_position(TitlePosition::Bottom)
            })
            .highlight_symbol(">>")
            .highlight_style(artist_highlight_style)
            .scroll_padding(10)
            .repeat_highlight_symbol(true);

        frame.render_stateful_widget(list, left[0], &mut self.state.selected_artist);

        helpers::render_scrollbar(
            frame,
            left[0],
            &mut self.state.artists_scroll_state,
            &self.theme,
        );

        if self.locally_searching && self.state.active_section == ActiveSection::List {
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Searching: {}", self.state.artists_search_term))
                    .border_type(self.border_type)
                    .border_style(self.theme.resolve(&self.theme.border_focused)),
                left[0],
            );
        }
    }

    fn render_library_albums(&mut self, frame: &mut Frame, left: std::rc::Rc<[Rect]>) {
        let album_block = match self.state.active_section {
            ActiveSection::List => Block::new()
                .borders(Borders::ALL)
                .border_style(self.theme.resolve(&self.theme.border_focused)),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(self.theme.resolve(&self.theme.border)),
        }
        .border_type(self.border_type);

        let selected_album = self.get_id_of_selected(&self.albums, Selectable::Album);

        let mut album_highlight_style = match self.state.active_section {
            ActiveSection::List => Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(self.theme.resolve(&self.theme.selected_active_foreground))
                .bg(self.theme.resolve(&self.theme.selected_active_background)),
            _ => Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(self.theme.resolve(&self.theme.selected_inactive_background))
                .fg(self.theme.resolve(&self.theme.selected_inactive_foreground)),
        };

        if let Some(song) = self.state.queue.get(self.state.current_playback_state.current_index) {
            if song.album_id == selected_album {
                album_highlight_style = album_highlight_style.add_modifier(Modifier::ITALIC);
            }
        }

        let albums = search_ranked_refs(&self.albums, &self.state.albums_search_term, true);

        let terminal_height = frame.area().height as usize;
        let selection = self.state.selected_album.selected().unwrap_or(0);
        // dynamic pageup/down height calc
        let playlist_block_inner_h = album_block.inner(left[0]).height as usize;
        self.left_list_height = playlist_block_inner_h.max(1);

        let items = albums
            .iter()
            .enumerate()
            .map(|(i, album)| {
                if i < selection.saturating_sub(terminal_height) || i > selection + terminal_height
                {
                    return ListItem::new(Text::raw(""));
                }

                let color = if let Some(song) =
                    self.state.queue.get(self.state.current_playback_state.current_index)
                {
                    if song.album_id == album.id {
                        self.theme.primary_color
                    } else {
                        self.theme.resolve(&self.theme.foreground)
                    }
                } else {
                    self.theme.resolve(&self.theme.foreground)
                };

                // underline the matching search subsequence ranges
                let mut item = Text::default();
                let mut last_end = 0;

                if album.user_data.is_favorite {
                    item.push_span(Span::styled(
                        "♥ ",
                        Style::default().fg(self.theme.primary_color),
                    ));
                }

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

                item.push_span(Span::styled(
                    format!(
                        " › {}",
                        album
                            .album_artists
                            .iter()
                            .map(|a| a.name.as_str())
                            .collect::<Vec<&str>>()
                            .join(", ")
                    ),
                    Style::default().fg(self.theme.resolve(&self.theme.foreground_dim)),
                ));

                ListItem::new(item)
            })
            .collect::<Vec<ListItem>>();

        let albums_title_color = match self.state.active_section {
            ActiveSection::List => self.theme.resolve(&self.theme.border_focused),
            _ => self.theme.resolve(&self.theme.section_title),
        };

        let items_len = items.len();
        let list = List::new(items)
            .block(if self.state.albums_search_term.is_empty() {
                album_block
                    .title_alignment(Alignment::Right)
                    .title_top(Line::from("All").fg(albums_title_color).left_aligned())
                    .title_top(
                        Line::from(format!("({} albums)", self.albums.len()))
                            .fg(albums_title_color)
                            .right_aligned(),
                    )
                    .title_position(TitlePosition::Bottom)
            } else {
                album_block
                    .title_alignment(Alignment::Right)
                    .title_top(
                        Line::from(format!("Matching: {}", self.state.albums_search_term))
                            .fg(albums_title_color)
                            .left_aligned(),
                    )
                    .title_top(
                        Line::from(format!("({} albums)", items_len))
                            .fg(albums_title_color)
                            .right_aligned(),
                    )
                    .title_position(TitlePosition::Bottom)
            })
            .highlight_symbol(">>")
            .highlight_style(album_highlight_style)
            .scroll_padding(10)
            .repeat_highlight_symbol(true);

        frame.render_stateful_widget(list, left[0], &mut self.state.selected_album);

        helpers::render_scrollbar(frame, left[0], &mut self.state.albums_scroll_state, &self.theme);

        if self.locally_searching && self.state.active_section == ActiveSection::List {
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Searching: {}", self.state.albums_search_term))
                    .border_type(self.border_type)
                    .border_style(self.theme.resolve(&self.theme.border_focused)),
                left[0],
            );
        }
    }

    /// Individual widget rendering functions
    pub fn render_library_right(&mut self, frame: &mut Frame, right: std::rc::Rc<[Rect]>) {
        let has_lyrics = self.lyrics.as_ref().is_some_and(|(_, l, _)| !l.is_empty());

        let show_panel = match self.lyrics_visibility {
            LyricsVisibility::Auto => has_lyrics,
            LyricsVisibility::Always => true,
            LyricsVisibility::Never => false,
        };

        if show_panel {
            let section_title_color = match self.state.active_section {
                ActiveSection::Lyrics => self.theme.resolve(&self.theme.border_focused),
                _ => self.theme.resolve(&self.theme.section_title),
            };
            let lyrics_block = match self.state.active_section {
                ActiveSection::Lyrics => Block::new()
                    .borders(Borders::ALL)
                    .border_style(self.theme.resolve(&self.theme.border_focused)),
                _ => Block::new()
                    .borders(Borders::ALL)
                    .border_style(self.theme.resolve(&self.theme.border)),
            }
            .border_type(self.border_type);

            if !has_lyrics {
                let message_paragraph = Paragraph::new("No lyrics available")
                    .block(
                        lyrics_block
                            .title_alignment(Alignment::Left)
                            .title(Line::from("Lyrics").fg(section_title_color)),
                    )
                    .fg(self.theme.resolve(&self.theme.foreground))
                    .wrap(Wrap { trim: false })
                    .alignment(Alignment::Center);

                frame.render_widget(message_paragraph, right[0]);
            } else if let Some((_, lyrics, time_synced)) = &self.lyrics {
                // this will show the lyrics in a scrolling list
                let items = lyrics
                    .iter()
                    .enumerate()
                    .map(|(index, lyric)| {
                        let mut style = Style::default();
                        if *time_synced {
                            let is_current = index == self.state.current_lyric
                                && Some(index) != self.state.selected_lyric.selected();
                            if is_current {
                                style =
                                    style.fg(self.theme.primary_color).add_modifier(Modifier::BOLD);
                            } else {
                                if index > self.state.current_lyric {
                                    style = style.fg(self.theme.resolve(&self.theme.foreground));
                                } else {
                                    style =
                                        style.fg(self.theme.resolve(&self.theme.foreground_dim));
                                }
                            }
                        } else {
                            style = style.fg(self.theme.resolve(&self.theme.foreground));
                        }

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
                    .block(
                        lyrics_block
                            .title_alignment(Alignment::Left)
                            .title(Line::from("Lyrics").fg(section_title_color)),
                    )
                    .highlight_symbol(">>")
                    .highlight_style(
                        Style::default()
                            .add_modifier(Modifier::BOLD)
                            .bg(self.theme.resolve(&self.theme.selected_active_background))
                            .fg(self.theme.resolve(&self.theme.selected_active_foreground)),
                    )
                    .scroll_padding((right[0].height / 2) as usize)
                    .repeat_highlight_symbol(false);

                frame.render_stateful_widget(list, right[0], &mut self.state.selected_lyric);
            }
        }
        let queue_block = match self.state.active_section {
            ActiveSection::Queue => Block::new()
                .borders(Borders::ALL)
                .border_style(self.theme.resolve(&self.theme.border_focused)),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(self.theme.resolve(&self.theme.border)),
        }
        .border_type(self.border_type);

        let total = self.state.queue.len();
        let height = right[1].height.saturating_sub(2) as usize;

        let current = self.state.current_playback_state.current_index;
        let auto_scroll = self.state.active_section != ActiveSection::Queue;

        let offset = if !auto_scroll {
            self.state.selected_queue_item.offset()
        } else {
            current.saturating_sub(1).min(total.saturating_sub(height))
        };

        let items = self
            .state
            .queue
            .iter()
            .enumerate()
            .map(|(index, song)| {
                let mut text = Text::default();

                if song.is_in_queue {
                    text.push_span(Span::styled(
                        "+ ",
                        Style::default().fg(self.theme.primary_color),
                    ));
                }
                if song.is_favorite {
                    text.push_span(Span::styled(
                        "♥ ",
                        Style::default().fg(self.theme.primary_color),
                    ));
                }

                let queue_focused = self.state.active_section == ActiveSection::Queue;

                let (main_fg, artist_fg) = if queue_focused {
                    (
                        self.theme.resolve(&self.theme.foreground),
                        self.theme.resolve(&self.theme.foreground_dim),
                    )
                } else {
                    // queue NOT focused → dim history
                    if index < current {
                        (
                            self.theme.resolve(&self.theme.foreground_dim),
                            self.theme.resolve(&self.theme.foreground_dim),
                        )
                    } else {
                        (
                            self.theme.resolve(&self.theme.foreground),
                            self.theme.resolve(&self.theme.foreground_dim),
                        )
                    }
                };

                text.push_span(Span::styled(song.name.as_str(), Style::default().fg(main_fg)));

                let artist_list = song
                    .artist_items
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<&str>>()
                    .join(", ");

                text.push_span(Span::styled(
                    format!(" › {}", artist_list),
                    Style::default().fg(artist_fg),
                ));

                ListItem::new(text)
            })
            .collect::<Vec<ListItem>>();

        let queue_title_color = match self.state.active_section {
            ActiveSection::Queue => self.theme.resolve(&self.theme.border_focused),
            _ => self.theme.resolve(&self.theme.section_title),
        };
        let list = List::new(items)
            .block(
                queue_block
                    .title_alignment(Alignment::Right)
                    .title_top(Line::from("Queue").fg(queue_title_color).left_aligned())
                    .title_top(if self.state.queue.is_empty() {
                        Line::from("")
                    } else {
                        Line::from(format!(
                            "({}/{})",
                            self.state.current_playback_state.current_index + 1,
                            self.state.queue.len()
                        ))
                        .fg(queue_title_color)
                        .right_aligned()
                    })
                    .title_position(TitlePosition::Bottom)
                    .title_bottom(if self.state.shuffle {
                        Line::from("(shuffle)").fg(queue_title_color).right_aligned()
                    } else {
                        Line::from("")
                    }),
            )
            .highlight_symbol(">>")
            .highlight_style(
                Style::default()
                    .bold()
                    .fg(self.theme.resolve(&self.theme.selected_active_foreground))
                    .bg(self.theme.resolve(&self.theme.selected_active_background)),
            )
            .repeat_highlight_symbol(true);

        self.state.selected_queue_item = self.state.selected_queue_item.clone().with_offset(offset);

        frame.render_stateful_widget(list, right[1], &mut self.state.selected_queue_item);

        if let Some(download_item) = &self.download_item {
            let progress = (download_item.progress * 100.0).round() / 100.0;
            let progress_text = format!("{:.1}%", progress);

            let p = Paragraph::new(format!(
                "{} {} - {}",
                &self.spinner_stages[self.spinner], progress_text, &download_item.name,
            ))
            .style(Style::default().fg(self.theme.resolve(&self.theme.foreground)))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(
                        Line::from("Downloading").fg(self.theme.resolve(&self.theme.section_title)),
                    )
                    .border_type(self.border_type)
                    .fg(self.theme.resolve(&self.theme.border)),
            );

            frame.render_widget(p, right[2]);
        }
    }

    fn render_library_center(&mut self, frame: &mut Frame, center: &std::rc::Rc<[Rect]>) {
        let track_block = match self.state.active_section {
            ActiveSection::Tracks => Block::new()
                .borders(Borders::ALL)
                .border_style(self.theme.resolve(&self.theme.border_focused)),
            _ => Block::new()
                .borders(Borders::ALL)
                .border_style(self.theme.resolve(&self.theme.border)),
        }
        .border_type(self.border_type);

        // dynamic pageup/down height calc
        let table_block_inner = track_block.inner(center[0]);
        let header_h: u16 = 1;
        let table_body_h = table_block_inner.height.saturating_sub(header_h) as usize;
        self.track_list_height = table_body_h.max(1);

        let current_track = self.state.queue.get(self.state.current_playback_state.current_index);

        let mut track_highlight_style = match self.state.active_section {
            ActiveSection::Tracks => Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(self.theme.resolve(&self.theme.selected_active_foreground))
                .bg(self.theme.resolve(&self.theme.selected_active_background)),
            _ => Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(self.theme.resolve(&self.theme.selected_inactive_background))
                .fg(self.theme.resolve(&self.theme.selected_inactive_foreground)),
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
                " Confirm ".fg(self.theme.resolve(&self.theme.section_title)),
                "<Enter>".fg(self.theme.primary_color).bold(),
                " Clear and keep selection ".fg(self.theme.resolve(&self.theme.section_title)),
                "<Esc> ".fg(self.theme.primary_color).bold(),
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
                        .border_type(self.border_type)
                        .border_style(self.theme.resolve(&self.theme.border_focused)),
                    center[0],
                );
            }
        }

        helpers::render_scrollbar(
            frame,
            center[0],
            if self.state.active_tab == ActiveTab::Library {
                &mut self.state.tracks_scroll_state
            } else {
                &mut self.state.album_tracks_scroll_state
            },
            &self.theme,
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
        let tracks = search_ranked_refs(&self.tracks, &self.state.tracks_search_term, true);

        let show_disc = self
            .tracks
            .iter()
            .filter(|t| !t.id.starts_with("_album_"))
            .any(|t| (if t.parent_index_number > 0 { t.parent_index_number } else { 1 }) != 1);
        let show_lyrics_column = !matches!(self.lyrics_visibility, LyricsVisibility::Never);

        let terminal_height = frame.area().height as usize;
        let selection = self.state.selected_track.selected().unwrap_or(0);

        let items = tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                if i < selection.saturating_sub(terminal_height) || i > selection + terminal_height
                {
                    return Row::default();
                }
                let title_str = track.name.to_string();

                if track.id.starts_with("_album_") {
                    let total_time = track.run_time_ticks / 10_000_000;
                    let seconds = total_time % 60;
                    let minutes = (total_time / 60) % 60;
                    let hours = total_time / 60 / 60;
                    let hours_optional_text =
                        if hours == 0 { String::new() } else { format!("{}:", hours) };
                    let duration = format!("{}{:02}:{:02}", hours_optional_text, minutes, seconds);
                    let album_id = track.id.clone().replace("_album_", "");

                    let (any_queued, any_downloading, any_not_downloaded, all_downloaded) = self
                        .tracks
                        .iter()
                        .filter(|t| t.album_id == album_id)
                        .fold((false, false, false, true), |(aq, ad, and, all), track| {
                            (
                                aq || matches!(track.download_status, DownloadStatus::Queued),
                                ad || matches!(track.download_status, DownloadStatus::Downloading),
                                and || matches!(
                                    track.download_status,
                                    DownloadStatus::NotDownloaded
                                ),
                                all && matches!(track.download_status, DownloadStatus::Downloaded),
                            )
                        });

                    let download_status =
                        match (any_queued, any_downloading, all_downloaded, any_not_downloaded) {
                            (_, true, _, false) => self.spinner_stages[self.spinner],
                            (true, _, _, false) => "◴",
                            (_, _, true, false) => "⇊",
                            _ => "",
                        };

                    // this is the dummy that symbolizes the name of the album
                    let mut cells = vec![
                        Cell::from(format!("{}", track.production_year)).style(
                            Style::default()
                                .fg(self.theme.resolve(&self.theme.album_header_foreground)),
                        ),
                        Cell::from(title_str)
                            .fg(self.theme.resolve(&self.theme.album_header_foreground)),
                        Cell::from(""), // Album
                    ];
                    if show_disc {
                        cells.push(Cell::from(""));
                    }
                    cells.push(Cell::from(download_status));
                    cells.push(
                        Cell::from(if track.user_data.is_favorite { "♥" } else { "" })
                            .style(Style::default().fg(self.theme.primary_color)),
                    );
                    if show_lyrics_column {
                        cells.push(Cell::from(""));
                    }
                    cells.push(Cell::from("")); // Plays
                    cells.push(Cell::from(duration));

                    let mut row = Row::new(cells)
                        .style(
                            Style::default()
                                .fg(self.theme.resolve(&self.theme.album_header_foreground)),
                        )
                        .bold();
                    if let Some(album_header_background) =
                        self.theme.resolve_opt(&self.theme.album_header_background)
                    {
                        row = row.bg(album_header_background);
                    }
                    return row;
                }

                // track.run_time_ticks is in microseconds
                let seconds = (track.run_time_ticks / 10_000_000) % 60;
                let minutes = (track.run_time_ticks / 10_000_000 / 60) % 60;
                let hours = (track.run_time_ticks / 10_000_000 / 60) / 60;
                let hours_optional_text =
                    if hours == 0 { String::new() } else { format!("{}:", hours) };

                let all_subsequences = helpers::find_all_subsequences(
                    &self.state.tracks_search_term.to_lowercase(),
                    &track.name.to_lowercase(),
                );

                let mut title = vec![];
                let mut last_end = 0;
                let color = if track.id == self.active_song_id {
                    self.theme.primary_color
                } else if track.disliked {
                    self.theme.resolve(&self.theme.foreground_dim)
                } else {
                    self.theme.resolve(&self.theme.foreground)
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
                    title.push(Span::styled(&track.name[last_end..], Style::default().fg(color)));
                }

                let mut cells: Vec<Cell> = vec![
                    Cell::from(format!("{}.", track.index_number)).style(
                        if track.id == self.active_song_id {
                            Style::default().fg(color)
                        } else {
                            Style::default().fg(self.theme.resolve(&self.theme.foreground_dim))
                        },
                    ),
                    Cell::from(if all_subsequences.is_empty() {
                        title_str.into()
                    } else {
                        Line::from(title)
                    }),
                    Cell::from(track.album.clone()),
                ];

                if show_disc {
                    cells.push(Cell::from(if track.parent_index_number > 0 {
                        format!("{}", track.parent_index_number)
                    } else {
                        String::from("1")
                    }));
                }

                // ⇊ (download)
                cells.push(Cell::from(match track.download_status {
                    DownloadStatus::Downloaded => Line::from("⇊"),
                    DownloadStatus::Queued => Line::from("◴"),
                    DownloadStatus::Downloading => Line::from(self.spinner_stages[self.spinner]),
                    DownloadStatus::NotDownloaded => Line::from(""),
                }));

                // ♥ (favorite)
                cells.push(
                    Cell::from(if track.user_data.is_favorite { "♥" } else { "" })
                        .style(Style::default().fg(self.theme.primary_color)),
                );

                // ♪
                if show_lyrics_column {
                    cells.push(Cell::from(if track.has_lyrics { "♪" } else { "" }));
                }

                // plays
                cells.push(Cell::from(format!("{}", track.user_data.play_count)));

                // duration
                cells.push(Cell::from(format!(
                    "{}{:02}:{:02}",
                    hours_optional_text, minutes, seconds
                )));

                let style = if track.id == self.active_song_id {
                    Style::default().fg(self.theme.primary_color).italic()
                } else if track.disliked {
                    Style::default().fg(self.theme.resolve(&self.theme.foreground_dim))
                } else {
                    Style::default().fg(self.theme.resolve(&self.theme.foreground))
                };

                Row::new(cells).style(style)
            })
            .collect::<Vec<Row>>();

        let track_instructions = Line::from(vec![
            " Help ".fg(self.theme.resolve(&self.theme.section_title)),
            "<?>".fg(self.theme.primary_color).bold(),
            " Quit ".fg(self.theme.resolve(&self.theme.section_title)),
            "<^C> ".fg(self.theme.primary_color).bold(),
        ]);

        let mut widths: Vec<Constraint> = vec![
            Constraint::Length(4),
            Constraint::Percentage(70), // Title
            Constraint::Percentage(30), // Album
        ];
        if show_disc {
            widths.push(Constraint::Length(1));
        }
        widths.push(Constraint::Length(1)); // ⇊
        widths.push(Constraint::Length(1)); // ♥
        if show_lyrics_column {
            widths.push(Constraint::Length(1)); // ♪
        }
        widths.push(Constraint::Length(5)); // Plays
        widths.push(Constraint::Length(10)); // Duration

        let section_title_color = match self.state.active_section {
            ActiveSection::Tracks => self.theme.resolve(&self.theme.border_focused),
            _ => self.theme.resolve(&self.theme.section_title),
        };

        if self.tracks.is_empty() {
            let message_paragraph = Paragraph::new("jellyfin-tui")
                .block(
                    track_block
                        .title(Line::from("Tracks").fg(section_title_color))
                        .fg(self.theme.resolve(&self.theme.border))
                        .border_type(self.border_type)
                        .padding(Padding::new(0, 0, center[0].height / 2, 0))
                        .title_bottom(track_instructions.alignment(Alignment::Center)),
                )
                .fg(self.theme.resolve(&self.theme.foreground))
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
        let hours_optional_text = if hours == 0 { String::new() } else { format!("{}:", hours) };
        let duration = format!("{}{:02}:{:02}", hours_optional_text, minutes, seconds);

        let selected_is_album =
            tracks.get(selection).map_or(false, |t| t.id.starts_with("_album_"));

        let mut header_cells: Vec<&str> =
            vec![if selected_is_album { "Yr." } else { "No." }, "Title", "Album"];
        if show_disc {
            header_cells.push("○");
        }
        header_cells.push("⇊");
        header_cells.push("♥");
        if show_lyrics_column {
            header_cells.push("♪");
        }
        header_cells.push("Plays");
        header_cells.push("Duration");

        let table = Table::new(items, widths)
            .block(
                if self.state.tracks_search_term.is_empty()
                    && !self.state.current_artist.name.is_empty()
                {
                    track_block
                        .title(
                            Line::from(format!(
                                "{}{}",
                                self.state.current_artist.name,
                                if self.discography_stale {
                                    format!(" {}", &self.spinner_stages[self.spinner])
                                } else {
                                    String::new()
                                }
                            ))
                            .fg(section_title_color),
                        )
                        .title_top(
                            Line::from(format!(
                                "({} tracks - {})",
                                self.tracks.iter().filter(|t| !t.id.starts_with("_album_")).count(),
                                duration
                            ))
                            .fg(section_title_color)
                            .right_aligned(),
                        )
                        .title_bottom(track_instructions.centered())
                } else {
                    track_block
                        .title(
                            Line::from(format!("Matching: {}", self.state.tracks_search_term))
                                .fg(section_title_color),
                        )
                        .title_top(
                            Line::from(format!("({} tracks)", items_len))
                                .fg(section_title_color)
                                .right_aligned(),
                        )
                        .title_bottom(track_instructions.alignment(Alignment::Center))
                },
            )
            .row_highlight_style(track_highlight_style)
            .highlight_symbol(">>")
            .style(
                Style::default()
                    .bg(self.theme.resolve_opt(&self.theme.background).unwrap_or(Color::Reset)),
            )
            .header(
                Row::new(header_cells)
                    .style(Style::new().bold().fg(self.theme.resolve(&self.theme.foreground)))
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
        let tracks =
            search_ranked_refs(&self.album_tracks, &self.state.album_tracks_search_term, true);

        let show_disc = self.album_tracks.iter().any(|t| t.parent_index_number > 1);
        let show_lyrics_column = !matches!(self.lyrics_visibility, LyricsVisibility::Never);

        let terminal_height = frame.area().height as usize;
        let selection = self.state.selected_album_track.selected().unwrap_or(0);

        let items = tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                if i < selection.saturating_sub(terminal_height) || i > selection + terminal_height
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
                    self.theme.primary_color
                } else if track.disliked {
                    self.theme.resolve(&self.theme.foreground_dim)
                } else {
                    self.theme.resolve(&self.theme.foreground)
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
                    title.push(Span::styled(&track.name[last_end..], Style::default().fg(color)));
                }

                let mut cells: Vec<Cell> = vec![
                    Cell::from(format!("{}.", track.index_number)).style(
                        if track.id == self.active_song_id {
                            Style::default().fg(color)
                        } else {
                            Style::default().fg(self.theme.resolve(&self.theme.foreground_dim))
                        },
                    ),
                    Cell::from(if all_subsequences.is_empty() {
                        track.name.to_string().into()
                    } else {
                        Line::from(title)
                    }),
                ];

                if show_disc {
                    cells.push(Cell::from(if track.parent_index_number > 0 {
                        format!("{}", track.parent_index_number)
                    } else {
                        String::from("1")
                    }));
                }

                // ⇊
                cells.push(Cell::from(match track.download_status {
                    DownloadStatus::Downloaded => Line::from("⇊"),
                    DownloadStatus::Queued => Line::from("◴"),
                    DownloadStatus::Downloading => Line::from(self.spinner_stages[self.spinner]),
                    DownloadStatus::NotDownloaded => Line::from(""),
                }));

                // ♥
                cells.push(
                    Cell::from(if track.user_data.is_favorite { "♥" } else { "" })
                        .style(Style::default().fg(self.theme.primary_color)),
                );

                // ♪
                if show_lyrics_column {
                    cells.push(Cell::from(if track.has_lyrics { "♪" } else { "" }));
                }

                // plays
                cells.push(Cell::from(format!("{}", track.user_data.play_count)));

                // duration
                cells.push(Cell::from(format!(
                    "{}{:02}:{:02}",
                    hours_optional_text, minutes, seconds
                )));

                Row::new(cells).style(if track.id == self.active_song_id {
                    Style::default().fg(self.theme.primary_color).italic()
                } else if track.disliked {
                    Style::default().fg(self.theme.resolve(&self.theme.foreground_dim))
                } else {
                    Style::default().fg(self.theme.resolve(&self.theme.foreground))
                })
            })
            .collect::<Vec<Row>>();

        let track_instructions = Line::from(vec![
            " Help ".fg(self.theme.resolve(&self.theme.section_title)),
            "<?>".fg(self.theme.primary_color).bold(),
            " Quit ".fg(self.theme.resolve(&self.theme.section_title)),
            "<^C> ".fg(self.theme.primary_color).bold(),
        ]);

        let mut widths: Vec<Constraint> = vec![
            Constraint::Length(items.len().to_string().len() as u16 + 2),
            Constraint::Percentage(100),
        ];
        if show_disc {
            widths.push(Constraint::Length(1));
        }
        widths.push(Constraint::Length(1)); // ⇊
        widths.push(Constraint::Length(1)); // ♥
        if show_lyrics_column {
            widths.push(Constraint::Length(1)); // ♪
        }
        widths.push(Constraint::Length(5)); // Plays
        widths.push(Constraint::Length(10)); // Duration

        let section_title_color = match self.state.active_section {
            ActiveSection::Tracks => self.theme.resolve(&self.theme.border_focused),
            _ => self.theme.resolve(&self.theme.section_title),
        };

        if self.album_tracks.is_empty() {
            let message_paragraph = Paragraph::new("jellyfin-tui")
                .block(
                    track_block
                        .title(Line::from("Tracks").fg(section_title_color))
                        .fg(self.theme.resolve(&self.theme.border))
                        .border_type(self.border_type)
                        .padding(Padding::new(0, 0, center[0].height / 2, 0))
                        .title_bottom(track_instructions.alignment(Alignment::Center)),
                )
                .fg(self.theme.resolve(&self.theme.foreground))
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Center);
            frame.render_widget(message_paragraph, center[0]);
            return;
        }

        let items_len = items.len();
        let totaltime =
            self.album_tracks.iter().map(|t| t.run_time_ticks).sum::<u64>() / 10_000_000;
        let seconds = totaltime % 60;
        let minutes = (totaltime / 60) % 60;
        let hours = totaltime / 60 / 60;
        let hours_optional_text = match hours {
            0 => String::from(""),
            _ => format!("{}:", hours),
        };
        let duration = format!("{}{:02}:{:02}", hours_optional_text, minutes, seconds);

        let mut header_cells: Vec<&str> = vec!["No.", "Title"];
        if show_disc {
            header_cells.push("○");
        }
        header_cells.push("⇊");
        header_cells.push("♥");
        if show_lyrics_column {
            header_cells.push("♪");
        }
        header_cells.push("Plays");
        header_cells.push("Duration");

        let release = format_release_date(&self.state.current_album.premiere_date)
            .unwrap_or_else(|| "".to_string());

        let table = Table::new(items, widths)
            .block(
                if self.state.album_tracks_search_term.is_empty()
                    && !self.state.current_album.name.is_empty()
                {
                    track_block
                        .title(
                            Line::from(format!(
                                "{} - {}{}",
                                self.state.current_album.name,
                                self.state
                                    .current_album
                                    .album_artists
                                    .iter()
                                    .map(|a| a.name.as_str())
                                    .collect::<Vec<&str>>()
                                    .join(", "),
                                release
                            ))
                            .fg(section_title_color),
                        )
                        .title_top(
                            Line::from(format!(
                                "({} tracks - {})",
                                self.album_tracks
                                    .iter()
                                    .filter(|t| !t.id.starts_with("_album_"))
                                    .count(),
                                duration
                            ))
                            .fg(section_title_color)
                            .right_aligned(),
                        )
                        .title_bottom(track_instructions.alignment(Alignment::Center))
                } else {
                    track_block
                        .title(
                            Line::from(format!(
                                "Matching: {}",
                                self.state.album_tracks_search_term
                            ))
                            .fg(section_title_color),
                        )
                        .title_top(
                            Line::from(format!("({} tracks)", items_len))
                                .fg(section_title_color)
                                .right_aligned(),
                        )
                        .title_bottom(track_instructions.alignment(Alignment::Center))
                },
            )
            .row_highlight_style(track_highlight_style)
            .highlight_symbol(">>")
            .style(
                Style::default()
                    .bg(self.theme.resolve_opt(&self.theme.background).unwrap_or(Color::Reset)),
            )
            .header(
                Row::new(header_cells)
                    .style(Style::new().bold().fg(self.theme.resolve(&self.theme.foreground)))
                    .bottom_margin(0),
            );

        frame.render_widget(Clear, center[0]);
        frame.render_stateful_widget(table, center[0], &mut self.state.selected_album_track);
    }

    pub fn render_player(&mut self, frame: &mut Frame, center: &std::rc::Rc<[Rect]>) {
        let current_song = self.state.queue.get(self.state.current_playback_state.current_index);

        let metadata_spans: Vec<Span> = current_song
            .map(|song| {
                if self.state.current_playback_state.audio_samplerate == 0
                    && self.state.current_playback_state.hr_channels.is_empty()
                {
                    return vec![Span::styled(
                        format!("{} Loading metadata", self.spinner_stages[self.spinner]),
                        Style::default().fg(self.theme.resolve(&self.theme.foreground)),
                    )];
                }

                let sep = |s: &str| {
                    Span::styled(
                        format!(" {} ", s),
                        Style::default().fg(self.theme.resolve(&self.theme.foreground_dim)),
                    )
                };

                let fg = self.theme.resolve(&self.theme.foreground);
                let sr = self.state.current_playback_state.audio_samplerate as f32;
                let khz = sr / 1000.0;

                let samplerate = if khz.fract() == 0.0 {
                    format!("{} kHz", khz as u32)
                } else {
                    format!("{:.1} kHz", khz)
                };

                let mut out = vec![
                    Span::styled(
                        &self.state.current_playback_state.file_format,
                        Style::default().fg(fg),
                    ),
                    sep("-"),
                    Span::styled(samplerate, Style::default().fg(fg)),
                    sep("-"),
                    Span::styled(
                        &self.state.current_playback_state.hr_channels,
                        Style::default().fg(fg),
                    ),
                    sep("-"),
                    Span::styled(
                        format!("{} kbps", self.state.current_playback_state.audio_bitrate),
                        Style::default().fg(fg),
                    ),
                ];

                let mut flags = Vec::new();

                if song.is_transcoded {
                    flags.push("tc");
                }
                if song.url.contains("jellyfin-tui/downloads") {
                    flags.push("⇊");
                }

                if !flags.is_empty() {
                    out.push(Span::styled(
                        " › ",
                        Style::default().fg(fg).add_modifier(Modifier::DIM),
                    ));

                    out.push(Span::styled(flags.join(" "), Style::default().fg(fg)));
                }
                out
            })
            .unwrap_or_else(|| {
                vec![Span::styled(
                    "No track playing",
                    Style::default().fg(self.theme.resolve(&self.theme.foreground)),
                )]
            });

        let bottom = Block::default()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .fg(self.theme.resolve(&self.theme.border))
            .padding(Padding::new(0, 0, 0, 0));

        let inner = bottom.inner(center[1]);
        frame.render_widget(bottom, center[1]);

        // split the bottom into two parts
        let bottom_split = Layout::default()
            .flex(Flex::SpaceAround)
            .direction(Direction::Horizontal)
            .constraints(if self.cover_art.is_some() && !self.preferences.large_art {
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

        let layout = if self.preferences.large_art {
            Layout::vertical(vec![Constraint::Length(2), Constraint::Length(2)])
        } else {
            Layout::vertical(vec![Constraint::Length(3), Constraint::Length(3)])
        }
        .split(bottom_split[3]);

        let current_track = self.state.queue.get(self.state.current_playback_state.current_index);
        let lines = match current_track {
            Some(song) => {
                let large = self.cover_art.is_some() && self.preferences.large_art;
                let artists = song
                    .artist_items
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<&str>>()
                    .join(", ");

                let mut title = vec![
                    song.name.as_str().fg(self.theme.resolve(&self.theme.foreground)),
                    " — ".fg(self.theme.resolve(&self.theme.foreground_dim)),
                    song.album.as_str().fg(self.theme.resolve(&self.theme.foreground)),
                    if song.production_year > 0 {
                        format!(" ({})", song.production_year)
                            .fg(self.theme.resolve(&self.theme.foreground))
                    } else {
                        Span::default()
                    },
                ];

                if large {
                    if !artists.is_empty() {
                        title.push(Span::styled(
                            " › ",
                            Style::default().fg(self.theme.resolve(&self.theme.foreground_dim)),
                        ));
                        title.push(Span::styled(
                            artists,
                            Style::default()
                                .fg(self.theme.resolve(&self.theme.foreground_secondary)),
                        ));
                    }
                    vec![Line::from(title)]
                } else {
                    let mut lines = vec![Line::from(title)];
                    if !artists.is_empty() {
                        lines.push(Line::from(vec![
                            Span::styled(
                                "› ",
                                Style::default().fg(self.theme.resolve(&self.theme.foreground_dim)),
                            ),
                            Span::styled(
                                artists,
                                Style::default()
                                    .fg(self.theme.resolve(&self.theme.foreground_secondary)),
                            ),
                        ]));
                    }
                    lines
                }
            }
            None => {
                vec![Line::from("No track playing").fg(self.theme.resolve(&self.theme.foreground))]
            }
        };

        if self.cover_art.is_some() && !self.preferences.large_art {
            let image = StatefulImage::default();
            frame.render_stateful_widget(image, bottom_split[1], self.cover_art.as_mut().unwrap());
        }

        let total_seconds = current_track
            .map(|s| s.run_time_ticks as f64 / 10_000_000.0)
            .unwrap_or(self.state.current_playback_state.duration);
        let duration = match total_seconds {
            0.0 => "0:00 / 0:00".to_string(),
            _ => {
                let current_time = self.state.current_playback_state.position;
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

        // current song
        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::bordered().borders(Borders::NONE).padding(Padding::new(0, 0, 1, 0)))
                .left_aligned()
                .style(Style::default().fg(self.theme.resolve(&self.theme.foreground))),
            layout[0],
        );

        let progress_bar_area = Layout::default()
            .direction(Direction::Horizontal)
            .flex(Flex::Center)
            .constraints(vec![Constraint::Fill(100), Constraint::Min(duration.len() as u16 + 5)])
            .split(layout[1]);

        let visible_position = if self.state.current_playback_state.seek_active {
            match self.hard_seek_target {
                Some(position) => position,
                _ => self.state.current_playback_state.position,
            }
        } else {
            self.state.current_playback_state.position
        };
        let percentage =
            if total_seconds > 0.0 { (visible_position / total_seconds) * 100.0 } else { 0.0 };

        frame.render_widget(
            LineGauge::default()
                .block(Block::bordered().borders(Borders::NONE))
                .filled_style(if self.buffering {
                    Style::default().fg(self.theme.primary_color).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(self.theme.resolve(&self.theme.progress_fill))
                        .add_modifier(Modifier::BOLD)
                })
                .unfilled_style(
                    Style::default()
                        .fg(self.theme.resolve(&self.theme.progress_track))
                        .add_modifier(Modifier::BOLD),
                )
                .style(Style::default().fg(self.theme.resolve(&self.theme.foreground)))
                .ratio(percentage.clamp(0.0, 100.0) / 100.0)
                .label(Line::from(format!(
                    "{}   {:.0}% ",
                    if self.buffering {
                        self.spinner_stages[self.spinner]
                    } else if self.paused {
                        "⏸︎"
                    } else {
                        "►"
                    },
                    percentage,
                ))),
            progress_bar_area[0],
        );

        frame.render_widget(
            Paragraph::new(Line::from(metadata_spans))
                .centered()
                .block(Block::bordered().borders(Borders::NONE).padding(Padding::new(0, 0, 1, 0))),
            if self.preferences.large_art { layout[1] } else { progress_bar_area[0] },
        );

        frame.render_widget(
            Paragraph::new(duration)
                .centered()
                .block(Block::bordered().borders(Borders::NONE).padding(Padding::ZERO))
                .style(Style::default().fg(self.theme.resolve(&self.theme.foreground))),
            progress_bar_area[1],
        );
    }
}
