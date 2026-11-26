use crate::client::{Album, Artist};
use crate::helpers::{Preferences, Symbols};
use crate::popup::{sort_of, PopupCommand, PopupMenu};
use crate::sort::compare_album_artist;
use crate::tui::Sort;

fn album(name: &str, artists: &[&str]) -> Album {
    Album {
        name: name.to_string(),
        album_artists: artists
            .iter()
            .map(|name| {
                let mut artist = Artist::default();
                artist.name = name.to_string();
                artist
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn album_artist_sort_uses_only_the_first_artist() {
    let mut albums = [album("First", &["Zebra", "Alpha"]), album("Second", &["Beta"])];
    albums.sort_by(compare_album_artist);
    assert_eq!(albums.map(|a| a.name), ["Second", "First"]);
}

#[test]
fn album_artist_sort_ignores_case_and_breaks_ties_by_album_name() {
    let mut albums = [
        album("zoo", &["alpha"]),
        album("The beta", &["ALPHA"]),
        album("Aardvark", &["Alpha"]),
        album("Earlier title", &["Beta"]),
    ];
    albums.sort_by(compare_album_artist);
    assert_eq!(albums.map(|a| a.name), ["Aardvark", "The beta", "zoo", "Earlier title"]);
}

#[test]
fn album_artist_sort_puts_missing_artists_last_and_sorts_their_titles() {
    let mut albums = [album("Beta", &[]), album("Zebra", &["Zebra"]), album("alpha", &[])];
    albums.sort_by(compare_album_artist);
    assert_eq!(albums.map(|a| a.name), ["Zebra", "alpha", "Beta"]);
    let mut empty: Vec<Album> = vec![];
    empty.sort_by(compare_album_artist);
    assert!(empty.is_empty());
}

#[test]
fn album_artist_sort_is_an_album_only_menu_option() {
    let symbols = Symbols::default();
    let options = PopupMenu::AlbumsChangeSort {}.options(&symbols);
    let row = options.iter().position(|o| o.label == "Album artist").unwrap();
    assert!(matches!(options[row].action, PopupCommand::AlbumArtist));
    assert!(sort_of(&options[row].action) == Some(Sort::AlbumArtist));
    assert_eq!(
        options.iter().position(|o| sort_of(&o.action) == Some(Sort::AlbumArtist)),
        Some(row)
    );
    for menu in [
        PopupMenu::ArtistsChangeSort {},
        PopupMenu::PlaylistsChangeSort {},
        PopupMenu::TrackAlbumOrder {},
    ] {
        assert!(menu
            .options(&symbols)
            .iter()
            .all(|o| { sort_of(&o.action) != Some(Sort::AlbumArtist) }));
    }
}

#[test]
fn album_sort_preferences_round_trip_and_keep_legacy_values() {
    let preferences = Preferences::new("server".to_string());
    let mut json = serde_json::to_value(preferences).unwrap();
    for value in [
        "Ascending",
        "Descending",
        "PremiereDate",
        "Duration",
        "DateCreated",
        "DateCreatedInverse",
        "Random",
        "AlbumArtist",
    ] {
        json["album_sort"] = value.into();
        let preferences: Preferences = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(preferences).unwrap()["album_sort"], value);
    }
    json.as_object_mut().unwrap().remove("album_sort");
    let preferences: Preferences = serde_json::from_value(json.clone()).unwrap();
    assert!(preferences.album_sort == Sort::Ascending);
    json["album_sort"] = "NotASort".into();
    assert!(serde_json::from_value::<Preferences>(json).is_err());
}
