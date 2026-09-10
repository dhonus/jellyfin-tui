//! Tests for the genre / year rows of the Albums tab (src/album_groups.rs).

use crate::album_groups::{group_rows, is_group_row, AlbumFacet, AlbumView, GroupSort};
use crate::client::Album;

fn album(id: &str, genres: &[&str], year: u64) -> Album {
    Album {
        id: id.to_string(),
        name: id.to_string(),
        genres: genres.iter().map(|g| g.to_string()).collect(),
        production_year: year,
        ..Default::default()
    }
}

fn library() -> Vec<Album> {
    vec![
        album("a", &["Rock", "Jazz"], 1999),
        album("b", &["rock "], 2024),
        album("c", &["Rock", "rock"], 2024),
        album("d", &[], 0),
        album("e", &["  "], 0),
    ]
}

fn names(rows: &[Album]) -> Vec<&str> {
    rows.iter().map(|r| r.name.as_str()).collect()
}

#[test]
fn genres_merge_case_and_whitespace_and_count_each_album_once() {
    let (rows, counts) = group_rows(AlbumView::Genres, &library(), GroupSort::Ascending);
    assert_eq!(names(&rows), ["Jazz", "Rock", "No genre"]);
    assert_eq!(counts[&rows[0].id], 1);
    assert_eq!(counts[&rows[1].id], 3);
    // a blank tag is no tag
    assert_eq!(counts[&rows[2].id], 2);
}

#[test]
fn years_sort_either_way_with_unknown_last() {
    let (rows, _) = group_rows(AlbumView::Years, &library(), GroupSort::Descending);
    assert_eq!(names(&rows), ["2024", "1999", "Unknown year"]);
    let (rows, _) = group_rows(AlbumView::Years, &library(), GroupSort::Ascending);
    assert_eq!(names(&rows), ["1999", "2024", "Unknown year"]);
}

#[test]
fn most_albums_first_still_keeps_untagged_last() {
    // outnumbers Jazz, still last
    let (rows, _) = group_rows(AlbumView::Genres, &library(), GroupSort::MostAlbums);
    assert_eq!(names(&rows), ["Rock", "Jazz", "No genre"]);
}

#[test]
fn no_untagged_row_when_everything_is_tagged() {
    let tagged = vec![album("a", &["Rock"], 1999)];
    let (rows, _) = group_rows(AlbumView::Genres, &tagged, GroupSort::Ascending);
    assert_eq!(names(&rows), ["Rock"]);
}

#[test]
fn row_ids_round_trip_and_never_look_like_albums() {
    for facet in [
        AlbumFacet::Genre("Hip-Hop: East".into()),
        AlbumFacet::Year(1987),
        AlbumFacet::NoGenre,
        AlbumFacet::NoYear,
    ] {
        let id = facet.row_id();
        assert!(is_group_row(&id));
        assert_eq!(AlbumFacet::from_row_id(&id), Some(facet));
    }
    assert!(!is_group_row("3bcba17ac9404413b2444a99a3007fe8"));
    assert_eq!(AlbumFacet::from_row_id("3bcba17ac9404413b2444a99a3007fe8"), None);
}

#[test]
fn a_facet_matches_what_its_row_counted() {
    let albums = library();
    for view in [AlbumView::Genres, AlbumView::Years] {
        let (rows, counts) = group_rows(view, &albums, GroupSort::Ascending);
        for row in rows {
            let facet = AlbumFacet::from_row_id(&row.id).unwrap();
            let matched = albums.iter().filter(|a| facet.matches(a)).count();
            assert_eq!(matched, counts[&row.id], "{}", row.name);
        }
    }
}
