/* --------------------------
Shared chrome for the main panes: border, title, count and selection highlight.
    - Every list and table in the app is built from these, so they can't drift apart.
    - Anything changed here shows up in Library, Albums, Playlists and Search at once.
-------------------------- */
use crate::tui::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, ListState, Row, TableState};
use unicode_truncate::UnicodeTruncateStr;

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

    /// Clip a row to `width` columns with an ellipsis; ratatui would cut it without a mark.
    pub(crate) fn ellipsize<'a>(text: Text<'a>, width: usize) -> Text<'a> {
        // display width never exceeds byte length in UTF-8, so this skips the width scan
        let bytes: usize = text.lines.iter().flat_map(|l| &l.spans).map(|s| s.content.len()).sum();
        if bytes <= width || text.width() <= width {
            return text;
        }
        if width == 0 {
            return Text::default();
        }

        let lines = text
            .lines
            .into_iter()
            .map(|line| {
                if line.width() <= width {
                    return line;
                }
                let budget = width - 1; // the ellipsis takes one
                let mut spans: Vec<Span<'a>> = vec![];
                let mut used = 0;
                let mut last_style = Style::default();

                for span in line.spans {
                    last_style = span.style;
                    if used + span.width() <= budget {
                        used += span.width();
                        spans.push(span);
                        continue;
                    }
                    let (kept, _) = span.content.unicode_truncate(budget - used);
                    if !kept.is_empty() {
                        let kept = kept.to_string();
                        spans.push(Span::styled(kept, span.style));
                    }
                    break;
                }

                spans.push(Span::styled("…", last_style));
                Line::from(spans)
            })
            .collect::<Vec<_>>();

        Text::from(lines)
    }

    /// What's left for the name column once the cursor, figure, scrollbar and gaps take theirs.
    pub(crate) fn left_name_width(&self, inner: Rect, trailing_width: usize) -> usize {
        (inner.width as usize)
            .saturating_sub(self.selector().chars().count() + trailing_width + 1 + 2)
    }

    /// Header row for the left-hand lists.
    pub(crate) fn left_header(&self, cells: Vec<Cell<'static>>) -> Row<'static> {
        Row::new(cells)
            .style(Style::new().bold().fg(self.theme.resolve(&self.theme.section_title)))
            .bottom_margin(0)
    }

    /// The rows to build, and the state to render them with. Building every row and letting
    /// ratatui skip the off-screen ones costs allocations apiece — 273us against 10us for the
    /// forty you can see. The returned state is renumbered into the slice; `state` keeps the
    /// real index. `height` excludes any header row.
    pub(crate) fn visible_window(
        state: &mut TableState,
        len: usize,
        height: usize,
        padding: usize,
    ) -> (std::ops::Range<usize>, TableState) {
        Self::apply_scroll_padding(state, len, height, padding);

        let start = state.offset().min(len);
        // one spare: ratatui draws a partial row when the body doesn't divide evenly
        let end = start.saturating_add(height).saturating_add(1).min(len);
        let local =
            TableState::default().with_selected(state.selected().map(|s| s.saturating_sub(start)));

        (start..end, local)
    }

    /// Where the window starts. Keeps the cursor on screen — ratatui can't, once it only sees
    /// a slice — and holds `padding` rows either side, which `Table` has no equivalent for.
    /// Plain function over the numbers: `ListState` and `TableState` share no trait.
    pub(crate) fn scroll_offset(
        selected: Option<usize>,
        offset: usize,
        len: usize,
        height: usize,
        padding: usize,
    ) -> usize {
        let Some(selected) = selected else {
            return offset;
        };
        if height == 0 || len == 0 {
            return offset;
        }
        // a short viewport can't hold padding on both sides
        let padding = padding.min(height.saturating_sub(1) / 2);

        let offset = offset
            .min(selected.saturating_sub(padding))
            .max((selected + padding + 1).saturating_sub(height));
        offset.min(len.saturating_sub(height))
    }

    /// `visible_window` for the panes still on `List`.
    pub(crate) fn visible_window_list(
        state: &mut ListState,
        len: usize,
        height: usize,
        padding: usize,
    ) -> (std::ops::Range<usize>, ListState) {
        *state.offset_mut() =
            Self::scroll_offset(state.selected(), state.offset(), len, height, padding);

        let start = state.offset().min(len);
        let end = start.saturating_add(height).saturating_add(1).min(len);
        let local =
            ListState::default().with_selected(state.selected().map(|s| s.saturating_sub(start)));

        (start..end, local)
    }

    pub(crate) fn apply_scroll_padding(
        state: &mut TableState,
        len: usize,
        height: usize,
        padding: usize,
    ) {
        *state.offset_mut() =
            Self::scroll_offset(state.selected(), state.offset(), len, height, padding);
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
