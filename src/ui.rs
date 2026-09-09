/* --------------------------
Shared chrome for the main panes: border, title, count and selection highlight.
    - Every list and table in the app is built from these, so they can't drift apart.
    - Anything changed here shows up in Library, Albums, Playlists and Search at once.
-------------------------- */
use crate::tui::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell};

/// What a row shows in the select-mode gutter.
pub(crate) enum Mark {
    /// Nothing marked on or under this row.
    None,
    /// The row itself is marked, or everything under it is.
    Selected,
    /// Some of what's under this row is marked, but not all of it.
    Partial,
}

/// The select-mode mark. Its own cell, so track and album-header marks share a column.
pub(crate) fn mark_cell(mark: Mark) -> Cell<'static> {
    Cell::from(match mark {
        Mark::None => "",
        Mark::Selected => "✓",
        Mark::Partial => "•",
    })
}

impl App {
    /// The bordered block every pane sits in. `focused` lights the border.
    pub(crate) fn pane_block(&self, focused: bool) -> Block<'static> {
        Block::new().borders(Borders::ALL).border_type(self.border_type).border_style(if focused {
            self.theme.resolve(&self.theme.border_focused)
        } else {
            self.theme.resolve(&self.theme.border)
        })
    }

    /// Colour for a pane's title and count. Follows the border.
    pub(crate) fn pane_accent(&self, focused: bool) -> Color {
        if focused {
            self.theme.resolve(&self.theme.border_focused)
        } else {
            self.theme.resolve(&self.theme.section_title)
        }
    }

    /// A pane's title, for the top-left corner.
    pub(crate) fn pane_title(&self, text: impl Into<String>, focused: bool) -> Line<'static> {
        Line::from(text.into()).fg(self.pane_accent(focused))
    }

    /// `(1234 artists)` for the top-right corner.
    pub(crate) fn pane_count(
        &self,
        count: impl std::fmt::Display,
        unit: &str,
        focused: bool,
    ) -> Line<'static> {
        self.pane_meta(&[(count.to_string(), unit.to_string())], focused)
    }

    /// `(57 tracks - 1:02:11)`. An empty unit renders the figure alone.
    pub(crate) fn pane_meta(&self, parts: &[(String, String)], focused: bool) -> Line<'static> {
        let text = parts
            .iter()
            .map(
                |(figure, unit)| {
                    if unit.is_empty() {
                        figure.clone()
                    } else {
                        format!("{} {}", figure, unit)
                    }
                },
            )
            .collect::<Vec<String>>()
            .join(" - ");

        Line::from(format!("({})", text)).fg(self.pane_accent(focused))
    }

    /// The list cursor, used verbatim - width is whatever is configured.
    pub(crate) fn selector(&self) -> String {
        self.symbols.selector.clone()
    }

    /// Lit while the pane has focus, muted otherwise.
    pub(crate) fn selection_style(&self, focused: bool) -> Style {
        let (bg, fg) = if focused {
            (&self.theme.selected_active_background, &self.theme.selected_active_foreground)
        } else {
            (&self.theme.selected_inactive_background, &self.theme.selected_inactive_foreground)
        };
        Style::default()
            .add_modifier(Modifier::BOLD)
            .bg(self.theme.resolve(bg))
            .fg(self.theme.resolve(fg))
    }
}
