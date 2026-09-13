//! `album_column` parsing and the auto rule.

use crate::config::AlbumColumn;

fn parse(yaml: &str) -> AlbumColumn {
    let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    AlbumColumn::from_config(value.get("album_column"))
}

#[test]
fn anything_but_a_boolean_is_auto() {
    assert_eq!(parse("album_column: true"), AlbumColumn::Always);
    assert_eq!(parse("album_column: false"), AlbumColumn::Never);
    assert_eq!(parse("album_column: auto"), AlbumColumn::Auto);
    assert_eq!(parse("art: true"), AlbumColumn::Auto);
}

#[test]
fn auto_hides_the_column_below_the_threshold_unless_searching() {
    assert!(AlbumColumn::Auto.is_visible(false, 200, 140));
    assert!(!AlbumColumn::Auto.is_visible(false, 100, 140));
    // ranking reorders rows across albums, so the album is worth the width
    assert!(AlbumColumn::Auto.is_visible(true, 100, 140));
}

#[test]
fn always_and_never_ignore_search_and_width() {
    for (searching, width) in [(false, 40), (true, 200)] {
        assert!(AlbumColumn::Always.is_visible(searching, width, 140));
        assert!(!AlbumColumn::Never.is_visible(searching, width, 140));
    }
}
