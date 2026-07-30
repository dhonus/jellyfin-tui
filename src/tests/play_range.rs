//! What Enter enqueues, and the invariant that folding can't change it.

use std::collections::HashSet;

use super::{folded, model, visible_ids};
use crate::client::DiscographySong;
use crate::discography::{play_range, DiscographyView};

fn sample() -> Vec<DiscographySong> {
    model(&[("A", &["a1", "a2"]), ("B", &["b1"]), ("C", &["c1", "c2"])])
}

fn range_for(
    tracks: &[DiscographySong],
    collapsed: &HashSet<String>,
    id: &str,
) -> Option<std::ops::Range<usize>> {
    let view = DiscographyView::build(tracks, "", collapsed);
    let row = view.row_of_id(tracks, id)?;
    play_range(tracks, &view, row)
}

#[test]
fn a_track_plays_itself_and_the_rest_of_the_discography() {
    let tracks = sample();
    let none = HashSet::new();

    // A-a2 sits at model index 2; the queue runs to the end of the artist
    assert_eq!(range_for(&tracks, &none, "A-a2"), Some(2..tracks.len()));
    assert_eq!(range_for(&tracks, &none, "B-b1"), Some(4..tracks.len()));
}

#[test]
fn an_album_header_plays_exactly_that_album() {
    let tracks = sample();
    let none = HashSet::new();

    assert_eq!(range_for(&tracks, &none, "_album_A"), Some(1..3));
    assert_eq!(range_for(&tracks, &none, "_album_B"), Some(4..5));
    assert_eq!(range_for(&tracks, &none, "_album_C"), Some(6..8));
}

#[test]
fn a_folded_header_still_plays_its_whole_album() {
    let tracks = sample();

    // with B folded the row after its header is C's header, so anything inferring album scope from
    // the next visible row enqueues nothing
    assert_eq!(range_for(&tracks, &folded(&["B"]), "_album_B"), Some(4..5));
    assert_eq!(range_for(&tracks, &folded(&["A", "B", "C"]), "_album_C"), Some(6..8));
}

/// The temporary-queue paths resolve a header the same way. Those need `App`, but the scoping they
/// depend on is checkable here.
#[test]
fn an_album_resolves_to_its_own_tracks_and_nothing_after_it() {
    let tracks = sample();

    // never "this album and everything after", which is what a bare start index gives you
    let a = crate::discography::album_tracks(&tracks, "A");
    assert_eq!(a.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["A-a1", "A-a2"]);

    // and a middle album must not run on into the next
    let b = crate::discography::album_tracks(&tracks, "B");
    assert_eq!(b.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["B-b1"]);
}

#[test]
fn folding_does_not_change_what_a_track_enqueues() {
    let tracks = sample();
    let expected = range_for(&tracks, &HashSet::new(), "A-a1");

    // A stays open so a1 has a row at all, but folding behind it must not truncate the queue
    assert_eq!(range_for(&tracks, &folded(&["B"]), "A-a1"), expected);
    assert_eq!(range_for(&tracks, &folded(&["B", "C"]), "A-a1"), expected);
}

#[test]
fn folding_shifts_rows_without_shifting_the_resolved_range() {
    let tracks = sample();
    let collapsed = folded(&["A"]);
    let view = DiscographyView::build(&tracks, "", &collapsed);

    // B-b1 moved from row 4 to row 2 …
    assert_eq!(
        visible_ids(&view, &tracks),
        vec!["_album_A", "_album_B", "B-b1", "_album_C", "C-c1", "C-c2"]
    );
    assert_eq!(view.row_of_id(&tracks, "B-b1"), Some(2));
    // … but still resolves to the same slice of the model
    assert_eq!(play_range(&tracks, &view, 2), Some(4..tracks.len()));
}

#[test]
fn a_search_hit_plays_the_rest_of_the_discography_not_just_the_matches() {
    let tracks = sample();
    let view = DiscographyView::build(&tracks, "b1", &HashSet::new());

    // one visible row, but the queue is the model tail
    assert_eq!(view.len(), 1);
    assert_eq!(play_range(&tracks, &view, 0), Some(4..tracks.len()));
}

#[test]
fn a_row_that_does_not_exist_resolves_to_nothing() {
    let tracks = sample();
    let view = DiscographyView::build(&tracks, "", &HashSet::new());

    assert_eq!(play_range(&tracks, &view, 999), None);
    assert_eq!(play_range(&[], &DiscographyView::build(&[], "", &HashSet::new()), 0), None);
}
