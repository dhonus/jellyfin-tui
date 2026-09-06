//! Unit tests for logic that's awkward to exercise through the full App/terminal stack, grouped
//! by the subsystem they cover. discography_view and play_range test the row/model mapping and
//! play-range resolution of album folding; popup tests the queue track popup; select tests the
//! generic select-mode state machine. All of it is pure functions or plain state mutation, so
//! none of it needs a terminal, server or mpv handle.

mod album_column;
mod discography_view;
mod play_range;
mod popup;
mod select;

use crate::client::DiscographySong;

/// A track list shaped the way `group_tracks_into_albums` emits one. Mirrors its field conventions:
/// a header has an empty `album_id` and the album id in `parent_id`, a track has both.
pub fn model(albums: &[(&str, &[&str])]) -> Vec<DiscographySong> {
    let mut tracks = vec![];
    for (album_id, titles) in albums {
        tracks.push(DiscographySong {
            id: format!("{}{}", crate::discography::ALBUM_HEADER_PREFIX, album_id),
            name: format!("Album {}", album_id),
            album_id: String::new(),
            parent_id: album_id.to_string(),
            ..Default::default()
        });
        for (i, title) in titles.iter().enumerate() {
            tracks.push(DiscographySong {
                id: format!("{}-{}", album_id, title),
                name: title.to_string(),
                album: format!("Album {}", album_id),
                album_id: album_id.to_string(),
                parent_id: album_id.to_string(),
                index_number: i as u64 + 1,
                ..Default::default()
            });
        }
    }
    tracks
}

pub fn visible_ids(
    view: &crate::discography::DiscographyView,
    tracks: &[DiscographySong],
) -> Vec<String> {
    view.rows().iter().map(|&m| tracks[m].id.clone()).collect()
}

pub fn folded(albums: &[&str]) -> std::collections::HashSet<String> {
    albums.iter().map(|s| s.to_string()).collect()
}
