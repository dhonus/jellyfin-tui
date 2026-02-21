/*
This file can look very daunting, but it actually just defines a sort of structure to render popups.
- Each popup is defined as an enum, and each enum variant has a different set of actions that can be taken.
- The `PopupState` struct keeps track of the current state of the popup, such as which option is selected.
- We make a decision as to which action to take based on the current state :)
- The `create_popup` function is responsible for creating and rendering the popup on the screen.
*/
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::Color;
use ratatui::text::Line;
use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    prelude::Text,
    style::{self, Style, Stylize},
    text::Span,
    widgets::{Block, Clear, List, ListItem},
    Frame,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::client::{Album, DiscographySong, LibraryView};
use crate::database::database::{
    t_discography_updater, Command, DeleteCommand, DownloadCommand, RemoveCommand, RenameCommand,
    UpdateCommand,
};
use crate::database::extension::{get_album_tracks, set_selected_libraries, DownloadStatus};
use crate::helpers::{find_all_subsequences, Searchable, Selectable};
use crate::keyboard::{search_ranked_indices, search_ranked_refs};
use crate::themes::theme::Theme;
use crate::{
    client::{Artist, Playlist, ScheduledTask},
    keyboard::{ActiveSection, ActiveTab},
    tui::{Filter, Sort},
};

/// helper function to create a centered rect using up certain percentage of the available rect `r`
fn popup_area(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical =
        Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Start).margin(0);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PopupMenu {
    GenericMessage {
        title: String,
        message: String,
    },
    /**
     * Global commands
     */
    GlobalRoot {
        large_art: bool,
        downloading: bool,
    },
    GlobalRunScheduledTask {
        tasks: Vec<ScheduledTask>,
    },
    GlobalShuffle {
        tracks_n: usize,
        only_played: bool,
        only_unplayed: bool,
        #[serde(default)]
        only_favorite: bool,
    },
    GlobalPickTheme {},
    GlobalSetThemes {
        themes: Vec<crate::themes::theme::Theme>,
    },
    GlobalSelectLibraries {
        libraries: Vec<LibraryView>,
    },
    /**
     * Playlist related popups
     */
    PlaylistRoot {
        playlist_name: String,
    },
    PlaylistSetName {
        playlist_name: String,
        new_name: String,
    },
    PlaylistConfirmRename {
        new_name: String,
    },
    PlaylistConfirmDelete {
        playlist_name: String,
    },
    PlaylistCreate {
        name: String,
        public: bool,
    },
    PlaylistsChangeSort {},
    PlaylistsChangeFilter {},
    /**
     * Track related popups
     */
    TrackRoot {
        track_id: String,
        track_name: String,
        parent_id: String,
        disliked: bool,
    },
    TrackAddToPlaylist {
        track_name: String,
        track_id: String,
        playlists: Vec<Playlist>,
    },
    TrackAlbumsChangeSort {},
    /**
     * Playlist tracks related popups
     */
    PlaylistTracksRoot {
        track_name: String,
        disliked: bool,
    },
    PlaylistTrackAddToPlaylist {
        track_name: String,
        track_id: String,
        playlists: Vec<Playlist>,
    },
    PlaylistTracksRemove {
        track_name: String,
        track_id: String,
        playlist_name: String,
        playlist_id: String,
    },
    /**
     * Artist related popups
     */
    ArtistRoot {
        artist: Artist,
        playing_artists: Option<Vec<Artist>>,
    },
    ArtistJumpToCurrent {
        // this one is for if there are multiple artists for a track
        artists: Vec<Artist>,
    },
    ArtistsChangeFilter {},
    ArtistsChangeSort {},
    /**
     * Albums related popups
     */
    AlbumsRoot {
        album: Album,
    },
    AlbumsChangeFilter {},
    AlbumsChangeSort {},
    /**
     * Album tracks related popups
     */
    AlbumTrackRoot {
        track_id: String,
        track_name: String,
        disliked: bool,
    },
}

#[derive(Debug, Clone)]
pub enum Action {
    None,
    Yes,
    No,
    Ok,
    Play,
    Rename,
    Confirm,
    Type,
    Delete,
    Append,
    AppendTemporary,
    Cancel,
    CancelDownloads,
    AddToPlaylist { playlist_id: String },
    GoAlbum,
    JumpToCurrent,
    Download,
    RemoveDownload,
    Refresh,
    Create,
    Toggle,
    ChangeFilter,
    ChangeOrder,
    PremiereDate,
    Ascending,
    Descending,
    DateCreated,
    DateCreatedInverse,
    DurationAsc,
    DurationDesc,
    TitleAsc,
    TitleDesc,
    Random,
    Normal,
    ShowFavoritesFirst,
    RunScheduledTasks,
    ToggleLibrary { library_id: String },
    SelectLibraries,
    RunScheduledTask { task: Option<ScheduledTask> },
    ChangeCoverArtLayout,
    OnlyPlayed,
    OnlyUnplayed,
    OnlyFavorite,
    OfflineRepair,
    ResetSectionWidths,
    FetchArt,
    GlobalSetTheme,
    SetTheme { theme: crate::themes::theme::Theme },
    Custom,
    SetCustomTheme { theme: crate::themes::theme::Theme },
    Dislike,
}

#[derive(Clone, Debug)]
pub struct PopupAction {
    label: String,
    pub action: Action,
    id: String,
    style: Style,
    online: bool,
}

impl Searchable for PopupAction {
    fn id(&self) -> &str {
        self.id.as_str()
    }
    fn name(&self) -> &str {
        self.label.as_str()
    }
}

impl PopupAction {
    fn new(label: String, action: Action, style: Style, online: bool) -> Self {
        // this better be unique :)
        let id = format!("{}-{:?}", label, action);
        Self { label, action, id, style, online }
    }
}

impl PopupMenu {
    fn title(&self) -> String {
        match self {
            PopupMenu::GenericMessage { title, .. } => title.to_string(),
            // ---------- Global commands ---------- //
            PopupMenu::GlobalRoot { .. } => "Global Commands".to_string(),
            PopupMenu::GlobalRunScheduledTask { .. } => "Run a Jellyfin task".to_string(),
            PopupMenu::GlobalShuffle { .. } => "Global Shuffle".to_string(),
            PopupMenu::GlobalSetThemes { .. } => "Set Theme".to_string(),
            PopupMenu::GlobalPickTheme { .. } => "Pick variant".to_string(),
            PopupMenu::GlobalSelectLibraries { .. } => "Select Libraries".to_string(),
            // ---------- Playlists ---------- //
            PopupMenu::PlaylistRoot { playlist_name, .. } => playlist_name.to_string(),
            PopupMenu::PlaylistSetName { .. } => "Type to change name".to_string(),
            PopupMenu::PlaylistConfirmRename { .. } => "Confirm Rename".to_string(),
            PopupMenu::PlaylistConfirmDelete { .. } => "Confirm Delete".to_string(),
            PopupMenu::PlaylistCreate { .. } => "Create Playlist".to_string(),
            PopupMenu::PlaylistsChangeSort {} => "Change sort order".to_string(),
            PopupMenu::PlaylistsChangeFilter {} => "Change filter".to_string(),
            // ---------- Tracks ---------- //
            PopupMenu::TrackRoot { track_name, .. } => track_name.to_string(),
            PopupMenu::TrackAddToPlaylist { track_name, .. } => track_name.to_string(),
            PopupMenu::TrackAlbumsChangeSort {} => "Change album order".to_string(),
            // ---------- Playlist tracks ---------- //
            PopupMenu::PlaylistTracksRoot { track_name, .. } => track_name.to_string(),
            PopupMenu::PlaylistTrackAddToPlaylist { track_name, .. } => track_name.to_string(),
            PopupMenu::PlaylistTracksRemove { track_name, .. } => track_name.to_string(),
            // ---------- Artists ---------- //
            PopupMenu::ArtistRoot { artist, .. } => artist.name.to_string(),
            PopupMenu::ArtistJumpToCurrent { artists, .. } => {
                format!("Which of these {} to jump to?", artists.len())
            }
            PopupMenu::ArtistsChangeFilter {} => "Change filter".to_string(),
            PopupMenu::ArtistsChangeSort {} => "Change sort".to_string(),
            // ---------- Albums ---------- //
            PopupMenu::AlbumsRoot { album } => album.name.to_string(),
            PopupMenu::AlbumsChangeFilter {} => "Change filter".to_string(),
            PopupMenu::AlbumsChangeSort {} => "Change sort".to_string(),
            // ---------- Album tracks ---------- //
            PopupMenu::AlbumTrackRoot { track_name, .. } => track_name.to_string(),
        }
    }

