use std::str::FromStr;
use ratatui::prelude::Color;
use serde::{Deserialize, Serialize};

/*
TODO:
- allow settings any color to auto_color
- move auto_color toggle inside Theme struct?
- changing a theme doesnt update the UI unless manually refreshed (eg. switch tab) !!
*/

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub dark: bool,
    // Colors !! When adding new colors, also update apply_overrides() !!
    pub(crate) background: Option<Color>,
    pub(crate) foreground: Color,
    pub(crate) foreground_dim: Color,
    pub(crate) section_title: Color,
    pub(crate) accent: Color,
    pub(crate) border: Color,
    pub(crate) selected_background: Color,
    pub(crate) selected_foreground: Color,
    pub(crate) selected_inactive_background: Color,
    pub(crate) selected_inactive_foreground: Color,
    pub(crate) scrollbar_thumb: Color,
    pub(crate) scrollbar_track: Color,
    pub(crate) progress_fill: Color,
    pub(crate) progress_track: Color,
    pub(crate) tab_active: Color,
    pub(crate) tab_inactive: Color,
}

impl Theme {
    pub fn default() -> Self {
        Self::dark()
    }

    pub fn builtin_themes() -> Vec<Self> {
        vec![
            Self::dark(),
            Self::soft_dark(),
            Self::light(),
            Self::gruvbox_dark(),
            Self::gruvbox_light(),
            Self::gruvbox_light_neutral(),
            Self::terminal(),
        ]
    }

    // Load user-defined themes from the config. Array of themes, name is required.
    pub fn from_config(config: &serde_yaml::Value) -> Vec<Self> {
        let mut themes_vec = Vec::new();

        if let Some(themes) = config.get("themes").and_then(|v| v.as_sequence()) {
            for theme_cfg in themes {
                let name = theme_cfg
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unnamed Theme")
                    .to_string();

                // you can either specify a base theme to override
                if let Some(base_name) = theme_cfg.get("base").and_then(|v| v.as_str()) {
                    if let Some(mut theme) = Theme::builtin_themes()
                        .into_iter()
                        .find(|t| t.name.eq_ignore_ascii_case(base_name))
                    {
                        theme.name = name;
                        Theme::apply_overrides(&mut theme, theme_cfg);
                        themes_vec.push(theme);
                        continue;
                    } else {
                        log::warn!("Theme '{}' specified unknown base '{}', skipping.",name, base_name);
                        continue;
                    }
                }

                // or you can specify dark: true/false to start from a default theme
                if let Some(dark) = theme_cfg.get("dark").and_then(|v| v.as_bool()) {
                    let mut theme = if dark { Theme::dark() } else { Theme::light() };
                    theme.name = name;
                    Theme::apply_overrides(&mut theme, theme_cfg);
                    themes_vec.push(theme);
                } else {
                    log::warn!("Theme '{}' does not specify 'base' or 'dark' property, skipping.", name);
                }
            }
        }

        themes_vec
    }

    fn apply_overrides(theme: &mut Self, overrides: &serde_yaml::Value) {
        let set_color = |key: &str, out: &mut Color| {
            if let Some(s) = overrides.get(key).and_then(|v| v.as_str()) {
                match Color::from_str(s) {
                    Ok(c) => *out = c,
                    Err(_) => log::warn!("Invalid color for '{}': {}", key, s),
                }
            }
        };
        // this is separated as it's the only optional color
        if let Some(s) = overrides.get("background").and_then(|v| v.as_str()) {
            if s.eq_ignore_ascii_case("none") {
                theme.background = None;
            } else {
                match Color::from_str(s) {
                    Ok(c) => theme.background = Some(c),
                    Err(_) => log::warn!("Invalid color for 'background_color': {}", s),
                }
            }
        }

        // The remaining colors are just cleanly overridden
        set_color("foreground", &mut theme.foreground);
        set_color("foreground_dim", &mut theme.foreground_dim);
        set_color("section_title", &mut theme.section_title);
        set_color("accent", &mut theme.accent);
        set_color("border", &mut theme.border);
        set_color("selected_background", &mut theme.selected_background);
        set_color("selected_foreground", &mut theme.selected_foreground);
        set_color("selected_inactive_background", &mut theme.selected_inactive_background);
        set_color("selected_inactive_foreground", &mut theme.selected_inactive_foreground);
        set_color("scrollbar_thumb", &mut theme.scrollbar_thumb);
        set_color("scrollbar_track", &mut theme.scrollbar_track);
        set_color("progress_fill", &mut theme.progress_fill);
        set_color("progress_track", &mut theme.progress_track);
        set_color("tab_active", &mut theme.tab_active);
        set_color("tab_inactive", &mut theme.tab_inactive);
    }

