use std::collections::HashSet;

use super::{folded, model, visible_ids};
use crate::discography::{album_span, album_tracks, header_index, DiscographyView};

fn sample() -> Vec<crate::client::DiscographySong> {
    model(&[("A", &["a1", "a2"]), ("B", &["b1"]), ("C", &["c1", "c2", "c3"])])
}

#[test]
fn nothing_folded_shows_every_row_in_model_order() {
    let tracks = sample();
    let view = DiscographyView::build(&tracks, "", &HashSet::new());

    assert_eq!(view.len(), tracks.len());
    assert_eq!(view.rows(), (0..tracks.len()).collect::<Vec<_>>().as_slice());
    // the one case where rows and model indices coincide
    for m in 0..tracks.len() {
        assert_eq!(view.row_of(m), Some(m));
        assert_eq!(view.model_index(m), Some(m));
    }
}

#[test]
fn folding_an_album_hides_its_tracks_but_keeps_its_header() {
    let tracks = sample();
    let view = DiscographyView::build(&tracks, "", &folded(&["B"]));

    assert_eq!(
        visible_ids(&view, &tracks),
        vec!["_album_A", "A-a1", "A-a2", "_album_B", "_album_C", "C-c1", "C-c2", "C-c3"]
    );
}

#[test]
fn folding_everything_leaves_only_headers() {
    let tracks = sample();
    let view = DiscographyView::build(&tracks, "", &folded(&["A", "B", "C"]));

    assert_eq!(visible_ids(&view, &tracks), vec!["_album_A", "_album_B", "_album_C"]);
}

#[test]
fn hidden_tracks_have_no_row() {
    let tracks = sample();
    let view = DiscographyView::build(&tracks, "", &folded(&["A"]));

    assert_eq!(view.row_of_id(&tracks, "A-a1"), None);
    assert_eq!(view.row_of_id(&tracks, "_album_A"), Some(0));
    // B's header shifted up by the two hidden A tracks
    assert_eq!(view.row_of_id(&tracks, "_album_B"), Some(1));
    assert_eq!(view.row_of_id(&tracks, "B-b1"), Some(2));
}

#[test]
fn row_and_id_lookups_round_trip() {
    let tracks = sample();
    let view = DiscographyView::build(&tracks, "", &folded(&["A", "C"]));

    for row in 0..view.len() {
        let id = view.track(&tracks, row).unwrap().id.clone();
        assert_eq!(view.row_of_id(&tracks, &id), Some(row), "row {} did not round-trip", row);
    }
}

#[test]
fn an_active_search_ignores_folding_so_every_track_stays_reachable() {
    let tracks = sample();
    // b1 lives in B, which is folded
    let view = DiscographyView::build(&tracks, "b1", &folded(&["A", "B", "C"]));

    assert_eq!(visible_ids(&view, &tracks), vec!["B-b1"]);
    assert!(view.row_of_id(&tracks, "B-b1").is_some());
}

#[test]
fn header_at_or_above_finds_the_owning_album_across_folded_rows() {
    let tracks = sample();
    let view = DiscographyView::build(&tracks, "", &folded(&["A"]));

    // rows: 0 _album_A (folded), 1 _album_B, 2 B-b1, 3 _album_C, 4 C-c1
    assert_eq!(view.header_at_or_above(&tracks, 0), Some((0, "A".to_string())));
    assert_eq!(view.header_at_or_above(&tracks, 2), Some((1, "B".to_string())));
    assert_eq!(view.header_at_or_above(&tracks, 4), Some((3, "C".to_string())));
}

#[test]
fn empty_model_yields_an_empty_view() {
    let view = DiscographyView::build(&[], "", &HashSet::new());
    assert!(view.is_empty());
    assert_eq!(view.model_index(0), None);
    assert_eq!(view.row_of(0), None);
}

#[test]
fn album_span_covers_exactly_that_albums_tracks() {
    let tracks = sample();

    assert_eq!(album_span(&tracks, header_index(&tracks, "A").unwrap()), 1..3);
    assert_eq!(album_span(&tracks, header_index(&tracks, "B").unwrap()), 4..5);
    // the last album runs to the end
    assert_eq!(album_span(&tracks, header_index(&tracks, "C").unwrap()), 6..9);
}

#[test]
fn album_span_of_an_album_with_no_tracks_is_empty() {
    let tracks = model(&[("A", &["a1"]), ("B", &[]), ("C", &["c1"])]);
    let span = album_span(&tracks, header_index(&tracks, "B").unwrap());

    assert!(span.is_empty());
    assert!(album_tracks(&tracks, "B").is_empty());
}

#[test]
fn album_tracks_excludes_the_header_despite_the_shared_parent_id() {
    let tracks = sample();
    let ids: Vec<&str> = album_tracks(&tracks, "A").iter().map(|t| t.id.as_str()).collect();

    // a naive `parent_id == album_id` filter would pick up the header too
    assert_eq!(ids, vec!["A-a1", "A-a2"]);
}

#[test]
fn album_tracks_of_an_unknown_album_is_empty() {
    let tracks = sample();
    assert!(album_tracks(&tracks, "nope").is_empty());
    assert_eq!(header_index(&tracks, "nope"), None);
}
