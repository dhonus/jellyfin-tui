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

/// Playlist track fixture; duplicates share `id` but have different positions.
fn entry(media_id: &str, position: i64) -> DiscographySong {
    DiscographySong { id: media_id.to_string(), playlist_position: position, ..Default::default() }
}

#[test]
fn selection_resolves_position_keys_back_to_media_ids() {
    // keys are entry positions; adding to a playlist needs the media id, not the key
    let tracks = vec![entry("media-a", 0), entry("media-b", 1)];

    let mut select = SelectMode::default();
    select.enter(SelectPane::PlaylistTracks, Some("pl:1:media-b".to_string()));
    select.toggle("pl:0:media-a".to_string());

    assert_eq!(selected_playlist_media_ids(&tracks, &select), vec!["media-a", "media-b"]);
}

#[test]
fn selection_is_returned_in_playlist_order_not_click_order() {
    let tracks = vec![entry("a", 0), entry("b", 1), entry("c", 2)];

    let mut select = SelectMode::default();
    select.enter(SelectPane::PlaylistTracks, None);
    select.toggle("pl:2:c".to_string());
    select.toggle("pl:0:a".to_string());

    assert_eq!(selected_playlist_media_ids(&tracks, &select), vec!["a", "c"]);
}

#[test]
fn selection_is_empty_for_other_panes() {
    let tracks = vec![entry("a", 0)];
    let mut select = SelectMode::default();
    select.enter(SelectPane::LibraryTracks, Some("a".to_string()));
    assert!(selected_playlist_media_ids(&tracks, &select).is_empty());
}

#[test]
fn playlist_track_key_is_position_and_media_id_and_does_not_collide_for_duplicates() {
    // both position and media id: holds duplicates apart, and a stale position won't resolve.
    let first = entry("media-a", 0);
    let second = entry("media-a", 1);
    let moved = entry("media-a", 3);

    assert_eq!(playlist_track_key(&first), "pl:0:media-a");
    assert_eq!(playlist_track_key(&second), "pl:1:media-a");
    assert_eq!(playlist_track_key(&moved), "pl:3:media-a");
    assert_ne!(playlist_track_key(&first), playlist_track_key(&second));
    assert_ne!(playlist_track_key(&first), playlist_track_key(&moved));
}

use crate::helpers::{format_seconds, format_ticks, wrap_to_width};

#[test]
fn durations_only_grow_an_hours_field_when_they_reach_an_hour() {
    assert_eq!(format_seconds(0), "0:00");
    assert_eq!(format_seconds(9), "0:09");
    assert_eq!(format_seconds(225), "3:45");
    assert_eq!(format_seconds(3599), "59:59");
    assert_eq!(format_seconds(3600), "1:00:00");
    assert_eq!(format_seconds(3753), "1:02:33");
    assert_eq!(format_ticks(225 * 10_000_000), "3:45");
}

#[test]
fn wrapping_measures_columns_not_bytes() {
    // "příliš" is 6 columns but 8 bytes; wrapping on byte length broke a line early for
    // every accented, Cyrillic or CJK lyric
    assert_eq!(wrap_to_width("příliš žluťoučký", 16), vec!["příliš žluťoučký"]);
    assert_eq!(wrap_to_width("příliš žluťoučký", 15), vec!["příliš", "žluťoučký"]);
}

#[test]
fn wrapping_never_emits_a_leading_empty_line_or_panics_when_narrow() {
    assert_eq!(wrap_to_width("supercalifragilistic", 5), vec!["supercalifragilistic"]);
    assert_eq!(wrap_to_width("", 20), vec![""]);
    assert_eq!(wrap_to_width("a b", 0), vec!["a b"]);
}

use crate::helpers::{find_all_subsequences, search_ranked_indices, Searchable};

fn check_ranges(needle: &str, haystack: &str) -> Vec<String> {
    let ranges = find_all_subsequences(needle, haystack);
    let mut last = 0;
    let mut matched = vec![];
    for (start, end) in ranges {
        assert!(last <= start, "{:?} overlaps the previous range in {:?}", (start, end), haystack);
        let _ = &haystack[last..start];
        matched.push(haystack[start..end].to_string());
        last = end;
    }
    let _ = &haystack[last..];
    matched
}

#[test]
fn ranges_index_the_haystack_as_given_not_a_lowercased_copy() {
    assert_eq!(check_ranges("iç", "İstanbul'da çay"), vec!["İ", "ç"]);
    assert_eq!(check_ranges("ny", "İnce çay"), vec!["n", "y"]);
    assert_eq!(check_ranges("i", "İ"), vec!["İ"]);
}

#[test]
fn matching_ignores_case_and_diacritics() {
    assert_eq!(check_ranges("prilis", "Příliš"), vec!["P", "ř", "í", "l", "i", "š"]);
    assert_eq!(check_ranges("ZL", "žluťoučký"), vec!["ž", "l"]);
    assert_eq!(check_ranges("ay", "Çay"), vec!["a", "y"]);
}

#[test]
fn a_needle_that_does_not_fit_matches_nothing() {
    assert!(find_all_subsequences("xyz", "İstanbul").is_empty());
    assert!(find_all_subsequences("ba", "abc").is_empty());
    assert_eq!(find_all_subsequences("", "İstanbul"), vec![]);
}

struct Named(&'static str);
impl Searchable for Named {
    fn id(&self) -> &str {
        self.0
    }
    fn name(&self) -> &str {
        self.0
    }
}

#[test]
fn ranking_prefers_the_tighter_match_and_survives_wide_chars() {
    let items = [Named("Sıla"), Named("İstanbul"), Named("Sokak Lambası")];
    assert_eq!(
        search_ranked_indices(&items, "sla", false)
            .into_iter()
            .map(|i| items[i].0)
            .collect::<Vec<_>>(),
        vec!["Sıla", "Sokak Lambası"]
    );
    assert_eq!(search_ranked_indices(&items, "ist", false), vec![1]);
}
