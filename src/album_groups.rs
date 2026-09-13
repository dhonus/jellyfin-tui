//! Genre / year rows for the Albums tab. They're synthetic `Album`s, like the discography's album
//! headers, so the list code works on them unchanged.

use crate::client::{Album, DiscographySong};
use crate::database::extension::get_album_tracks;
use crate::helpers::{LogErr, Selectable};
use crate::keyboard::{Action, ActiveSection};
use crate::sort;
use crate::tui::App;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Id prefix of a synthetic group row.
pub const ALBUM_GROUP_PREFIX: &str = "_group_";

/// Albums per server request when queueing a group.
const FETCH_CHUNK: usize = 50;

/// Groups this big warn before fetching.
const SLOW_GROUP_ALBUMS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AlbumView {
    #[default]
    Albums,
    Genres,
    Years,
}

impl AlbumView {
    pub fn next(self) -> Self {
        match self {
            AlbumView::Albums => AlbumView::Genres,
            AlbumView::Genres => AlbumView::Years,
            AlbumView::Years => AlbumView::Albums,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            AlbumView::Albums => "Albums",
            AlbumView::Genres => "Genres",
            AlbumView::Years => "Years",
        }
    }

    /// For the vertical layout's tab bar.
    pub fn short_name(self) -> &'static str {
        match self {
            AlbumView::Albums => "Alb",
            AlbumView::Genres => "Gen",
            AlbumView::Years => "Year",
        }
    }
}

/// The untagged row always comes last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GroupSort {
    /// A to Z, oldest year first
    #[default]
    Ascending,
    /// Z to A, newest year first
    Descending,
    MostAlbums,
}

impl GroupSort {
    pub fn most_albums() -> Self {
        GroupSort::MostAlbums
    }
}

#[derive(Debug, Clone, Copy)]
pub enum GroupQueue {
    Play,
    Append,
    Temporary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlbumFacet {
    Genre(String),
    Year(u64),
    NoGenre,
    NoYear,
}

impl AlbumFacet {
    /// Where Esc returns to.
    pub fn view(&self) -> AlbumView {
        match self {
            AlbumFacet::Genre(_) | AlbumFacet::NoGenre => AlbumView::Genres,
            AlbumFacet::Year(_) | AlbumFacet::NoYear => AlbumView::Years,
        }
    }

    fn untagged(view: AlbumView) -> Option<Self> {
        match view {
            AlbumView::Albums => None,
            AlbumView::Genres => Some(AlbumFacet::NoGenre),
            AlbumView::Years => Some(AlbumFacet::NoYear),
        }
    }

    pub fn matches(&self, album: &Album) -> bool {
        match self {
            AlbumFacet::Genre(genre) => {
                let genre = genre.to_lowercase();
                album.genres.iter().any(|g| g.trim().to_lowercase() == genre)
            }
            AlbumFacet::Year(year) => album.production_year == *year,
            AlbumFacet::NoGenre => album.genres.iter().all(|g| g.trim().is_empty()),
            AlbumFacet::NoYear => album.production_year == 0,
        }
    }

    pub fn label(&self) -> String {
        match self {
            AlbumFacet::Genre(genre) => genre.clone(),
            AlbumFacet::Year(year) => year.to_string(),
            AlbumFacet::NoGenre => "No genre".to_string(),
            AlbumFacet::NoYear => "Unknown year".to_string(),
        }
    }

    pub fn row_id(&self) -> String {
        match self {
            AlbumFacet::Genre(genre) => format!("{}genre:{}", ALBUM_GROUP_PREFIX, genre),
            AlbumFacet::Year(year) => format!("{}year:{}", ALBUM_GROUP_PREFIX, year),
            AlbumFacet::NoGenre => format!("{}nogenre", ALBUM_GROUP_PREFIX),
            AlbumFacet::NoYear => format!("{}noyear", ALBUM_GROUP_PREFIX),
        }
    }

