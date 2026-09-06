//! Tests for the non-obvious decisions of the generic select-mode state machine (src/select.rs).
//! The rest is a thin IndexSet wrapper not worth pinning down.

use crate::select::{SelectMode, SelectPane};

#[test]
fn enter_ignores_an_empty_seed() {
    let mut select = SelectMode::default();
    select.enter(SelectPane::PlaylistTracks, Some(String::new()));
    assert!(select.is_active());
    assert!(select.is_empty());
}

#[test]
fn entering_a_new_session_drops_the_previous_selection() {
    let mut select = SelectMode::default();
    select.enter(SelectPane::PlaylistTracks, Some("old".to_string()));
    select.enter(SelectPane::PlaylistTracks, None);
    assert!(!select.is_selected("old"));
    assert!(select.is_empty());
}

#[test]
fn toggle_is_a_no_op_while_inactive() {
    let mut select = SelectMode::default();
    select.toggle("a".to_string());
    assert!(select.is_empty());
}

#[test]
fn toggle_ignores_empty_keys() {
    let mut select = SelectMode::default();
    select.enter(SelectPane::PlaylistTracks, None);
    select.toggle(String::new());
    assert!(select.is_empty());
}

#[test]
fn entering_another_pane_steals_the_session() {
    let mut select = SelectMode::default();
    select.enter(SelectPane::PlaylistTracks, None);
    select.enter(SelectPane::LibraryTracks, Some("x".to_string()));
    assert_eq!(select.pane(), Some(SelectPane::LibraryTracks));
    assert!(!select.is_active_in(SelectPane::PlaylistTracks));
}

#[test]
fn ordered_keys_follows_the_source_list_not_the_click_order() {
    let mut select = SelectMode::default();
    select.enter(SelectPane::AlbumTracks, None);
    // marked back to front
    select.toggle("c".to_string());
    select.toggle("a".to_string());
    select.toggle("b".to_string());

    let album = ["a".to_string(), "b".to_string(), "c".to_string()];
    assert_eq!(select.ordered_keys(&album), vec!["a", "b", "c"]);
}

#[test]
fn ordered_keys_puts_unknown_keys_last_in_insertion_order() {
    let mut select = SelectMode::default();
    select.enter(SelectPane::AlbumTracks, None);
    select.toggle("ghost".to_string());
    select.toggle("b".to_string());
    select.toggle("phantom".to_string());
    select.toggle("a".to_string());

    let album = ["a".to_string(), "b".to_string()];
    assert_eq!(select.ordered_keys(&album), vec!["a", "b", "ghost", "phantom"]);
}

#[test]
fn keys_are_returned_in_insertion_order() {
    let mut select = SelectMode::default();
    select.enter(SelectPane::PlaylistTracks, Some("first".to_string()));
    select.toggle("second".to_string());
    select.toggle("third".to_string());
    assert_eq!(select.keys(), vec!["first", "second", "third"]);
}

#[test]
fn unmarking_then_remarking_moves_a_key_to_the_end() {
    let mut select = SelectMode::default();
    select.enter(SelectPane::PlaylistTracks, None);
    select.toggle("a".to_string());
    select.toggle("b".to_string());
    select.toggle("a".to_string()); // off
    select.toggle("a".to_string()); // back on
    assert_eq!(select.keys(), vec!["b", "a"]);
    // ...but the canonical order still wins where it knows the key
    let order = ["a".to_string(), "b".to_string()];
    assert_eq!(select.ordered_keys(&order), vec!["a", "b"]);
}
