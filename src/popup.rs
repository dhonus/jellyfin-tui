/*
This file can look very daunting, but it actually just defines a sort of structure to render popups.
- Each popup is defined as an enum, and each enum variant has a different set of actions that can be taken.
- The `PopupState` struct keeps track of the current state of the popup, such as which option is selected.
- We make a decision as to which action to take based on the current state :)
- The `create_popup` function is responsible for creating and rendering the popup on the screen.
*/

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{self, Style, Stylize},
    text::Span,
    widgets::{Block, Clear, List, ListItem},
    Frame,
};
use serde::{Deserialize, Serialize};

use crate::{
    client::{Artist, Playlist, ScheduledTask},
    keyboard::{search_results, ActiveSection, ActiveTab, Selectable}, tui::{Filter, Sort},
};

/// helper function to create a centered rect using up certain percentage of the available rect `r`
fn popup_area(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Start)
        .margin(0);
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
    },
    GlobalRunScheduledTask {
        tasks: Vec<ScheduledTask>,
    },
    GlobalShuffle {
        tracks_n: usize,
        only_played: bool,
        only_unplayed: bool,
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
    },
    TrackAddToPlaylist {
        track_name: String,
        track_id: String,
        playlists: Vec<Playlist>,
    },
    /**
     * Playlist tracks related popups
     */
    PlaylistTracksRoot {
        track_name: String,
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
    AlbumsRoot {},
    AlbumsChangeFilter {},
    AlbumsChangeSort {},
}

enum Action {
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
    AddToPlaylist,
    GoAlbum,
    JumpToCurrent,
    Refresh,
    Create,
    Toggle,
    ChangeFilter,
    ChangeOrder,
    Ascending,
    Descending,
    DateCreated,
    Normal,
    ShowFavoritesFirst,
    RunScheduledTask,
    ChangeCoverArtLayout,
    OnlyPlayed,
    OnlyUnplayed,
}

struct PopupAction {
    label: String,
    action: Action,
    style: Style,
}

impl PopupMenu {
    fn title(&self) -> String {
        match self {
            PopupMenu::GenericMessage { title, .. } => format!("{}", title),
            // ---------- Global commands ---------- //
            PopupMenu::GlobalRoot { .. }=> "Global Commands".to_string(),
            PopupMenu::GlobalRunScheduledTask { .. } => "Run a scheduled task".to_string(),
            PopupMenu::GlobalShuffle { .. } => "Global Shuffle".to_string(),
            // ---------- Playlists ---------- //
            PopupMenu::PlaylistRoot { playlist_name, .. } => format!("{}", playlist_name),
            PopupMenu::PlaylistSetName { .. } => "Type to change name".to_string(),
            PopupMenu::PlaylistConfirmRename { .. } => "Confirm Rename".to_string(),
            PopupMenu::PlaylistConfirmDelete { .. } => "Confirm Delete".to_string(),
            PopupMenu::PlaylistCreate { .. } => "Create Playlist".to_string(),
            PopupMenu::PlaylistsChangeSort {  } => "Change sort order".to_string(),
            PopupMenu::PlaylistsChangeFilter {  } => "Change filter".to_string(),
            // ---------- Tracks ---------- //
            PopupMenu::TrackRoot { track_name, .. } => format!("{}", track_name),
            PopupMenu::TrackAddToPlaylist { track_name, .. } => format!("{}", track_name),
            // ---------- Playlist tracks ---------- //
            PopupMenu::PlaylistTracksRoot { track_name, .. } => format!("{}", track_name),
            PopupMenu::PlaylistTrackAddToPlaylist { track_name, .. } => format!("{}", track_name),
            PopupMenu::PlaylistTracksRemove { track_name, .. } => format!("{}", track_name),
            // ---------- Artists ---------- //
            PopupMenu::ArtistRoot { artist, .. } => format!("{}", artist.name),
            PopupMenu::ArtistJumpToCurrent { artists, .. } => {
                format!("Which of these {} to jump to?", artists.len())
            }
            PopupMenu::ArtistsChangeFilter {  } => "Change filter".to_string(),
            PopupMenu::ArtistsChangeSort {  } => "Change sort".to_string(),
            // ---------- Albums ---------- //
            PopupMenu::AlbumsRoot {  } => "Albums".to_string(),
            PopupMenu::AlbumsChangeFilter {  } => "Change filter".to_string(),
            PopupMenu::AlbumsChangeSort {  } => "Change sort".to_string(),
        }
    }

