use crate::themes::theme::Theme;
use ratatui::style::Color;

#[test]
fn buffered_sits_closer_to_the_track_than_the_fill() {
    let theme = Theme::dark();
    assert_eq!(theme.derived_buffered(Color::White), Some(Color::Rgb(144, 144, 144)));
}

#[test]
fn buffered_blends_towards_the_track_on_light_themes_too() {
    let theme = Theme::light();
    let Some(Color::Rgb(buffered, ..)) = theme.derived_buffered(Color::Rgb(60, 60, 60)) else {
        panic!("light theme colours should blend");
    };
    assert!(buffered > 60);
}

#[test]
fn colours_with_no_known_rgb_value_are_not_blended() {
    assert_eq!(Theme::dark().derived_buffered(Color::Blue), None);
}