    pub fn from_row_id(id: &str) -> Option<Self> {
        let rest = id.strip_prefix(ALBUM_GROUP_PREFIX)?;
        match rest {
            "nogenre" => return Some(AlbumFacet::NoGenre),
            "noyear" => return Some(AlbumFacet::NoYear),
            _ => {}
        }
        if let Some(genre) = rest.strip_prefix("genre:") {
            return Some(AlbumFacet::Genre(genre.to_string()));
        }
        rest.strip_prefix("year:")?.parse().ok().map(AlbumFacet::Year)
    }
}

pub fn is_group_row(id: &str) -> bool {
    id.starts_with(ALBUM_GROUP_PREFIX)
}

/// Rows plus album counts by row id. Genres merge case-insensitively, untagged albums go last.
pub fn group_rows(
    view: AlbumView,
    albums: &[Album],
    sort: GroupSort,
) -> (Vec<Album>, HashMap<String, usize>) {
    let Some(untagged) = AlbumFacet::untagged(view) else {
        return (vec![], HashMap::new());
    };

    let mut groups: HashMap<String, (AlbumFacet, usize)> = HashMap::new();
    let mut untagged_count = 0;
    for album in albums {
        let facets: Vec<AlbumFacet> = match view {
            AlbumView::Genres => {
                // "Rock" and "rock" on one album count once
                let mut seen = HashSet::new();
                album
                    .genres
                    .iter()
                    .map(|g| g.trim())
                    .filter(|g| !g.is_empty() && seen.insert(g.to_lowercase()))
                    .map(|g| AlbumFacet::Genre(g.to_string()))
                    .collect()
            }
            AlbumView::Years if album.production_year > 0 => {
                vec![AlbumFacet::Year(album.production_year)]
            }
            _ => vec![],
        };
        if facets.is_empty() {
            untagged_count += 1;
        }
        for facet in facets {
            groups.entry(facet.label().to_lowercase()).or_insert((facet, 0)).1 += 1;
        }
    }

    let mut groups: Vec<(AlbumFacet, usize)> = groups.into_values().collect();
    groups.sort_by(|(a, a_count), (b, b_count)| {
        let by_label = match (a, b) {
            (AlbumFacet::Year(a), AlbumFacet::Year(b)) => a.cmp(b),
            _ => sort::compare(&a.label().to_lowercase(), &b.label().to_lowercase()),
        };
        match sort {
            GroupSort::Ascending => by_label,
            GroupSort::Descending => by_label.reverse(),
            GroupSort::MostAlbums => b_count.cmp(a_count).then(by_label),
        }
    });
    if untagged_count > 0 {
        groups.push((untagged, untagged_count));
    }

    let counts = groups.iter().map(|(facet, n)| (facet.row_id(), *n)).collect();
    let rows = groups
        .into_iter()
        .map(|(facet, _)| Album { id: facet.row_id(), name: facet.label(), ..Default::default() })
        .collect();
    (rows, counts)
}

impl App {
    pub fn cycle_album_view(&mut self) {
        self.state.album_view = self.state.album_view.next();
        self.state.album_facet = None;
        self.state.albums_search_term.clear();
        self.state.active_section = ActiveSection::List;
        self.reorder_lists();
        self.album_select_by_index(0);
        self.rearm_auto_browse();

        if !self.preferences.album_views_discovered {
            self.preferences.album_views_discovered = true;
            let _ = self.preferences.save().log_err("save preferences");
        }
    }

    /// Shown until the views have been used once.
    pub fn hint_album_views(&mut self) {
        if self.preferences.album_views_discovered {
            return;
        }
        let key = self.key_hint(&Action::Tab(2), "<2>");
        self.tip(format!("Press {} again to browse albums by genre or year", key));
    }

    /// False for a real album.
    pub fn open_album_group(&mut self, id: &str) -> bool {
        let Some(facet) = AlbumFacet::from_row_id(id) else {
            return false;
        };
        self.state.album_facet = Some(facet);
        self.state.album_view = AlbumView::Albums;
        self.state.albums_search_term.clear();
        self.reorder_lists();
        self.album_select_by_index(0);
        self.rearm_auto_browse();
        true
    }

    pub fn close_album_group(&mut self) -> bool {
        let Some(facet) = self.state.album_facet.take() else {
            return false;
        };
        self.state.album_view = facet.view();
        self.state.albums_search_term.clear();
        self.reorder_lists();
        self.reposition_cursor(&facet.row_id(), Selectable::Album);
        true
    }

