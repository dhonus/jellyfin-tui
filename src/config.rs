use dirs::config_dir;
use serde_json::Value;

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

pub fn get_primary_color(config: &Value) -> Color {
    if let Some(primary_color) = config["primary_color"].as_str() {
        if let Ok(color) = Color::from_str(primary_color) {
            return color;
        }
    }
    Color::Blue
}
