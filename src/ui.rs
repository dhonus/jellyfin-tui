/* --------------------------
Shared chrome for the main panes.
    - Every list and table in the app is the same object: a bordered block that brightens when it
      holds focus, a title in the top-left, a count in the top-right, and a selection highlight
      that goes flat when focus moves elsewhere.
    - These helpers build that object once so the panes can't drift apart from each other. Anything
      here shows up in Library, Albums, Playlists and Search at the same time.
-------------------------- */
use crate::tui::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell};

/// What a row shows in the select-mode gutter.
pub(crate) enum Mark {
    /// Nothing marked on or under this row.
    None,
    /// The row itself is marked.
    Selected,
    /// The row can't be marked itself, but something under it is - an album header sitting over
    /// marked tracks. Deliberately not a tick: the album isn't selected, and never can be.
    Partial,
}

/// The select-mode mark, as its own leading column. Kept a separate cell rather than glued onto
/// the text beside it so that a marked track and a marked album header land in the same gutter -
/// tacked onto the number and the album title respectively, they sit in different columns and
/// never line up. Blank when unmarked, so an unmarked list stays quiet.
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

    /// Colour for a pane's title and count. Follows the border, so a focused pane reads as one
    /// piece rather than a lit border wrapped around an idle title.
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

    /// The same corner when it carries more than one figure: `(57 tracks - 1:02:11)`. Each part is
    /// a (figure, unit) pair; an empty unit renders the figure alone.
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

    /// The list cursor, drawn in the gutter left of the selected row. Used verbatim, so its
    /// display width is whatever the configured string is — a table's columns start after it.
    pub(crate) fn selector(&self) -> String {
        self.symbols.selector.clone()
    }

    /// The selection highlight. Focus decides whether the cursor is the loud one or the muted
    /// marker showing where you'd land on coming back to this pane.
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
