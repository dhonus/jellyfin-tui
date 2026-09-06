//! Tests for the small pure helpers in src/helpers.rs.

use crate::helpers::iso8601_now;

#[test]
fn iso8601_now_does_not_panic_and_round_trips() {
    // chrono's Display returns Err for an unsupported specifier, which `to_string()` turns into
    // a panic. This shipped once as `%.7f` and only blew up on playlist creation, so pin it.
    let now = iso8601_now();
    assert!(chrono::DateTime::parse_from_rfc3339(&now).is_ok(), "not valid rfc3339: {}", now);
}

#[test]
fn iso8601_now_sorts_against_jellyfin_timestamps() {
    // DateCreated is compared as a plain string by the playlist sorts, so ours has to order
    // correctly against the 7-fractional-digit shape the server sends.
    let now = iso8601_now();
    let past = "2001-01-01T00:00:00.0000000Z".to_string();
    let future = "2999-01-01T00:00:00.0000000Z".to_string();

    assert!(past < now, "{} should sort before {}", past, now);
    assert!(now < future, "{} should sort before {}", now, future);

    let mut dates = vec![future.clone(), past.clone(), now.clone()];
    dates.sort();
    assert_eq!(dates, vec![past, now, future]);
}

use crate::client::DiscographySong;
use crate::helpers::{playlist_track_key, selected_playlist_media_ids};
use crate::select::{SelectMode, SelectPane};

/// A playlist track: `playlist_item_id` is the per-entry id the server assigns, `id` is the media.
fn entry(media_id: &str, entry_id: &str) -> DiscographySong {
    DiscographySong {
        id: media_id.to_string(),
        playlist_item_id: entry_id.to_string(),
        ..Default::default()
    }
}

#[test]
fn selection_resolves_entry_ids_back_to_media_ids() {
    // select mode keys playlist tracks by entry id; adding to another playlist needs the media
    // id, and passing the entry ids straight through would ask the server for the wrong tracks
    let tracks = vec![entry("media-a", "entry-1"), entry("media-b", "entry-2")];

    let mut select = SelectMode::default();
    select.enter(SelectPane::PlaylistTracks, Some("entry-2".to_string()));
    select.toggle("entry-1".to_string());

    assert_eq!(selected_playlist_media_ids(&tracks, &select), vec!["media-a", "media-b"]);
}

#[test]
fn selection_is_returned_in_playlist_order_not_click_order() {
    let tracks = vec![entry("a", "e1"), entry("b", "e2"), entry("c", "e3")];

    let mut select = SelectMode::default();
    select.enter(SelectPane::PlaylistTracks, None);
    select.toggle("e3".to_string());
    select.toggle("e1".to_string());

    assert_eq!(selected_playlist_media_ids(&tracks, &select), vec!["a", "c"]);
}

#[test]
fn selection_is_empty_for_other_panes() {
    let tracks = vec![entry("a", "e1")];
    let mut select = SelectMode::default();
    select.enter(SelectPane::LibraryTracks, Some("a".to_string()));
    assert!(selected_playlist_media_ids(&tracks, &select).is_empty());
}

#[test]
fn playlist_track_key_falls_back_to_the_media_id() {
    // tracks appended optimistically have no entry id until the sync fills it in
    let mut track = entry("media-a", "");
    assert_eq!(playlist_track_key(&track), "media-a");
    track.playlist_item_id = "entry-1".to_string();
    assert_eq!(playlist_track_key(&track), "entry-1");
}