    /// Clears the narrowing if it hides `album_id`.
    pub fn unhide_album(&mut self, album_id: &str) {
        let narrowed =
            self.state.album_facet.is_some() || self.state.album_view != AlbumView::Albums;
        if narrowed && !self.albums.iter().any(|a| a.id == album_id) {
            self.state.album_facet = None;
            self.state.album_view = AlbumView::Albums;
            self.reorder_lists();
        }
    }

    pub fn album_group_sort(&self, view: AlbumView) -> GroupSort {
        match view {
            AlbumView::Genres => self.preferences.genre_sort,
            AlbumView::Years => self.preferences.year_sort,
            AlbumView::Albums => GroupSort::default(),
        }
    }

    pub fn set_album_group_sort(&mut self, view: AlbumView, sort: GroupSort) {
        match view {
            AlbumView::Genres => self.preferences.genre_sort = sort,
            AlbumView::Years => self.preferences.year_sort = sort,
            AlbumView::Albums => return,
        }
        let _ = self.preferences.save().log_err("save preferences");
        self.reorder_lists();
    }

    /// Album after album, by artist then release. Uncached albums are fetched in batches.
    pub async fn album_group_tracks(&self, facet: &AlbumFacet) -> Vec<DiscographySong> {
        let artist = |album: &Album| {
            album.album_artists.first().map(|a| a.name.to_lowercase()).unwrap_or_default()
        };
        let mut albums: Vec<&Album> =
            self.original_albums.iter().filter(|a| facet.matches(a)).collect();
        albums.sort_by(|a, b| {
            sort::compare(&artist(a), &artist(b))
                .then_with(|| a.premiere_date.cmp(&b.premiere_date))
                .then_with(|| sort::compare(&a.name.to_lowercase(), &b.name.to_lowercase()))
        });

        let mut by_album: HashMap<String, Vec<DiscographySong>> = HashMap::new();
        let mut missing = vec![];
        for album in &albums {
            match get_album_tracks(&self.db.pool, &album.id, self.client.as_ref()).await {
                Ok(tracks) if !tracks.is_empty() => {
                    by_album.insert(album.id.clone(), tracks);
                }
                _ => missing.push(album.id.clone()),
            }
        }
        if let Some(client) = &self.client {
            for chunk in missing.chunks(FETCH_CHUNK) {
                let Ok(tracks) = client.tracks_by_album_ids(chunk).await else {
                    continue;
                };
                for track in tracks {
                    by_album.entry(track.album_id.clone()).or_default().push(track);
                }
            }
        }

        albums
            .iter()
            .filter_map(|album| by_album.remove(&album.id))
            .flat_map(|mut tracks| {
                tracks.sort_by_key(|t| (t.parent_index_number, t.index_number));
                tracks
            })
            .filter(|t| !t.disliked)
            .collect()
    }

    pub async fn queue_album_group(&mut self, facet: &AlbumFacet, how: GroupQueue) {
        // the fetch blocks the UI; offline it's quick
        let albums = self.original_albums.iter().filter(|a| facet.matches(a)).count();
        if self.client.is_some() && albums >= SLOW_GROUP_ALBUMS {
            self.notify(format!(
                "Gathering {} albums from {}, this might take a while",
                albums,
                facet.label()
            ));
        }
        let tracks = self.album_group_tracks(facet).await;
        if tracks.is_empty() {
            self.warn(format!("Nothing playable in {}", facet.label()));
            return;
        }
        match how {
            GroupQueue::Play => self.initiate_main_queue(&tracks, 0).await,
            GroupQueue::Append => self.append_to_main_queue(&tracks, 0).await,
            GroupQueue::Temporary => self.push_to_temporary_queue(&tracks, 0, tracks.len()).await,
        }
    }

    pub fn album_pane_noun(&self) -> &'static str {
        match self.state.album_view {
            AlbumView::Albums => "albums",
            AlbumView::Genres => "genres",
            AlbumView::Years => "years",
        }
    }
}