    // Return the list of options displayed by this menu
    fn options(&self) -> Vec<PopupAction> {
        match self {
            PopupMenu::GenericMessage { message, .. } => vec![
                PopupAction {
                    label: format!("{}", message),
                    action: Action::Ok,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Ok".to_string(),
                    action: Action::Ok,
                    style: Style::default(),
                },
            ],
            // ---------- Global commands ---------- //
            PopupMenu::GlobalRoot { large_art } => vec![
                PopupAction {
                    label: "Refresh library".to_string(),
                    action: Action::Refresh,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Run a scheduled task".to_string(),
                    action: Action::RunScheduledTask,
                    style: Style::default(),
                },
                PopupAction {
                    label: if *large_art {
                        "Switch to small cover art".to_string()
                    } else {
                        "Switch to large cover art".to_string()
                    },
                    action: Action::ChangeCoverArtLayout,
                    style: Style::default(),
                },
            ],
            PopupMenu::GlobalRunScheduledTask { tasks } => {
                let mut actions = vec![];
                let mut categories = tasks.iter().map(|t| t.category.clone()).collect::<Vec<String>>();
                categories.sort();
                categories.dedup();
                for category in categories {
                    for task in tasks.iter().filter(|t| t.category == category) {
                        actions.push(PopupAction {
                            label: format!("{}: {} ({})", category, task.name, task.description),
                            action: Action::RunScheduledTask,
                            style: Style::default(),
                        });
                    }
                }
                actions
            }
            PopupMenu::GlobalShuffle { tracks_n, only_played, only_unplayed } => vec![
                PopupAction {
                    label: format!("Shuffle {} tracks. +/- to change", tracks_n),
                    action: Action::None,
                    style: Style::default(),
                },
                PopupAction {
                    label: if *only_played { "✓ Only played tracks" } else { "  Only played tracks" }.to_string(),
                    action: Action::OnlyPlayed,
                    style: Style::default(),
                },
                PopupAction {
                    label: if *only_unplayed { "✓ Only unplayed tracks" } else { "  Only unplayed tracks" }.to_string(),
                    action: Action::OnlyUnplayed,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Play".to_string(),
                    action: Action::Play,
                    style: Style::default(),
                }
            ],
            // ---------- Playlists ----------
            PopupMenu::PlaylistRoot { .. } => vec![
                PopupAction {
                    label: "Play".to_string(),
                    action: Action::Play,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Append to main queue".to_string(),
                    action: Action::Append,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Append to temporary queue".to_string(),
                    action: Action::AppendTemporary,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Rename".to_string(),
                    action: Action::Rename,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Create new playlist".to_string(),
                    action: Action::Create,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Change filter".to_string(),
                    action: Action::ChangeFilter,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Change sort order".to_string(),
                    action: Action::ChangeOrder,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Delete".to_string(),
                    action: Action::Delete,
                    style: Style::default().fg(style::Color::Red),
                },
            ],
            PopupMenu::PlaylistSetName { new_name, .. } => {
                vec![
                    PopupAction {
                        // if new_name is empty, then the user has not typed anything yet. Otherwise show the new name
                        label: if new_name.is_empty() {
                            "Type in the new name".to_string()
                        } else {
                            format!("Name: {}", new_name)
                        },
                        action: Action::Type,
                        style: Style::default(),
                    },
                    PopupAction {
                        label: "Confirm".to_string(),
                        action: Action::Confirm,
                        style: Style::default(),
                    },
                    PopupAction {
                        label: "Cancel".to_string(),
                        action: Action::Cancel,
                        style: Style::default(),
                    },
                ]
            }
            PopupMenu::PlaylistConfirmRename { new_name, .. } => vec![
                PopupAction {
                    label: format!("Rename to: {}", new_name),
                    action: Action::Rename,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Yes".to_string(),
                    action: Action::Yes,
                    style: Style::default(),
                },
                PopupAction {
                    label: "No".to_string(),
                    action: Action::No,
                    style: Style::default(),
                },
            ],
            PopupMenu::PlaylistConfirmDelete { playlist_name } => vec![
                PopupAction {
                    label: format!("Delete playlist: {}", playlist_name),
                    action: Action::Delete,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Yes".to_string(),
                    action: Action::Yes,
                    style: Style::default(),
                },
                PopupAction {
                    label: "No".to_string(),
                    action: Action::No,
                    style: Style::default(),
                },
            ],
            PopupMenu::PlaylistCreate { name, public } => vec![
                PopupAction {
                    label: if name.is_empty() {
                        format!("Type in the new playlist name")
                    } else {
                        format!("Name: {}", name)
                    },
                    action: Action::Type,
                    style: Style::default(),
                },
                PopupAction {
                    label: format!("Public: {}", public),
                    action: Action::Toggle,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Create".to_string(),
                    action: Action::Create,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Cancel".to_string(),
                    action: Action::Cancel,
                    style: Style::default(),
                },
            ],
            PopupMenu::PlaylistsChangeSort {  } => vec![
                PopupAction {
                    label: "Ascending".to_string(),
                    action: Action::Ascending,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Descending".to_string(),
                    action: Action::Descending,
                    style: Style::default(),
                },
            ],
            PopupMenu::PlaylistsChangeFilter {  } => vec![
                PopupAction {
                    label: "Normal".to_string(),
                    action: Action::Normal,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Show favorites first".to_string(),
                    action: Action::ShowFavoritesFirst,
                    style: Style::default(),
                },
            ],
            // ---------- Tracks ---------- //
            PopupMenu::TrackRoot { .. } => vec![
                PopupAction {
                    label: "Jump to current song".to_string(),
                    action: Action::JumpToCurrent,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Add to playlist".to_string(),
                    action: Action::AddToPlaylist,
                    style: Style::default(),
                },
            ],
            PopupMenu::TrackAddToPlaylist { playlists, .. } => {
                let mut actions = vec![];
                for playlist in playlists {
                    actions.push(PopupAction {
                        label: format!("{} ({})", playlist.name, playlist.child_count),
                        action: Action::AddToPlaylist,
                        style: Style::default(),
                    });
                }
                actions
            }
            // ---------- Playlist tracks ---------- //
            PopupMenu::PlaylistTracksRoot { .. } => vec![
                PopupAction {
                    label: "Go to album".to_string(),
                    action: Action::GoAlbum,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Add to playlist".to_string(),
                    action: Action::AddToPlaylist,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Remove from this playlist".to_string(),
                    action: Action::Delete,
                    style: Style::default().fg(style::Color::Red),
                },
            ],
            PopupMenu::PlaylistTrackAddToPlaylist { playlists, .. } => {
                let mut actions = vec![];
                for playlist in playlists {
                    actions.push(PopupAction {
                        label: format!("{} ({})", playlist.name, playlist.child_count),
                        action: Action::AddToPlaylist,
                        style: Style::default(),
                    });
                }
                actions
            }
            PopupMenu::PlaylistTracksRemove { track_name, .. } => vec![
                PopupAction {
                    label: format!("Remove {} from playlist?", track_name),
                    action: Action::None,
                    style: Style::default().fg(style::Color::Red),
                },
                PopupAction {
                    label: "Yes".to_string(),
                    action: Action::Yes,
                    style: Style::default().fg(style::Color::Red),
                },
                PopupAction {
                    label: "No".to_string(),
                    action: Action::No,
                    style: Style::default(),
                },
            ],
            // ---------- Artists ---------- //
            PopupMenu::ArtistRoot {
                playing_artists, ..
            } => {
                let mut actions = vec![];
                if let Some(artists) = playing_artists {
                    actions.push(PopupAction {
                        label: format!(
                            "Jump to current artist: {}",
                            artists
                                .into_iter()
                                .map(|a| a.name.clone())
                                .collect::<Vec<String>>()
                                .join(", ")
                        ),
                        action: Action::JumpToCurrent,
                        style: Style::default(),
                    });
                }
                actions.push(PopupAction {
                    label: "Change filter".to_string(),
                    action: Action::ChangeFilter,
                    style: Style::default(),
                });
                actions.push(PopupAction {
                    label: "Change sort order".to_string(),
                    action: Action::ChangeOrder,
                    style: Style::default(),
                });
                actions
            }
            PopupMenu::ArtistJumpToCurrent { artists, .. } => {
                let mut actions = vec![];
                for artist in artists {
                    actions.push(PopupAction {
                        label: format!("{}", artist.name),
                        action: Action::JumpToCurrent,
                        style: Style::default(),
                    });
                }
                actions
            }
            PopupMenu::ArtistsChangeFilter {  } => vec![
                PopupAction {
                    label: "Normal".to_string(),
                    action: Action::Normal,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Show favorites first".to_string(),
                    action: Action::ShowFavoritesFirst,
                    style: Style::default(),
                },
            ],
            PopupMenu::ArtistsChangeSort {  } => vec![
                PopupAction {
                    label: "Ascending".to_string(),
                    action: Action::Ascending,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Descending".to_string(),
                    action: Action::Descending,
                    style: Style::default(),
                },
            ],
            // ---------- Albums ---------- //
            PopupMenu::AlbumsRoot {  } => vec![
                PopupAction {
                    label: "Change filter".to_string(),
                    action: Action::ChangeFilter,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Change sort order".to_string(),
                    action: Action::ChangeOrder,
                    style: Style::default(),
                },
            ],
            PopupMenu::AlbumsChangeFilter {  } => vec![
                PopupAction {
                    label: "Normal".to_string(),
                    action: Action::Normal,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Show favorites first".to_string(),
                    action: Action::ShowFavoritesFirst,
                    style: Style::default(),
                },
            ],
            PopupMenu::AlbumsChangeSort {  } => vec![
                PopupAction {
                    label: "Ascending".to_string(),
                    action: Action::Ascending,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Descending".to_string(),
                    action: Action::Descending,
                    style: Style::default(),
                },
                PopupAction {
                    label: "Date created".to_string(),
                    action: Action::DateCreated,
                    style: Style::default(),
                },
            ],
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct PopupState {
    pub selected: ratatui::widgets::ListState,
    pub current_menu: Option<PopupMenu>,
    pub editing: bool,
    editing_original: String,
    editing_new: String,
    pub global: bool, // if true the popup will be for global commands. Set before calling create_popup
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
            KeyCode::Char('+') => {
                if let Some(PopupMenu::GlobalShuffle { tracks_n, only_played, only_unplayed }) = &self.popup.current_menu {
                    self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                        tracks_n: tracks_n + 10,
                        only_played: *only_played,
                        only_unplayed: *only_unplayed,
                    });
                }
            }
            KeyCode::Char('-') => {
                if let Some(PopupMenu::GlobalShuffle { tracks_n, only_played, only_unplayed }) = &self.popup.current_menu {
                    if *tracks_n > 1 {
                        self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                            tracks_n: tracks_n - 10,
                            only_played: *only_played,
                            only_unplayed: *only_unplayed,
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

        let options = menu.options();

        let action = match options.get(selected).map(|a| &a.action) {
            Some(action) => action,
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
                ActiveSection::Artists => {
                    self.apply_artist_action(&action, menu.clone());
                }
                _ => {}
            },
            ActiveTab::Albums => match self.state.last_section {
                ActiveSection::Artists => {
                    self.apply_album_action(&action, menu.clone());
                }
                _ => {}
            },
            ActiveTab::Playlists => match self.state.last_section {
                ActiveSection::Artists => {
                    if let None = self.apply_playlist_action(&action, menu.clone()).await {
                        self.close_popup();
                    }
                }
                ActiveSection::Tracks => {
                    self.apply_playlist_tracks_action(&action, menu.clone())
                        .await;
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
            PopupMenu::GlobalRoot { .. } => match action {
                Action::Refresh => {
                    if let Ok(_) = self.refresh().await {
                        self.popup.current_menu = Some(PopupMenu::GenericMessage {
                            title: "Library refreshed".to_string(),
                            message: "Library has been refreshed.".to_string(),
                        });
                    } else {
                        self.popup.current_menu = Some(PopupMenu::GenericMessage {
                            title: "Error refreshing library".to_string(),
                            message: "Failed to refresh library.".to_string(),
                        });
                    }
                    self.popup.selected.select(Some(1));
                }
                Action::ChangeCoverArtLayout => {
                    self.state.large_art = !self.state.large_art;
                    self.close_popup();
                }
                Action::RunScheduledTask => {
                    let tasks = self.client.as_ref()?.scheduled_tasks().await.unwrap_or(vec![]);
                    self.popup.current_menu = Some(PopupMenu::GlobalRunScheduledTask { tasks });
                    self.popup.selected.select(Some(0));
                }
                _ => {}
            },
            PopupMenu::GlobalRunScheduledTask { tasks } => {
                let selected = self.popup.selected.selected()?;
                let mut mapped_tasks = vec![];
                let mut categories = tasks.iter().map(|t| t.category.clone()).collect::<Vec<String>>();
                categories.sort();
                categories.dedup();
                for category in categories {
                    for task in tasks.iter().filter(|t| t.category == category) {
                        mapped_tasks.push(task.clone());
                    }
                }
                let task = mapped_tasks.get(selected)?;
                if let Ok(_) = self.client.as_ref()?.run_scheduled_task(&task.id).await {
                    self.popup.current_menu = Some(PopupMenu::GenericMessage {
                        title: format!("Task {} executed successfully", task.name),
                        message: format!("Try reloading your library to see changes."),
                    });
                } else {
                    self.popup.current_menu = Some(PopupMenu::GenericMessage {
                        title: "Error executing task".to_string(),
                        message: format!("Failed to execute task {}.", task.name),
                    });
                }
            }
            PopupMenu::GlobalShuffle { tracks_n, only_played, only_unplayed } => match action {
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
                        });
                    } else {
                        self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                            tracks_n,
                            only_played: false,
                            only_unplayed: false,
                        });
                    }
                }
                Action::OnlyUnplayed => {
                    if !only_unplayed {
                        self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                            tracks_n,
                            only_played: false,
                            only_unplayed: true,
                        });
                    } else {
                        self.popup.current_menu = Some(PopupMenu::GlobalShuffle {
                            tracks_n,
                            only_played: false,
                            only_unplayed: false,
                        });
                    }
                }
                Action::Play => {
                    let tracks = self.client.as_ref()?.random_tracks(tracks_n, only_played, only_unplayed).await.unwrap_or(vec![]);
                    self.replace_queue(&tracks, 0).await;
                    self.close_popup();
                    self.state.preffered_global_shuffle = PopupMenu::GlobalShuffle {
                        tracks_n,
                        only_played,
                        only_unplayed,
                    };
                }
                _ => {
                    self.close_popup();
                }
            },
            _ => {}
        }
        Some(())
    }
    async fn apply_track_action(&mut self, action: &Action, menu: PopupMenu) -> Option<()> {
        match menu {
            PopupMenu::TrackRoot {
                track_id,
                track_name,
            } => match action {
                Action::AddToPlaylist => {
                    self.popup.current_menu = Some(PopupMenu::TrackAddToPlaylist {
                        track_name,
                        track_id,
                        playlists: self.playlists.clone(),
                    });
                    self.popup.selected.select(Some(0));
                },
                Action::JumpToCurrent => {
                    let current_track = self.state.queue.get(self.state.current_playback_state.current_index as usize)?;
                    let artist = self.artists.iter().find(
                        |a| current_track.artist_items.get(0).is_some_and(|item| a.id == item.id)
                    )?;
                    let artist_id = artist.id.clone();
                    let current_track_id = current_track.id.clone();
                    if artist_id != self.state.current_artist.id {
                        let index = self.artists.iter().position(|a| a.id == artist_id).unwrap_or(0);
                        self.artist_select_by_index(index);
                        self.discography(&artist_id).await;
                        self.artists[index].jellyfintui_recently_added = false;
                    }
                    if let Some(track) = self.tracks.iter().find(|t| t.id == current_track_id) {
                        let index = self
                            .tracks
                            .iter()
                            .position(|t| t.id == track.id)
                            .unwrap_or(0);
                        self.track_select_by_index(index);
                    }
                    self.close_popup();
                },
                _ => {
                    self.close_popup();
                }
            },
            PopupMenu::TrackAddToPlaylist {
                track_name,
                track_id,
                playlists,
            } => match action {
                Action::AddToPlaylist => {
                    let selected = self.popup.selected.selected()?;
                    let playlist_id = &playlists[selected].id;
                    if let Some(client) = self.client.as_ref() {
                        if let Ok(_) = client.add_to_playlist(&track_id, playlist_id).await {
                            self.popup.current_menu = Some(PopupMenu::GenericMessage {
                                title: "Track added".to_string(),
                                message: format!(
                                    "Track {} successfully added to playlist {}.",
                                    track_name, playlists[selected].name
                                ),
                            });
                        } else {
                            self.popup.current_menu = Some(PopupMenu::GenericMessage {
                                title: "Error adding track".to_string(),
                                message: format!(
                                    "Failed to add track {} to playlist {}.",
                                    track_name, playlists[selected].name
                                ),
                            });
                        }
                    }
                },
                _ => {
                    self.close_popup();
                }
            },
            _ => {}
        }
        Some(())
    }

    fn apply_album_action(&mut self, action: &Action, menu: PopupMenu) -> Option<()> {
        match menu {
            PopupMenu::AlbumsRoot { .. } => match action {
                Action::ChangeFilter => {
                    self.popup.current_menu = Some(PopupMenu::AlbumsChangeFilter {});
                    self.popup.selected.select(
                        match self.state.album_filter {
                            Filter::Normal => Some(0),
                            Filter::FavoritesFirst => Some(1),
                        }
                    )
                }
                Action::ChangeOrder => {
                    self.popup.current_menu = Some(PopupMenu::AlbumsChangeSort {});
                    self.popup.selected.select(Some(
                        match self.state.album_sort {
                            Sort::Ascending => 0,
                            Sort::Descending => 1,
                            Sort::DateCreated => 2,
                        }
                    ));
                }
                _ => {}
            },
            PopupMenu::AlbumsChangeFilter { .. } => match action {
                Action::Normal => {
                    self.state.album_filter = Filter::Normal;
                    self.reorder_lists();
                    self.close_popup();
                }
                Action::ShowFavoritesFirst => {
                    self.state.album_filter = Filter::FavoritesFirst;
                    self.reorder_lists();
                    self.close_popup();
                }
                _ => {}
            },
            PopupMenu::AlbumsChangeSort { .. } => match action {
                Action::Ascending => {
                    self.state.album_sort = Sort::Ascending;
                    self.reorder_lists();
                    self.close_popup();
                }
                Action::Descending => {
                    self.state.album_sort = Sort::Descending;
                    self.reorder_lists();
                    self.close_popup();
                }
                Action::DateCreated => {
                    self.state.album_sort = Sort::DateCreated;
                    self.reorder_lists();
                    self.close_popup();
                }
                _ => {}
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
            PopupMenu::PlaylistTracksRoot { .. } => {
                let selected = match self.state.selected_playlist_track.selected() {
                    Some(i) => i,
                    None => {
                        self.close_popup();
                        return None;
                    }
                };
                let items = search_results(&self.tracks_playlist, &self.state.playlist_tracks_search_term, true);
                let track = match items.get(selected) {
                    Some(track) => {
                        let track = self.tracks_playlist.iter().find(|t| t.id == *track)?;
                        track.clone()
                    }
                    None => {
                        return None;
                    }
                };
                match action {
                    Action::GoAlbum => {
                        self.close_popup();
                        // in the Music tab, select this artist
                        self.state.active_tab = ActiveTab::Library;
                        self.state.active_section = ActiveSection::Artists;
                        self.state.tracks_search_term = String::from("");

                        let track_id = track.id.clone();

                        let artist_id = if !track.album_artists.is_empty() {
                            track.album_artists[0].id.clone()
                        } else {
                            String::from("")
                        };
                        self.artist_select_by_index(0);

                        if let Some(artist) = self.artists.iter().find(|a| a.id == artist_id) {
                            let index = self
                                .artists
                                .iter()
                                .position(|a| a.id == artist.id)
                                .unwrap_or(0);
                            self.artist_select_by_index(index);

                            let selected = self.state.selected_artist.selected().unwrap_or(0);
                            self.discography(&self.artists[selected].id.clone()).await;
                            self.artists[selected].jellyfintui_recently_added = false;
                            self.track_select_by_index(0);

                            // now find the first track that matches this album
                            if let Some(track) = self.tracks.iter().find(|t| t.id == track_id) {
                                let index = self
                                    .tracks
                                    .iter()
                                    .position(|t| t.id == track.id)
                                    .unwrap_or(0);
                                self.track_select_by_index(index);
                            }
                        }
                    }
                    Action::AddToPlaylist => {
                        self.popup.current_menu = Some(PopupMenu::PlaylistTrackAddToPlaylist {
                            track_name: track.name.clone(),
                            track_id: track.id.clone(),
                            playlists: self.playlists.clone(),
                        });
                        self.popup.selected.select(Some(0));
                    }
                    Action::Delete => {
                        self.popup.current_menu = Some(PopupMenu::PlaylistTracksRemove {
                            track_name: track.name,
                            track_id: track.id,
                            playlist_name: self.state.current_playlist.name.clone(),
                            playlist_id: self.state.current_playlist.id.clone(),
                        });
                        self.popup.selected.select(Some(1));
                    }
                    _ => {}
                }
            }
            PopupMenu::PlaylistTrackAddToPlaylist {
                track_name,
                track_id,
                playlists,
            } => if let Action::AddToPlaylist = action {
                let selected = self.popup.selected.selected()?;
                let playlist_id = &playlists[selected].id;
                if let Some(client) = self.client.as_ref() {
                    if let Ok(_) = client.add_to_playlist(&track_id, playlist_id).await {
                        self.popup.current_menu = Some(PopupMenu::GenericMessage {
                            title: "Track added".to_string(),
                            message: format!(
                                "Track {} successfully added to playlist {}.",
                                track_name, playlists[selected].name
                            ),
                        });
                    } else {
                        self.popup.current_menu = Some(PopupMenu::GenericMessage {
                            title: "Error adding track".to_string(),
                            message: format!(
                                "Failed to add track {} to playlist {}.",
                                track_name, playlists[selected].name
                            ),
                        });
                    }
                }
            },
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
                    if let Some(client) = self.client.as_ref() {
                        if let Ok(_) = client.remove_from_playlist(&track_id, &playlist_id).await {
                            self.tracks_playlist
                                .retain(|t| t.playlist_item_id != track_id);
                            self.popup.current_menu = Some(PopupMenu::GenericMessage {
                                title: format!("{} removed", track_name),
                                message: format!("Successfully removed from {}.", playlist_name),
                            });
                        } else {
                            self.popup.current_menu = Some(PopupMenu::GenericMessage {
                                title: "Error removing track".to_string(),
                                message: format!(
                                    "Failed to remove track {} from playlist {}.",
                                    track_name, playlist_name
                                ),
                            });
                        }
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
        let selected_playlist = self.playlists.iter().find(|p| p.id == id)?.clone();

        match menu {
            PopupMenu::PlaylistRoot { .. } => {
                match action {
                    Action::Play => {
                        if let Some(client) = self.client.as_ref() {
                            if let Ok(playlist) = client.playlist(&id).await {
                                self.state.current_playlist = selected_playlist.clone();
                                self.replace_queue(&playlist.items, 0).await;
                            }
                        }
                        self.close_popup();
                    }
                    Action::Append => {
                        if let Some(client) = self.client.as_ref() {
                            if let Ok(playlist) = client.playlist(&id).await {
                                self.append_to_queue(&playlist.items, 0).await;
                                self.close_popup();
                            }
                        }
                    }
                    Action::AppendTemporary => {
                        if let Some(client) = self.client.as_ref() {
                            if let Ok(playlist) = client.playlist(&id).await {
                                self.push_to_queue(&playlist.items, 0, playlist.items.len())
                                    .await;
                                self.close_popup();
                            }
                        }
                    }
                    Action::Rename => {
                        
                        self.popup.current_menu = Some(PopupMenu::PlaylistSetName {
                            playlist_name: selected_playlist.name.clone(),
                            new_name: selected_playlist.name.clone(),
                        });
                        self.popup.editing_original = selected_playlist.name.clone();
                        self.popup.editing_new = selected_playlist.name.clone();
                        self.popup.selected.select(Some(0));
                        self.popup.editing = true;
                    }
                    Action::Create => {
                        self.popup.current_menu = Some(PopupMenu::PlaylistCreate {
                            name: "".to_string(),
                            public: false,
                        });
                        self.popup.editing_original = "".to_string();
                        self.popup.editing_new = "".to_string();
                        self.popup.selected.select(Some(0));
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
                        // self.popup.selected.select(Some(0));
                        self.popup.selected.select(Some(if self.state.playlist_filter == Filter::Normal { 0 } else { 1 }));
                    }
                    Action::ChangeOrder => {
                        self.popup.current_menu = Some(PopupMenu::PlaylistsChangeSort {});
                        self.popup.selected.select(Some(if self.state.playlist_sort == Sort::Ascending { 0 } else { 1 }));
                    }
                    _ => {}
                }
            }
            PopupMenu::PlaylistSetName {
                playlist_name,
                new_name,
            } => match action {
                Action::Type => {
                    self.popup.editing = true;
                }
                Action::Confirm => {
                    if new_name.trim().is_empty() {
                        self.popup.editing = true;
                        self.popup.selected.select(Some(0));
                        return None;
                    }
                    self.popup.current_menu = Some(PopupMenu::PlaylistConfirmRename {
                        new_name: new_name.clone(),
                    });
                    self.popup.selected.select(Some(1));
                }
                Action::Cancel => {
                    self.popup.current_menu = Some(PopupMenu::PlaylistRoot {
                        playlist_name: playlist_name.clone(),
                    });
                    self.popup.selected.select(Some(3));
                }
                _ => {}
            },
            PopupMenu::PlaylistConfirmRename { new_name, .. } => match action {
                Action::Rename => {
                    self.popup.selected.select_next();
                }
                Action::Yes => {
                    if let Some(client) = self.client.as_ref() {
                        let old_name = selected_playlist.name.clone();
                        // self.playlists[selected].name = new_name.clone();
                        self.playlists.iter_mut().find(|p| p.id == id)?.name = new_name.clone();
                        if let Ok(_) = client.update_playlist(&selected_playlist).await {
                            self.popup.current_menu = Some(PopupMenu::GenericMessage {
                                title: "Playlist renamed".to_string(),
                                message: format!("Playlist successfully renamed to {}.", new_name),
                            });
                        } else {
                            self.popup.current_menu = Some(PopupMenu::GenericMessage {
                                title: "Error renaming playlist".to_string(),
                                message: format!("Failed to rename playlist to {}.", new_name),
                            });
                            self.playlists.iter_mut().find(|p| p.id == id)?.name = old_name;
                        }
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
                        if let Some(client) = self.client.as_ref() {
                            if let Ok(_) = client.delete_playlist(&id).await {
                                self.playlists.retain(|p| p.id != id);
                                let items = search_results(&self.playlists, &self.state.playlists_search_term, false);
                                let _ = self.state.playlists_scroll_state.content_length(items.len().saturating_sub(1));

                                self.popup.current_menu = Some(PopupMenu::GenericMessage {
                                    title: "Playlist deleted".to_string(),
                                    message: format!(
                                        "Playlist {} successfully deleted.",
                                        playlist_name
                                    ),
                                });
                            } else {
                                self.popup.current_menu = Some(PopupMenu::GenericMessage {
                                    title: "Error deleting playlist".to_string(),
                                    message: format!(
                                        "Failed to delete playlist {}.",
                                        playlist_name
                                    ),
                                });
                            }
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
                    self.popup.current_menu = Some(PopupMenu::PlaylistCreate {
                        name: name.clone(),
                        public,
                    });
                }
                Action::Create => {
                    if name.trim().is_empty() {
                        self.popup.editing = true;
                        self.popup.selected.select(Some(0));
                        return None;
                    }
                    if let Some(client) = self.client.as_ref() {
                        if let Ok(id) = client.create_playlist(&name, public).await {
                            if let Err(_) = self.refresh().await {
                                self.popup.current_menu = Some(PopupMenu::GenericMessage {
                                    title: "Error refreshing library".to_string(),
                                    message: format!("The playlist {} was created but the library could not be refreshed. Consider restarting jellyfin-tui.", name),
                                });
                                return None;
                            }

                            let index = self
                                .playlists
                                .iter()
                                .position(|p| p.id == id)
                                .unwrap_or(0);
                            self.state.selected_playlist.select(Some(index));

                            self.popup.current_menu = Some(PopupMenu::GenericMessage {
                                title: "Playlist created".to_string(),
                                message: format!("Playlist {} successfully created.", name),
                            });
                        } else {
                            self.popup.current_menu = Some(PopupMenu::GenericMessage {
                                title: "Error creating playlist".to_string(),
                                message: format!("Failed to create playlist {}.", name),
                            });
                        }
                    }
                }
                Action::Cancel => {
                    self.close_popup();
                }
                _ => {}
            },
            PopupMenu::PlaylistsChangeFilter {  } => match action {
                Action::Normal => {
                    self.state.playlist_filter = Filter::Normal;
                    self.close_popup();
                    self.reorder_lists();
                }
                Action::ShowFavoritesFirst => {
                    self.state.playlist_filter = Filter::FavoritesFirst;
                    self.close_popup();
                    self.reorder_lists();
                }
                _ => {}
            },
            PopupMenu::PlaylistsChangeSort {  } => match action {
                Action::Ascending => {
                    self.state.playlist_sort = Sort::Ascending;
                    self.close_popup();
                    self.reorder_lists();
                }
                Action::Descending => {
                    self.state.playlist_sort = Sort::Descending;
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
                    let artists = match self
                        .state.queue
                        .get(self.state.current_playback_state.current_index as usize)
                    {
                        Some(song) => &song.artist_items,
                        None => return,
                    };
                    if artists.len() == 1 {
                        let artist = artists[0].clone();
                        self.reposition_cursor(&artist.id, Selectable::Artist);
                        self.close_popup();
                    } else {
                        self.popup.current_menu = Some(PopupMenu::ArtistJumpToCurrent {
                            artists: artists.clone(),
                        });
                        self.popup.selected.select(Some(0));
                    }
                }
                Action::ChangeFilter => {
                    self.popup.current_menu = Some(PopupMenu::ArtistsChangeFilter {});
                    self.popup.selected.select(Some(if self.state.artist_filter == Filter::Normal { 0 } else { 1 }));
                }
                Action::ChangeOrder => {
                    self.popup.current_menu = Some(PopupMenu::ArtistsChangeSort {});
                    self.popup.selected.select(Some(if self.state.artist_sort == Sort::Ascending { 0 } else { 1 }));
                }
                _ => {}
            },
            PopupMenu::ArtistJumpToCurrent { artists, .. } => if let Action::JumpToCurrent = action {
                let selected = match self.popup.selected.selected() {
                    Some(i) => i,
                    None => return,
                };
                let artist = &artists[selected];
                self.reposition_cursor(&artist.id, Selectable::Artist);
                self.close_popup();
            },
            PopupMenu::ArtistsChangeFilter {  } => match action {
                Action::Normal => {
                    self.state.artist_filter = Filter::Normal;
                    self.close_popup();
                    self.reorder_lists();
                }
                Action::ShowFavoritesFirst => {
                    self.state.artist_filter = Filter::FavoritesFirst;
                    self.close_popup();
                    self.reorder_lists();
                }
                _ => {}
            },
            PopupMenu::ArtistsChangeSort {  } => match action {
                Action::Ascending => {
                    self.state.artist_sort = Sort::Ascending;
                    self.close_popup();
                    self.reorder_lists();
                }
                Action::Descending => {
                    self.state.artist_sort = Sort::Descending;
                    self.close_popup();
                    self.reorder_lists();
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// Closes the popup including common state
    fn close_popup(&mut self) {
        self.popup.current_menu = None;
        self.popup.selected.select(None);
        self.state.active_section = self.state.last_section;
        self.popup.editing = false;
        self.popup.global = false;
    }

    /// Create popup based on the current selected tab and section
    ///
    pub fn create_popup(&mut self, frame: &mut Frame) -> Option<()> {
        if self.state.active_section != ActiveSection::Popup {
            return None;
        }

        if self.popup.global {
            if self.popup.current_menu.is_none() {
                self.popup.current_menu = Some(PopupMenu::GlobalRoot { large_art: self.state.large_art });
                self.popup.selected.select(Some(0));
            }
            self.render_popup(frame);
            return Some(());
        }

        match self.state.active_tab {
            ActiveTab::Library => match self.state.last_section {
                ActiveSection::Tracks => {
                    let id = self.get_id_of_selected(&self.tracks, Selectable::Track);
                    if self.popup.current_menu.is_none() {
                        self.popup.current_menu = Some(PopupMenu::TrackRoot {
                            track_name: self.tracks.iter().find(|t| t.id == id)?.name.clone(),
                            track_id: id,
                        });
                        self.popup.selected.select(Some(0));
                    }
                }
                ActiveSection::Artists => {
                    if self.popup.current_menu.is_none() {
                        let artists = self.get_id_of_selected(&self.artists, Selectable::Artist);
                        let artist = self.artists.iter().find(|a| a.id == artists)?.clone();
                        self.popup.current_menu = Some(PopupMenu::ArtistRoot {
                            artist: artist.clone(),
                            playing_artists: self
                                .state.queue
                                .get(self.state.current_playback_state.current_index as usize)
                                .map(|s| s.artist_items.clone()),
                        });
                        self.popup.selected.select(Some(0));
                    }
                }
                _ => {
                    self.close_popup();
                }
            },
            ActiveTab::Albums => match self.state.last_section {
                ActiveSection::Artists => {
                    if self.popup.current_menu.is_none() {
                        self.popup.current_menu = Some(PopupMenu::AlbumsRoot {});
                        self.popup.selected.select(Some(0));
                    }
                }
                _ => {
                    self.close_popup();
                }
            }, 
            ActiveTab::Playlists => match self.state.last_section {
                ActiveSection::Artists => {
                    if self.popup.current_menu.is_none() {
                        let id = self.get_id_of_selected(&self.playlists, Selectable::Playlist);
                        let playlist = self.playlists.iter().find(|p| p.id == id)?.clone();
                        self.popup.current_menu = Some(PopupMenu::PlaylistRoot {
                            playlist_name: playlist.name,
                        });
                        self.popup.selected.select(Some(0));
                    }
                }
                ActiveSection::Tracks => {
                    let id =
                        self.get_id_of_selected(&self.tracks_playlist, Selectable::PlaylistTrack);
                    if self.popup.current_menu.is_none() {
                        self.popup.current_menu = Some(PopupMenu::PlaylistTracksRoot {
                            track_name: self
                                .tracks_playlist
                                .iter()
                                .find(|t| t.id == id)?
                                .name
                                .clone(),
                        });
                        self.popup.selected.select(Some(0));
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
            let options = menu.options();

            let block = Block::bordered()
                .title(menu.title())
                .border_style(self.primary_color);

            let items = options
                .iter()
                .map(|action| ListItem::new(Span::styled(action.label.clone(), action.style)));

            let list = List::new(items)
                .block(block)
                .highlight_style(
                    Style::default()
                        .bg(if self.popup.editing {
                            style::Color::LightBlue
                        } else {
                            style::Color::White
                        })
                        .fg(style::Color::Black)
                        .bold(),
                )
                .style(Style::default().fg(style::Color::White))
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
