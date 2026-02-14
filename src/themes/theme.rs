use ratatui::prelude::Color;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration, SystemTime};

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

    // internal color value, not set by user. Overridden by set_primary_color() to `accent` or cover art color
    pub(crate) primary_color: Color,

    // !! when adding new colors, also update apply_overrides() !!
    pub(crate) background: Option<AutoColor>,
    // pub(crate) background_active: Option<AutoColor>, // TODO
    pub(crate) foreground: AutoColor,
    pub(crate) foreground_secondary: AutoColor,
    pub(crate) foreground_dim: AutoColor,
    pub(crate) foreground_disabled: AutoColor,
    pub(crate) section_title: AutoColor,
    pub(crate) accent: AutoColor,
    pub(crate) border: AutoColor,
    pub(crate) border_focused: AutoColor,
    pub(crate) selected_active_background: AutoColor,
    pub(crate) selected_active_foreground: AutoColor,
    pub(crate) selected_inactive_background: AutoColor,
    pub(crate) selected_inactive_foreground: AutoColor,
    pub(crate) scrollbar_thumb: AutoColor,
    pub(crate) scrollbar_track: AutoColor,
    pub(crate) progress_fill: AutoColor,
    pub(crate) progress_track: AutoColor,
    pub(crate) tab_active_foreground: AutoColor,
    pub(crate) tab_inactive_foreground: AutoColor,
    pub(crate) album_header_background: Option<AutoColor>,
    pub(crate) album_header_foreground: AutoColor,

    // auto-color interpolation
    #[serde(skip)]
    #[serde(default)]
    pub(crate) last_primary: Color,
    #[serde(skip)]
    #[serde(default)]
    pub(crate) target_primary: Color,
    #[serde(skip)]
    #[serde(default)]
    pub(crate) lerp_elapsed_ms: u64,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            name: "Default".into(),
            dark: true,
            primary_color: Color::White,

            background: None,
            foreground: AutoColor::Fixed(Color::White),
            foreground_secondary: AutoColor::Fixed(Color::Gray),
            foreground_dim: AutoColor::Fixed(Color::Gray),
            foreground_disabled: AutoColor::Fixed(Color::Gray),
            section_title: AutoColor::Fixed(Color::White),
            accent: AutoColor::Fixed(Color::White),
            border: AutoColor::Fixed(Color::White),
            border_focused: AutoColor::Fixed(Color::White),
            selected_active_background: AutoColor::Fixed(Color::White),
            selected_active_foreground: AutoColor::Fixed(Color::Black),
            selected_inactive_background: AutoColor::Fixed(Color::White),
            selected_inactive_foreground: AutoColor::Fixed(Color::Black),
            scrollbar_thumb: AutoColor::Fixed(Color::White),
            scrollbar_track: AutoColor::Fixed(Color::Black),
            progress_fill: AutoColor::Fixed(Color::White),
            progress_track: AutoColor::Fixed(Color::Black),
            tab_active_foreground: AutoColor::Fixed(Color::White),
            tab_inactive_foreground: AutoColor::Fixed(Color::Black),
            album_header_background: None,
            album_header_foreground: AutoColor::Fixed(Color::White),

            last_primary: Color::White,
            target_primary: Color::White,
            lerp_elapsed_ms: 0,
        }
    }
}