    // Default, opinionated dark theme
    pub fn dark() -> Self {
        Self {
            name: "Dark".to_string(),
            dark: true,
            background: None,
            foreground: Color::White,
            foreground_dim: Color::DarkGray,
            section_title: Color::White,
            accent: Color::Gray,
            border: Color::Gray,

            selected_background: Color::White,
            selected_foreground: Color::Indexed(232),
            selected_inactive_background: Color::Indexed(236),
            selected_inactive_foreground: Color::White,

            scrollbar_thumb: Color::Gray,
            scrollbar_track: Color::DarkGray,

            progress_fill: Color::White,
            progress_track: Color::DarkGray,

            tab_active: Color::White,
            tab_inactive: Color::DarkGray,
        }
    }

    pub fn soft_dark() -> Self {
        Self {
            name: "Soft Dark".to_string(),
            dark: true,

            background: None,

            foreground: Color::Rgb(230, 230, 230),       // light gray (softer than pure white)
            foreground_dim: Color::Rgb(140, 140, 140),   // muted gray

            // strong but not too white
            section_title: Color::Rgb(245, 245, 245),

            // neutral mid-gray
            accent: Color::Rgb(180, 180, 180),

            // darker-ish mid-gray
            border: Color::Rgb(100, 100, 100),

            selected_background: Color::Rgb(80, 80, 80),   // medium gray highlight
            selected_foreground: Color::Rgb(240, 240, 240), // light text
            selected_inactive_background: Color::Rgb(50, 50, 50),  // darker gray
            selected_inactive_foreground: Color::Rgb(200, 200, 200), // dimmer text

            scrollbar_thumb: Color::Rgb(160, 160, 160),
            scrollbar_track: Color::Rgb(70, 70, 70),

            progress_fill: Color::Rgb(230, 230, 230),
            progress_track: Color::Rgb(80, 80, 80),

            tab_active: Color::Rgb(240, 240, 240), // bright text
            tab_inactive: Color::Rgb(120, 120, 120), // dimmer gray
        }
    }


    pub fn light() -> Self {
        Self {
            name: "Light".to_string(),
            dark: false,

            // warm but mostly neutral background
            background: Some(Color::Rgb(246, 246, 244)), // #f6f6f4

            foreground: Color::Rgb(30, 30, 30),          // near black
            foreground_dim: Color::Rgb(110, 110, 110),   // muted gray

            section_title: Color::Rgb(20, 20, 20),

            accent: Color::Rgb(100, 100, 100),           // mid-gray

            border: Color::Rgb(180, 180, 180),

            selected_background: Color::Rgb(210, 210, 210), // light gray highlight
            selected_foreground: Color::Rgb(20, 20, 20),    // dark text
            selected_inactive_background: Color::Rgb(235, 235, 235), // very pale gray
            selected_inactive_foreground: Color::Rgb(80, 80, 80),    // mid-dark gray

            scrollbar_thumb: Color::Rgb(120, 120, 120),
            scrollbar_track: Color::Rgb(220, 220, 220),

            progress_fill: Color::Rgb(60, 60, 60),         // strong dark gray
            progress_track: Color::Rgb(210, 210, 210),

            tab_active: Color::Rgb(25, 25, 25),            // near black
            tab_inactive: Color::Rgb(120, 120, 120),       // mid gray
        }
    }
    //

