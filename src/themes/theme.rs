use std::str::FromStr;
use ratatui::prelude::Color;
use serde::{Deserialize, Serialize};

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
    pub(crate) foreground_secondary: AutoColor,
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
    pub(crate) album_header_foreground: AutoColor,
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
            Self::nord_dark(),          Self::nord_light(),
            Self::solarized_dark(),     Self::solarized_light(),
            Self::catppuccin_mocha(),   Self::catppuccin_latte(),
            Self::tokyonight(),         Self::tokyonight_light(),
            Self::one_dark(),           Self::one_light(),
            Self::everforest_dark(),    Self::everforest_light(),
            Self::monokai_dark(),       Self::monokai_light(),
            Self::dracula(),            Self::dracula_light(),
            Self::ayu_dark(),           Self::ayu_light(),
            Self::kanagawa_wave(),      Self::kanagawa_lotus(),
            Self::night_owl(),          Self::day_owl(),
            Self::github_dark(),        Self::github_light(),
            Self::material_palenight(), Self::material_light(),
            Self::papercolor_dark(),    Self::papercolor_light(),
            Self::monochrome_dark(),    Self::monochrome_light(),
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
                    match Color::from_str(s) {
                        Ok(c) => c,
                        Err(_) => {
                            log::warn!("Invalid primary_color '{}', falling back", s);
                            Color::Reset
                        }
                    }
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
        set_color("album_header_foreground", &mut theme.album_header_foreground);

        set_opt_color("album_header_background", &mut theme.album_header_background);
    }

    pub fn set_primary_color(&mut self, color: Color, auto_color: bool) {
        if !auto_color {
            return;
        }
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
            foreground_secondary: AutoColor::Fixed(Color::Gray),
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
            album_header_foreground: AutoColor::Fixed(Color::White),
        }
    }

    pub fn soft_dark() -> Self {
        Self {
            name: "Soft Dark".to_string(),
            dark: true,
            primary_color: Color::Blue,

            background: None,

            foreground: AutoColor::Fixed(Color::Rgb(230, 230, 230)),       // light gray (softer than pure white)
            foreground_secondary: AutoColor::Fixed(Color::Rgb(185, 185, 185)),
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
            album_header_foreground: AutoColor::Fixed(Color::Rgb(230, 230, 230)), // light text
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
            foreground_secondary: AutoColor::Fixed(Color::Rgb(70, 70, 70)),
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
            album_header_foreground: AutoColor::Fixed(Color::Rgb(30, 30, 30)), // dark text
        }
    }

    pub fn gruvbox_dark() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(0x28, 0x28, 0x28)); // #282828
        let bg_soft = AutoColor::Fixed(Color::Rgb(0x32, 0x30, 0x2f)); // #32302f
        let bg_hl = AutoColor::Fixed(Color::Rgb(0x50, 0x49, 0x45)); // #504945
        let fg = AutoColor::Fixed(Color::Rgb(0xeb, 0xdb, 0xb2)); // #ebdbb2
        let fg_sec = AutoColor::Fixed(Color::Rgb(219, 209, 180));
        let fg_dim = AutoColor::Fixed(Color::Rgb(0xeb, 0xdb, 0xb2));
        let fg_dark = AutoColor::Fixed(Color::Rgb(0x3c, 0x38, 0x36)); // #3c3836

        let blue = AutoColor::Fixed(Color::Rgb(0x83, 0xa5, 0x98)); // #83a598
        let border_col = AutoColor::Fixed(Color::Rgb(0x66, 0x5c, 0x54)); // #665c54

        Self {
            name: "Gruvbox Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(0x83, 0xa5, 0x98),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
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
            progress_fill: fg,
            progress_track: fg_dark,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: None,
            album_header_foreground: fg,
        }
    }

    pub fn gruvbox_light() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(0xfb, 0xf1, 0xc7)); // #fbf1c7
        let bg_soft = AutoColor::Fixed(Color::Rgb(0xf2, 0xe5, 0xbc)); // #f2e5bc
        let bg_hl = AutoColor::Fixed(Color::Rgb(0xeb, 0xdb, 0xb2)); // #ebdbb2
        let fg = AutoColor::Fixed(Color::Rgb(0x3c, 0x38, 0x36)); // #3c3836
        let fg_sec = AutoColor::Fixed(Color::Rgb(120, 110, 100));
        let fg_dim = AutoColor::Fixed(Color::Rgb(0x7c, 0x6f, 0x64)); // #7c6f64 (dimmer text)
        let fg_light = AutoColor::Fixed(Color::Rgb(0xeb, 0xdb, 0xb2)); // for tracks on light bg

        let blue = AutoColor::Fixed(Color::Rgb(0x45, 0x85, 0x88)); // #458588
        let border_col = AutoColor::Fixed(Color::Rgb(0xbd, 0xae, 0x93)); // #bdae93

        Self {
            name: "Gruvbox Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(0x45, 0x85, 0x88),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
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
            progress_fill: fg,
            progress_track: fg_light,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: None,
            album_header_foreground: fg,
        }
    }

    // A more neutral, less yellowy light theme
    pub fn gruvbox_light_neutral() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(0xf5, 0xf2, 0xeb)); // #f5f2eb (off-white, parchment)
        let bg_soft = AutoColor::Fixed(Color::Rgb(0xec, 0xe7, 0xdf)); // softer gray-beige
        let bg_hl = AutoColor::Fixed(Color::Rgb(0xe0, 0xdb, 0xd2)); // highlight, subtle
        let fg = AutoColor::Fixed(Color::Rgb(0x3c, 0x38, 0x36)); // #3c3836 dark text
        let fg_sec = AutoColor::Fixed(Color::Rgb(120, 110, 100));
        let fg_dim = AutoColor::Fixed(Color::Rgb(0x7c, 0x6f, 0x64)); // muted gray-brown
        let fg_light = AutoColor::Fixed(Color::Rgb(0xd0, 0xcb, 0xc2)); // for tracks, lighter neutral

        let blue = AutoColor::Fixed(Color::Rgb(0x45, 0x85, 0x88)); // #458588
        let border_col = AutoColor::Fixed(Color::Rgb(0xbd, 0xae, 0x93)); // soft border

        Self {
            name: "Gruvbox Light Neutral".to_string(),
            dark: false,
            primary_color: Color::Rgb(0x45, 0x85, 0x88),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
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
            progress_fill: fg,
            progress_track: fg_light,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: None,
            album_header_foreground: fg,
        }
    }

    pub fn nord_dark() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(46, 52, 64));   // #2E3440
        let bg_soft = AutoColor::Fixed(Color::Rgb(59, 66, 82));   // #3B4252
        let bg_hl   = AutoColor::Fixed(Color::Rgb(67, 76, 94));   // #434C5E
        let fg      = AutoColor::Fixed(Color::Rgb(216, 222, 233));// #D8DEE9
        let fg_sec = AutoColor::Fixed(Color::Rgb(190, 200, 210));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(167, 177, 194));// ~#A7B1C2
        let border  = AutoColor::Fixed(Color::Rgb(76, 86, 106));  // #4C566A
        let accent  = AutoColor::Fixed(Color::Rgb(136, 192, 208));// #88C0D0

        Self {
            name: "Nord Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(136, 192, 208),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: border,

            tab_active: fg,
            tab_inactive: fg_dim,

            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn nord_light() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(236, 239, 244)); // #ECEFF4
        let bg_soft = AutoColor::Fixed(Color::Rgb(229, 233, 240)); // #E5E9F0
        let bg_hl   = AutoColor::Fixed(Color::Rgb(216, 222, 233)); // #D8DEE9
        let fg      = AutoColor::Fixed(Color::Rgb(46, 52, 64));    // #2E3440
        let fg_sec = AutoColor::Fixed(Color::Rgb(60, 68, 85));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(76, 86, 106));   // #4C566A
        let border  = AutoColor::Fixed(Color::Rgb(216, 222, 233)); // #D8DEE9
        let accent  = AutoColor::Fixed(Color::Rgb(94, 129, 172));  // #5E81AC

        Self {
            name: "Nord Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(94, 129, 172),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_hl,

            tab_active: fg,
            tab_inactive: fg_dim,

            // lighter header than selection
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    // ------------------- SOLARIZED -------------------

    pub fn solarized_dark() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(0, 43, 54));      // base03
        let bg_soft = AutoColor::Fixed(Color::Rgb(7, 54, 66));      // base02
        let bg_hl   = AutoColor::Fixed(Color::Rgb(88, 110, 117));   // base01
        let fg      = AutoColor::Fixed(Color::Rgb(147, 161, 161));  // base1
        let fg_sec = AutoColor::Fixed(Color::Rgb(140, 154, 154));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(131, 148, 150));  // base0
        let border  = AutoColor::Fixed(Color::Rgb(88, 110, 117));   // base01
        let accent  = AutoColor::Fixed(Color::Rgb(38, 139, 210));   // blue
        let altfg   = AutoColor::Fixed(Color::Rgb(238, 232, 213));  // base2

        Self {
            name: "Solarized Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(38, 139, 210),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,

            selected_background: bg_hl,
            selected_foreground: altfg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: border,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_soft,

            tab_active: fg,
            tab_inactive: fg_dim,

            album_header_background: Some(bg_soft),
            album_header_foreground: altfg,
        }
    }

    pub fn solarized_light() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(253, 246, 227));  // base3
        let bg_soft = AutoColor::Fixed(Color::Rgb(238, 232, 213));  // base2
        let bg_hl   = AutoColor::Fixed(Color::Rgb(238, 232, 213));  // same/darker than bg_soft
        let fg      = AutoColor::Fixed(Color::Rgb(101, 123, 131));  // base00
        let fg_sec = AutoColor::Fixed(Color::Rgb(120, 140, 146));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(147, 161, 161));  // base1
        let border  = AutoColor::Fixed(Color::Rgb(238, 232, 213));  // base2
        let accent  = AutoColor::Fixed(Color::Rgb(38, 139, 210));   // blue

        Self {
            name: "Solarized Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(38, 139, 210),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,

            // darker selection on light
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_hl,

            tab_active: fg,
            tab_inactive: fg_dim,

            // header lighter than selection
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    // ----------------- CATPPUCCIN --------------------

    pub fn catppuccin_mocha() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(30, 30, 46));     // #1e1e2e
        let bg_soft = AutoColor::Fixed(Color::Rgb(49, 50, 68));     // #313244
        let bg_hl   = AutoColor::Fixed(Color::Rgb(69, 71, 90));     // #45475a
        let fg      = AutoColor::Fixed(Color::Rgb(205, 214, 244));  // #cdd6f4
        let fg_sec = AutoColor::Fixed(Color::Rgb(185, 194, 222));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(166, 173, 200));  // #a6adc8
        let border  = AutoColor::Fixed(Color::Rgb(69, 71, 90));     // #45475a
        let accent  = AutoColor::Fixed(Color::Rgb(137, 180, 250));  // #89b4fa

        Self {
            name: "Catppuccin Mocha".to_string(),
            dark: true,
            primary_color: Color::Rgb(137, 180, 250),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_soft,

            tab_active: fg,
            tab_inactive: fg_dim,

            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn catppuccin_latte() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(239, 241, 245));  // #eff1f5
        let bg_soft = AutoColor::Fixed(Color::Rgb(230, 233, 239));  // #e6e9ef
        let bg_hl   = AutoColor::Fixed(Color::Rgb(204, 208, 218));  // #ccd0da
        let fg      = AutoColor::Fixed(Color::Rgb(76, 79, 105));    // #4c4f69
        let fg_sec = AutoColor::Fixed(Color::Rgb(92, 95, 118));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(108, 111, 133));  // #6c6f85
        let border  = AutoColor::Fixed(Color::Rgb(204, 208, 218));  // #ccd0da
        let accent  = AutoColor::Fixed(Color::Rgb(30, 102, 245));   // #1e66f5

        Self {
            name: "Catppuccin Latte".to_string(),
            dark: false,
            primary_color: Color::Rgb(30, 102, 245),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_hl,

            tab_active: fg,
            tab_inactive: fg_dim,

            album_header_background: Some(bg_soft), // lighter than selection
            album_header_foreground: fg,
        }
    }

    // ---------------- TOKYO NIGHT --------------------

    pub fn tokyonight() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(26, 27, 38));     // #1a1b26
        let bg_soft = AutoColor::Fixed(Color::Rgb(36, 40, 59));     // #24283b
        let bg_hl   = AutoColor::Fixed(Color::Rgb(40, 52, 87));     // #283457
        let fg      = AutoColor::Fixed(Color::Rgb(192, 202, 245));  // #c0caf5
        let fg_sec = AutoColor::Fixed(Color::Rgb(180, 190, 228));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(169, 177, 214));  // #a9b1d6
        let border  = AutoColor::Fixed(Color::Rgb(59, 66, 97));     // #3b4261
        let accent  = AutoColor::Fixed(Color::Rgb(122, 162, 247));  // #7aa2f7

        Self {
            name: "Tokyo Night".to_string(),
            dark: true,
            primary_color: Color::Rgb(122, 162, 247),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: border,

            tab_active: fg,
            tab_inactive: fg_dim,

            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn tokyonight_light() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(225, 226, 231));  // ~tokyo day
        let bg_soft = AutoColor::Fixed(Color::Rgb(213, 214, 219));
        let bg_hl   = AutoColor::Fixed(Color::Rgb(205, 213, 240));  // faint blue highlight
        let fg      = AutoColor::Fixed(Color::Rgb(31, 35, 53));     // deep slate
        let fg_sec  = AutoColor::Fixed(Color::Rgb(70, 80, 100));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(91, 96, 120));    // dimmer slate
        let border  = AutoColor::Fixed(Color::Rgb(192, 195, 215));
        let accent  = AutoColor::Fixed(Color::Rgb(46, 125, 233));   // vivid blue

        Self {
            name: "Tokyo Night Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(46, 125, 233),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_hl,

            tab_active: fg,
            tab_inactive: fg_dim,

            album_header_background: Some(bg_soft), // lighter header
            album_header_foreground: fg,
        }
    }

    // -------------------- ONE -----------------------

    pub fn one_dark() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(40, 44, 52));     // #282C34
        let bg_soft = AutoColor::Fixed(Color::Rgb(44, 49, 60));     // #2C313C
        let bg_hl   = AutoColor::Fixed(Color::Rgb(62, 68, 81));     // #3E4451
        let fg      = AutoColor::Fixed(Color::Rgb(171, 178, 191));  // #ABB2BF
        let fg_sec = AutoColor::Fixed(Color::Rgb(150, 155, 168));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(127, 132, 142));  // dim text
        let border  = bg_hl;
        let accent  = AutoColor::Fixed(Color::Rgb(97, 175, 239));   // #61AFEF

        Self {
            name: "One Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(97, 175, 239),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_hl,

            tab_active: fg,
            tab_inactive: fg_dim,

            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn one_light() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(250, 250, 250));  // #FAFAFA
        let bg_soft = AutoColor::Fixed(Color::Rgb(240, 240, 241));  // soft gray
        let bg_hl   = AutoColor::Fixed(Color::Rgb(229, 229, 230));  // border gray
        let fg      = AutoColor::Fixed(Color::Rgb(56, 58, 66));     // #383A42
        let fg_sec = AutoColor::Fixed(Color::Rgb(90, 95, 105));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(106, 115, 125));  // #6A737D
        let border  = bg_hl;
        let accent  = AutoColor::Fixed(Color::Rgb(64, 120, 242));   // #4078F2

        Self {
            name: "One Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(64, 120, 242),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_hl,

            tab_active: fg,
            tab_inactive: fg_dim,

            album_header_background: Some(bg_soft), // lighter header
            album_header_foreground: fg,
        }
    }

    // ----------------- EVERFOREST -------------------

    pub fn everforest_dark() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(45, 53, 59));     // #2d353b
        let bg_soft = AutoColor::Fixed(Color::Rgb(52, 63, 68));     // #343f44
        let bg_hl   = AutoColor::Fixed(Color::Rgb(60, 71, 77));     // #3c474d
        let fg      = AutoColor::Fixed(Color::Rgb(211, 198, 170));  // #d3c6aa
        let fg_sec = AutoColor::Fixed(Color::Rgb(185, 183, 165));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(157, 169, 160));  // #9da9a0
        let border  = AutoColor::Fixed(Color::Rgb(71, 82, 88));     // #475258
        let accent  = AutoColor::Fixed(Color::Rgb(167, 192, 128));  // #a7c080

        Self {
            name: "Everforest Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(167, 192, 128),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_hl,

            tab_active: fg,
            tab_inactive: fg_dim,

            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn everforest_light() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(243, 234, 211));  // #f3ead3
        let bg_soft = AutoColor::Fixed(Color::Rgb(237, 228, 205));  // #ede4cd
        let bg_hl   = AutoColor::Fixed(Color::Rgb(230, 223, 200));  // #e6dfc8
        let fg      = AutoColor::Fixed(Color::Rgb(92, 106, 114));   // #5c6a72
        let fg_sec = AutoColor::Fixed(Color::Rgb(110, 125, 120));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(130, 145, 129));  // #829181
        let border  = AutoColor::Fixed(Color::Rgb(216, 211, 200));  // #d8d3c8
        let accent  = AutoColor::Fixed(Color::Rgb(167, 192, 128));  // #a7c080

        Self {
            name: "Everforest Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(167, 192, 128),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_hl,

            tab_active: fg,
            tab_inactive: fg_dim,

            album_header_background: Some(bg_soft), // lighter header
            album_header_foreground: fg,
        }
    }

    // --------------------- MONOKAI ---------------------

    pub fn monokai_dark() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(39, 40, 34));    // #272822
        let bg_soft = AutoColor::Fixed(Color::Rgb(62, 61, 50));    // #3e3d32
        let bg_hl   = AutoColor::Fixed(Color::Rgb(73, 72, 62));    // #49483e
        let fg      = AutoColor::Fixed(Color::Rgb(248, 248, 242)); // #f8f8f2
        let fg_sec = AutoColor::Fixed(Color::Rgb(205, 205, 200));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(166, 166, 157)); // muted
        let border  = AutoColor::Fixed(Color::Rgb(91, 90, 78));
        let accent  = AutoColor::Fixed(Color::Rgb(102, 217, 239)); // blue

        Self {
            name: "Monokai Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(102, 217, 239),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent, border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: border,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn monokai_light() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(249, 248, 245)); // #f9f8f5
        let bg_soft = AutoColor::Fixed(Color::Rgb(239, 238, 233)); // #efeee9
        let bg_hl   = AutoColor::Fixed(Color::Rgb(232, 232, 226)); // #e8e8e2
        let fg      = AutoColor::Fixed(Color::Rgb(39, 40, 34));    // #272822
        let fg_sec = AutoColor::Fixed(Color::Rgb(78, 76, 64));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(117, 113, 94));  // #75715e
        let border  = bg_hl;
        let accent  = AutoColor::Fixed(Color::Rgb(102, 217, 239));

        Self {
            name: "Monokai Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(102, 217, 239),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent, border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: bg_hl,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: Some(bg_soft), // lighter header
            album_header_foreground: fg,
        }
    }

    // --------------------- DRACULA --------------------

    pub fn dracula() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(40, 42, 54));     // #282a36
        let bg_soft = AutoColor::Fixed(Color::Rgb(47, 50, 66));
        let bg_hl   = AutoColor::Fixed(Color::Rgb(68, 71, 90));     // #44475a
        let fg      = AutoColor::Fixed(Color::Rgb(248, 248, 242));  // #f8f8f2
        let fg_sec = AutoColor::Fixed(Color::Rgb(215, 215, 210));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(182, 183, 198));
        let border  = bg_hl;
        let accent  = AutoColor::Fixed(Color::Rgb(189, 147, 249));  // purple

        Self {
            name: "Dracula".to_string(),
            dark: true,
            primary_color: Color::Rgb(189, 147, 249),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent, border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: border,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn dracula_light() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(248, 248, 242));  // #f8f8f2
        let bg_soft = AutoColor::Fixed(Color::Rgb(239, 239, 234));
        let bg_hl   = AutoColor::Fixed(Color::Rgb(230, 230, 227));
        let fg      = AutoColor::Fixed(Color::Rgb(40, 42, 54));     // #282a36
        let fg_sec  = AutoColor::Fixed(Color::Rgb(70, 73, 89));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(91, 95, 120));
        let border  = bg_hl;
        let accent  = AutoColor::Fixed(Color::Rgb(189, 147, 249));

        Self {
            name: "Dracula Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(189, 147, 249),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent, border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: bg_hl,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: Some(bg_soft), // lighter header
            album_header_foreground: fg,
        }
    }

    // ----------------------- AYU ---------------------

    pub fn ayu_dark() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(15, 20, 25));     // #0f1419
        let bg_soft = AutoColor::Fixed(Color::Rgb(19, 23, 33));     // #131721
        let bg_hl   = AutoColor::Fixed(Color::Rgb(26, 31, 41));     // #1a1f29
        let fg      = AutoColor::Fixed(Color::Rgb(230, 225, 207));  // #e6e1cf
        let fg_sec = AutoColor::Fixed(Color::Rgb(155, 160, 161));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(92, 103, 115));   // #5c6773
        let border  = AutoColor::Fixed(Color::Rgb(45, 52, 65));
        let accent  = AutoColor::Fixed(Color::Rgb(57, 186, 230));   // #39bae6

        Self {
            name: "Ayu Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(57, 186, 230),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent, border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: border,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn ayu_light() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(250, 250, 250));  // #fafafa
        let bg_soft = AutoColor::Fixed(Color::Rgb(240, 240, 240));
        let bg_hl   = AutoColor::Fixed(Color::Rgb(234, 234, 234));
        let fg      = AutoColor::Fixed(Color::Rgb(92, 103, 115));   // #5c6773
        let fg_sec = AutoColor::Fixed(Color::Rgb(112, 120, 138));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(135, 147, 161));  // #8793a1
        let border  = AutoColor::Fixed(Color::Rgb(220, 220, 220));
        let accent  = AutoColor::Fixed(Color::Rgb(85, 180, 212));   // #55b4d4

        Self {
            name: "Ayu Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(85, 180, 212),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent, border,

            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: bg_hl,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: Some(bg_soft), // lighter header
            album_header_foreground: fg,
        }
    }

    // ------------------- KANAGAWA --------------------

    pub fn kanagawa_wave() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(31, 31, 40));     // #1f1f28
        let bg_soft = AutoColor::Fixed(Color::Rgb(42, 42, 55));     // #2a2a37
        let bg_hl   = AutoColor::Fixed(Color::Rgb(54, 54, 70));     // #363646
        let fg      = AutoColor::Fixed(Color::Rgb(220, 215, 186));  // #dcd7ba
        let fg_sec = AutoColor::Fixed(Color::Rgb(195, 190, 170));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(165, 166, 156));
        let border  = AutoColor::Fixed(Color::Rgb(78, 78, 100));
        let accent  = AutoColor::Fixed(Color::Rgb(126, 156, 216));  // #7e9cd8

        Self {
            name: "Kanagawa Wave".to_string(),
            dark: true,
            primary_color: Color::Rgb(126, 156, 216),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent, border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: border,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn kanagawa_lotus() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(242, 236, 188)); // #f2ecbc
        let bg_soft = AutoColor::Fixed(Color::Rgb(229, 223, 181)); // #e5dfb5
        let bg_hl = AutoColor::Fixed(Color::Rgb(221, 214, 168)); // #ddd6a8
        let fg = AutoColor::Fixed(Color::Rgb(84, 84, 100)); // #545464
        let fg_sec = AutoColor::Fixed(Color::Rgb(95, 95, 112));
        let fg_dim = AutoColor::Fixed(Color::Rgb(110, 110, 126)); // #6e6e7e
        let border = AutoColor::Fixed(Color::Rgb(201, 195, 165));
        let accent = AutoColor::Fixed(Color::Rgb(106, 140, 188)); // #6a8cbc

        Self {
            name: "Kanagawa Lotus".to_string(),
            dark: false,
            primary_color: Color::Rgb(106, 140, 188),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: bg_hl,
            tab_active: fg,
            tab_inactive: fg_dim,
            // album header less prominent than selected
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    // ------------------- NIGHT OWL -------------------

    pub fn night_owl() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(1, 22, 39));      // #011627
        let bg_soft = AutoColor::Fixed(Color::Rgb(11, 41, 66));     // #0b2942
        let bg_hl   = AutoColor::Fixed(Color::Rgb(18, 45, 66));     // #122d42
        let fg      = AutoColor::Fixed(Color::Rgb(214, 222, 235));  // #d6deeb
        let fg_sec = AutoColor::Fixed(Color::Rgb(190, 200, 220));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(167, 179, 194));
        let border  = AutoColor::Fixed(Color::Rgb(29, 59, 83));     // #1d3b53
        let accent  = AutoColor::Fixed(Color::Rgb(130, 170, 255));  // #82aaff

        Self {
            name: "Night Owl".to_string(),
            dark: true,
            primary_color: Color::Rgb(130, 170, 255),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent, border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: border,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn day_owl() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(234, 242, 255)); // soft day
        let bg_soft = AutoColor::Fixed(Color::Rgb(221, 230, 247));
        let bg_hl = AutoColor::Fixed(Color::Rgb(208, 218, 239));
        let fg = AutoColor::Fixed(Color::Rgb(64, 63, 83));
        let fg_sec = AutoColor::Fixed(Color::Rgb(90, 90, 110));
        let fg_dim = AutoColor::Fixed(Color::Rgb(122, 128, 146));
        let border = AutoColor::Fixed(Color::Rgb(200, 209, 225));
        let accent = AutoColor::Fixed(Color::Rgb(94, 151, 246)); // #5e97f6

        Self {
            name: "Night Owl Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(94, 151, 246),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: bg_hl,
            tab_active: fg,
            tab_inactive: fg_dim,
            // less prominent than selected
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    // --------------------- GITHUB --------------------

    pub fn github_dark() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(13, 17, 23));     // #0d1117
        let bg_soft = AutoColor::Fixed(Color::Rgb(22, 27, 34));     // #161b22
        let bg_hl   = AutoColor::Fixed(Color::Rgb(33, 38, 45));     // #21262d
        let fg      = AutoColor::Fixed(Color::Rgb(201, 209, 217));  // #c9d1d9
        let fg_sec = AutoColor::Fixed(Color::Rgb(170, 178, 188));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(139, 148, 158));  // #8b949e
        let border  = AutoColor::Fixed(Color::Rgb(48, 54, 61));     // #30363d
        let accent  = AutoColor::Fixed(Color::Rgb(88, 166, 255));   // #58a6ff

        Self {
            name: "GitHub Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(88, 166, 255),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent, border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: border,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn github_light() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(246, 248, 250)); // #f6f8fa
        let bg_soft = AutoColor::Fixed(Color::Rgb(238, 242, 247)); // #eef2f7
        let bg_hl = AutoColor::Fixed(Color::Rgb(234, 238, 242)); // #eaeef2
        let fg = AutoColor::Fixed(Color::Rgb(36, 41, 47)); // #24292f
        let fg_sec = AutoColor::Fixed(Color::Rgb(66, 72, 82));
        let fg_dim = AutoColor::Fixed(Color::Rgb(87, 96, 106)); // #57606a
        let border = AutoColor::Fixed(Color::Rgb(208, 215, 222)); // #d0d7de
        let accent = AutoColor::Fixed(Color::Rgb(9, 105, 218)); // #0969da

        Self {
            name: "GitHub Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(9, 105, 218),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: bg_hl,
            tab_active: fg,
            tab_inactive: fg_dim,
            // less prominent than selected
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    // --------------------- MATERIAL ------------------

    pub fn material_palenight() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(41, 45, 62));     // #292d3e
        let bg_soft = AutoColor::Fixed(Color::Rgb(47, 51, 70));     // #2f3346
        let bg_hl   = AutoColor::Fixed(Color::Rgb(58, 63, 88));     // #3a3f58
        let fg      = AutoColor::Fixed(Color::Rgb(166, 172, 205));  // #a6accd
        let fg_sec = AutoColor::Fixed(Color::Rgb(150, 160, 190));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(135, 150, 176));
        let border  = bg_hl;
        let accent  = AutoColor::Fixed(Color::Rgb(130, 170, 255));  // #82aaff

        Self {
            name: "Material Palenight".to_string(),
            dark: true,
            primary_color: Color::Rgb(130, 170, 255),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent, border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: border,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn material_light() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(250, 250, 250)); // #fafafa
        let bg_soft = AutoColor::Fixed(Color::Rgb(240, 240, 240));
        let bg_hl = AutoColor::Fixed(Color::Rgb(230, 230, 230));
        let fg = AutoColor::Fixed(Color::Rgb(55, 71, 79)); // #37474f
        let fg_sec = AutoColor::Fixed(Color::Rgb(80, 95, 110));
        let fg_dim = AutoColor::Fixed(Color::Rgb(96, 125, 139)); // #607d8b
        let border = AutoColor::Fixed(Color::Rgb(217, 217, 217));
        let accent = AutoColor::Fixed(Color::Rgb(41, 121, 255)); // #2979ff

        Self {
            name: "Material Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(41, 121, 255),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: bg_hl,
            tab_active: fg,
            tab_inactive: fg_dim,
            // less prominent than selected
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    // -------------------- PAPERCOLOR ------------------

    pub fn papercolor_dark() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(28, 28, 28));     // #1c1c1c
        let bg_soft = AutoColor::Fixed(Color::Rgb(38, 38, 38));     // #262626
        let bg_hl   = AutoColor::Fixed(Color::Rgb(58, 58, 58));     // #3a3a3a
        let fg      = AutoColor::Fixed(Color::Rgb(208, 208, 208));  // #d0d0d0
        let fg_sec = AutoColor::Fixed(Color::Rgb(185, 185, 185));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(168, 168, 168));  // #a8a8a8
        let border  = AutoColor::Fixed(Color::Rgb(68, 68, 68));     // #444444
        let accent  = AutoColor::Fixed(Color::Rgb(95, 135, 215));   // #5f87d7

        Self {
            name: "Papercolor Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(95, 135, 215),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent, border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: border,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn papercolor_light() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(238, 238, 238)); // #eeeeee
        let bg_soft = AutoColor::Fixed(Color::Rgb(226, 226, 226)); // #e2e2e2
        let bg_hl = AutoColor::Fixed(Color::Rgb(214, 214, 214)); // #d6d6d6
        let fg = AutoColor::Fixed(Color::Rgb(77, 77, 76)); // #4d4d4c
        let fg_sec = AutoColor::Fixed(Color::Rgb(90, 90, 86));
        let fg_dim = AutoColor::Fixed(Color::Rgb(114, 114, 113)); // #727271
        let border = AutoColor::Fixed(Color::Rgb(207, 207, 207)); // #cfcfcf
        let accent = AutoColor::Fixed(Color::Rgb(0, 135, 175)); // #0087af

        Self {
            name: "Papercolor Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(0, 135, 175),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: bg_hl,
            tab_active: fg,
            tab_inactive: fg_dim,
            // less prominent than selected
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    // -------------------- MONOCHROME ------------------

    pub fn monochrome_dark() -> Self {
        let bg      = AutoColor::Fixed(Color::Rgb(16, 16, 16));
        let bg_soft = AutoColor::Fixed(Color::Rgb(24, 24, 24));
        let bg_hl   = AutoColor::Fixed(Color::Rgb(42, 42, 42));
        let fg      = AutoColor::Fixed(Color::Rgb(238, 238, 238));
        let fg_sec = AutoColor::Fixed(Color::Rgb(215, 215, 215));
        let fg_dim  = AutoColor::Fixed(Color::Rgb(189, 189, 189));
        let border  = AutoColor::Fixed(Color::Rgb(51, 51, 51));
        let accent  = AutoColor::Fixed(Color::Rgb(179, 179, 179));

        Self {
            name: "Monochrome Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(179, 179, 179),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent, border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: border,
            tab_active: fg,
            tab_inactive: fg_dim,
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
        }
    }

    pub fn monochrome_light() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(255, 255, 255));
        let bg_soft = AutoColor::Fixed(Color::Rgb(242, 242, 242));
        let bg_hl = AutoColor::Fixed(Color::Rgb(230, 230, 230));
        let fg = AutoColor::Fixed(Color::Rgb(17, 17, 17));
        let fg_sec = AutoColor::Fixed(Color::Rgb(60, 60, 60));
        let fg_dim = AutoColor::Fixed(Color::Rgb(110, 110, 110));
        let border = AutoColor::Fixed(Color::Rgb(217, 217, 217));
        let accent = AutoColor::Fixed(Color::Rgb(102, 102, 102));

        Self {
            name: "Monochrome Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(102, 102, 102),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            section_title: fg,
            accent,
            border,
            selected_background: bg_hl,
            selected_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: bg_hl,
            tab_active: fg,
            tab_inactive: fg_dim,
            // less prominent than selected
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,
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
            foreground_secondary: AutoColor::Fixed(Color::Reset),
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
            album_header_foreground: AutoColor::Fixed(Color::Reset),
        }
    }
}