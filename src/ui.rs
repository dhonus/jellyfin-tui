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

/// The select-mode mark, as its own leading column. A separate cell so that marked tracks and
/// marked album headers line up - glued to their own text they land in different columns.
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

    /// Colour for a pane's title and count. Follows the border so a focused pane reads as one piece.
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

    /// Same corner with more than one figure: `(57 tracks - 1:02:11)`. Empty unit = figure alone.
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

    /// The list cursor. Used verbatim, so its width is whatever the configured string is.
    pub(crate) fn selector(&self) -> String {
        self.symbols.selector.clone()
    }

    /// The selection highlight: lit while the pane has focus, muted otherwise.
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
