use std::str::FromStr;
use ratatui::prelude::Color;
use serde::{Deserialize, Serialize};

/*
TODO:
- allow settings any color to auto_color
- move auto_color toggle inside Theme struct?
- changing a theme doesnt update the UI unless manually refreshed (eg. switch tab) !!
*/


// A color that can either be a fixed color or "auto" (use primary color from cover art)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AutoColor {
    Fixed(Color),
    Auto,
}

impl AutoColor {
    pub fn resolve(&self, primary: Color) -> Color {
        match self {
            AutoColor::Fixed(c) => *c,
            AutoColor::Auto => primary,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub dark: bool,

    pub(crate) primary_color: Color, // primary color from cover art, used for "auto" colors

    // Colors !! When adding new colors, also update apply_overrides() !!
    pub(crate) background: Option<AutoColor>,
    pub(crate) foreground: AutoColor,
    pub(crate) foreground_dim: AutoColor,
    pub(crate) section_title: AutoColor,
    pub(crate) accent: AutoColor,
    pub(crate) border: AutoColor,
    pub(crate) selected_background: AutoColor,
    pub(crate) selected_foreground: AutoColor,
    pub(crate) selected_inactive_background: AutoColor,
    pub(crate) selected_inactive_foreground: AutoColor,
    pub(crate) scrollbar_thumb: AutoColor,
    pub(crate) scrollbar_track: AutoColor,
    pub(crate) progress_fill: AutoColor,
    pub(crate) progress_track: AutoColor,
    pub(crate) tab_active: AutoColor,
    pub(crate) tab_inactive: AutoColor,
    pub(crate) album_header_background: Option<AutoColor>,
    pub(crate) album_header_foreground: Option<AutoColor>,
}

impl Theme {
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

                let primary_color = if let Some(s) = theme_cfg.get("primary_color").and_then(|v| v.as_str()) {
                    Color::from_str(s).unwrap_or_else(|_| Color::Blue)
                } else {
                    Color::Blue
                };

                // you can either specify a base theme to override
                if let Some(base_name) = theme_cfg.get("base").and_then(|v| v.as_str()) {
                    if let Some(mut theme) = Theme::builtin_themes()
                        .into_iter()
                        .find(|t| t.name.eq_ignore_ascii_case(base_name))
                    {
                        theme.name = name;
                        theme.primary_color = primary_color;
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
                    theme.primary_color = primary_color;
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
        let set_color = |key: &str, out: &mut AutoColor| {
            if let Some(s) = overrides.get(key).and_then(|v| v.as_str()) {
                if s.eq_ignore_ascii_case("auto") {
                    *out = AutoColor::Auto;
                } else {
                    match Color::from_str(s) {
                        Ok(c) => *out = AutoColor::Fixed(c),
                        Err(_) => log::warn!("Invalid color for '{}': {}", key, s),
                    }
                }
            }
        };

        let set_opt_color = |key: &str, out: &mut Option<AutoColor>| {
            if let Some(s) = overrides.get(key).and_then(|v| v.as_str()) {
                if s.eq_ignore_ascii_case("none") {
                    *out = None;
                } else if s.eq_ignore_ascii_case("auto") {
                    *out = Some(AutoColor::Auto);
                } else {
                    match Color::from_str(s) {
                        Ok(c) => *out = Some(AutoColor::Fixed(c)),
                        Err(_) => log::warn!("Invalid color for '{}': {}", key, s),
                    }
                }
            }
        };

        // Apply overrides
        set_opt_color("background", &mut theme.background);

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

        set_opt_color("album_header_background", &mut theme.album_header_background);
        set_opt_color("album_header_foreground", &mut theme.album_header_foreground);
    }

    pub fn set_primary_color(&mut self, color: Color) {
        self.primary_color = color;
    }

    pub fn resolve(&self, c: &AutoColor) -> Color {
        c.resolve(self.primary_color)
    }

    pub fn resolve_opt(&self, c: &Option<AutoColor>) -> Option<Color> {
        c.as_ref().map(|a| a.resolve(self.primary_color))
    }

    // Default, opinionated dark theme
    pub fn dark() -> Self {
        Self {
            name: "Dark".to_string(),
            dark: true,
            primary_color: Color::Blue,

            background: None,
            foreground: AutoColor::Fixed(Color::White),
            foreground_dim: AutoColor::Fixed(Color::DarkGray),
            section_title: AutoColor::Fixed(Color::White),
            accent: AutoColor::Fixed(Color::Gray),
            border: AutoColor::Fixed(Color::Gray),

            selected_background: AutoColor::Fixed(Color::White),
            selected_foreground: AutoColor::Fixed(Color::Indexed(232)),
            selected_inactive_background: AutoColor::Fixed(Color::Indexed(236)),
            selected_inactive_foreground: AutoColor::Fixed(Color::White),

            scrollbar_thumb: AutoColor::Fixed(Color::Gray),
            scrollbar_track: AutoColor::Fixed(Color::DarkGray),

            progress_fill: AutoColor::Fixed(Color::White),
            progress_track: AutoColor::Fixed(Color::DarkGray),

            tab_active: AutoColor::Fixed(Color::White),
            tab_inactive: AutoColor::Fixed(Color::DarkGray),

            album_header_background: None,
            album_header_foreground: None,
        }
    }

    pub fn soft_dark() -> Self {
        Self {
            name: "Soft Dark".to_string(),
            dark: true,
            primary_color: Color::Blue,

            background: None,

            foreground: AutoColor::Fixed(Color::Rgb(230, 230, 230)),       // light gray (softer than pure white)
            foreground_dim: AutoColor::Fixed(Color::Rgb(140, 140, 140)),   // muted gray

            // strong but not too white
            section_title: AutoColor::Fixed(Color::Rgb(245, 245, 245)),

            // neutral mid-gray
            accent: AutoColor::Fixed(Color::Rgb(180, 180, 180)),

            // darker-ish mid-gray
            border: AutoColor::Fixed(Color::Rgb(100, 100, 100)),

            selected_background: AutoColor::Fixed(Color::Rgb(80, 80, 80)),   // medium gray highlight
            selected_foreground: AutoColor::Fixed(Color::Rgb(240, 240, 240)), // light text
            selected_inactive_background: AutoColor::Fixed(Color::Rgb(50, 50, 50)),  // darker gray
            selected_inactive_foreground: AutoColor::Fixed(Color::Rgb(200, 200, 200)), // dimmer text

            scrollbar_thumb: AutoColor::Fixed(Color::Rgb(160, 160, 160)),
            scrollbar_track: AutoColor::Fixed(Color::Rgb(70, 70, 70)),

            progress_fill: AutoColor::Fixed(Color::Rgb(230, 230, 230)),
            progress_track: AutoColor::Fixed(Color::Rgb(80, 80, 80)),

            tab_active: AutoColor::Fixed(Color::Rgb(240, 240, 240)), // bright text
            tab_inactive: AutoColor::Fixed(Color::Rgb(120, 120, 120)), // dimmer gray

            album_header_background: None,
            album_header_foreground: None,
        }
    }

    pub fn light() -> Self {
        Self {
            name: "Light".to_string(),
            dark: false,
            primary_color: Color::Blue,

            // warm but mostly neutral background
            background: Some(AutoColor::Fixed(Color::Rgb(246, 246, 244))), // #f6f6f4

            foreground: AutoColor::Fixed(Color::Rgb(30, 30, 30)),          // near black
            foreground_dim: AutoColor::Fixed(Color::Rgb(110, 110, 110)),   // muted gray

            section_title: AutoColor::Fixed(Color::Rgb(20, 20, 20)),

            accent: AutoColor::Fixed(Color::Rgb(100, 100, 100)),           // mid-gray

            border: AutoColor::Fixed(Color::Rgb(180, 180, 180)),

            selected_background: AutoColor::Fixed(Color::Rgb(210, 210, 210)), // light gray highlight
            selected_foreground: AutoColor::Fixed(Color::Rgb(20, 20, 20)),    // dark text
            selected_inactive_background: AutoColor::Fixed(Color::Rgb(235, 235, 235)), // very pale gray
            selected_inactive_foreground: AutoColor::Fixed(Color::Rgb(80, 80, 80)),    // mid-dark gray

            scrollbar_thumb: AutoColor::Fixed(Color::Rgb(120, 120, 120)),
            scrollbar_track: AutoColor::Fixed(Color::Rgb(220, 220, 220)),

            progress_fill: AutoColor::Fixed(Color::Rgb(60, 60, 60)),         // strong dark gray
            progress_track: AutoColor::Fixed(Color::Rgb(210, 210, 210)),

            tab_active: AutoColor::Fixed(Color::Rgb(25, 25, 25)),            // near black
            tab_inactive: AutoColor::Fixed(Color::Rgb(120, 120, 120)),       // mid gray

            album_header_background: None,
            album_header_foreground: None,
        }
    }

    pub fn gruvbox_dark() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(0x28, 0x28, 0x28)); // #282828
        let bg_soft = AutoColor::Fixed(Color::Rgb(0x32, 0x30, 0x2f)); // #32302f
        let bg_hl = AutoColor::Fixed(Color::Rgb(0x50, 0x49, 0x45)); // #504945
        let fg = AutoColor::Fixed(Color::Rgb(0xeb, 0xdb, 0xb2)); // #ebdbb2
        let fg_dim = AutoColor::Fixed(Color::Rgb(0xeb, 0xdb, 0xb2));
        let fg_dark = AutoColor::Fixed(Color::Rgb(0x3c, 0x38, 0x36)); // #3c3836

        let blue = AutoColor::Fixed(Color::Rgb(0x83, 0xa5, 0x98)); // #83a598
        let aqua = AutoColor::Fixed(Color::Rgb(0x8e, 0xc0, 0x7c)); // #8ec07c
        let border_col = AutoColor::Fixed(Color::Rgb(0x66, 0x5c, 0x54)); // #665c54

        Self {
            name: "Gruvbox Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(0x83, 0xa5, 0x98),
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
            album_header_background: None,
            album_header_foreground: None,
        }
    }

    pub fn gruvbox_light() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(0xfb, 0xf1, 0xc7)); // #fbf1c7
        let bg_soft = AutoColor::Fixed(Color::Rgb(0xf2, 0xe5, 0xbc)); // #f2e5bc
        let bg_hl = AutoColor::Fixed(Color::Rgb(0xeb, 0xdb, 0xb2)); // #ebdbb2
        let fg = AutoColor::Fixed(Color::Rgb(0x3c, 0x38, 0x36)); // #3c3836
        let fg_dim = AutoColor::Fixed(Color::Rgb(0x7c, 0x6f, 0x64)); // #7c6f64 (dimmer text)
        let fg_light = AutoColor::Fixed(Color::Rgb(0xeb, 0xdb, 0xb2)); // for tracks on light bg

