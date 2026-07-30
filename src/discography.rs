/// The discography pane's two coordinate systems.
///
/// `App::tracks` is the model: every track for the current artist, never filtered, with an album
/// header row before each album. `DiscographyView` is what's rendered, after search and folding.
/// `State::selected_track` is always a view row, never a model index.
///
/// Playback resolves selections back to model ranges (see `play_range`), so folding can't change
/// what ends up in the queue.
use std::collections::HashSet;
use std::ops::Range;

use crate::client::DiscographySong;
use crate::helpers::search_ranked_indices;

pub const ALBUM_HEADER_PREFIX: &str = "_album_";

impl DiscographySong {
    pub fn is_album_header(&self) -> bool {
        self.id.starts_with(ALBUM_HEADER_PREFIX)
    }

    /// `None` for real tracks. Header rows carry an empty `album_id`, so this is the only way to
    /// get an album id out of one.
    pub fn header_album_id(&self) -> Option<&str> {
        self.id.strip_prefix(ALBUM_HEADER_PREFIX)
    }
}

pub fn header_index(tracks: &[DiscographySong], album_id: &str) -> Option<usize> {
    tracks.iter().position(|t| t.header_album_id() == Some(album_id))
}

/// An album's tracks, header excluded. Relies on `group_tracks_into_albums` emitting each album
/// contiguously, header first.
pub fn album_span(tracks: &[DiscographySong], header_index: usize) -> Range<usize> {
    let start = header_index + 1;
    let end = tracks
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, t)| t.is_album_header())
        .map(|(i, _)| i)
        .unwrap_or(tracks.len());
    start..end.max(start)
}

/// Use this rather than filtering on `parent_id` — headers share their album's `parent_id`, so a
/// naive filter picks up an unplayable row.
pub fn album_tracks<'a>(tracks: &'a [DiscographySong], album_id: &str) -> Vec<&'a DiscographySong> {
    match header_index(tracks, album_id) {
        Some(h) => tracks[album_span(tracks, h)].iter().collect(),
        None => vec![],
    }
}

/// Owns its buffers and borrows nothing, so callers can still take `&mut self` after building one.
/// The model slice is passed back into the methods that need it.
#[derive(Debug, Clone, Default)]
pub struct DiscographyView {
    /// row -> index into the model
    rows: Vec<usize>,
    /// index into the model -> row, if currently visible
    row_of_model: Vec<Option<usize>>,
}

impl DiscographyView {
    /// Search wins over folding: ranking reorders rows across album boundaries anyway, and every
    /// track has to stay reachable by search.
    pub fn build(
        tracks: &[DiscographySong],
        search_term: &str,
        collapsed: &HashSet<String>,
    ) -> Self {
        let rows: Vec<usize> = if !search_term.is_empty() {
            search_ranked_indices(tracks, search_term, false)
        } else if collapsed.is_empty() {
            (0..tracks.len()).collect()
        } else {
            tracks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.is_album_header() || !collapsed.contains(&t.album_id))
                .map(|(i, _)| i)
                .collect()
        };

        let mut row_of_model = vec![None; tracks.len()];
        for (row, &model) in rows.iter().enumerate() {
            row_of_model[model] = Some(row);
        }

        Self { rows, row_of_model }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn rows(&self) -> &[usize] {
        &self.rows
    }

    pub fn model_index(&self, row: usize) -> Option<usize> {
        self.rows.get(row).copied()
    }

    pub fn row_of(&self, model_index: usize) -> Option<usize> {
        self.row_of_model.get(model_index).copied().flatten()
    }

    pub fn row_of_id(&self, tracks: &[DiscographySong], id: &str) -> Option<usize> {
        self.row_of(tracks.iter().position(|t| t.id == id)?)
    }

    pub fn track<'a>(
        &self,
        tracks: &'a [DiscographySong],
        row: usize,
    ) -> Option<&'a DiscographySong> {
        self.model_index(row).and_then(|m| tracks.get(m))
    }

    /// The album the cursor is in, as `(row, album id)`. Walks the view, not the model, so folded
    /// rows in between don't throw it off.
    pub fn header_at_or_above(
        &self,
        tracks: &[DiscographySong],
        row: usize,
    ) -> Option<(usize, String)> {
        self.rows
            .iter()
            .enumerate()
            .take(row + 1)
            .rev()
            .find_map(|(r, &m)| tracks[m].header_album_id().map(|id| (r, id.to_string())))
    }
}

/// What Enter on `row` enqueues: a header gives its album, a track gives itself plus the rest of
/// the discography. Folding is deliberately not consulted.
pub fn play_range(
    tracks: &[DiscographySong],
    view: &DiscographyView,
    row: usize,
) -> Option<Range<usize>> {
    let model_index = view.model_index(row)?;
    if tracks.get(model_index)?.is_album_header() {
        Some(album_span(tracks, model_index))
    } else {
        Some(model_index..tracks.len())
    }
}
