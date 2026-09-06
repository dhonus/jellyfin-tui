/* --------------------------
Generic select mode
    - Lets a list pane mark several items, then act on them all at once
    - What a key means is up to the pane (playlist entry id, media id, ...); it only has to
      be stable for the lifetime of the session
    - A pane opts in by picking a `SelectPane` variant, entering with `SelectMode::enter`
      seeded with the item under the cursor, and rendering markers via `SelectMode::is_selected`
-------------------------- */

use indexmap::IndexSet;

/// Identifies the list pane a select-mode session is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectPane {
    /// Artist's discography in the Library tab
    LibraryTracks,
    /// Album's tracks in the Albums tab
    AlbumTracks,
    /// A playlist's tracks in the Playlists tab
    PlaylistTracks,
}

/// State of a select-mode session.
///
/// What a key means is up to the pane (playlist entry id, media id, ...); it only has to be
/// stable for the lifetime of the session.
#[derive(Debug, Default)]
pub struct SelectMode {
    /// Pane the session is bound to; `None` while inactive.
    active_pane: Option<SelectPane>,
    selected: IndexSet<String>,
}

impl SelectMode {
    pub fn is_active(&self) -> bool {
        self.active_pane.is_some()
    }

    pub fn is_active_in(&self, pane: SelectPane) -> bool {
        self.active_pane == Some(pane)
    }

    /// The pane the session is bound to; `None` while inactive.
    pub fn pane(&self) -> Option<SelectPane> {
        self.active_pane
    }

    /// Enter select mode in `pane`, seeding the selection with the item under the cursor.
    pub fn enter(&mut self, pane: SelectPane, cursor_key: Option<String>) {
        self.active_pane = Some(pane);
        self.selected.clear();
        if let Some(key) = cursor_key.filter(|k| !k.is_empty()) {
            self.selected.insert(key);
        }
    }

    /// Leave select mode, forgetting every marked item.
    pub fn exit(&mut self) {
        self.active_pane = None;
        self.selected.clear();
    }

    /// Mark `key`, or unmark it if it was already marked. No-op while inactive.
    pub fn toggle(&mut self, key: String) {
        if self.active_pane.is_none() || key.is_empty() {
            return;
        }
        if !self.selected.shift_remove(&key) {
            self.selected.insert(key);
        }
    }

    pub fn is_selected(&self, key: &str) -> bool {
        self.selected.contains(key)
    }

    pub fn len(&self) -> usize {
        self.selected.len()
    }

    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// Every marked key, in insertion order (deterministic, unlike `HashSet`).
    pub fn keys(&self) -> Vec<String> {
        self.selected.iter().cloned().collect()
    }

    /// Every marked key sorted to match `canonical_order` (e.g. album order).
    /// Keys missing from the order keep their insertion order at the end.
    pub fn ordered_keys(&self, canonical_order: &[String]) -> Vec<String> {
        use std::collections::HashMap;
        let position: HashMap<&str, usize> =
            canonical_order.iter().enumerate().map(|(i, k)| (k.as_str(), i)).collect();
        let mut keys = self.keys();
        keys.sort_by_key(|k| position.get(k.as_str()).copied().unwrap_or(usize::MAX));
        keys
    }
}