        let blue = AutoColor::Fixed(Color::Rgb(0x45, 0x85, 0x88)); // #458588
        let aqua = AutoColor::Fixed(Color::Rgb(0x68, 0x9d, 0x6a)); // #689d6a
        let border_col = AutoColor::Fixed(Color::Rgb(0xbd, 0xae, 0x93)); // #bdae93

        Self {
            name: "Gruvbox Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(0x45, 0x85, 0x88),
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
            album_header_background: None,
            album_header_foreground: None,
        }
    }

    // A more neutral, less yellowy light theme
    pub fn gruvbox_light_neutral() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(0xf5, 0xf2, 0xeb)); // #f5f2eb (off-white, parchment)
        let bg_soft = AutoColor::Fixed(Color::Rgb(0xec, 0xe7, 0xdf)); // softer gray-beige
        let bg_hl = AutoColor::Fixed(Color::Rgb(0xe0, 0xdb, 0xd2)); // highlight, subtle
        let fg = AutoColor::Fixed(Color::Rgb(0x3c, 0x38, 0x36)); // #3c3836 dark text
        let fg_dim = AutoColor::Fixed(Color::Rgb(0x7c, 0x6f, 0x64)); // muted gray-brown
        let fg_light = AutoColor::Fixed(Color::Rgb(0xd0, 0xcb, 0xc2)); // for tracks, lighter neutral

        let blue = AutoColor::Fixed(Color::Rgb(0x45, 0x85, 0x88)); // #458588
        let aqua = AutoColor::Fixed(Color::Rgb(0x68, 0x9d, 0x6a)); // #689d6a
        let border_col = AutoColor::Fixed(Color::Rgb(0xbd, 0xae, 0x93)); // soft border

        Self {
            name: "Gruvbox Light Neutral".to_string(),
            dark: false,
            primary_color: Color::Rgb(0x45, 0x85, 0x88),
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
            album_header_background: None,
            album_header_foreground: None,
        }
    }

    // this theme uses the terminal's default colors
    pub fn terminal() -> Self {
        Self {
            name: "Terminal".to_string(),
            dark: false,
            primary_color: Color::Reset,
            background: None,
            foreground: AutoColor::Fixed(Color::Reset),
            foreground_dim: AutoColor::Fixed(Color::Reset),
            section_title: AutoColor::Fixed(Color::Reset),
            accent: AutoColor::Fixed(Color::Reset),
            border: AutoColor::Fixed(Color::Reset),
            selected_background: AutoColor::Fixed(Color::Reset),
            selected_foreground: AutoColor::Fixed(Color::Reset),
            selected_inactive_background: AutoColor::Fixed(Color::Reset),
            selected_inactive_foreground: AutoColor::Fixed(Color::Reset),
            scrollbar_thumb: AutoColor::Fixed(Color::Reset),
            scrollbar_track: AutoColor::Fixed(Color::Reset),
            progress_fill: AutoColor::Fixed(Color::Reset),
            progress_track: AutoColor::Fixed(Color::Reset),
            tab_active: AutoColor::Fixed(Color::Reset),
            tab_inactive: AutoColor::Fixed(Color::Reset),
            album_header_background: None,
            album_header_foreground: None,
        }
    }
}