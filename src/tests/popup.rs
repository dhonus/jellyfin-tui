use crate::client::Playlist;
use crate::helpers::{search_ranked_refs, Searchable, State, Symbols};
use crate::keyboard::ActiveSection;
use crate::popup::{
    filter_options, is_track_root, open_queue_track_popup, section_row, selectable_row,
    PopupAction, PopupCommand, PopupMenu, PopupState, HEADING, MULTI, NONE,
};
use crate::themes::theme::Theme;
use crate::tui::Song;

#[test]
fn queue_popup_focuses_the_selected_track() {
    let mut state = State::new();
    state.active_section = ActiveSection::Queue;
    state.queue = vec![
        Song { id: "first-id".to_string(), name: "First track".to_string(), ..Default::default() },
        Song {
            id: "selected-id".to_string(),
            name: "Selected track".to_string(),
            ..Default::default()
        },
    ];
    state.selected_queue_item.select(Some(1));
    let mut popup = PopupState::default();

    assert!(open_queue_track_popup(&mut state, &mut popup));
    assert_eq!(state.last_section, ActiveSection::Queue);
    assert_eq!(state.active_section, ActiveSection::Popup);

    match popup.current_menu {
        Some(PopupMenu::QueueTrackRoot { track_name, track_id }) => {
            assert_eq!(track_name, "Selected track");
            assert_eq!(track_id, "selected-id");
        }
        _ => panic!("expected the selected queue track popup"),
    }
}

#[test]
fn queue_popup_without_a_selection_keeps_queue_focus() {
    let mut state = State::new();
    state.active_section = ActiveSection::Queue;
    state.queue =
        vec![Song { id: "track-id".to_string(), name: "Track".to_string(), ..Default::default() }];
    let mut popup = PopupState::default();

    assert!(!open_queue_track_popup(&mut state, &mut popup));
    assert_eq!(state.active_section, ActiveSection::Queue);
    assert!(popup.current_menu.is_none());
}

#[test]
fn queue_track_popup_offers_add_to_playlist() {
    let menu = PopupMenu::QueueTrackRoot {
        track_name: "Track".to_string(),
        track_id: "track-id".to_string(),
    };

    let options = menu.options(&Symbols::default());

    let add = options
        .iter()
        .find(|o| matches!(&o.action, PopupCommand::AddToPlaylist { playlist_id } if playlist_id.is_empty()))
        .expect("queue popup should offer add-to-playlist");
    // add-to-playlist mutates server state, so it must disappear when offline
    assert!(add.has(crate::popup::ONLINE));

    // the two jumps are local-only, so they stay available offline
    for command in [PopupCommand::JumpToCurrent, PopupCommand::LocateSelected] {
        let jump = options
            .iter()
            .find(|o| std::mem::discriminant(&o.action) == std::mem::discriminant(&command))
            .unwrap_or_else(|| panic!("queue popup should offer {:?}", command));
        assert!(!jump.has(crate::popup::ONLINE));
    }
}

#[test]
fn playlist_removal_popup_counts_every_marked_track() {
    let menu = PopupMenu::PlaylistTracksRemove {
        keys: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        playlist_name: "Mix".to_string(),
        playlist_id: "playlist-id".to_string(),
    };

    let options = menu.options(&Symbols::default());
    assert!(options[0].name().contains('3'), "{}", options[0].name());
}

/// Select mode filters a root menu down to its multi-capable actions, so a root with none would
/// open a popup with nothing in it. Every root has to offer at least one.
#[test]
fn every_root_menu_offers_a_multi_action() {
    let symbols = Symbols::default();
    let roots = [
        (
            "TrackRoot",
            PopupMenu::TrackRoot {
                track: Default::default(),
                transcoding: false,
                now_playing_name: None,
            },
        ),
        (
            "AlbumTrackRoot",
            PopupMenu::AlbumTrackRoot {
                track_id: "id".to_string(),
                track_name: "name".to_string(),
                disliked: false,
                transcoding: false,
                now_playing_name: None,
            },
        ),
        (
            "PlaylistTracksRoot",
            PopupMenu::PlaylistTracksRoot { track: Default::default(), transcoding: false },
        ),
    ];

    for (name, menu) in roots {
        assert!(is_track_root(&menu), "{name} should count as a root");
        assert!(
            menu.options(&symbols).iter().any(|o| o.has(MULTI)),
            "{name} would filter to nothing in select mode"
        );
    }
}

/// Menus below a root list targets rather than actions, so their entries carry no MULTI and the
/// root filter must not reach them - it would leave the picker empty.
#[test]
fn menus_below_a_root_are_not_filtered_by_multi() {
    let symbols = Symbols::default();
    let picker = PopupMenu::TracksAddToPlaylist {
        track_ids: vec!["a".to_string(), "b".to_string()],
        playlists: vec![Playlist {
            id: "p".to_string(),
            name: "P".to_string(),
            ..Default::default()
        }],
    };

    assert!(!is_track_root(&picker));
    assert!(!picker.options(&symbols).is_empty());
    assert!(picker.options(&symbols).iter().all(|o| !o.has(MULTI)));
}