    pub fn gruvbox_dark() -> Self {
        let bg         = Color::Rgb(0x28, 0x28, 0x28); // #282828
        let bg_soft    = Color::Rgb(0x32, 0x30, 0x2f); // #32302f
        let bg_hl      = Color::Rgb(0x50, 0x49, 0x45); // #504945
        let fg         = Color::Rgb(0xeb, 0xdb, 0xb2); // #ebdbb2
        let fg_dim     = Color::Rgb(0xeb, 0xdb, 0xb2);
        let fg_dark    = Color::Rgb(0x3c, 0x38, 0x36); // #3c3836

        let blue       = Color::Rgb(0x83, 0xa5, 0x98); // #83a598
        let aqua       = Color::Rgb(0x8e, 0xc0, 0x7c); // #8ec07c
        let border_col = Color::Rgb(0x66, 0x5c, 0x54); // #665c54

        Self {
            name: "Gruvbox Dark".to_string(),
            dark: true,
            background: Some(bg),
            foreground: fg,
            foreground_dim: fg_dim,
            section_title: fg,
            accent: blue,
            border: border_col,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: aqua,
            progress_track: fg_dark,
            tab_active: fg,
            tab_inactive: fg_dim,
        }
    }

    pub fn gruvbox_light() -> Self {
        let bg         = Color::Rgb(0xfb, 0xf1, 0xc7); // #fbf1c7
        let bg_soft    = Color::Rgb(0xf2, 0xe5, 0xbc); // #f2e5bc
        let bg_hl      = Color::Rgb(0xeb, 0xdb, 0xb2); // #ebdbb2
        let fg         = Color::Rgb(0x3c, 0x38, 0x36); // #3c3836
        let fg_dim     = Color::Rgb(0x7c, 0x6f, 0x64); // #7c6f64 (dimmer text)
        let fg_light   = Color::Rgb(0xeb, 0xdb, 0xb2); // for tracks on light bg

        let blue       = Color::Rgb(0x45, 0x85, 0x88); // #458588
        let aqua       = Color::Rgb(0x68, 0x9d, 0x6a); // #689d6a
        let border_col = Color::Rgb(0xbd, 0xae, 0x93); // #bdae93

        Self {
            name: "Gruvbox Light".to_string(),
            dark: false,
            background: Some(bg),
            foreground: fg,
            foreground_dim: fg_dim,
            section_title: fg,
            accent: blue,
            border: border_col,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: aqua,
            progress_track: fg_light,
            tab_active: fg,
            tab_inactive: fg_dim,
        }
    }

    // A more neutral, less yellowy light theme
    pub fn gruvbox_light_neutral() -> Self {
        let bg         = Color::Rgb(0xf5, 0xf2, 0xeb); // #f5f2eb (off-white, parchment)
        let bg_soft    = Color::Rgb(0xec, 0xe7, 0xdf); // softer gray-beige
        let bg_hl      = Color::Rgb(0xe0, 0xdb, 0xd2); // highlight, subtle
        let fg         = Color::Rgb(0x3c, 0x38, 0x36); // #3c3836 dark text
        let fg_dim     = Color::Rgb(0x7c, 0x6f, 0x64); // muted gray-brown
        let fg_light   = Color::Rgb(0xd0, 0xcb, 0xc2); // for tracks, lighter neutral

        let blue       = Color::Rgb(0x45, 0x85, 0x88); // #458588
        let aqua       = Color::Rgb(0x68, 0x9d, 0x6a); // #689d6a
        let border_col = Color::Rgb(0xbd, 0xae, 0x93); // soft border

        Self {
            name: "Gruvbox Light Neutral".to_string(),
            dark: false,
            background: Some(bg),
            foreground: fg,
            foreground_dim: fg_dim,
            section_title: fg,
            accent: blue,
            border: border_col,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: aqua,
            progress_track: fg_light,
            tab_active: fg,
            tab_inactive: fg_dim,
        }
    }

    // this theme uses the terminal's default colors
    pub fn terminal() -> Self {
        Self {
            name: "Terminal".to_string(),
            dark: false,
            background: None,
            foreground: Color::Reset,
            foreground_dim: Color::Reset,
            section_title: Color::Reset,
            accent: Color::Reset,
            border: Color::Reset,
            selected_background: Color::Reset,
            selected_foreground: Color::Reset,
            selected_inactive_background: Color::Reset,
            selected_inactive_foreground: Color::Reset,
            scrollbar_thumb: Color::Reset,
            scrollbar_track: Color::Reset,
            progress_fill: Color::Reset,
            progress_track: Color::Reset,
            tab_active: Color::Reset,
            tab_inactive: Color::Reset,
        }
    }
}
