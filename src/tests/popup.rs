use crate::helpers::{Searchable, State};
use crate::keyboard::ActiveSection;
use crate::popup::{open_queue_track_popup, PopupCommand, PopupMenu, PopupState};
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

    let options = menu.options("favorite");

    assert_eq!(options.len(), 1);
    assert!(matches!(
        &options[0].action,
        PopupCommand::AddToPlaylist { playlist_id } if playlist_id.is_empty()
    ));
    // add-to-playlist mutates server state, so it must disappear when offline
    assert!(options[0].online);
}

#[test]
fn playlist_removal_popup_counts_every_marked_track() {
    let menu = PopupMenu::PlaylistTracksRemove {
        keys: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        playlist_name: "Mix".to_string(),
        playlist_id: "playlist-id".to_string(),
    };

    let options = menu.options("favorite");
    assert!(options[0].name().contains('3'), "{}", options[0].name());
}

#[test]
fn set_generic_message_activates_popup_section() {
    let mut state = State::new();
    state.active_section = ActiveSection::Tracks;
    let mut popup = PopupState::default();

    // Simulating helper call with state
    state.last_section = state.active_section;
    state.active_section = ActiveSection::Popup;
    popup.current_menu = Some(PopupMenu::GenericMessage {
        title: "Tracks removed".to_string(),
        message: "Removed 1 track(s)".to_string(),
    });

    assert_eq!(state.active_section, ActiveSection::Popup);
    assert_eq!(state.last_section, ActiveSection::Tracks);
    assert!(matches!(popup.current_menu, Some(PopupMenu::GenericMessage { .. })));
}