/// The regression: the root filter must not reach the menus below a root. It once did, so opening
/// the playlist picker from a selection emptied it, and the empty-menu guard closed the popup.
#[test]
fn a_live_selection_does_not_empty_the_playlist_picker() {
    let symbols = Symbols::default();
    let picker = PopupMenu::TracksAddToPlaylist {
        track_ids: vec!["a".to_string(), "b".to_string()],
        playlists: vec![Playlist {
            id: "p".to_string(),
            name: "P".to_string(),
            ..Default::default()
        }],
    };

    let shown = filter_options(picker.options(&symbols), &picker, false, true);
    assert!(!shown.is_empty(), "the picker was filtered to nothing while selecting");
}

/// The other half: a root *is* filtered, down to exactly the multi-capable actions.
#[test]
fn a_live_selection_filters_a_root_to_its_multi_actions() {
    let symbols = Symbols::default();
    let root = PopupMenu::TrackRoot {
        track: Default::default(),
        transcoding: false,
        now_playing_name: None,
    };

    let shown = filter_options(root.options(&symbols), &root, false, true);
    assert!(!shown.is_empty());
    assert!(shown.iter().all(|o| o.has(MULTI)));
    assert!(shown.len() < root.options(&symbols).len(), "nothing was filtered out");
}

fn rows() -> Vec<PopupAction> {
    PopupMenu::GlobalPickTheme {}.options(&Symbols::default())
}

#[test]
fn themes_sit_under_the_heading_matching_their_brightness() {
    let mut dark_section = None;
    for row in rows() {
        match &row.action {
            PopupCommand::SetTheme { theme } => {
                assert_eq!(Some(theme.dark), dark_section, "{}", theme.name)
            }
            _ if row.label == "Dark" => dark_section = Some(true),
            _ if row.label == "Light" => dark_section = Some(false),
            _ => {}
        }
    }
}

#[test]
fn every_builtin_theme_is_listed_once() {
    let listed =
        rows().iter().filter(|r| matches!(r.action, PopupCommand::SetTheme { .. })).count();
    assert_eq!(listed, Theme::builtin_themes().len());
}

#[test]
fn cursor_skips_headings_and_stops_at_the_ends() {
    let rows = rows();
    let last = rows.len() - 1;
    let first = selectable_row(&rows, 0, true).unwrap();
    assert_eq!(first, 1);
    assert_eq!(selectable_row(&rows, first - 1, false), None);
    assert_eq!(selectable_row(&rows, usize::MAX, false), Some(last));
    assert_eq!(selectable_row(&rows, last + 1, true), None);

    let light = rows.iter().position(|r| r.label == "Light").unwrap();
    assert_eq!(selectable_row(&rows, light - 1, true), Some(light + 1));
    assert_eq!(selectable_row(&rows, light, false), Some(light - 2));
}

#[test]
fn only_the_named_theme_counts_as_applied() {
    let applied: Vec<_> = rows().into_iter().filter(|r| r.applies_theme("Gruvbox Light")).collect();
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].label.trim(), "Gruvbox Light");
}

#[test]
fn jumping_toggles_between_dark_and_light() {
    let rows = rows();
    let dark = 1;
    let light = rows.iter().position(|r| r.label == "Light").unwrap() + 1;
    let custom = rows.len() - 1;

    assert_eq!(section_row(&rows, dark, true), Some(light));
    assert_eq!(section_row(&rows, dark + 3, true), Some(light));
    assert_eq!(section_row(&rows, light, true), Some(dark));
    assert_eq!(section_row(&rows, dark, false), Some(light));
    assert_eq!(section_row(&rows, light, false), Some(dark));
    assert_eq!(section_row(&rows, light + 2, false), Some(light));
    assert_eq!(section_row(&rows, custom, true), Some(dark));
}

#[test]
fn jumping_does_nothing_without_sections() {
    let plain = vec![PopupAction::new("Only", PopupCommand::None, NONE)];
    assert_eq!(section_row(&plain, 0, true), None);
    assert_eq!(section_row(&[], 0, false), None);
}

#[test]
fn no_rows_means_no_selection() {
    assert_eq!(selectable_row(&[], 0, true), None);
    assert_eq!(selectable_row(&[], usize::MAX, false), None);
}

#[test]
fn search_finds_themes_but_not_headings() {
    let rows = rows();
    let found = search_ranked_refs(&rows, "dark", true);
    assert!(!found.is_empty());
    assert!(found.iter().all(|r| !r.has(HEADING)));
}