    // Return the list of options displayed by this menu
    pub fn options(&self) -> Vec<PopupAction> {
        match self {
            PopupMenu::GenericMessage { message, .. } => vec![
                PopupAction::new(message.to_string(), Action::Ok, Style::default(), false),
                PopupAction::new("Ok".to_string(), Action::Ok, Style::default(), false),
            ],
            // ---------- Global commands ---------- //
            PopupMenu::GlobalRoot { large_art, downloading } => vec![
                PopupAction::new(
                    "Synchronize with Jellyfin (runs every 10 minutes)".to_string(),
                    Action::Refresh,
                    Style::default(),
                    true,
                ),
                PopupAction::new(
                    "Run a Jellyfin task".to_string(),
                    Action::RunScheduledTasks,
                    Style::default(),
                    true,
                ),
                PopupAction::new(
                    if *large_art {
                        "Switch to small artwork".to_string()
                    } else {
                        "Switch to large artwork".to_string()
                    },
                    Action::ChangeCoverArtLayout,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Theme".to_string(),
                    Action::GlobalSetTheme,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Select music libraries".to_string(),
                    Action::SelectLibraries,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Repair offline downloads (could take a minute)".to_string(),
                    Action::OfflineRepair,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Stop downloading and abort queued".to_string(),
                    Action::CancelDownloads,
                    Style::default().fg(if *downloading {
                        style::Color::Red
                    } else {
                        style::Color::DarkGray
                    }),
                    true,
                ),
                PopupAction::new(
                    "Reset section widths".to_string(),
                    Action::ResetSectionWidths,
                    Style::default(),
                    false,
                ),
            ],
            PopupMenu::GlobalRunScheduledTask { tasks } => {
                let mut actions = vec![];
                let mut categories =
                    tasks.iter().map(|t| t.category.clone()).collect::<Vec<String>>();
                categories.sort();
                categories.dedup();
                for category in categories {
                    for task in tasks.iter().filter(|t| t.category == category) {
                        actions.push(PopupAction::new(
                            format!("{}: {} ({})", category, task.name, task.description),
                            Action::RunScheduledTask { task: Some(task.clone()) },
                            Style::default(),
                            true,
                        ));
                    }
                }
                actions
            }
            PopupMenu::GlobalSelectLibraries { libraries } => {
                let mut actions = vec![];

                for library in libraries {
                    actions.push(PopupAction::new(
                        if library.selected {
                            format!("✓ {}", library.name)
                        } else {
                            format!("  {}", library.name)
                        },
                        Action::ToggleLibrary { library_id: library.id.clone() },
                        Style::default(),
                        false,
                    ));
                }
                actions.push(PopupAction::new(
                    "Confirm".to_string(),
                    Action::Confirm,
                    Style::default(),
                    false,
                ));
                actions
            }
            PopupMenu::GlobalShuffle { tracks_n, only_played, only_unplayed, only_favorite } => {
                vec![
                    PopupAction::new(
                        format!("Shuffle {} tracks. +/- to change", tracks_n),
                        Action::None,
                        Style::default(),
                        true,
                    ),
                    PopupAction::new(
                        if *only_played {
                            "✓ Only played tracks"
                        } else {
                            "  Only played tracks"
                        }
                        .to_string(),
                        Action::OnlyPlayed,
                        Style::default(),
                        true,
                    ),
                    PopupAction::new(
                        if *only_unplayed {
                            "✓ Only unplayed tracks"
                        } else {
                            "  Only unplayed tracks"
                        }
                        .to_string(),
                        Action::OnlyUnplayed,
                        Style::default(),
                        true,
                    ),
                    PopupAction::new(
                        if *only_favorite {
                            "✓ Only favorite tracks"
                        } else {
                            "  Only favorite tracks"
                        }
                        .to_string(),
                        Action::OnlyFavorite,
                        Style::default(),
                        true,
                    ),
                    PopupAction::new("Play".to_string(), Action::Play, Style::default(), true),
                ]
            }
            PopupMenu::GlobalPickTheme {} => {
                let mut actions: Vec<PopupAction> = Theme::builtin_themes()
                    .into_iter()
                    .map(|t| {
                        PopupAction::new(
                            t.name.clone(),
                            Action::SetTheme { theme: t },
                            Style::default(),
                            false,
                        )
                    })
                    .collect();

                actions.push(PopupAction::new(
                    "Custom Themes".to_string(),
                    Action::Custom,
                    Style::default(),
                    false,
                ));

                actions
            }
            PopupMenu::GlobalSetThemes { themes } => {
                let mut actions = vec![];
                for theme in themes {
                    actions.push(PopupAction::new(
                        theme.name.clone(),
                        Action::SetCustomTheme { theme: theme.clone() },
                        Style::default(),
                        false,
                    ));
                }
                actions.push(PopupAction::new(
                    "Back".to_string(),
                    Action::None,
                    Style::default(),
                    false,
                ));
                actions
            }
            // ---------- Playlists ----------
            PopupMenu::PlaylistRoot { .. } => vec![
                PopupAction::new("Play".to_string(), Action::Play, Style::default(), false),
                PopupAction::new(
                    "Append to main queue".to_string(),
                    Action::Append,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Append to temporary queue".to_string(),
                    Action::AppendTemporary,
                    Style::default(),
                    false,
                ),
                PopupAction::new("Rename".to_string(), Action::Rename, Style::default(), true),
                PopupAction::new(
                    "Download all tracks".to_string(),
                    Action::Download,
                    Style::default(),
                    true,
                ),
                PopupAction::new(
                    "Remove downloaded tracks".to_string(),
                    Action::RemoveDownload,
                    Style::default(),
                    true,
                ),
                PopupAction::new(
                    "Create new playlist".to_string(),
                    Action::Create,
                    Style::default(),
                    true,
                ),
                PopupAction::new(
                    "Change filter".to_string(),
                    Action::ChangeFilter,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Change sort order".to_string(),
                    Action::ChangeOrder,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Delete".to_string(),
                    Action::Delete,
                    Style::default().fg(style::Color::Red),
                    true,
                ),
            ],
            PopupMenu::PlaylistSetName { new_name, .. } => {
                vec![
                    PopupAction::new(
                        // if new_name is empty, then the user has not typed anything yet. Otherwise show the new name
                        if new_name.is_empty() {
                            "Type in the new name".to_string()
                        } else {
                            format!("Name: {}", new_name)
                        },
                        Action::Type,
                        Style::default(),
                        true,
                    ),
                    PopupAction::new(
                        "Confirm".to_string(),
                        Action::Confirm,
                        Style::default(),
                        true,
                    ),
                    PopupAction::new("Cancel".to_string(), Action::Cancel, Style::default(), true),
                ]
            }
            PopupMenu::PlaylistConfirmRename { new_name, .. } => vec![
                PopupAction::new(
                    format!("Rename to: {}", new_name),
                    Action::Rename,
                    Style::default(),
                    true,
                ),
                PopupAction::new("Yes".to_string(), Action::Yes, Style::default(), true),
                PopupAction::new("No".to_string(), Action::No, Style::default(), true),
            ],
            PopupMenu::PlaylistConfirmDelete { playlist_name } => vec![
                PopupAction::new(
                    format!("Delete playlist: {}", playlist_name),
                    Action::Delete,
                    Style::default(),
                    true,
                ),
                PopupAction::new("Yes".to_string(), Action::Yes, Style::default(), true),
                PopupAction::new("No".to_string(), Action::No, Style::default(), true),
            ],
            PopupMenu::PlaylistCreate { name, public } => vec![
                PopupAction::new(
                    if name.is_empty() {
                        "Type in the new playlist name".into()
                    } else {
                        format!("Name: {}", name)
                    },
                    Action::Type,
                    Style::default(),
                    true,
                ),
                PopupAction::new(
                    format!("Public: {}", public),
                    Action::Toggle,
                    Style::default(),
                    true,
                ),
                PopupAction::new("Create".to_string(), Action::Create, Style::default(), true),
                PopupAction::new("Cancel".to_string(), Action::Cancel, Style::default(), true),
            ],
            PopupMenu::PlaylistsChangeSort {} => vec![
                PopupAction::new(
                    "Ascending".to_string(),
                    Action::Ascending,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Descending".to_string(),
                    Action::Descending,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Date created".to_string(),
                    Action::DateCreated,
                    Style::default(),
                    false,
                ),
                PopupAction::new("Random".to_string(), Action::Random, Style::default(), false),
            ],
            PopupMenu::PlaylistsChangeFilter {} => vec![
                PopupAction::new("Normal".to_string(), Action::Normal, Style::default(), false),
                PopupAction::new(
                    "Show favorites first".to_string(),
                    Action::ShowFavoritesFirst,
                    Style::default(),
                    false,
                ),
            ],
            // ---------- Tracks ---------- //
            PopupMenu::TrackRoot { disliked, .. } => vec![
                PopupAction::new(
                    "Jump to currently playing track".to_string(),
                    Action::JumpToCurrent,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Append to main queue".to_string(),
                    Action::Append,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Append to temporary queue".to_string(),
                    Action::AppendTemporary,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Add to playlist".to_string(),
                    Action::AddToPlaylist { playlist_id: String::new() },
                    Style::default(),
                    true,
                ),
                PopupAction::new(
                    if *disliked {
                        "Remove dislike".to_string()
                    } else {
                        "Dislike track".to_string()
                    },
                    Action::Dislike,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Change album order".to_string(),
                    Action::ChangeOrder,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Re-fetch artwork".to_string(),
                    Action::FetchArt,
                    Style::default(),
                    true,
                ),
            ],
            PopupMenu::TrackAddToPlaylist { playlists, .. } => {
                let mut actions = vec![];
                for playlist in playlists {
                    actions.push(PopupAction::new(
                        format!(
                            "{}{} ({})",
                            if playlist.user_data.is_favorite { "♥ " } else { "" },
                            playlist.name,
                            playlist.child_count
                        ),
                        Action::AddToPlaylist { playlist_id: playlist.id.clone() },
                        Style::default(),
                        true,
                    ));
                }
                actions
            }
            PopupMenu::TrackAlbumsChangeSort {} => vec![
                PopupAction::new(
                    "Release date - Ascending".to_string(),
                    Action::Ascending,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Release date - Descending".to_string(),
                    Action::Descending,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Date added - Ascending".to_string(),
                    Action::DateCreated,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Date added - Descending".to_string(),
                    Action::DateCreatedInverse,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Duration - Ascending".to_string(),
                    Action::DurationAsc,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Duration - Descending".to_string(),
                    Action::DurationDesc,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Title - Ascending".to_string(),
                    Action::TitleAsc,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Title - Descending".to_string(),
                    Action::TitleDesc,
                    Style::default(),
                    false,
                ),
                PopupAction::new("Random".to_string(), Action::Random, Style::default(), false),
            ],
            // ---------- Playlist tracks ---------- //
            PopupMenu::PlaylistTracksRoot { disliked, .. } => vec![
                PopupAction::new(
                    "Jump to album".to_string(),
                    Action::GoAlbum,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Add to playlist".to_string(),
                    Action::AddToPlaylist { playlist_id: String::new() },
                    Style::default(),
                    true,
                ),
                PopupAction::new(
                    if *disliked {
                        "Remove dislike".to_string()
                    } else {
                        "Dislike track".to_string()
                    },
                    Action::Dislike,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Remove from this playlist".to_string(),
                    Action::Delete,
                    Style::default().fg(style::Color::Red),
                    true,
                ),
            ],
            PopupMenu::PlaylistTrackAddToPlaylist { playlists, .. } => {
                let mut actions = vec![];
                for playlist in playlists {
                    actions.push(PopupAction::new(
                        format!(
                            "{}{} ({})",
                            if playlist.user_data.is_favorite { "♥ " } else { "" },
                            playlist.name,
                            playlist.child_count
                        ),
                        Action::AddToPlaylist { playlist_id: playlist.id.clone() },
                        Style::default(),
                        true,
                    ));
                }
                actions
            }
            PopupMenu::PlaylistTracksRemove { track_name, .. } => vec![
                PopupAction::new(
                    format!("Remove {} from playlist?", track_name),
                    Action::None,
                    Style::default().fg(style::Color::Red),
                    true,
                ),
                PopupAction::new(
                    "Yes".to_string(),
                    Action::Yes,
                    Style::default().fg(style::Color::Red),
                    true,
                ),
                PopupAction::new("No".to_string(), Action::No, Style::default(), true),
            ],
            // ---------- Artists ---------- //
            PopupMenu::ArtistRoot { playing_artists, .. } => {
                let mut actions = vec![];
                if let Some(artists) = playing_artists {
                    actions.push(PopupAction::new(
                        format!(
                            "Jump to current artist: {}",
                            artists
                                .into_iter()
                                .map(|a| a.name.clone())
                                .collect::<Vec<String>>()
                                .join(", ")
                        ),
                        Action::JumpToCurrent,
                        Style::default(),
                        false,
                    ));
                }
                actions.push(PopupAction::new(
                    "Change filter".to_string(),
                    Action::ChangeFilter,
                    Style::default(),
                    false,
                ));
                actions.push(PopupAction::new(
                    "Change sort order".to_string(),
                    Action::ChangeOrder,
                    Style::default(),
                    false,
                ));
                actions
            }
            PopupMenu::ArtistJumpToCurrent { artists, .. } => {
                let mut actions = vec![];
                for artist in artists {
                    actions.push(PopupAction::new(
                        artist.name.to_string(),
                        Action::JumpToCurrent,
                        Style::default(),
                        false,
                    ));
                }
                actions
            }
            PopupMenu::ArtistsChangeFilter {} => vec![
                PopupAction::new("Normal".to_string(), Action::Normal, Style::default(), false),
                PopupAction::new(
                    "Show favorites first".to_string(),
                    Action::ShowFavoritesFirst,
                    Style::default(),
                    false,
                ),
            ],
            PopupMenu::ArtistsChangeSort {} => vec![
                PopupAction::new(
                    "Ascending".to_string(),
                    Action::Ascending,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Descending".to_string(),
                    Action::Descending,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Date Created - Ascending".to_string(),
                    Action::DateCreated,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Date Created - Descending".to_string(),
                    Action::DateCreatedInverse,
                    Style::default(),
                    false,
                ),
                PopupAction::new("Random".to_string(), Action::Random, Style::default(), false),
            ],
            // ---------- Albums ---------- //
            PopupMenu::AlbumsRoot { .. } => vec![
                PopupAction::new(
                    "Jump to current album".to_string(),
                    Action::JumpToCurrent,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Download album".to_string(),
                    Action::Download,
                    Style::default(),
                    true,
                ),
                PopupAction::new(
                    "Append to main queue".to_string(),
                    Action::Append,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Append to temporary queue".to_string(),
                    Action::AppendTemporary,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Change filter".to_string(),
                    Action::ChangeFilter,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Change sort order".to_string(),
                    Action::ChangeOrder,
                    Style::default(),
                    false,
                ),
            ],
            PopupMenu::AlbumsChangeFilter {} => vec![
                PopupAction::new("Normal".to_string(), Action::Normal, Style::default(), false),
                PopupAction::new(
                    "Show favorites first".to_string(),
                    Action::ShowFavoritesFirst,
                    Style::default(),
                    false,
                ),
            ],
            PopupMenu::AlbumsChangeSort {} => vec![
                PopupAction::new(
                    "Ascending".to_string(),
                    Action::Ascending,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Descending".to_string(),
                    Action::Descending,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Premiere Date".to_string(),
                    Action::PremiereDate,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Duration".to_string(),
                    Action::DurationAsc,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Date created".to_string(),
                    Action::DateCreated,
                    Style::default(),
                    false,
                ),
                PopupAction::new("Random".to_string(), Action::Random, Style::default(), false),
            ],
            // ---------- Album tracks ---------- //
            PopupMenu::AlbumTrackRoot { disliked, .. } => vec![
                PopupAction::new(
                    "Jump to currently playing song".to_string(),
                    Action::JumpToCurrent,
                    Style::default(),
                    false,
                ),
                PopupAction::new(
                    "Add to playlist".to_string(),
                    Action::AddToPlaylist { playlist_id: String::new() },
                    Style::default(),
                    true,
                ),
                PopupAction::new(
                    if *disliked {
                        "Remove dislike".to_string()
                    } else {
                        "Dislike track".to_string()
                    },
                    Action::Dislike,
                    Style::default(),
                    false,
                ),
            ],
        }
    }
}

