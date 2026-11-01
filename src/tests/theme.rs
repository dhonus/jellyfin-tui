use crate::themes::theme::Theme;

#[test]
fn every_builtin_theme_sets_its_buffered_colour() {
    for theme in Theme::builtin_themes() {
        assert!(theme.progress_buffered.is_some(), "{}", theme.name);
    }
}
