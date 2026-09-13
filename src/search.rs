/* --------------------------
Search tab rendering
    - The entry point is the render_search function, it runs at each frame and renders the search tab.
    - The search tab is split into 2 parts, the search area and the results area.
    - The results area contains 3 lists, artists, albums, and tracks.
-------------------------- */

use crate::keyboard::{self, *};
use crate::tui::App;

use crate::helpers;
use ratatui::{
    prelude::*,
    widgets::*,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

impl App {
    pub fn render_search(&mut self, app_container: Rect, frame: &mut Frame) {
        // search bar up top, results in 3 lists. Artists, Albums, Tracks
        // split the app container into 2 parts
        let search_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Min(3), Constraint::Percentage(95)])
            .split(app_container);

        let search_area = search_layout[0];
        let results_area = search_layout[1];

        let fg = self.theme.resolve(&self.theme.foreground);
        let instructions = if self.searching {
            Line::from(vec![
                " Search ".fg(fg),
                self.key_hint(&Action::Enter, "<Enter>").fg(self.theme.primary_color).bold(),
                " Clear search ".fg(fg),
                self.key_hint(&Action::Delete, "<Delete>").fg(self.theme.primary_color).bold(),
                " Cancel ".fg(fg),
                format!("{} ", self.key_hint(&Action::Cancel, "<Esc>"))
                    .fg(self.theme.primary_color)
                    .bold(),
            ])
        } else {
            let mut parts = vec![
                " Go ".fg(fg),
                self.key_hint(&Action::Enter, "<Enter>").fg(self.theme.primary_color).bold(),
                " Search ".fg(fg),
                format!(
                    "{} {}",
                    self.key_hint(&Action::SearchLocally, "</>"),
                    self.key_hint(&Action::Tab(4), "<4>")
                )
                .fg(self.theme.primary_color)
                .bold(),
                " Next Section ".fg(fg),
                self.key_hint(&Action::CyclePrimaryPanes, "<Tab>")
                    .fg(self.theme.primary_color)
                    .bold(),
                " Previous Section ".fg(fg),
                self.key_hint(&Action::CycleSecondaryPanes, "<Shift-Tab>")
                    .fg(self.theme.primary_color)
                    .bold(),
            ];
            let total_pages =
                self.search_track_total.saturating_add(keyboard::SEARCH_TRACK_PAGE_SIZE - 1)
                    / keyboard::SEARCH_TRACK_PAGE_SIZE;
            if total_pages > 1 && matches!(self.state.search_section, SearchSection::Tracks) {
                parts.push(" Prev/Next Page ".fg(fg));
                parts.push(
                    format!(
                        "{}/{} ",
                        self.key_hint(&Action::PageUp, "<PgUp>"),
                        self.key_hint(&Action::PageDown, "<PgDn>")
                    )
                    .fg(self.theme.primary_color)
                    .bold(),
                );
            } else {
                parts.push(" ".fg(fg));
            }
            Line::from(parts)
        };

        let title_line = Line::from(if self.searching {
            "Search".to_string()
        } else {
            format!("Matching: {}", self.search_term_last)
        })
        .fg(if self.searching {
            self.theme.primary_color
        } else {
            self.theme.resolve(&self.theme.section_title)
        });

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title_line)
            .title_bottom(instructions.alignment(Alignment::Center))
            .border_type(self.border_type)
            .border_style(Style::default().fg(if self.searching {
                self.theme.primary_color
            } else {
                self.theme.resolve(&self.theme.border)
            }));

        let search_term =
            Paragraph::new(self.search_term.clone()).block(block).wrap(Wrap { trim: false });

        frame.render_widget(search_term, search_area);

        // split results area into 3 parts — horizontal when wide, stacked vertically
        // when narrow, matching the Library/Playlists vertical layout threshold.
        let is_vertical = self.layout_mode.is_vertical(results_area.width, self.vertical_threshold);
        let results_layout = Layout::default()
            .direction(if is_vertical { Direction::Vertical } else { Direction::Horizontal })
            .constraints(vec![
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(results_area);

        let border = self.theme.resolve(&self.theme.border);
        let foreground = self.theme.resolve(&self.theme.foreground);
        let foreground_dim = self.theme.resolve(&self.theme.foreground_dim);
        let border_type = self.border_type;

        let has_results = !self.search_result_artists.is_empty()
            || !self.search_result_albums.is_empty()
            || !self.search_result_tracks.is_empty();

        if !has_results {
            let body = if !self.search_term_last.is_empty() {
                Line::from(format!("No results for \"{}\"", self.search_term_last))
                    .fg(foreground_dim)
            } else if self.searching {
                Line::from(vec![
                    "Type what you're looking for, then ".fg(foreground),
                    self.key_hint(&Action::Enter, "<Enter>").fg(self.theme.primary_color).bold(),
                    " to search.".fg(foreground),
                ])
            } else {
                Line::from(vec![
                    "Search the whole library — artists, albums and tracks. Press ".fg(foreground),
                    self.key_hint(&Action::SearchLocally, "</>")
                        .fg(self.theme.primary_color)
                        .bold(),
                    " to start typing.".fg(foreground),
                ])
            };
            frame.render_widget(
                Paragraph::new(body).centered().wrap(Wrap { trim: false }).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(border_type)
                        .border_style(border)
                        .padding(Padding::new(0, 0, results_area.height / 2, 0)),
                ),
                results_area,
            );
            return;
        }

        let playing = self.state.queue.get(self.state.current_playback_state.current_index);

        // 3 lists, artists, albums, tracks
        let artists = self
            .search_result_artists
            .iter()
            .map(|artist| {
                let is_playing = playing.is_some_and(|song| {
                    song.album_artists.iter().any(|a| a.id == artist.id || a.name == artist.name)
                });
                let mut item = Text::default();
                if artist.user_data.is_favorite {
                    item.push_span(Span::styled(
                        format!("{} ", &self.symbols.favorite),
                        Style::default().fg(self.theme.primary_color),
                    ));
                }
                item.push_span(Span::styled(
                    artist.name.as_str(),
                    if is_playing {
                        Style::default().fg(self.theme.primary_color).italic()
                    } else {
                        Style::default().fg(foreground)
                    },
                ));
                ListItem::new(item)
            })
            .collect::<Vec<ListItem>>();

        let albums = self
            .search_result_albums
            .iter()
            .map(|album| {
                let is_playing = playing.is_some_and(|song| song.album_id == album.id);
                let mut item = Text::default();
                if album.user_data.is_favorite {
                    item.push_span(Span::styled(
                        format!("{} ", &self.symbols.favorite),
                        Style::default().fg(self.theme.primary_color),
                    ));
                }
                item.push_span(Span::styled(
                    album.name.as_str(),
                    if is_playing {
                        Style::default().fg(self.theme.primary_color).italic()
                    } else {
                        Style::default().fg(foreground)
                    },
                ));
                let album_artists =
                    album.album_artists.iter().map(|a| a.name.as_str()).collect::<Vec<&str>>();
                if !album_artists.is_empty() {
                    item.push_span(Span::styled(
                        format!(" {} {}", self.symbols.separator, album_artists.join(", ")),
                        Style::default().fg(foreground_dim),
                    ));
                }
                ListItem::new(item)
            })
            .collect::<Vec<ListItem>>();

        let tracks = self
            .search_result_tracks
            .iter()
            .map(|track| {
                let title = format!("{} - {}", track.name, track.album);

                let mut time_span_text =
                    format!("  {}", helpers::format_ticks(track.run_time_ticks));
                if track.has_lyrics {
                    time_span_text.push(' ');
                    time_span_text.push_str(&self.symbols.lyrics);
                }

                let mut item = Text::default();
                if track.user_data.is_favorite {
                    item.push_span(Span::styled(
                        format!("{} ", &self.symbols.favorite),
                        Style::default().fg(self.theme.primary_color),
                    ));
                }
                item.push_span(Span::styled(
                    title,
                    if track.id == self.active_song_id {
                        Style::default().fg(self.theme.primary_color).italic()
                    } else {
                        Style::default().fg(foreground)
                    },
                ));
                item.push_span(Span::styled(
                    time_span_text,
                    Style::default().fg(foreground_dim).add_modifier(Modifier::ITALIC),
                ));
                ListItem::new(item)
            })
            .collect::<Vec<ListItem>>();

        let total_pages =
            self.search_track_total.saturating_add(keyboard::SEARCH_TRACK_PAGE_SIZE - 1)
                / keyboard::SEARCH_TRACK_PAGE_SIZE;
        let tracks_title = if total_pages > 1 {
            format!("Tracks ({}/{})", self.search_track_page + 1, total_pages)
        } else {
            "Tracks".to_string()
        };

        // Focused pane titles follow border_focused and carry a result count, the same as
        // the lists in every other tab.
        let result_block = |focused: bool, title: String, count: u64, unit: &str| {
            self.pane_block(focused)
                .title(self.pane_title(title, focused))
                .title_top(self.pane_count(count, unit, focused).right_aligned())
        };

        let artists_focused = matches!(self.state.search_section, SearchSection::Artists);
        let albums_focused = matches!(self.state.search_section, SearchSection::Albums);
        let tracks_focused = matches!(self.state.search_section, SearchSection::Tracks);

        let artists_list = List::new(artists)
            .block(result_block(
                artists_focused,
                "Artists".to_string(),
                self.search_result_artists.len() as u64,
                "artists",
            ))
            .highlight_symbol(self.selector())
            .highlight_style(self.selection_style(artists_focused))
            .scroll_padding(10)
            .repeat_highlight_symbol(true);

        let albums_list = List::new(albums)
            .block(result_block(
                albums_focused,
                "Albums".to_string(),
                self.search_result_albums.len() as u64,
                "albums",
            ))
            .highlight_symbol(self.selector())
            .highlight_style(self.selection_style(albums_focused))
            .scroll_padding(10)
            .repeat_highlight_symbol(true);

        let tracks_list = List::new(tracks)
            .block(result_block(tracks_focused, tracks_title, self.search_track_total, "tracks"))
            .highlight_symbol(self.selector())
            .highlight_style(self.selection_style(tracks_focused))
            .scroll_padding(10)
            .repeat_highlight_symbol(true);

        // frame.render_widget(artists_list, results_layout[0]);
        frame.render_stateful_widget(
            artists_list,
            results_layout[0],
            &mut self.state.selected_search_artist,
        );
        frame.render_stateful_widget(
            albums_list,
            results_layout[1],
            &mut self.state.selected_search_album,
        );
        frame.render_stateful_widget(
            tracks_list,
            results_layout[2],
            &mut self.state.selected_search_track,
        );

        helpers::render_scrollbar(
            frame,
            results_layout[0],
            &mut self.state.search_artist_scroll_state,
            &self.theme,
        );
        helpers::render_scrollbar(
            frame,
            results_layout[1],
            &mut self.state.search_album_scroll_state,
            &self.theme,
        );
        helpers::render_scrollbar(
            frame,
            results_layout[2],
            &mut self.state.search_track_scroll_state,
            &self.theme,
        );
    }
}
