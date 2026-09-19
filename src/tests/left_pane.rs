//! The pure bits of the left-hand tables: the scrolling window the panes are built from, and
//! the ellipsis marking a row ratatui would otherwise clip without a trace.

use crate::tui::App;
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::TableState;

fn offset_after(selected: usize, offset: usize, len: usize, height: usize) -> usize {
    let mut state = TableState::default().with_selected(Some(selected)).with_offset(offset);
    App::apply_scroll_padding(&mut state, len, height, 10);
    state.offset()
}

#[test]
fn padding_keeps_rows_visible_above_the_cursor() {
    assert_eq!(offset_after(50, 45, 1000, 30), 40);
}

#[test]
fn padding_keeps_rows_visible_below_the_cursor() {
    assert_eq!(offset_after(26, 0, 1000, 30), 7);
}

#[test]
fn an_offset_already_padded_on_both_sides_is_left_alone() {
    assert_eq!(offset_after(50, 35, 1000, 30), 35);
}

#[test]
fn the_window_never_scrolls_past_the_end_of_the_list() {
    assert_eq!(offset_after(39, 30, 40, 30), 10);
}

#[test]
fn a_list_shorter_than_the_viewport_stays_at_the_top() {
    assert_eq!(offset_after(3, 0, 5, 30), 0);
}

#[test]
fn a_viewport_too_short_for_the_padding_still_shows_the_cursor() {
    // 5 rows can't hold 10 either side, but the cursor must stay on screen
    let offset = offset_after(50, 0, 1000, 5);
    assert!(offset <= 50 && 50 < offset + 5, "cursor fell outside the window: {offset}");
}

#[test]
fn an_unselected_table_is_not_scrolled() {
    let mut state = TableState::default().with_offset(17);
    App::apply_scroll_padding(&mut state, 1000, 30, 10);
    assert_eq!(state.offset(), 17);
}

#[test]
fn a_row_that_fits_is_returned_untouched() {
    let text = Text::from("Aphex Twin");
    assert_eq!(App::ellipsize(text, 20).to_string(), "Aphex Twin");
}

#[test]
fn an_overlong_row_is_cut_and_marked() {
    let text = Text::from("Godspeed You! Black Emperor");
    let out = App::ellipsize(text, 12);
    assert_eq!(out.to_string(), "Godspeed Yo…");
    assert_eq!(out.width(), 12, "the ellipsis has to fit inside the budget");
}

#[test]
fn the_cut_keeps_the_styling_of_each_surviving_span() {
    // a search-underlined name keeps its underline after the cut
    let text = Text::from(Line::from(vec![
        Span::styled("God", Style::new().underlined()),
        Span::raw("speed You! Black Emperor"),
    ]));
    let out = App::ellipsize(text, 8);
    assert_eq!(out.to_string(), "Godspee…");
    assert_eq!(out.lines[0].spans[0].style, Style::new().underlined());
    assert_eq!(out.lines[0].spans[0].content, "God");
}

#[test]
fn a_cut_lands_on_a_character_boundary_not_a_byte_one() {
    // 2 columns per glyph, so 5 holds two plus the ellipsis
    let out = App::ellipsize(Text::from("実験的音楽"), 5);
    assert_eq!(out.to_string(), "実験…");
    assert!(out.width() <= 5);
}

#[test]
fn a_budget_of_one_leaves_just_the_ellipsis() {
    assert_eq!(App::ellipsize(Text::from("Aphex Twin"), 1).to_string(), "…");
}

#[test]
fn a_budget_of_zero_renders_nothing() {
    assert_eq!(App::ellipsize(Text::from("Aphex Twin"), 0).to_string(), "");
}

fn window(
    selected: usize,
    offset: usize,
    len: usize,
    height: usize,
) -> (std::ops::Range<usize>, usize) {
    let mut state = TableState::default().with_selected(Some(selected)).with_offset(offset);
    let (range, local) = App::visible_window(&mut state, len, height, 0);
    (range, local.selected().unwrap())
}

#[test]
fn the_window_holds_only_what_fits_plus_a_partial_row() {
    let (range, _) = window(0, 0, 5000, 40);
    assert_eq!(range, 0..41);
}

#[test]
fn the_selection_is_renumbered_into_the_window() {
    let (range, local) = window(1000, 980, 5000, 40);
    assert_eq!(range.start, 980);
    assert_eq!(local, 20, "row 1000 is the 20th row of a window starting at 980");
}

#[test]
fn the_selected_row_is_always_inside_the_window() {
    let mut state = TableState::default().with_selected(Some(0));
    for selected in 0..3000 {
        state.select(Some(selected));
        let (range, local) = App::visible_window(&mut state, 3000, 40, 0);
        let local = local.selected().unwrap();
        assert!(range.contains(&selected), "row {selected} not in {range:?}");
        assert_eq!(range.start + local, selected, "renumbering lost row {selected}");
    }
}

#[test]
fn the_window_never_runs_past_the_end() {
    let (range, local) = window(4999, 4990, 5000, 40);
    assert_eq!(range.end, 5000);
    assert_eq!(range.start + local, 4999);
}

#[test]
fn a_list_shorter_than_the_window_is_taken_whole() {
    let (range, local) = window(3, 0, 10, 40);
    assert_eq!(range, 0..10);
    assert_eq!(local, 3);
}

#[test]
fn an_empty_list_yields_an_empty_window() {
    let (range, _) = window(0, 0, 0, 40);
    assert!(range.is_empty());
}

#[test]
fn scroll_padding_still_applies_through_the_window() {
    let mut state = TableState::default().with_selected(Some(1000)).with_offset(995);
    let (range, local) = App::visible_window(&mut state, 5000, 40, 10);
    assert_eq!(range.start, 990, "ten rows should stay above the cursor");
    assert_eq!(range.start + local.selected().unwrap(), 1000);
}