impl Theme {
    pub fn builtin_themes() -> Vec<Self> {
        vec![
            Self::dark(),
            Self::light(),
            Self::gruvbox_dark(),
            Self::gruvbox_light(),
            Self::nord_dark(),
            Self::nord_light(),
            Self::catppuccin_mocha(),
            Self::catppuccin_latte(),
            Self::catppuccin_frappe(),
            Self::tokyonight(),
            Self::tokyonight_light(),
            Self::kanagawa_wave(),
            Self::kanagawa_lotus(),
            Self::github_dark(),
            Self::github_light(),
            Self::monochrome_dark(),
            Self::monochrome_light(),
            Self::dracula_dark(),
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
                        log::warn!(
                            "Theme '{}' specified unknown base '{}', skipping.",
                            name,
                            base_name
                        );
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
                    log::warn!(
                        "Theme '{}' does not specify 'base' or 'dark' property, skipping.",
                        name
                    );
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

        set_opt_color("background", &mut theme.background);
        set_opt_color("album_header_background", &mut theme.album_header_background);

        set_color("foreground", &mut theme.foreground);
        set_color("foreground_dim", &mut theme.foreground_dim);
        set_color("foreground_secondary", &mut theme.foreground_secondary);
        set_color("foreground_disabled", &mut theme.foreground_disabled);
        set_color("section_title", &mut theme.section_title);
        set_color("accent", &mut theme.accent);
        set_color("border", &mut theme.border);
        set_color("border_focused", &mut theme.border_focused);
        set_color("selected_active_background", &mut theme.selected_active_background);
        set_color("selected_active_foreground", &mut theme.selected_active_foreground);
        set_color("selected_inactive_background", &mut theme.selected_inactive_background);
        set_color("selected_inactive_foreground", &mut theme.selected_inactive_foreground);
        set_color("scrollbar_thumb", &mut theme.scrollbar_thumb);
        set_color("scrollbar_track", &mut theme.scrollbar_track);
        set_color("progress_fill", &mut theme.progress_fill);
        set_color("progress_track", &mut theme.progress_track);
        set_color("tab_active_foreground", &mut theme.tab_active_foreground);
        set_color("tab_inactive_foreground", &mut theme.tab_inactive_foreground);
        set_color("album_header_foreground", &mut theme.album_header_foreground);
    }

    pub fn set_primary_color(&mut self, color: Color) {
        if self.target_primary == color {
            return;
        }
        self.last_primary = self.primary_color;
        self.target_primary = color;
        self.lerp_elapsed_ms = 0;
    }

    pub fn tick_lerp(&mut self, dt_ms: u64, lerp_duration_ms: u64) -> bool {
        if self.lerp_elapsed_ms >= lerp_duration_ms {
            self.primary_color = self.target_primary;
            return false;
        }
        self.lerp_elapsed_ms += dt_ms;
        // clamp to duration
        let elapsed = if self.lerp_elapsed_ms > lerp_duration_ms {
            lerp_duration_ms
        } else {
            self.lerp_elapsed_ms
        };

        let raw_t = elapsed as f32 / lerp_duration_ms as f32;

        // smoothstep
        let t = raw_t * raw_t * (3.0 - 2.0 * raw_t);
        let old = self.primary_color;
        self.primary_color = Self::lerp_color(self.last_primary, self.target_primary, t);

        old != self.target_primary
    }

    fn lerp_color(a: Color, b: Color, t: f32) -> Color {
        match (a, b) {
            (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
                let r = r1 as f32 + (r2 as f32 - r1 as f32) * t;
                let g = g1 as f32 + (g2 as f32 - g1 as f32) * t;
                let b = b1 as f32 + (b2 as f32 - b1 as f32) * t;
                Color::Rgb(r as u8, g as u8, b as u8)
            }
            (_, new) => new,
        }
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
            foreground_secondary: AutoColor::Fixed(Color::Rgb(160, 160, 160)),
            foreground_dim: AutoColor::Fixed(Color::Rgb(142, 142, 142)),
            foreground_disabled: AutoColor::Fixed(Color::Rgb(110, 110, 110)),
            section_title: AutoColor::Fixed(Color::White),
            accent: AutoColor::Fixed(Color::Blue),
            border: AutoColor::Fixed(Color::Rgb(55, 55, 55)),
            border_focused: AutoColor::Auto,

            selected_active_background: AutoColor::Fixed(Color::White),
            selected_active_foreground: AutoColor::Fixed(Color::Indexed(232)),
            selected_inactive_background: AutoColor::Fixed(Color::Indexed(235)),
            selected_inactive_foreground: AutoColor::Fixed(Color::White),

            scrollbar_thumb: AutoColor::Fixed(Color::Gray),
            scrollbar_track: AutoColor::Fixed(Color::DarkGray),

            progress_fill: AutoColor::Fixed(Color::White),
            progress_track: AutoColor::Fixed(Color::DarkGray),

            tab_active_foreground: AutoColor::Fixed(Color::White),
            tab_inactive_foreground: AutoColor::Fixed(Color::DarkGray),

            album_header_background: None,
            album_header_foreground: AutoColor::Fixed(Color::White),

            ..Default::default()
        }
    }

    pub fn light() -> Self {
        Self {
            name: "Light".to_string(),
            dark: false,
            primary_color: Color::Blue,

            background: Some(AutoColor::Fixed(Color::Rgb(246, 246, 244))),

            foreground: AutoColor::Fixed(Color::Rgb(30, 30, 30)),
            foreground_secondary: AutoColor::Fixed(Color::Rgb(80, 80, 80)),
            foreground_dim: AutoColor::Fixed(Color::Rgb(150, 155, 165)),
            foreground_disabled: AutoColor::Fixed(Color::Rgb(185, 190, 200)),

            section_title: AutoColor::Fixed(Color::Rgb(20, 20, 20)),

            accent: AutoColor::Fixed(Color::Rgb(100, 100, 100)), // mid-gray

            border: AutoColor::Fixed(Color::Rgb(226, 226, 226)), // bg-20
            border_focused: AutoColor::Auto,

            selected_active_background: AutoColor::Fixed(Color::Rgb(220, 220, 220)), // bg-18
            selected_active_foreground: AutoColor::Fixed(Color::Rgb(20, 20, 20)),    // dark text
            selected_inactive_background: AutoColor::Fixed(Color::Rgb(236, 236, 236)), // bg-10
            selected_inactive_foreground: AutoColor::Fixed(Color::Rgb(80, 80, 80)), // mid-dark gray

            scrollbar_thumb: AutoColor::Fixed(Color::Rgb(120, 120, 120)),
            scrollbar_track: AutoColor::Fixed(Color::Rgb(220, 220, 220)),

            progress_fill: AutoColor::Fixed(Color::Rgb(60, 60, 60)), // strong dark gray
            progress_track: AutoColor::Fixed(Color::Rgb(210, 210, 210)),

            tab_active_foreground: AutoColor::Fixed(Color::Rgb(25, 25, 25)), // near black
            tab_inactive_foreground: AutoColor::Fixed(Color::Rgb(120, 120, 120)), // mid gray

            album_header_background: None,
            album_header_foreground: AutoColor::Fixed(Color::Rgb(80, 80, 80)), // secondary

            ..Default::default()
        }
    }