#[derive(Default)]
pub struct PopupState {
    pub selected: ratatui::widgets::ListState,
    pub current_menu: Option<PopupMenu>,
    pub editing: bool,
    editing_original: String,
    editing_new: String,
    pub global: bool, // if true the popup will be for global commands. Set before calling create_popup
    displayed_options: Vec<PopupAction>,
}
impl crate::tui::App {
    /// This function is called when a key is pressed while the popup is open
    ///
    pub async fn popup_handle_keys(&mut self, key_event: KeyEvent) {
        if self.popup.editing {
            self.handle_editing_keys(key_event).await;
            match &mut self.popup.current_menu {
                Some(PopupMenu::PlaylistSetName { new_name, .. }) => {
                    *new_name = self.popup.editing_new.clone();
                }
                Some(PopupMenu::PlaylistCreate { name, .. }) => {
                    *name = self.popup.editing_new.clone();
                }
                _ => {}
            }
            return;
        }
        if self.locally_searching {
            self.handle_search(key_event).await;
            return;
        }
        self.handle_special_keys(key_event).await;
        self.handle_navigational_keys(key_event).await;
    }

    /// The "editing text" implementation here is a bit hacky, it just lets you remove or add characters.
    ///
    async fn handle_editing_keys(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc => {
                self.popup.editing = false;
                self.close_popup();
            }
            KeyCode::Enter => {
                self.popup.editing = false;
            }
            KeyCode::Char(c) => {
                self.popup.editing_new.push(c);
            }
            KeyCode::Backspace => {
                self.popup.editing_new.pop();
            }
            _ => {}
        }
    }

    /// This function handles some special keys for the popup menu
    ///
    async fn handle_special_keys(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('/') => {
                self.locally_searching = true;
            }
            KeyCode::Char('+') => {
                if let Some(PopupMenu::GlobalShuffle {
                    tracks_n,
                    only_played,
                    only_unplayed,
                    only_favorite,
                }) = &self.popup.current_menu
                {
                    self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                        tracks_n: tracks_n + 10,
                        only_played: *only_played,
                        only_unplayed: *only_unplayed,
                        only_favorite: *only_favorite,
                    });
                }
            }
            KeyCode::Char('-') => {
                if let Some(PopupMenu::GlobalShuffle {
                    tracks_n,
                    only_played,
                    only_unplayed,
                    only_favorite,
                }) = &self.popup.current_menu
                {
                    if *tracks_n > 1 {
                        self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                            tracks_n: tracks_n - 10,
                            only_played: *only_played,
                            only_unplayed: *only_unplayed,
                            only_favorite: *only_favorite,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    /// This function handles the navigational keys for the popup menu
    ///
    async fn handle_navigational_keys(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.popup.selected.select_next();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.popup.selected.select_previous();
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.popup.selected.select_first();
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.popup.selected.select_last();
            }

            KeyCode::Esc => {
                self.close_popup();
            }

            KeyCode::Enter => {
                self.apply_action().await;
                self.popup_search_term.clear();
                self.locally_searching = false;
            }
            _ => {}
        }
    }

    async fn handle_search(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char(c) => {
                self.popup_search_term.push(c);
                self.popup.selected.select_first();
            }
            KeyCode::Delete => {
                let selected_id = self.get_id_of_selected(
                    &self.popup.current_menu.as_ref().map_or(vec![], |m| m.options()),
                    Selectable::Popup,
                );
                self.popup_search_term.clear();
                self.reposition_cursor(&selected_id, Selectable::Popup);
            }
            KeyCode::Backspace => {
                let selected_id = self.get_id_of_selected(
                    &self.popup.current_menu.as_ref().map_or(vec![], |m| m.options()),
                    Selectable::Popup,
                );
                self.popup_search_term.pop();
                self.reposition_cursor(&selected_id, Selectable::Popup);
            }
            KeyCode::Esc => {
                let selected_id = self.get_id_of_selected(
                    &self.popup.current_menu.as_ref().map_or(vec![], |m| m.options()),
                    Selectable::Popup,
                );
                self.popup_search_term.clear();
                self.reposition_cursor(&selected_id, Selectable::Popup);
                self.locally_searching = false;
            }
            KeyCode::Enter => {
                self.locally_searching = false;
            }
            _ => {}
        }
    }

    // Apply the Enter key action
    async fn apply_action(&mut self) {
        let m = self.popup.current_menu.as_ref();
        let menu = match m {
            Some(menu) => menu,
            None => return,
        };

        let selected = match self.popup.selected.selected() {
            Some(i) => i,
            None => return,
        };

        let options = if self.client.is_some() {
            menu.options()
        } else {
            menu.options().into_iter().filter(|o| !o.online).collect::<Vec<PopupAction>>()
        };

        if options.is_empty() {
            return;
        }

        let action = match self.popup.displayed_options.get(selected).map(|a| &a.action) {
            Some(action) => action.clone(),
            None => return,
        };

        if let PopupMenu::GenericMessage { .. } = menu {
            if let Action::Ok = action {
                self.close_popup();
            }
            return;
        }

        if self.popup.global {
            self.apply_global_action(&action, menu.clone()).await;
            return;
        }

        match self.state.active_tab {
            ActiveTab::Library => match self.state.last_section {
                ActiveSection::Tracks => {
                    self.apply_track_action(&action, menu.clone()).await;
                }
                ActiveSection::List => {
                    self.apply_artist_action(&action, menu.clone());
                }
                _ => {}
            },
            ActiveTab::Albums => match self.state.last_section {
                ActiveSection::List => {
                    self.apply_album_action(&action, menu.clone()).await;
                }
                ActiveSection::Tracks => {
                    self.apply_album_track_action(&action, menu.clone()).await;
                }
                _ => {}
            },
            ActiveTab::Playlists => match self.state.last_section {
                ActiveSection::List => {
                    if let None = self.apply_playlist_action(&action, menu.clone()).await {
                        self.close_popup();
                    }
                }
                ActiveSection::Tracks => {
                    self.apply_playlist_tracks_action(&action, menu.clone()).await;
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// Following functions separate actions based on UI sections
    ///
    async fn apply_global_action(&mut self, action: &Action, menu: PopupMenu) -> Option<()> {
        match menu {
            PopupMenu::GlobalRoot { downloading, .. } => match action {
                Action::Refresh => {
                    let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::Library)).await;
                    self.close_popup();
                }
                Action::ChangeCoverArtLayout => {
                    self.preferences.large_art = !self.preferences.large_art;
                    let _ = self.preferences.save();
                    self.close_popup();
                }
                Action::ResetSectionWidths => {
                    self.preferences.constraint_width_percentages_music =
                        crate::helpers::Preferences::default_music_column_widths();
                    if let Err(e) = self.preferences.save() {
                        log::error!("Failed to save preferences: {}", e);
                    }
                    self.close_popup();
                }
                Action::GlobalSetTheme => {
                    self.popup.current_menu = Some(PopupMenu::GlobalPickTheme {});
                    let builtin_themes = Theme::builtin_themes();
                    let current_theme_index =
                        builtin_themes.iter().position(|t| t.name == self.preferences.theme);
                    if let Some(index) = current_theme_index {
                        self.popup.selected.select(Some(index));
                    } else {
                        self.popup.selected.select_last();
                    }
                }
                Action::RunScheduledTasks => {
                    let tasks = self.client.as_ref()?.scheduled_tasks().await.unwrap_or(vec![]);
                    if tasks.is_empty() {
                        self.set_generic_message(
                            "No scheduled tasks",
                            "You may not have permissions to run tasks.",
                        );
                        return None;
                    }
                    self.popup.current_menu = Some(PopupMenu::GlobalRunScheduledTask { tasks });
                    self.popup.selected.select_first();
                }
                Action::SelectLibraries => {
                    self.popup.current_menu = Some(PopupMenu::GlobalSelectLibraries {
                        libraries: self.music_libraries.clone(),
                    });
                    self.popup.selected.select_first();
                }
                Action::OfflineRepair => {
                    if let Ok(_) =
                        self.db.cmd_tx.send(Command::Update(UpdateCommand::OfflineRepair)).await
                    {
                        self.db_updating = true;
                        self.close_popup();
                    } else {
                        log::error!("Failed to start offline repair");
                        self.set_generic_message(
                            "Failed to start offline repair",
                            "Please try again later.",
                        );
                    }
                }
                Action::CancelDownloads => {
                    if !downloading {
                        return None;
                    }
                    match self.db.cmd_tx.send(Command::CancelDownloads).await {
                        Ok(_) => self.close_popup(),
                        Err(e) => self.set_generic_message(
                            "Failed to abort downloads",
                            &format!("Error: {}", e.to_string()),
                        ),
                    }
                }
                _ => {}
            },
            PopupMenu::GlobalSetThemes { .. } => match action {
                Action::SetCustomTheme { theme } => {
                    self.preferences.theme = theme.name.clone();
                    let (theme, _, picker, ..) =
                        Self::load_theme_from_config(&self.config, &self.preferences);
                    self.theme = theme;
                    self.picker = picker;

                    if let Some(current_song) = self
                        .state
                        .queue
                        .get(self.state.current_playback_state.current_index)
                        .cloned()
                    {
                        self.update_cover_art(&current_song, true, false).await;
                    }
                    if let Err(e) = self.preferences.save() {
                        log::error!("Failed to save preferences: {}", e);
                    }
                }
                Action::None => {
                    self.popup.current_menu = Some(PopupMenu::GlobalPickTheme {});
                    self.popup.selected.select_first();
                }
                _ => {}
            },
            PopupMenu::GlobalRunScheduledTask { .. } => match action {
                Action::RunScheduledTask { task } => {
                    if let Some(task) = task {
                        if let Ok(_) = self.client.as_ref()?.run_scheduled_task(&task.id).await {
                            self.set_generic_message(
                                &format!("Task {} executed successfully", task.name),
                                "Try reloading your library to see changes.",
                            );
                        } else {
                            self.set_generic_message(
                                "Error executing task",
                                &format!("Failed to execute task {}.", task.name),
                            );
                        }
                    }
                    return None;
                }
                _ => {
                    self.close_popup();
                }
            },
            PopupMenu::GlobalSelectLibraries { libraries } => match action {
                Action::ToggleLibrary { library_id } => {
                    let mut new_libraries = libraries.clone();
                    if let Some(lib) = new_libraries.iter_mut().find(|l| l.id == *library_id) {
                        lib.selected = !lib.selected;
                    }
                    self.popup.current_menu =
                        Some(PopupMenu::GlobalSelectLibraries { libraries: new_libraries });
                }
                Action::Confirm => {
                    if !libraries.iter().any(|l| l.selected) {
                        self.set_generic_message(
                            "No libraries selected",
                            "Please select at least one library.",
                        );
                        return None;
                    }
                    self.music_libraries = libraries;

                    if let Err(e) =
                        set_selected_libraries(&self.db.pool, &self.music_libraries).await
                    {
                        log::error!("Failed to save selected libraries: {}", e);
                        self.set_generic_message("Failed to save libraries", "Please try again");
                        return None;
                    } else {
                        let (original_artists, original_albums, original_playlists) =
                            Self::init_library(&self.db.pool, self.client.is_some()).await;
                        self.original_artists = original_artists;
                        self.original_albums = original_albums;
                        self.original_playlists = original_playlists;
                        self.tracks = vec![];
                        self.album_tracks = vec![];
                        self.playlist_tracks = vec![];
                        self.reorder_lists();
                        self.close_popup();
                    }
                }
                _ => {
                    self.close_popup();
                }
            },
            PopupMenu::GlobalShuffle { tracks_n, only_played, only_unplayed, only_favorite } => {
                match action {
                    Action::None => {
                        self.popup.selected.select_next();
                    }
                    // we need to guarantee that it's either played or unplayed, or both FALSE
                    Action::OnlyPlayed => {
                        if !only_played {
                            self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                                tracks_n,
                                only_played: true,
                                only_unplayed: false,
                                only_favorite,
                            });
                        } else {
                            self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                                tracks_n,
                                only_played: false,
                                only_unplayed: false,
                                only_favorite,
                            });
                        }
                    }
                    Action::OnlyUnplayed => {
                        if !only_unplayed {
                            self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                                tracks_n,
                                only_played: false,
                                only_unplayed: true,
                                only_favorite,
                            });
                        } else {
                            self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                                tracks_n,
                                only_played: false,
                                only_unplayed: false,
                                only_favorite,
                            });
                        }
                    }
                    Action::OnlyFavorite => {
                        if !only_favorite {
                            self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                                tracks_n,
                                only_played,
                                only_unplayed,
                                only_favorite: true,
                            });
                        } else {
                            self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                                tracks_n,
                                only_played,
                                only_unplayed,
                                only_favorite: false,
                            });
                        }
                    }
                    Action::Play => {
                        let client = self.client.as_ref()?;
                        let mut tracks = client
                            .random_tracks(tracks_n, only_played, only_unplayed, only_favorite)
                            .await
                            .unwrap_or_default();
                        if !tracks.is_empty() {
                            tracks.retain(|t| !t.disliked);
                        }
                        // should just about always be enough, queueing 99 instead of 100 isn't a big deal
                        if tracks.len() < tracks_n {
                            let needed = tracks_n - tracks.len();
                            let mut extra = client
                                .random_tracks(
                                    needed * 5,
                                    only_played,
                                    only_unplayed,
                                    only_favorite,
                                )
                                .await
                                .unwrap_or_default();

                            extra.retain(|t| !t.disliked);
                            tracks.extend(extra);
                            tracks.truncate(tracks_n);
                        }
                        self.initiate_main_queue(&tracks, 0).await;
                        self.close_popup();

                        self.preferences.preferred_global_shuffle =
                            Some(PopupMenu::GlobalShuffle {
                                tracks_n,
                                only_played,
                                only_unplayed,
                                only_favorite,
                            });
                        let _ = self.preferences.save();
                    }
                    _ => {
                        self.close_popup();
                    }
                }
            }
            PopupMenu::GlobalPickTheme { .. } => match action {
                Action::SetTheme { theme } => {
                    self.preferences.theme = theme.name.clone();
                    let (theme, _, picker, ..) =
                        Self::load_theme_from_config(&self.config, &self.preferences);
                    self.theme = theme;
                    self.picker = picker;
                    if let Some(current_song) = self
                        .state
                        .queue
                        .get(self.state.current_playback_state.current_index)
                        .cloned()
                    {
                        self.update_cover_art(&current_song, true, false).await;
                    }
                    if let Err(e) = self.preferences.save() {
                        log::error!("Failed to save preferences: {}", e);
                    }
                }
                Action::Custom => {
                    self.popup.current_menu =
                        Some(PopupMenu::GlobalSetThemes { themes: self.themes.clone() });
                    let current_theme_index =
                        self.themes.iter().position(|t| t.name == self.preferences.theme);
                    if let Some(index) = current_theme_index {
                        self.popup.selected.select(Some(index));
                    } else {
                        self.popup.selected.select_last();
                    }
                }
                _ => {}
            },
            _ => {}
        }
        Some(())
    }
    async fn apply_track_action(&mut self, action: &Action, menu: PopupMenu) -> Option<()> {
        match menu {
            PopupMenu::TrackRoot { track_id, track_name, parent_id, disliked } => match action {
                Action::AddToPlaylist { .. } => {
                    self.popup.current_menu = Some(PopupMenu::TrackAddToPlaylist {
                        track_name,
                        track_id,
                        playlists: self.playlists.clone(),
                    });
                    self.popup.selected.select_first();
                }
                Action::JumpToCurrent => {
                    let current_track =
                        self.state.queue.get(self.state.current_playback_state.current_index)?;
                    let artist = self
                        .artists
                        .iter()
                        .find(|a| {
                            current_track.artist_items.first().is_some_and(|item| a.id == item.id)
                        })
                        .or_else(|| {
                            current_track
                                .artist_items
                                .first()
                                .and_then(|item| self.artists.iter().find(|a| a.name == item.name))
                        })?;

                    let artist_id = artist.id.clone();
                    let current_track_id = current_track.id.clone();
                    // open this artist if not yet open
                    if artist_id != self.state.current_artist.id {
                        let index =
                            self.artists.iter().position(|a| a.id == artist_id).unwrap_or(0);
                        self.artist_select_by_index(index);
                        self.discography(&artist_id).await;
                    }
                    if let Some(track) = self.tracks.iter().find(|t| t.id == current_track_id) {
                        let index = self.tracks.iter().position(|t| t.id == track.id).unwrap_or(0);
                        self.track_select_by_index(index);
                    }
                    self.close_popup();
                }
                Action::Append => {
                    let track = self.tracks.iter().find(|t| t.id == track_id)?;
                    if track.id.starts_with("_album_") {
                        let id = track.id.trim_start_matches("_album_").to_string();
                        let album_tracks = self
                            .tracks
                            .iter()
                            .filter(|t| t.album_id == id)
                            .cloned()
                            .collect::<Vec<DiscographySong>>();
                        self.append_to_main_queue(&album_tracks, 0).await;
                        self.close_popup();
                        return Some(());
                    }
                    self.append_to_main_queue(&vec![track.clone()], 0).await;
                    self.close_popup();
                }
                Action::AppendTemporary => {
                    let track = self.tracks.iter().find(|t| t.id == track_id)?;
                    if track.id.starts_with("_album_") {
                        let id = track.id.trim_start_matches("_album_").to_string();
                        let album_tracks = self
                            .tracks
                            .iter()
                            .filter(|t| t.album_id == id)
                            .cloned()
                            .collect::<Vec<DiscographySong>>();
                        self.push_to_temporary_queue(&album_tracks, 0, album_tracks.len()).await;
                        self.close_popup();
                        return Some(());
                    }
                    self.push_to_temporary_queue(&vec![track.clone()], 0, 1).await;
                    self.close_popup();
                }
                Action::Dislike => {
                    // send a message to the db thread
                    let _ = self
                        .db
                        .cmd_tx
                        .send(Command::DislikeTrack {
                            track_id: track_id.clone(),
                            disliked: !disliked,
                        })
                        .await;
                    if let Some(track) = self.tracks.iter_mut().find(|t| t.id == track_id) {
                        track.disliked = !disliked;
                    }
                    self.close_popup();
                }
                Action::ChangeOrder => {
                    self.popup.current_menu = Some(PopupMenu::TrackAlbumsChangeSort {});
                    self.popup.selected.select(Some(match self.preferences.tracks_sort {
                        Sort::Ascending => 0,
                        Sort::Descending => 1,
                        Sort::DateCreated => 2,
                        Sort::DateCreatedInverse => 3,
                        Sort::Duration => 4,
                        Sort::DurationDesc => 5,
                        Sort::Title => 6,
                        Sort::TitleDesc => 7,
                        Sort::Random => 8,
                        _ => 0,
                    }));
                }
                Action::FetchArt => {
                    let client = self.client.as_ref()?;
                    if let Err(_) = client.download_cover_art(&parent_id).await {
                        self.set_generic_message(
                            "Error fetching artwork",
                            &format!("Failed to fetch artwork for track {}.", track_name),
                        );
                    } else {
                        if let Some(current_song) = self
                            .state
                            .queue
                            .get(self.state.current_playback_state.current_index)
                            .cloned()
                        {
                            self.cover_art = None;
                            self.update_cover_art(&current_song, true, false).await;
                        }
                    }
                    self.close_popup();
                }
                _ => {
                    self.close_popup();
                }
            },
            PopupMenu::TrackAddToPlaylist { track_name, track_id, playlists } => match action {
                Action::AddToPlaylist { playlist_id } => {
                    let playlist = playlists.iter().find(|p| p.id == *playlist_id)?;
                    if let Err(_) =
                        self.client.as_ref()?.add_to_playlist(&track_id, playlist_id).await
                    {
                        self.set_generic_message(
                            "Error adding track",
                            &format!(
                                "Failed to add track {} to playlist {}.",
                                track_name, playlist.name
                            ),
                        );
                    }
                    self.playlists
                        .iter_mut()
                        .find(|p| p.id == playlist.id)
                        .map(|p| p.child_count += 1);

                    self.set_generic_message(
                        "Track added",
                        &format!(
                            "Track {} successfully added to playlist {}.",
                            track_name, playlist.name
                        ),
                    );
                }
                _ => {
                    self.close_popup();
                }
            },
            PopupMenu::TrackAlbumsChangeSort {} => {
                match action {
                    Action::Ascending => {
                        self.preferences.tracks_sort = Sort::Ascending;
                    }
                    Action::Descending => {
                        self.preferences.tracks_sort = Sort::Descending;
                    }
                    Action::DateCreated => {
                        self.preferences.tracks_sort = Sort::DateCreated;
                    }
                    Action::DateCreatedInverse => {
                        self.preferences.tracks_sort = Sort::DateCreatedInverse;
                    }
                    Action::DurationAsc => {
                        self.preferences.tracks_sort = Sort::Duration;
                    }
                    Action::DurationDesc => {
                        self.preferences.tracks_sort = Sort::DurationDesc;
                    }
                    Action::TitleAsc => {
                        self.preferences.tracks_sort = Sort::Title;
                    }
                    Action::TitleDesc => {
                        self.preferences.tracks_sort = Sort::TitleDesc;
                    }
                    Action::Random => {
                        self.preferences.tracks_sort = Sort::Random;
                    }
                    _ => {}
                }
                self.group_tracks_into_albums(self.tracks.clone(), None);
                self.close_popup();
            }
            _ => {}
        }
        Some(())
    }

    async fn apply_album_action(&mut self, action: &Action, menu: PopupMenu) -> Option<()> {
        match menu {
            PopupMenu::AlbumsRoot { album } => {
                match action {
                    Action::JumpToCurrent => {
                        let current_track = self
                            .state
                            .queue
                            .get(self.state.current_playback_state.current_index)?;

                        let target_index = if !self.state.albums_search_term.is_empty() {
                            let albums = search_ranked_refs(
                                &self.albums,
                                &self.state.albums_search_term,
                                true,
                            );

                            albums.iter().position(|a| a.id == current_track.album_id)
                        } else {
                            self.albums.iter().position(|a| a.id == current_track.album_id)
                        };

                        let Some(index) = target_index else {
                            return Some(());
                        };

                        self.state.albums_search_term.clear();
                        self.album_select_by_index(index);
                        self.close_popup();
                    }
                    Action::Download => {
                        let album_artist = album.album_artists.first().cloned();
                        let parent = if let Some(artist) = album_artist {
                            artist.id.clone()
                        } else {
                            album.parent_id.clone()
                        };

                        // need to make sure the album is in the db
                        if let Err(_) = t_discography_updater(
                            Arc::clone(&self.db.pool),
                            parent.clone(),
                            self.db.status_tx.clone(),
                            self.client.clone().unwrap(), /* this fn is online guarded */
                        )
                        .await
                        {
                            self.set_generic_message(
                                "Error downloading album",
                                &format!("Failed to fetch artist {}.", parent),
                            );
                            return None;
                        }

                        let tracks =
                            match get_album_tracks(&self.db.pool, &album.id, self.client.as_ref())
                                .await
                            {
                                Ok(tracks) => tracks,
                                Err(_) => {
                                    self.set_generic_message(
                                        "Error downloading album",
                                        &format!("Failed fetching tracks {}.", album.name),
                                    );
                                    return None;
                                }
                            };

                        let downloaded = self
                            .db
                            .cmd_tx
                            .send(Command::Download(DownloadCommand::Tracks {
                                tracks: tracks
                                    .into_iter()
                                    .filter(|t| {
                                        !matches!(t.download_status, DownloadStatus::Downloaded)
                                    })
                                    .collect::<Vec<DiscographySong>>(),
                            }))
                            .await;

                        match downloaded {
                            Ok(_) => {
                                self.set_generic_message(
                                    "Album download started",
                                    &format!("Album {} is being downloaded.", album.name),
                                );
                            }
                            Err(_) => {
                                self.set_generic_message(
                                    "Error downloading album",
                                    &format!("Failed to download album {}.", album.name),
                                );
                            }
                        }
                    }
                    Action::Append => {
                        self.album_tracks(&album.id).await;
                        let tracks = self.album_tracks.clone();
                        self.append_to_main_queue(&tracks, 0).await;
                        self.close_popup();
                    }
                    Action::AppendTemporary => {
                        self.album_tracks(&album.id).await;
                        let tracks = self.album_tracks.clone();
                        self.push_to_temporary_queue(&tracks, 0, tracks.len()).await;
                        self.close_popup();
                    }
                    Action::ChangeFilter => {
                        self.popup.current_menu = Some(PopupMenu::AlbumsChangeFilter {});
                        self.popup.selected.select(match self.preferences.album_filter {
                            Filter::Normal => Some(0),
                            Filter::FavoritesFirst => Some(1),
                        })
                    }
                    Action::ChangeOrder => {
                        self.popup.current_menu = Some(PopupMenu::AlbumsChangeSort {});
                        self.popup.selected.select(Some(match self.preferences.album_sort {
                            Sort::Ascending => 0,
                            Sort::Descending => 1,
                            Sort::DateCreated => 2,
                            Sort::DateCreatedInverse => 3,
                            Sort::Duration => 4,
                            Sort::DurationDesc => 5,
                            Sort::Title => 6,
                            Sort::TitleDesc => 7,
                            Sort::Random => 8,
                            _ => 0,
                        }));
                    }
                    _ => {}
                }
            }
            PopupMenu::AlbumsChangeFilter { .. } => match action {
                Action::Normal => {
                    self.preferences.album_filter = Filter::Normal;
                    self.reorder_lists();
                    self.close_popup();
                }
                Action::ShowFavoritesFirst => {
                    self.preferences.album_filter = Filter::FavoritesFirst;
                    self.reorder_lists();
                    self.close_popup();
                }
                _ => {}
            },
            PopupMenu::AlbumsChangeSort { .. } => match action {
                Action::Ascending => {
                    self.preferences.album_sort = Sort::Ascending;
                    self.reorder_lists();
                    self.close_popup();
                }
                Action::Descending => {
                    self.preferences.album_sort = Sort::Descending;
                    self.reorder_lists();
                    self.close_popup();
                }
                Action::PremiereDate => {
                    self.preferences.album_sort = Sort::PremiereDate;
                    self.reorder_lists();
                    self.close_popup();
                }
                Action::DurationAsc => {
                    self.preferences.album_sort = Sort::Duration;
                    self.reorder_lists();
                    self.close_popup();
                }
                Action::DateCreated => {
                    self.preferences.album_sort = Sort::DateCreated;
                    self.reorder_lists();
                    self.close_popup();
                }
                Action::Random => {
                    self.preferences.album_sort = Sort::Random;
                    self.reorder_lists();
                    self.close_popup();
                }
                _ => {}
            },
            _ => {}
        }

        Some(())
    }

    async fn apply_album_track_action(&mut self, action: &Action, menu: PopupMenu) -> Option<()> {
        match menu {
            PopupMenu::AlbumTrackRoot { disliked, .. } => {
                let selected = match self.state.selected_album_track.selected() {
                    Some(i) => i,
                    None => {
                        self.close_popup();
                        return None;
                    }
                };

                let tracks = search_ranked_refs(
                    &self.album_tracks,
                    &self.state.album_tracks_search_term,
                    true,
                );

                let Some(track) = tracks.get(selected) else {
                    return None;
                };

                let selected = self.state.selected_album_track.selected()?;
                let track_id = {
                    let tracks = search_ranked_refs(
                        &self.album_tracks,
                        &self.state.album_tracks_search_term,
                        true,
                    );

                    tracks.get(selected)?.id.clone()
                };

                match action {
                    Action::AddToPlaylist { .. } => {
                        self.popup.current_menu = Some(PopupMenu::TrackAddToPlaylist {
                            track_name: track.name.clone(),
                            track_id: track.id.clone(),
                            playlists: self.playlists.clone(),
                        });
                        self.popup.selected.select_first();
                    }
                    Action::Dislike => {
                        let _ = self
                            .db
                            .cmd_tx
                            .send(Command::DislikeTrack {
                                track_id: track_id.clone(),
                                disliked: !disliked,
                            })
                            .await;
                        if let Some(t) = self.album_tracks.iter_mut().find(|t| t.id == track_id) {
                            t.disliked = !disliked;
                        }
                        self.close_popup();
                    }
                    Action::JumpToCurrent => {
                        let current_track = self
                            .state
                            .queue
                            .get(self.state.current_playback_state.current_index)?;

                        let album_id = current_track.album_id.clone();
                        let track_id = current_track.id.clone();

                        if album_id != self.state.current_album.id {
                            if let Some(index) = self.albums.iter().position(|a| a.id == album_id) {
                                self.album_select_by_index(index);
                                self.album_tracks(&album_id).await;
                            }
                        }

                        if let Some(index) = self.album_tracks.iter().position(|t| t.id == track_id)
                        {
                            self.album_track_select_by_index(index);
                        }

                        self.close_popup();
                    }

                    _ => {}
                }
            }
            PopupMenu::TrackAddToPlaylist { track_name, track_id, playlists } => match action {
                Action::AddToPlaylist { playlist_id } => {
                    let playlist = playlists.iter().find(|p| p.id == *playlist_id)?;
                    if let Err(_) =
                        self.client.as_ref()?.add_to_playlist(&track_id, playlist_id).await
                    {
                        self.set_generic_message(
                            "Error adding track",
                            &format!(
                                "Failed to add track {} to playlist {}.",
                                track_name, playlist.name
                            ),
                        );
                    }
                    self.playlists
                        .iter_mut()
                        .find(|p| p.id == playlist.id)
                        .map(|p| p.child_count += 1);

                    self.set_generic_message(
                        "Track added",
                        &format!(
                            "Track {} successfully added to playlist {}.",
                            track_name, playlist.name
                        ),
                    );
                }
                _ => {
                    self.close_popup();
                }
            },
            _ => {}
        }
        Some(())
    }

    async fn apply_playlist_tracks_action(
        &mut self,
        action: &Action,
        menu: PopupMenu,
    ) -> Option<()> {
        match menu {
            PopupMenu::PlaylistTracksRoot { disliked, .. } => {
                let selected = match self.state.selected_playlist_track.selected() {
                    Some(i) => i,
                    None => {
                        self.close_popup();
                        return None;
                    }
                };
                let (track_id, track_name, artist_id_opt, artist_name_opt) = {
                    let tracks = search_ranked_refs(
                        &self.playlist_tracks,
                        &self.state.playlist_tracks_search_term,
                        true,
                    );

                    let track = tracks.get(selected)?;

                    (
                        track.id.clone(),
                        track.name.clone(),
                        track.album_artists.first().map(|a| a.id.clone()),
                        track.album_artists.first().map(|a| a.name.clone()),
                    )
                };
                match action {
                    Action::GoAlbum => {
                        self.close_popup();

                        self.state.active_tab = ActiveTab::Library;
                        self.state.active_section = ActiveSection::List;
                        self.state.tracks_search_term.clear();

                        let artist_index = artist_id_opt
                            .and_then(|artist_id| {
                                self.artists.iter().position(|a| a.id == artist_id)
                            })
                            .or_else(|| {
                                artist_name_opt.as_deref().and_then(|name| {
                                    self.artists
                                        .iter()
                                        .position(|a| a.name.eq_ignore_ascii_case(name))
                                })
                            });
                        // DONT WORK YET
                        if let Some(index) = artist_index {
                            self.artist_select_by_index(index);
                            let artist_id = self.artists[index].id.clone();
                            self.discography(&artist_id).await;
                        }
                        if let Some(index) = self.tracks.iter().position(|t| t.id == track_id) {
                            self.track_select_by_index(index);
                        }
                    }
                    Action::AddToPlaylist { .. } => {
                        self.popup.current_menu = Some(PopupMenu::PlaylistTrackAddToPlaylist {
                            track_name,
                            track_id,
                            playlists: self.playlists.clone(),
                        });
                        self.popup.selected.select_first();
                    }
                    Action::Dislike => {
                        let _ = self
                            .db
                            .cmd_tx
                            .send(Command::DislikeTrack {
                                track_id: track_id.clone(),
                                disliked: !disliked,
                            })
                            .await;
                        if let Some(t) = self.playlist_tracks.iter_mut().find(|t| t.id == track_id)
                        {
                            t.disliked = !disliked;
                        }
                        self.close_popup();
                    }
                    Action::Delete => {
                        self.popup.current_menu = Some(PopupMenu::PlaylistTracksRemove {
                            track_name,
                            track_id,
                            playlist_name: self.state.current_playlist.name.clone(),
                            playlist_id: self.state.current_playlist.id.clone(),
                        });
                        self.popup.selected.select(Some(1));
                    }
                    _ => {}
                }
            }
            PopupMenu::PlaylistTrackAddToPlaylist { track_name, track_id, playlists } => {
                if let Action::AddToPlaylist { playlist_id } = action {
                    let playlist = playlists.iter().find(|p| p.id == *playlist_id)?;
                    if let Err(_) =
                        self.client.as_ref()?.add_to_playlist(&track_id, playlist_id).await
                    {
                        self.set_generic_message(
                            "Error adding track",
                            &format!(
                                "Failed to add track {} to playlist {}.",
                                track_name, playlist.name
                            ),
                        );
                    }
                    self.playlists
                        .iter_mut()
                        .find(|p| p.id == playlist.id)
                        .map(|p| p.child_count += 1);

                    self.set_generic_message(
                        "Track added",
                        &format!(
                            "Track {} successfully added to playlist {}.",
                            track_name, playlist.name
                        ),
                    );
                } else {
                    self.close_popup();
                }
            }
            PopupMenu::PlaylistTracksRemove {
                track_name,
                track_id,
                playlist_name,
                playlist_id,
            } => match action {
                Action::None => {
                    self.popup.selected.select_next();
                }
                Action::Yes => {
                    if let Ok(_) =
                        self.client.as_ref()?.remove_from_playlist(&track_id, &playlist_id).await
                    {
                        self.playlist_tracks.retain(|t| t.playlist_item_id != track_id);
                        self.set_generic_message(
                            &format!("{} removed", track_name),
                            &format!("Successfully removed from {}.", playlist_name),
                        );
                    } else {
                        self.set_generic_message(
                            "Error removing track",
                            &format!(
                                "Failed to remove track {} from playlist {}.",
                                track_name, playlist_name
                            ),
                        );
                    }
                }
                _ => {
                    self.close_popup();
                }
            },
            _ => {}
        }
        Some(())
    }

    async fn apply_playlist_action(&mut self, action: &Action, menu: PopupMenu) -> Option<()> {
        let id = self.get_id_of_selected(&self.playlists, Selectable::Playlist);
        let mut selected_playlist = self.playlists.iter().find(|p| p.id == id)?.clone();

        match menu {
            PopupMenu::PlaylistRoot { .. } => {
                match action {
                    Action::Play => {
                        self.open_playlist(None).await;
                        self.initiate_main_queue(&self.playlist_tracks.clone(), 0).await;
                        self.close_popup();
                    }
                    Action::Append => {
                        self.open_playlist(None).await;
                        self.append_to_main_queue(&self.playlist_tracks.clone(), 0).await;
                        self.close_popup();
                    }
                    Action::AppendTemporary => {
                        self.open_playlist(None).await;
                        self.push_to_temporary_queue(
                            &self.playlist_tracks.clone(),
                            0,
                            self.playlist_tracks.len(),
                        )
                        .await;
                        self.close_popup();
                    }
                    Action::Rename => {
                        self.popup.current_menu = Some(PopupMenu::PlaylistSetName {
                            playlist_name: selected_playlist.name.clone(),
                            new_name: selected_playlist.name.clone(),
                        });
                        self.popup.editing_original = selected_playlist.name.clone();
                        self.popup.editing_new = selected_playlist.name.clone();
                        self.popup.selected.select_first();
                        self.popup.editing = true;
                    }
                    Action::Download => {
                        // this is about a hundred times easier... maybe later make it fetch in bck
                        self.open_playlist(None).await;
                        if self.state.current_playlist.id == id {
                            let _ = self
                                .db
                                .cmd_tx
                                .send(Command::Download(DownloadCommand::Tracks {
                                    tracks: self.playlist_tracks.clone(),
                                }))
                                .await;
                            self.close_popup();
                        } else {
                            self.set_generic_message(
                                "Playlist ID not matching",
                                "Please try again later.",
                            );
                        }
                    }
                    Action::RemoveDownload => {
                        self.open_playlist(None).await;
                        self.close_popup();
                        if self.state.current_playlist.id == id {
                            let _ = self
                                .db
                                .cmd_tx
                                .send(Command::Remove(RemoveCommand::Tracks {
                                    tracks: self.playlist_tracks.clone(),
                                }))
                                .await;
                        } else {
                            self.set_generic_message(
                                "Playlist ID not matching",
                                "Please try again later.",
                            );
                        }
                    }
                    Action::Create => {
                        self.popup.current_menu =
                            Some(PopupMenu::PlaylistCreate { name: "".to_string(), public: false });
                        self.popup.editing_original = "".to_string();
                        self.popup.editing_new = "".to_string();
                        self.popup.selected.select_first();
                        self.popup.editing = true;
                    }
                    Action::Delete => {
                        self.popup.current_menu = Some(PopupMenu::PlaylistConfirmDelete {
                            playlist_name: selected_playlist.name.clone(),
                        });
                        self.popup.selected.select(Some(1));
                    }
                    Action::ChangeFilter => {
                        self.popup.current_menu = Some(PopupMenu::PlaylistsChangeFilter {});
                        // self.popup.selected.select_first();
                        self.popup.selected.select(Some(
                            if self.preferences.playlist_filter == Filter::Normal { 0 } else { 1 },
                        ));
                    }
                    Action::ChangeOrder => {
                        self.popup.current_menu = Some(PopupMenu::PlaylistsChangeSort {});
                        self.popup.selected.select(Some(match self.preferences.playlist_sort {
                            Sort::Ascending => 0,
                            Sort::Descending => 1,
                            Sort::DateCreated => 2,
                            Sort::Random => 3,
                            _ => 0,
                        }));
                    }
                    _ => {}
                }
            }
            PopupMenu::PlaylistSetName { playlist_name, new_name } => match action {
                Action::Type => {
                    self.popup.editing = true;
                }
                Action::Confirm => {
                    if new_name.trim().is_empty() {
                        self.popup.editing = true;
                        self.popup.selected.select_first();
                        return None;
                    }
                    self.popup.current_menu =
                        Some(PopupMenu::PlaylistConfirmRename { new_name: new_name.clone() });
                    self.popup.selected.select(Some(1));
                }
                Action::Cancel => {
                    self.popup.current_menu =
                        Some(PopupMenu::PlaylistRoot { playlist_name: playlist_name.clone() });
                    self.popup.selected.select(Some(3));
                }
                _ => {}
            },
            PopupMenu::PlaylistConfirmRename { new_name, .. } => match action {
                Action::Rename => {
                    self.popup.selected.select_next();
                }
                Action::Yes => {
                    let old_name = selected_playlist.name.clone();
                    selected_playlist.name = new_name.clone();
                    // rename both view and original
                    self.playlists.iter_mut().find(|p| p.id == id)?.name = new_name.clone();
                    self.original_playlists.iter_mut().find(|p| p.id == id)?.name =
                        new_name.clone();

                    if let Ok(_) = self.client.as_ref()?.update_playlist(&selected_playlist).await {
                        let _ = self
                            .db
                            .cmd_tx
                            .send(Command::Rename(RenameCommand::Playlist {
                                id: id.clone(),
                                new_name: new_name.clone(),
                            }))
                            .await;
                        self.reorder_lists();
                        self.set_generic_message(
                            "Playlist renamed",
                            &format!("Playlist successfully renamed to {}.", new_name),
                        );
                    } else {
                        self.set_generic_message(
                            "Error renaming playlist",
                            &format!("Failed to rename playlist to {}.", new_name),
                        );
                        self.playlists.iter_mut().find(|p| p.id == id)?.name = old_name;
                    }
                }
                Action::No => {
                    self.close_popup();
                }
                _ => {}
            },
            PopupMenu::PlaylistConfirmDelete { playlist_name } => {
                match action {
                    Action::Delete => {
                        self.popup.selected.select_last();
                    }
                    Action::Yes => {
                        // Delete playlist: playlist_name
                        if let Ok(_) = self.client.as_ref()?.delete_playlist(&id).await {
                            self.original_playlists.retain(|p| p.id != id);
                            self.playlists.retain(|p| p.id != id);
                            let indices = search_ranked_indices(
                                &self.playlists,
                                &self.state.playlists_search_term,
                                true,
                            );
                            let _ = self
                                .state
                                .playlists_scroll_state
                                .content_length(indices.len().saturating_sub(1));

                            let _ = self
                                .db
                                .cmd_tx
                                .send(Command::Delete(DeleteCommand::Playlist { id: id.clone() }))
                                .await;

                            self.set_generic_message(
                                "Playlist deleted",
                                &format!("Playlist {} successfully deleted.", playlist_name),
                            );
                        } else {
                            self.set_generic_message(
                                "Error deleting playlist",
                                &format!("Failed to delete playlist {}.", playlist_name),
                            );
                        }
                    }
                    Action::No => {
                        self.close_popup();
                    }
                    _ => {}
                }
            }
            PopupMenu::PlaylistCreate { name, mut public } => match action {
                Action::Type => {
                    self.popup.editing = true;
                }
                Action::Toggle => {
                    public = !public;
                    self.popup.current_menu =
                        Some(PopupMenu::PlaylistCreate { name: name.clone(), public });
                }
                Action::Create => {
                    if name.trim().is_empty() {
                        self.popup.editing = true;
                        self.popup.selected.select_first();
                        return None;
                    }
                    if let Ok(id) = self.client.as_ref()?.create_playlist(&name, public).await {
                        let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::Library)).await;

                        let index = self.playlists.iter().position(|p| p.id == id).unwrap_or(0);
                        self.state.selected_playlist.select(Some(index));

                        self.set_generic_message(
                            "Playlist created",
                            &format!("Playlist {} successfully created.", name),
                        );
                    } else {
                        self.set_generic_message(
                            "Error creating playlist",
                            &format!("Failed to create playlist {}.", name),
                        );
                    }
                }
                Action::Cancel => {
                    self.close_popup();
                }
                _ => {}
            },
            PopupMenu::PlaylistsChangeFilter {} => match action {
                Action::Normal => {
                    self.preferences.playlist_filter = Filter::Normal;
                    self.close_popup();
                    self.reorder_lists();
                }
                Action::ShowFavoritesFirst => {
                    self.preferences.playlist_filter = Filter::FavoritesFirst;
                    self.close_popup();
                    self.reorder_lists();
                }
                _ => {}
            },
            PopupMenu::PlaylistsChangeSort {} => match action {
                Action::Ascending => {
                    self.preferences.playlist_sort = Sort::Ascending;
                    self.close_popup();
                    self.reorder_lists();
                }
                Action::Descending => {
                    self.preferences.playlist_sort = Sort::Descending;
                    self.close_popup();
                    self.reorder_lists();
                }
                Action::DateCreated => {
                    self.preferences.playlist_sort = Sort::DateCreated;
                    self.close_popup();
                    self.reorder_lists();
                }
                Action::Random => {
                    self.preferences.playlist_sort = Sort::Random;
                    self.close_popup();
                    self.reorder_lists();
                }
                _ => {}
            },
            _ => {}
        }

        Some(())
    }

    fn apply_artist_action(&mut self, action: &Action, menu: PopupMenu) {
        match menu {
            PopupMenu::ArtistRoot { .. } => match action {
                Action::JumpToCurrent => {
                    let artists =
                        match self.state.queue.get(self.state.current_playback_state.current_index)
                        {
                            Some(song) => &song.artist_items,
                            None => return,
                        };
                    if artists.len() == 1 {
                        let artist = artists[0].clone();
                        if self.artists.iter().any(|a| a.id == artist.id) {
                            self.reposition_cursor(&artist.id, Selectable::Artist);
                        } else {
                            // try by name... jellyfin can be such a pain (the IDs are not always the same lol)
                            if let Some(artist) =
                                self.artists.iter().find(|a| a.name == artist.name).cloned()
                            {
                                self.reposition_cursor(&artist.id, Selectable::Artist);
                            }
                        }
                        self.close_popup();
                    } else {
                        self.popup.current_menu =
                            Some(PopupMenu::ArtistJumpToCurrent { artists: artists.clone() });
                        self.popup.selected.select_first();
                    }
                }
                Action::ChangeFilter => {
                    self.popup.current_menu = Some(PopupMenu::ArtistsChangeFilter {});
                    self.popup.selected.select(Some(
                        if self.preferences.artist_filter == Filter::Normal { 0 } else { 1 },
                    ));
                }
                Action::ChangeOrder => {
                    self.popup.current_menu = Some(PopupMenu::ArtistsChangeSort {});
                    self.popup.selected.select(Some(match self.preferences.artist_sort {
                        Sort::Ascending => 0,
                        Sort::Descending => 1,
                        Sort::Random => 2,
                        _ => 0, // not applicable
                    }));
                }
                _ => {}
            },
            PopupMenu::ArtistJumpToCurrent { artists, .. } => {
                if let Action::JumpToCurrent = action {
                    let selected = match self.popup.selected.selected() {
                        Some(i) => i,
                        None => return,
                    };
                    let artist = &artists[selected];
                    self.reposition_cursor(&artist.id, Selectable::Artist);
                    self.close_popup();
                }
            }
            PopupMenu::ArtistsChangeFilter {} => match action {
                Action::Normal => {
                    self.preferences.artist_filter = Filter::Normal;
                    self.close_popup();
                    self.reorder_lists();
                }
                Action::ShowFavoritesFirst => {
                    self.preferences.artist_filter = Filter::FavoritesFirst;
                    self.close_popup();
                    self.reorder_lists();
                }
                _ => {}
            },
            PopupMenu::ArtistsChangeSort {} => {
                match action {
                    Action::Ascending => {
                        self.preferences.artist_sort = Sort::Ascending;
                    }
                    Action::Descending => {
                        self.preferences.artist_sort = Sort::Descending;
                    }
                    Action::DateCreated => {
                        self.preferences.artist_sort = Sort::DateCreated;
                    }
                    Action::DateCreatedInverse => {
                        self.preferences.artist_sort = Sort::DateCreatedInverse;
                    }
                    Action::Random => {
                        self.preferences.artist_sort = Sort::Random;
                    }
                    _ => {}
                }
                self.close_popup();
                self.reorder_lists();
            }
            _ => {}
        }
    }

    /// Closes the popup including common state
    ///
    fn close_popup(&mut self) {
        self.popup.current_menu = None;
        self.popup.selected.select(None);
        self.state.active_section = self.state.last_section;
        self.popup.editing = false;
        self.popup.global = false;
        let _ = self.preferences.save();

        self.popup_search_term.clear();
        self.locally_searching = false;
    }

    /// Opens a message with a title and message and an OK button
    ///
    pub fn set_generic_message(&mut self, title: &str, message: &str) {
        self.popup.current_menu = Some(PopupMenu::GenericMessage {
            title: title.to_string(),
            message: message.to_string(),
        });
        self.popup.selected.select_last(); // move selection to OK options
    }

    /// Clear popup state and ask to render a new default popup
    /// `create_popup` will then pick this up and render a new popup
    ///
    pub async fn request_popup(&mut self, global: bool) {
        self.popup.global = global;

        if self.state.active_section == ActiveSection::Popup {
            self.state.active_section = self.state.last_section;
            self.popup.current_menu = None;
        } else {
            self.state.last_section = self.state.active_section;
            self.state.active_section = ActiveSection::Popup;
        }
    }

    /// Create popup based on the current selected tab and section
    ///
    pub fn create_popup(&mut self, frame: &mut Frame) -> Option<()> {
        if self.state.active_section != ActiveSection::Popup {
            return None;
        }

        if self.popup.global {
            if self.popup.current_menu.is_none() {
                self.popup.current_menu = Some(PopupMenu::GlobalRoot {
                    large_art: self.preferences.large_art,
                    downloading: self.download_item.is_some(),
                });
                self.popup.selected.select_first();
            }
            self.render_popup(frame);
            return Some(());
        }

        match self.state.active_tab {
            ActiveTab::Library => match self.state.last_section {
                ActiveSection::Tracks => {
                    let id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                    let track = self.tracks.iter().find(|t| t.id == id)?.clone();
                    if self.popup.current_menu.is_none() {
                        self.popup.current_menu = Some(PopupMenu::TrackRoot {
                            track_name: track.name,
                            track_id: id,
                            parent_id: track.parent_id,
                            disliked: track.disliked,
                        });
                        self.popup.selected.select_first();
                    }
                }
                ActiveSection::List => {
                    if self.popup.current_menu.is_none() {
                        let artists = self.get_id_of_selected(&self.artists, Selectable::Artist);
                        let artist = self.artists.iter().find(|a| a.id == artists)?.clone();
                        self.popup.current_menu = Some(PopupMenu::ArtistRoot {
                            artist: artist.clone(),
                            playing_artists: self
                                .state
                                .queue
                                .get(self.state.current_playback_state.current_index)
                                .map(|s| s.artist_items.clone()),
                        });
                        self.popup.selected.select_first();
                    }
                }
                _ => {
                    self.close_popup();
                }
            },
            ActiveTab::Albums => match self.state.last_section {
                ActiveSection::List => {
                    if self.popup.current_menu.is_none() {
                        let id = self.get_id_of_selected(&self.albums, Selectable::Album);
                        let album = self.albums.iter().find(|a| a.id == id)?.clone();
                        self.popup.current_menu = Some(PopupMenu::AlbumsRoot { album });
                        self.popup.selected.select_first();
                    }
                }
                ActiveSection::Tracks => {
                    let id = self.get_id_of_selected(&self.album_tracks, Selectable::AlbumTrack);
                    if self.popup.current_menu.is_none() {
                        let track = self.album_tracks.iter().find(|t| t.id == id)?;
                        self.popup.current_menu = Some(PopupMenu::AlbumTrackRoot {
                            track_id: id.clone(),
                            track_name: track.name.clone(),
                            disliked: track.disliked,
                        });
                        self.popup.selected.select_first();
                    }
                }
                _ => {
                    self.close_popup();
                }
            },
            ActiveTab::Playlists => match self.state.last_section {
                ActiveSection::List => {
                    if self.popup.current_menu.is_none() {
                        let id = self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                        let playlist = self.playlists.iter().find(|p| p.id == id)?.clone();
                        self.popup.current_menu =
                            Some(PopupMenu::PlaylistRoot { playlist_name: playlist.name });
                        self.popup.selected.select_first();
                    }
                }
                ActiveSection::Tracks => {
                    let id =
                        self.get_id_of_selected(&self.playlist_tracks, Selectable::PlaylistTrack);
                    if self.popup.current_menu.is_none() {
                        let track = self.playlist_tracks.iter().find(|t| t.id == id)?;
                        self.popup.current_menu = Some(PopupMenu::PlaylistTracksRoot {
                            track_name: track.name.clone(),
                            disliked: track.disliked,
                        });
                        self.popup.selected.select_first();
                    }
                }
                _ => {
                    self.close_popup();
                }
            },
            _ => {
                self.close_popup();
            }
        }

        self.render_popup(frame);

        Some(())
    }

    /// This function decides which popup to draw based on state alone
    ///
    fn render_popup(&mut self, frame: &mut Frame) -> Option<()> {
        if let Some(menu) = &mut self.popup.current_menu {
            let area = frame.area();
            let options = if self.client.is_some() {
                menu.options()
            } else {
                menu.options().into_iter().filter(|o| !o.online).collect::<Vec<PopupAction>>()
            };

            if options.is_empty() {
                return None;
            }

            let refs = search_ranked_refs(&options, &self.popup_search_term, true);
            self.popup.displayed_options = refs
                .iter()
                .filter_map(|action| {
                    options.iter().find(|o| o.id() == action.id()).cloned() // store owned versions
                })
                .collect();

            let items = self
                .popup
                .displayed_options
                .iter()
                .map(|action| {
                    // underline the matching search subsequence ranges
                    let mut item = Text::default();
                    let mut last_end = 0;
                    let all_subsequences = find_all_subsequences(
                        &self.popup_search_term.to_lowercase(),
                        &action.label.to_lowercase(),
                    );
                    for (start, end) in all_subsequences {
                        if last_end < start {
                            item.push_span(Span::styled(
                                &action.label[last_end..start],
                                action.style,
                            ));
                        }

                        item.push_span(Span::styled(
                            &action.label[start..end],
                            action.style.underlined(),
                        ));

                        last_end = end;
                    }

                    if last_end < action.label.len() {
                        item.push_span(Span::styled(&action.label[last_end..], action.style));
                    }
                    ListItem::new(item)
                })
                .collect::<Vec<ListItem>>();

            let list = List::new(items)
                .block(
                    Block::bordered()
                        .title(
                            Line::from(menu.title())
                                .fg(self.theme.resolve(&self.theme.border_focused)),
                        )
                        .title_bottom(
                            (if self.locally_searching {
                                Line::from(format!("Searching: {}", self.popup_search_term))
                            } else if !self.popup_search_term.is_empty() {
                                Line::from(format!("Matching: {}", self.popup_search_term))
                            } else {
                                Line::from("")
                            })
                            .fg(self.theme.resolve(&self.theme.border_focused)),
                        )
                        .border_style(self.theme.resolve(&self.theme.border_focused))
                        .border_type(self.border_type)
                        .style(Style::default().bg(
                            self.theme.resolve_opt(&self.theme.background).unwrap_or(Color::Reset),
                        )),
                )
                .highlight_style(
                    Style::default()
                        .bg(if self.popup.editing {
                            self.theme.primary_color
                        } else {
                            self.theme.resolve(&self.theme.selected_active_background)
                        })
                        .fg(self.theme.resolve(&self.theme.selected_active_foreground))
                        .bold(),
                )
                .style(Style::default().fg(self.theme.resolve(&self.theme.foreground)))
                .highlight_symbol(if self.popup.editing { "E:" } else { ">>" });

            let window_height = area.height;
            let percent_height =
                ((options.len() + 2) as f32 / window_height as f32 * 100.0).ceil() as u16;

            let width = if let PopupMenu::GlobalRunScheduledTask { .. } = menu { 70 } else { 30 };

            let popup_area = popup_area(area, width, percent_height);
            frame.render_widget(Clear, popup_area); // clears the background

            frame.render_stateful_widget(list, popup_area, &mut self.popup.selected);
        }

        Some(())
    }
}
