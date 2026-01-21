/* --------------------------
Search tab rendering
    - The entry point is the render_search function, it runs at each frame and renders the search tab.
    - The search tab is split into 2 parts, the search area and the results area.
    - The results area contains 3 lists, artists, albums, and tracks.
-------------------------- */

use crate::keyboard::*;
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

        let instructions = if self.searching {
            Line::from(vec![
                " Search ".fg(self.theme.resolve(&self.theme.foreground)),
                "<Enter>".fg(self.theme.primary_color).bold(),
                " Clear search ".fg(self.theme.resolve(&self.theme.foreground)),
                "<Delete>".fg(self.theme.primary_color).bold(),
                " Cancel ".fg(self.theme.resolve(&self.theme.foreground)),
                "<Esc> ".fg(self.theme.primary_color).bold(),
            ])
        } else {
            Line::from(vec![
                " Go ".fg(self.theme.resolve(&self.theme.foreground)),
                "<Enter>".fg(self.theme.primary_color).bold(),
                " Search ".fg(self.theme.resolve(&self.theme.foreground)),
                "< / > <F2>".fg(self.theme.primary_color).bold(),
                " Next Section ".fg(self.theme.resolve(&self.theme.foreground)),
                "<Tab>".fg(self.theme.primary_color).bold(),
                " Previous Section ".fg(self.theme.resolve(&self.theme.foreground)),
                "<Shift+Tab> ".fg(self.theme.primary_color).bold(),
            ])
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

        // split results area into 3 parts
        let results_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(results_area);

        // render search results
        // 3 lists, artists, albums, tracks
        let artists = self
            .search_result_artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<&str>>();

        let albums = self
            .search_result_albums
            .iter()
            .map(|album| album.name.as_str())
            .collect::<Vec<&str>>();
        let tracks = self
            .search_result_tracks
            .iter()
            .map(|track| {
                let title = format!("{} - {}", track.name, track.album);
                // track.run_time_ticks is in microseconds
                let seconds = (track.run_time_ticks / 10_000_000) % 60;
                let minutes = (track.run_time_ticks / 10_000_000 / 60) % 60;
                let hours = (track.run_time_ticks / 10_000_000 / 60) / 60;
                let hours_optional_text = match hours {
                    0 => String::from(""),
                    _ => format!("{}:", hours),
                };

                let mut time_span_text =
                    format!("  {}{:02}:{:02}", hours_optional_text, minutes, seconds);
                if track.has_lyrics {
                    time_span_text.push_str(" ♪");
                }

                if track.id == self.active_song_id {
                    let mut time: Text = Text::from(Span::styled(
                        title,
                        Style::default().fg(self.theme.primary_color), // active title = primary
                    ));
                    time.push_span(Span::styled(
                        time_span_text,
                        Style::default()
                            .fg(self.theme.resolve(&self.theme.foreground_dim))
                            .add_modifier(Modifier::ITALIC),
                    ));
                    ListItem::new(time) // no outer .style(...)
                } else {
                    let mut time: Text = Text::from(Span::styled(
                        title,
                        Style::default().fg(self.theme.resolve(&self.theme.foreground)),
                    ));
                    time.push_span(Span::styled(
                        time_span_text,
                        Style::default()
                            .fg(self.theme.resolve(&self.theme.foreground_dim))
                            .add_modifier(Modifier::ITALIC),
                    ));
                    ListItem::new(time)
                }
            })
            .collect::<Vec<ListItem>>();

        let artists_list = match self.state.search_section {
            SearchSection::Artists => List::new(artists)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(self.theme.resolve(&self.theme.border_focused))
                        .border_type(self.border_type)
                        .title("Artists"),
                )
                .fg(self.theme.resolve(&self.theme.foreground))
                .highlight_symbol(">>")
                .highlight_style(
                    Style::default()
                        .fg(self.theme.resolve(&self.theme.selected_active_foreground))
                        .bg(self.theme.resolve(&self.theme.selected_active_background))
                        .add_modifier(Modifier::BOLD),
                )
                .scroll_padding(10)
                .repeat_highlight_symbol(true),
            _ => List::new(artists)
                .block(
                    Block::default()
                        .fg(self.theme.resolve(&self.theme.border))
                        .borders(Borders::ALL)
                        .border_type(self.border_type)
                        .title(
                            Line::from("Artists").fg(self.theme.resolve(&self.theme.section_title)),
                        ),
                )
                .fg(self.theme.resolve(&self.theme.foreground))
                .highlight_symbol(">>")
                .highlight_style(
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(self.theme.resolve(&self.theme.selected_inactive_foreground))
                        .bg(self.theme.resolve(&self.theme.selected_inactive_background)),
                )
                .scroll_padding(10)
                .repeat_highlight_symbol(true),
        };

        let albums_list = match self.state.search_section {
            SearchSection::Albums => List::new(albums)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(self.theme.resolve(&self.theme.border_focused))
                        .border_type(self.border_type)
                        .title("Albums"),
                )
                .fg(self.theme.resolve(&self.theme.foreground))
                .highlight_symbol(">>")
                .highlight_style(
                    Style::default()
                        .fg(self.theme.resolve(&self.theme.selected_active_foreground))
                        .bg(self.theme.resolve(&self.theme.selected_active_background))
                        .add_modifier(Modifier::BOLD),
                )
                .repeat_highlight_symbol(true),
            _ => List::new(albums)
                .block(
                    Block::default()
                        .fg(self.theme.resolve(&self.theme.border))
                        .borders(Borders::ALL)
                        .border_type(self.border_type)
                        .title(
                            Line::from("Albums").fg(self.theme.resolve(&self.theme.section_title)),
                        ),
                )
                .fg(self.theme.resolve(&self.theme.foreground))
                .highlight_symbol(">>")
                .highlight_style(
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .bg(self.theme.resolve(&self.theme.selected_inactive_background))
                        .fg(self.theme.resolve(&self.theme.selected_inactive_foreground)),
                )
                .repeat_highlight_symbol(true),
        };

        let tracks_list = match self.state.search_section {
            SearchSection::Tracks => List::new(tracks)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(self.theme.resolve(&self.theme.border_focused))
                        .border_type(self.border_type)
                        .title("Tracks"),
                )
                .highlight_symbol(">>")
                .highlight_style(
                    Style::default()
                        .bg(self.theme.resolve(&self.theme.selected_active_background))
                        .fg(self.theme.resolve(&self.theme.selected_active_foreground))
                        .add_modifier(Modifier::BOLD),
                )
                .repeat_highlight_symbol(true),
            _ => List::new(tracks)
                .block(
                    Block::default()
                        .fg(self.theme.resolve(&self.theme.border))
                        .borders(Borders::ALL)
                        .border_type(self.border_type)
                        .title(
                            Line::from("Tracks").fg(self.theme.resolve(&self.theme.section_title)),
                        ),
                )
                .highlight_symbol(">>")
                .highlight_style(
                    Style::default()
                        .bg(self.theme.resolve(&self.theme.selected_inactive_background))
                        .fg(self.theme.resolve(&self.theme.selected_inactive_foreground))
                        .add_modifier(Modifier::BOLD),
                )
                .repeat_highlight_symbol(true),
        };

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