    pub fn gruvbox_dark() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(0x28, 0x28, 0x28)); // #282828
        let bg_soft = AutoColor::Fixed(Color::Rgb(0x32, 0x30, 0x2f)); // #32302f
        let fg = AutoColor::Fixed(Color::Rgb(0xeb, 0xdb, 0xb2)); // #ebdbb2
        let fg_dim = AutoColor::Fixed(Color::Rgb(0x7c, 0x6f, 0x64));
        let fg_dark = AutoColor::Fixed(Color::Rgb(0x3c, 0x38, 0x36)); // #3c3836

        let blue = AutoColor::Fixed(Color::Rgb(0x83, 0xa5, 0x98)); // #83a598
        let green = AutoColor::Fixed(Color::Rgb(0xb8, 0xbb, 0x26));

        Self {
            name: "Gruvbox Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(0x83, 0xa5, 0x98),
            background: Some(bg),
            foreground: AutoColor::Fixed(Color::Rgb(235, 219, 178)),
            foreground_secondary: AutoColor::Fixed(Color::Rgb(219, 209, 180)),
            foreground_dim: AutoColor::Fixed(Color::Rgb(140, 120, 110)),
            foreground_disabled: AutoColor::Fixed(Color::Rgb(110, 100, 95)),
            section_title: fg,
            accent: blue,
            border: AutoColor::Fixed(Color::Rgb(68, 62, 55)), // bg+20 (warm)
            border_focused: AutoColor::Auto,

            selected_active_background: AutoColor::Fixed(Color::Rgb(230, 215, 175)),
            selected_active_foreground: AutoColor::Fixed(Color::Rgb(40, 35, 30)),
            selected_inactive_background: AutoColor::Fixed(Color::Rgb(72, 65, 59)), // bg+18
            selected_inactive_foreground: AutoColor::Fixed(Color::Rgb(0xeb, 0xdb, 0xb2)),

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: green,
            progress_track: fg_dark,
            tab_active_foreground: fg,
            tab_inactive_foreground: fg_dim,
            album_header_background: None,
            album_header_foreground: fg,

            ..Default::default()
        }
    }

    pub fn gruvbox_light() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(0xfb, 0xf1, 0xc7)); // #fbf1c7
        let bg_soft = AutoColor::Fixed(Color::Rgb(0xf2, 0xe5, 0xbc)); // #f2e5bc
        let bg_hl = AutoColor::Fixed(Color::Rgb(0xeb, 0xdb, 0xb2)); // #ebdbb2
        let fg = AutoColor::Fixed(Color::Rgb(0x3c, 0x38, 0x36)); // #3c3836
        let fg_sec = AutoColor::Fixed(Color::Rgb(120, 110, 100));
        let fg_dim = AutoColor::Fixed(Color::Rgb(0x7c, 0x6f, 0x64)); // #7c6f64
        let fg_light = AutoColor::Fixed(Color::Rgb(0xeb, 0xdb, 0xb2)); // for tracks on light bg

        let blue = AutoColor::Fixed(Color::Rgb(0x45, 0x85, 0x88)); // #458588
        let green = AutoColor::Fixed(Color::Rgb(0xb8, 0xbb, 0x26)); // #b8bb26
        let border_col = AutoColor::Fixed(Color::Rgb(0xbd, 0xae, 0x93)); // #bdae93

        Self {
            name: "Gruvbox Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(0x45, 0x85, 0x88),
            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_disabled: AutoColor::Fixed(Color::Rgb(200, 190, 175)),
            foreground_dim: fg_dim,
            section_title: fg,
            accent: blue,
            border: border_col,
            border_focused: AutoColor::Auto,
            selected_active_background: bg_hl,
            selected_active_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: green,
            progress_track: fg_light,
            tab_active_foreground: fg,
            tab_inactive_foreground: fg_dim,
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,

            ..Default::default()
        }
    }
    pub fn nord_dark() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(46, 52, 64)); // #2E3440
        let bg_soft = AutoColor::Fixed(Color::Rgb(59, 66, 82)); // #3B4252
        let fg = AutoColor::Fixed(Color::Rgb(216, 222, 233)); // #D8DEE9
        let bg3 = Color::Rgb(76, 86, 106); // #4C566A

        let fg_dim = AutoColor::Fixed(Color::Rgb(92, 101, 120)); // #4C566A
        let border = AutoColor::Fixed(Color::Rgb(53, 59, 75)); // #353B4B
        let accent = AutoColor::Fixed(Color::Rgb(136, 192, 208)); // #88C0D0

        let selected_bg = AutoColor::Fixed(Color::Rgb(201, 208, 221)); // #C9D0DD
        let selected_fg = AutoColor::Fixed(Color::Rgb(46, 52, 64)); // #2E3440
        let inactive_sel_bg = AutoColor::Fixed(Color::Rgb(62, 69, 85)); // #3E4555
        let inactive_sel_fg = AutoColor::Fixed(Color::Rgb(219, 224, 232)); // #DBE0E8

        let fg_secondary = AutoColor::Fixed(Color::Rgb(220, 225, 232)); // #DCE1E8
        let fg_disabled = AutoColor::Fixed(Color::Rgb(129, 141, 162)); // #818DA2

        let scrollbar_thumb = AutoColor::Fixed(Color::Rgb(120, 131, 151)); // #788397
        let scrollbar_track = bg_soft;

        Self {
            name: "Nord Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(136, 192, 208),

            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_secondary,
            foreground_dim: fg_dim,
            foreground_disabled: fg_disabled,

            section_title: fg,
            accent,
            border,
            border_focused: AutoColor::Auto,

            selected_active_background: selected_bg,
            selected_active_foreground: selected_fg,
            selected_inactive_background: inactive_sel_bg,
            selected_inactive_foreground: inactive_sel_fg,

            scrollbar_thumb,
            scrollbar_track,

            progress_fill: fg,
            progress_track: AutoColor::Fixed(bg3),

            tab_active_foreground: fg,
            tab_inactive_foreground: AutoColor::Fixed(Color::Rgb(224, 228, 237)), // fg_dim

            album_header_background: Some(bg_soft),
            album_header_foreground: fg,

            ..Default::default()
        }
    }

    pub fn nord_light() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(236, 239, 244)); // #ECEFF4
        let bg_soft = AutoColor::Fixed(Color::Rgb(229, 233, 240)); // #E5E9F0
        let bg_hl = AutoColor::Fixed(Color::Rgb(216, 222, 233)); // #D8DEE9
        let fg = AutoColor::Fixed(Color::Rgb(46, 52, 64)); // #2E3440

        let fg_dim = AutoColor::Fixed(Color::Rgb(82, 95, 116)); // #525F74
        let fg_secondary = AutoColor::Fixed(Color::Rgb(72, 83, 102)); // #485366
        let fg_disabled = AutoColor::Fixed(Color::Rgb(169, 178, 193)); // #A9B2C1

        let border = AutoColor::Fixed(Color::Rgb(226, 231, 239)); // #E2E7EF
        let accent = AutoColor::Fixed(Color::Rgb(94, 129, 172)); // #5E81AC

        let selected_bg = AutoColor::Fixed(Color::Rgb(215, 220, 230)); // #D7DCE6
        let selected_fg = fg;
        let inactive_sel_bg = bg_soft;
        let inactive_sel_fg = fg_dim;

        let scrollbar_thumb = fg_dim;
        let scrollbar_track = bg_soft;

        Self {
            name: "Nord Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(94, 129, 172),

            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_secondary,
            foreground_dim: fg_dim,
            foreground_disabled: fg_disabled,

            section_title: fg,
            accent,
            border,
            border_focused: AutoColor::Auto,

            selected_active_background: selected_bg,
            selected_active_foreground: selected_fg,
            selected_inactive_background: inactive_sel_bg,
            selected_inactive_foreground: inactive_sel_fg,

            scrollbar_thumb,
            scrollbar_track,

            progress_fill: fg,
            progress_track: bg_hl,

            tab_active_foreground: fg,
            tab_inactive_foreground: AutoColor::Fixed(Color::Rgb(82, 95, 116)), // fg_dim

            album_header_background: Some(bg_soft),
            album_header_foreground: fg,

            ..Default::default()
        }
    }
    // ----------------- CATPPUCCIN --------------------

    pub fn catppuccin_mocha() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(30, 30, 46)); // #1e1e2e
        let bg_soft = AutoColor::Fixed(Color::Rgb(49, 50, 68)); // #313244
        let fg = AutoColor::Fixed(Color::Rgb(205, 214, 244)); // #cdd6f4
        let fg_dim = AutoColor::Fixed(Color::Rgb(151, 159, 188)); // #a1a9c6
        let accent = AutoColor::Fixed(Color::Rgb(137, 180, 250)); // #89b4fa

        Self {
            name: "Catppuccin Mocha".to_string(),
            dark: true,
            primary_color: Color::Rgb(137, 180, 250),

            background: Some(bg),

            foreground: AutoColor::Fixed(Color::Rgb(205, 214, 244)),
            foreground_secondary: AutoColor::Fixed(Color::Rgb(185, 194, 222)),
            foreground_dim: AutoColor::Fixed(Color::Rgb(151, 159, 188)),
            foreground_disabled: AutoColor::Fixed(Color::Rgb(120, 125, 145)),
            section_title: fg,
            accent,
            border: AutoColor::Fixed(Color::Rgb(52, 54, 72)), // bg+20
            border_focused: AutoColor::Auto,

            selected_active_background: AutoColor::Fixed(Color::Rgb(220, 224, 235)),
            selected_active_foreground: AutoColor::Fixed(Color::Rgb(30, 32, 42)),
            selected_inactive_background: AutoColor::Fixed(Color::Rgb(70, 72, 90)),
            selected_inactive_foreground: AutoColor::Fixed(Color::Rgb(220, 224, 235)),

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_soft,

            tab_active_foreground: fg,
            tab_inactive_foreground: fg_dim,

            album_header_background: Some(bg_soft),
            album_header_foreground: fg,

            ..Default::default()
        }
    }

    pub fn catppuccin_latte() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(239, 241, 245)); // #eff1f5
        let bg_soft = AutoColor::Fixed(Color::Rgb(230, 233, 239)); // #e6e9ef
        let bg_hl = AutoColor::Fixed(Color::Rgb(204, 208, 218)); // #ccd0da
        let fg = AutoColor::Fixed(Color::Rgb(76, 79, 105)); // #4c4f69
        let fg_sec = AutoColor::Fixed(Color::Rgb(92, 95, 118));
        let fg_dim = AutoColor::Fixed(Color::Rgb(108, 111, 133)); // #6c6f85
        let accent = AutoColor::Fixed(Color::Rgb(30, 102, 245)); // #1e66f5

        Self {
            name: "Catppuccin Latte".to_string(),
            dark: false,
            primary_color: Color::Rgb(30, 102, 245),

            background: Some(bg),

            foreground: AutoColor::Fixed(Color::Rgb(76, 79, 105)),
            foreground_secondary: AutoColor::Fixed(Color::Rgb(92, 95, 118)),
            foreground_dim: AutoColor::Fixed(Color::Rgb(160, 165, 180)),
            foreground_disabled: AutoColor::Fixed(Color::Rgb(185, 190, 200)),
            section_title: fg,
            accent,
            border: AutoColor::Fixed(Color::Rgb(219, 222, 232)), // bg-20
            border_focused: AutoColor::Auto,

            selected_active_background: AutoColor::Fixed(Color::Rgb(221, 225, 235)), // bg-18
            selected_active_foreground: fg,
            selected_inactive_background: AutoColor::Fixed(Color::Rgb(227, 230, 238)), // bg-10
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_hl,

            tab_active_foreground: fg,
            tab_inactive_foreground: fg_dim,

            album_header_background: Some(bg_soft), // lighter than selection
            album_header_foreground: fg_sec,

            ..Default::default()
        }
    }

    pub fn catppuccin_frappe() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(48, 52, 70)); // #303446
        let bg_soft = AutoColor::Fixed(Color::Rgb(65, 69, 89)); // #414559
        let bg_hl = AutoColor::Fixed(Color::Rgb(81, 87, 109)); // #51576d

        let fg = AutoColor::Fixed(Color::Rgb(198, 208, 245)); // #c6d0f5
        let fg_sec = AutoColor::Fixed(Color::Rgb(181, 191, 226)); // #b5bfe2
        let fg_dim = AutoColor::Fixed(Color::Rgb(148, 156, 187)); // #949cbb

        let accent = AutoColor::Fixed(Color::Rgb(140, 170, 238)); // #8caaee (Blue)

        Self {
            name: "Catppuccin Frappe".to_string(),
            dark: true,
            primary_color: Color::Rgb(140, 170, 238),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            foreground_disabled: AutoColor::Fixed(Color::Rgb(115, 121, 148)), // muted text

            section_title: fg,
            accent,

            border: AutoColor::Fixed(Color::Rgb(73, 77, 100)), // bg+~25
            border_focused: AutoColor::Auto,

            selected_active_background: AutoColor::Fixed(Color::Rgb(198, 208, 245)), // bright selection
            selected_active_foreground: AutoColor::Fixed(Color::Rgb(48, 50, 66)),    // dark text

            selected_inactive_background: AutoColor::Fixed(Color::Rgb(90, 94, 120)), // muted selection
            selected_inactive_foreground: AutoColor::Fixed(Color::Rgb(198, 208, 245)),

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_hl,

            tab_active_foreground: fg,
            tab_inactive_foreground: fg_dim,

            album_header_background: Some(bg_soft),
            album_header_foreground: fg_sec,

            ..Default::default()
        }
    }

    // ---------------- TOKYO NIGHT --------------------

    pub fn tokyonight() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(26, 27, 38)); // #1a1b26
        let bg_soft = AutoColor::Fixed(Color::Rgb(36, 40, 59)); // #24283b
        let fg = AutoColor::Fixed(Color::Rgb(192, 202, 245)); // #c0caf5
        let fg_dim = AutoColor::Fixed(Color::Rgb(140, 150, 210)); // #939dd9
        let border = AutoColor::Fixed(Color::Rgb(59, 66, 97)); // #3b4261
        let accent = AutoColor::Fixed(Color::Rgb(122, 162, 247)); // #7aa2f7

        Self {
            name: "Tokyo Night".to_string(),
            dark: true,
            primary_color: Color::Rgb(122, 162, 247),

            background: Some(bg),

            foreground: AutoColor::Fixed(Color::Rgb(192, 202, 245)),
            foreground_secondary: AutoColor::Fixed(Color::Rgb(180, 190, 228)),
            foreground_dim: AutoColor::Fixed(Color::Rgb(140, 150, 210)),
            foreground_disabled: AutoColor::Fixed(Color::Rgb(110, 120, 160)),

            section_title: fg,
            accent,
            border: AutoColor::Fixed(Color::Rgb(46, 48, 70)), // bg+20
            border_focused: AutoColor::Auto,

            selected_active_background: AutoColor::Fixed(Color::Rgb(205, 210, 240)),
            selected_active_foreground: AutoColor::Fixed(Color::Rgb(30, 32, 45)),
            selected_inactive_background: AutoColor::Fixed(Color::Rgb(60, 65, 95)),
            selected_inactive_foreground: AutoColor::Fixed(Color::Rgb(205, 210, 240)),

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: border,

            tab_active_foreground: fg,
            tab_inactive_foreground: fg_dim,

            album_header_background: Some(bg_soft),
            album_header_foreground: fg,

            ..Default::default()
        }
    }

    pub fn tokyonight_light() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(225, 226, 231));
        let bg_soft = AutoColor::Fixed(Color::Rgb(213, 214, 219));
        let bg_hl = AutoColor::Fixed(Color::Rgb(205, 213, 240)); // light blue
        let fg = AutoColor::Fixed(Color::Rgb(31, 35, 53));
        let fg_sec = AutoColor::Fixed(Color::Rgb(70, 80, 100));
        let fg_dim = AutoColor::Fixed(Color::Rgb(91, 96, 120));
        let accent = AutoColor::Fixed(Color::Rgb(46, 125, 233));

        Self {
            name: "Tokyo Night Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(46, 125, 233),

            background: Some(bg),

            foreground: AutoColor::Fixed(Color::Rgb(31, 35, 53)),
            foreground_secondary: AutoColor::Fixed(Color::Rgb(70, 80, 100)),
            foreground_dim: AutoColor::Fixed(Color::Rgb(150, 155, 175)),
            foreground_disabled: AutoColor::Fixed(Color::Rgb(185, 190, 200)),

            section_title: fg,
            accent,
            border: AutoColor::Fixed(Color::Rgb(205, 207, 215)), // bg-20
            border_focused: AutoColor::Auto,

            selected_active_background: AutoColor::Fixed(Color::Rgb(207, 209, 217)), // bg-18
            selected_active_foreground: fg,
            selected_inactive_background: AutoColor::Fixed(Color::Rgb(214, 216, 223)), // bg-10
            selected_inactive_foreground: fg_dim,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: fg,
            progress_track: bg_hl,

            tab_active_foreground: fg,
            tab_inactive_foreground: fg_dim,

            album_header_background: Some(bg_soft), // lighter header
            album_header_foreground: fg_sec,

            ..Default::default()
        }
    }

    // ------------------- KANAGAWA --------------------

    pub fn kanagawa_wave() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(31, 31, 40)); // #1F1F28
        let bg_soft = AutoColor::Fixed(Color::Rgb(42, 42, 55)); // #2A2A37

        let fg = AutoColor::Fixed(Color::Rgb(220, 215, 186)); // #DCD7BA
        let fg_sec = AutoColor::Fixed(Color::Rgb(200, 192, 147)); // #C8C093
        let fg_dim = AutoColor::Fixed(Color::Rgb(150, 148, 122)); // #96947A (correct)
        let fg_disabled = AutoColor::Fixed(Color::Rgb(122, 119, 99)); // derived

        let border = AutoColor::Fixed(Color::Rgb(84, 84, 109)); // #54546D
        let accent = AutoColor::Fixed(Color::Rgb(126, 156, 216)); // #7E9CD8

        let selected_bg = AutoColor::Fixed(Color::Rgb(74, 74, 89)); // derived from bg3 blend
        let selected_fg = AutoColor::Fixed(Color::Rgb(220, 215, 186));
        let inactive_sel_bg = AutoColor::Fixed(Color::Rgb(47, 47, 66)); // corrected — no yellow
        let inactive_sel_fg = AutoColor::Fixed(Color::Rgb(220, 215, 186));

        Self {
            name: "Kanagawa Wave".into(),
            dark: true,
            primary_color: Color::Rgb(126, 156, 216),

            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: fg_dim,
            foreground_disabled: fg_disabled,

            section_title: AutoColor::Fixed(Color::Rgb(220, 215, 186)),
            accent,
            border,
            border_focused: AutoColor::Auto,

            selected_active_background: selected_bg,
            selected_active_foreground: selected_fg,
            selected_inactive_background: inactive_sel_bg,
            selected_inactive_foreground: inactive_sel_fg,

            scrollbar_thumb: AutoColor::Fixed(Color::Rgb(150, 148, 122)),
            scrollbar_track: bg_soft,
            progress_fill: AutoColor::Fixed(Color::Rgb(220, 215, 186)),
            progress_track: border,

            tab_active_foreground: AutoColor::Fixed(Color::Rgb(220, 215, 186)),
            tab_inactive_foreground: AutoColor::Fixed(Color::Rgb(150, 148, 122)),

            album_header_background: Some(bg_soft),
            album_header_foreground: AutoColor::Fixed(Color::Rgb(220, 215, 186)),

            ..Default::default()
        }
    }

    pub fn kanagawa_lotus() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(242, 236, 188)); // #F2ECBC
        let bg_soft = AutoColor::Fixed(Color::Rgb(229, 223, 181)); // #E5DFB5
        let bg_hl = AutoColor::Fixed(Color::Rgb(221, 214, 168)); // #DDD6A8

        let fg = AutoColor::Fixed(Color::Rgb(84, 84, 100)); // #545464
        let fg_sec = AutoColor::Fixed(Color::Rgb(110, 110, 126)); // #6E6E7E
        let fg_dim = AutoColor::Fixed(Color::Rgb(138, 138, 154)); // corrected #8A8A9A
        let fg_disabled = AutoColor::Fixed(Color::Rgb(180, 180, 190));

        let border = AutoColor::Fixed(Color::Rgb(197, 201, 197)); // #C5C9C5
        let accent = AutoColor::Fixed(Color::Rgb(106, 140, 188)); // #6A8CBC

        let selected_bg = AutoColor::Fixed(Color::Rgb(225, 218, 163)); // derived
        let selected_fg = AutoColor::Fixed(Color::Rgb(84, 84, 100));
        let inactive_sel_bg = AutoColor::Fixed(Color::Rgb(231, 225, 173));
        let inactive_sel_fg = fg_dim;

        Self {
            name: "Kanagawa Lotus".into(),
            dark: false,
            primary_color: Color::Rgb(106, 140, 188),

            background: Some(bg),
            foreground: fg,
            foreground_secondary: fg_sec,
            foreground_dim: AutoColor::Fixed(Color::Rgb(138, 138, 154)),
            foreground_disabled: fg_disabled,

            section_title: AutoColor::Fixed(Color::Rgb(84, 84, 100)),
            accent,
            border,
            border_focused: AutoColor::Auto,

            selected_active_background: selected_bg,
            selected_active_foreground: selected_fg,
            selected_inactive_background: inactive_sel_bg,
            selected_inactive_foreground: inactive_sel_fg,

            scrollbar_thumb: AutoColor::Fixed(Color::Rgb(138, 138, 154)),
            scrollbar_track: bg_soft,
            progress_fill: AutoColor::Fixed(Color::Rgb(84, 84, 100)),
            progress_track: bg_hl,

            tab_active_foreground: AutoColor::Fixed(Color::Rgb(84, 84, 100)),
            tab_inactive_foreground: AutoColor::Fixed(Color::Rgb(110, 110, 126)),

            album_header_background: Some(bg_soft),
            album_header_foreground: AutoColor::Fixed(Color::Rgb(110, 110, 126)),

            ..Default::default()
        }
    }

    // --------------------- GITHUB --------------------

    pub fn github_dark() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(13, 17, 23)); // #0d1117
        let bg_soft = AutoColor::Fixed(Color::Rgb(22, 27, 34)); // #161b22
        let fg = AutoColor::Fixed(Color::Rgb(201, 209, 217)); // #c9d1d9
        let fg_dim = AutoColor::Fixed(Color::Rgb(139, 148, 158)); // #8b949e
        let border = AutoColor::Fixed(Color::Rgb(48, 54, 61)); // #30363d
        let accent = AutoColor::Fixed(Color::Rgb(88, 166, 255)); // #58a6ff

        Self {
            name: "GitHub Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(88, 166, 255),

            background: Some(bg),
            foreground: AutoColor::Fixed(Color::Rgb(201, 209, 217)),
            foreground_secondary: AutoColor::Fixed(Color::Rgb(170, 178, 188)),
            foreground_dim: AutoColor::Fixed(Color::Rgb(139, 148, 158)),
            foreground_disabled: AutoColor::Fixed(Color::Rgb(110, 118, 130)),

            section_title: fg,
            accent,
            border: AutoColor::Fixed(Color::Rgb(33, 38, 45)), // bg+20
            border_focused: AutoColor::Auto,

            selected_active_background: AutoColor::Fixed(Color::Rgb(201, 209, 217)),
            selected_active_foreground: AutoColor::Fixed(Color::Rgb(22, 27, 34)),
            selected_inactive_background: AutoColor::Fixed(Color::Rgb(33, 38, 45)),
            selected_inactive_foreground: AutoColor::Fixed(Color::Rgb(201, 209, 217)),

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: border,
            tab_active_foreground: fg,
            tab_inactive_foreground: fg_dim,
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,

            ..Default::default()
        }
    }

    pub fn github_light() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(246, 248, 250)); // #f6f8fa
        let bg_soft = AutoColor::Fixed(Color::Rgb(238, 242, 247)); // #eef2f7
        let bg_hl = AutoColor::Fixed(Color::Rgb(234, 238, 242)); // #eaeef2
        let fg = AutoColor::Fixed(Color::Rgb(36, 41, 47)); // #24292f
        let fg_sec = AutoColor::Fixed(Color::Rgb(66, 72, 82));
        let fg_dim = AutoColor::Fixed(Color::Rgb(87, 96, 106)); // #57606a
        let accent = AutoColor::Fixed(Color::Rgb(9, 105, 218)); // #0969da

        Self {
            name: "GitHub Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(9, 105, 218),
            background: Some(bg),
            foreground: AutoColor::Fixed(Color::Rgb(36, 41, 47)),
            foreground_secondary: AutoColor::Fixed(Color::Rgb(66, 72, 82)),
            foreground_dim: AutoColor::Fixed(Color::Rgb(175, 184, 193)),
            foreground_disabled: AutoColor::Fixed(Color::Rgb(199, 208, 216)),
            section_title: fg,
            accent,
            border: AutoColor::Fixed(Color::Rgb(226, 229, 234)), // bg-20
            border_focused: AutoColor::Auto,
            selected_active_background: AutoColor::Fixed(Color::Rgb(232, 236, 240)), // bg-18
            selected_active_foreground: fg,
            selected_inactive_background: AutoColor::Fixed(Color::Rgb(238, 241, 245)), // bg-10
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: bg_hl,
            tab_active_foreground: fg,
            tab_inactive_foreground: fg_dim,
            // less prominent than selected
            album_header_background: Some(bg_soft),
            album_header_foreground: fg_sec,

            ..Default::default()
        }
    }

    // -------------------- MONOCHROME ------------------

    pub fn monochrome_dark() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(16, 16, 16));
        let bg_soft = AutoColor::Fixed(Color::Rgb(24, 24, 24));
        let bg_hl = AutoColor::Fixed(Color::Rgb(42, 42, 42));
        let fg = AutoColor::Fixed(Color::Rgb(238, 238, 238));
        let fg_dim = AutoColor::Fixed(Color::Rgb(179, 179, 179));
        let border = AutoColor::Fixed(Color::Rgb(51, 51, 51));
        let accent = AutoColor::Fixed(Color::Rgb(179, 179, 179));

        Self {
            name: "Monochrome Dark".to_string(),
            dark: true,
            primary_color: Color::Rgb(179, 179, 179),

            background: Some(bg),
            foreground: AutoColor::Fixed(Color::Rgb(238, 238, 238)),
            foreground_secondary: AutoColor::Fixed(Color::Rgb(215, 215, 215)),
            foreground_dim: AutoColor::Fixed(Color::Rgb(179, 179, 179)),
            foreground_disabled: AutoColor::Fixed(Color::Rgb(140, 140, 140)),

            section_title: fg,
            accent,
            border,
            border_focused: AutoColor::Auto,
            selected_active_background: bg_hl,
            selected_active_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: border,
            tab_active_foreground: fg,
            tab_inactive_foreground: fg_dim,
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,

            ..Default::default()
        }
    }

    pub fn monochrome_light() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(255, 255, 255));
        let bg_soft = AutoColor::Fixed(Color::Rgb(242, 242, 242));
        let bg_hl = AutoColor::Fixed(Color::Rgb(230, 230, 230));
        let fg = AutoColor::Fixed(Color::Rgb(17, 17, 17));
        let fg_dim = AutoColor::Fixed(Color::Rgb(110, 110, 110));
        let border = AutoColor::Fixed(Color::Rgb(217, 217, 217));
        let accent = AutoColor::Fixed(Color::Rgb(102, 102, 102));

        Self {
            name: "Monochrome Light".to_string(),
            dark: false,
            primary_color: Color::Rgb(102, 102, 102),
            background: Some(bg),
            foreground: AutoColor::Fixed(Color::Rgb(17, 17, 17)),
            foreground_secondary: AutoColor::Fixed(Color::Rgb(60, 60, 60)),
            foreground_dim: AutoColor::Fixed(Color::Rgb(160, 160, 160)),
            foreground_disabled: AutoColor::Fixed(Color::Rgb(200, 200, 200)),
            section_title: fg,
            accent,
            border,
            border_focused: AutoColor::Auto,
            selected_active_background: bg_hl,
            selected_active_foreground: fg,
            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg_dim,
            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,
            progress_fill: fg,
            progress_track: bg_hl,
            tab_active_foreground: fg,
            tab_inactive_foreground: fg_dim,
            // less prominent than selected
            album_header_background: Some(bg_soft),
            album_header_foreground: fg,

            ..Default::default()
        }
    }

    pub fn dracula_dark() -> Self {
        let bg = AutoColor::Fixed(Color::Rgb(0x28, 0x2a, 0x36)); // #282a36
        let bg_soft = AutoColor::Fixed(Color::Rgb(0x34, 0x37, 0x46)); // #343746
        let fg = AutoColor::Fixed(Color::Rgb(0xf8, 0xf8, 0xf2)); // #f8f8f2
        let fg_dim = AutoColor::Fixed(Color::Rgb(0x62, 0x72, 0xa4)); // comment
        let fg_disabled = AutoColor::Fixed(Color::Rgb(0x52, 0x58, 0x78));

        let _cyan = AutoColor::Fixed(Color::Rgb(0x8b, 0xe9, 0xfd)); // cyan
        let purple = AutoColor::Fixed(Color::Rgb(0xbd, 0x93, 0xf9)); // purple
        let pink = AutoColor::Fixed(Color::Rgb(0xff, 0x79, 0xc6)); // pink

        Self {
            name: "Dracula".to_string(),
            dark: true,
            primary_color: Color::Rgb(0xbd, 0x93, 0xf9),

            background: Some(bg),

            foreground: fg,
            foreground_secondary: AutoColor::Fixed(Color::Rgb(0xe0, 0xe0, 0xe0)),
            foreground_dim: fg_dim,
            foreground_disabled: fg_disabled,

            section_title: fg,
            accent: purple,

            border: AutoColor::Fixed(Color::Rgb(0x3f, 0x42, 0x55)),
            border_focused: AutoColor::Auto,

            selected_active_background: AutoColor::Fixed(Color::Rgb(0xf8, 0xf8, 0xf2)),
            selected_active_foreground: AutoColor::Fixed(Color::Rgb(0x28, 0x2a, 0x36)),

            selected_inactive_background: bg_soft,
            selected_inactive_foreground: fg,

            scrollbar_thumb: fg_dim,
            scrollbar_track: bg_soft,

            progress_fill: pink,
            progress_track: bg_soft,

            tab_active_foreground: fg,
            tab_inactive_foreground: fg_dim,

            album_header_background: Some(bg_soft),
            album_header_foreground: fg,

            ..Default::default()
        }
    }
}

pub struct ConfigWatcher {
    pub path: PathBuf,
    pub last_check: std::time::Instant,
    pub last_mtime: Option<SystemTime>,
    pub interval: Duration,
}

impl ConfigWatcher {
    pub fn new(path: impl Into<PathBuf>, interval: Duration) -> Self {
        Self {
            path: path.into(),
            last_check: std::time::Instant::now(),
            last_mtime: None,
            interval,
        }
    }

    pub fn poll(&mut self) -> bool {
        if self.last_check.elapsed() < self.interval {
            return false;
        }
        self.last_check = std::time::Instant::now();

        let metadata = match std::fs::metadata(&self.path) {
            Ok(m) => m,
            Err(_) => return false,
        };

        let modified = match metadata.modified() {
            Ok(t) => t,
            Err(_) => return false,
        };

        // if first time just store and skip
        match self.last_mtime {
            None => {
                self.last_mtime = Some(modified);
                false
            }
            Some(old) if old == modified => return false,
            _ => {
                // file changed
                self.last_mtime = Some(modified);
                true
            }
        }
    }
}
