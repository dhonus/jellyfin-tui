use serde_json::Value;
use dirs::config_dir;

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