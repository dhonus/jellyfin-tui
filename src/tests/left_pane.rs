//! The two pure bits of the left-hand tables: the scroll padding that replaces
//! `List::scroll_padding` (which `Table` has no equivalent for), and the ellipsis that marks a
//! row ratatui would otherwise clip without a trace.

use crate::tui::App;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::TableState;

fn offset_after(selected: usize, offset: usize, len: usize, height: usize) -> usize {
    let mut state = TableState::default().with_selected(Some(selected)).with_offset(offset);
    App::apply_scroll_padding(&mut state, len, height, 10);
    state.offset()
}

#[test]
fn padding_keeps_rows_visible_above_the_cursor() {
    // cursor at 50 with the window starting at 45 leaves only 5 rows above it
    assert_eq!(offset_after(50, 45, 1000, 30), 40);
}

#[test]
fn padding_keeps_rows_visible_below_the_cursor() {
    // window 0..30 puts the cursor 4 rows from the bottom edge
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
    // 5 rows can't hold 10 above and below, but the cursor must stay on screen
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
    // mirrors a search-underlined name: the match keeps its underline after the cut
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
    // 2 columns per CJK glyph, so a 5 column budget holds 2 glyphs plus the ellipsis
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
