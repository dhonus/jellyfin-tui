use serde_json::Value;
use dirs::config_dir;

use ratatui::style::Color;
use std::str::FromStr;

pub fn get_config() -> Result<Value, Box<dyn std::error::Error>> {
    let config_dir = match config_dir() {
        Some(dir) => dir,
        None => {
            return Err("Could not find config directory".into());
        }
    };

    let config_file = config_dir.join("jellyfin-tui").join("config.yaml");

    let f = std::fs::File::open(config_file)?;
    let d: Value = serde_yaml::from_reader(f)?;

    Ok(d)
}

pub fn get_primary_color() -> Color {
    let config = match get_config() {
        Ok(config) => config,
        Err(_) => {
            return Color::Blue;
        }
    };

    let primary_color = match config["primary_color"].as_str() {
        Some(color) => color,
        None => {
            return Color::Blue;
        }
    };

    if let Ok(color) = ratatui::style::Color::from_str(primary_color) {
        return color;
    }

    Color::Blue
}
