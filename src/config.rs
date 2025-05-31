use std::collections::HashMap;
use dirs::{cache_dir, config_dir};
use ratatui::style::Color;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::str::FromStr;
use dialoguer::{Confirm, Input, Password};
use crate::client::SelectedServer;
use crate::themes::dialoguer::DialogTheme;

pub fn get_config() -> Result<serde_yaml::Value, Box<dyn std::error::Error>> {
    let config_dir = match config_dir() {
        Some(dir) => dir,
        None => {
            return Err("Could not find config directory".into());
        }
    };

    let config_file = config_dir.join("jellyfin-tui").join("config.yaml");

    let f = std::fs::File::open(config_file)?;
    let d = serde_yaml::from_reader(f)?;

    Ok(d)
}

pub fn get_primary_color(config: &serde_yaml::Value) -> Color {
    if let Some(primary_color) = config["primary_color"].as_str() {
        if let Ok(color) = Color::from_str(primary_color) {
            return color;
        }
    }
    Color::Blue
}

pub fn select_server(config: &serde_yaml::Value, force_server_select: bool) -> Option<SelectedServer> {

    // we now supposed servers as an array
    let servers = match config["servers"].as_sequence() {
        Some(s) => s,
        None => {
            println!(" ! Could not find servers in config file");
            std::process::exit(1);
        }
    };

    if servers.is_empty() {
        println!(" ! No servers configured in config file");
        std::process::exit(1);
    }

    let selected_server = if servers.len() == 1 {
        // if there is only one server, we use that one
        servers[0].clone()
    } else {
        // server set to default skips the selection dialog :)
        if let Some(default_server) = servers.iter().find(|s| s.get("default").and_then(|v| v.as_bool()).unwrap_or(false)) {
            if !force_server_select {
                println!(" - Chose {} ({}), --select-server to override.",
                    default_server["name"].as_str().unwrap_or("Unnamed"),
                    default_server["url"].as_str().unwrap_or("Unknown"));
                return Some(SelectedServer {
                    url: default_server["url"].as_str().unwrap_or("").to_string(),
                    name: default_server["name"].as_str().unwrap_or("Unnamed").to_string(),
                    username: default_server["username"].as_str().unwrap_or("").to_string(),
                    password: default_server["password"].as_str().unwrap_or("").to_string(),
                });
            }
        }
        // otherwise if there are multiple servers, we ask the user to select one
        let server_names: Vec<String> = servers
            .iter()
            // Name (URL)
            .filter_map(|s| format!("{} ({})", s["name"].as_str().unwrap_or("Unnamed"), s["url"].as_str().unwrap_or("Unknown")).into())
            .collect();
        if server_names.is_empty() {
            println!(" ! No servers configured in config file");
            std::process::exit(1);
        }
        let selection = dialoguer::Select::with_theme(&DialogTheme::default())
            .with_prompt("Which server would you like to use?")
            .items(&server_names)
            .default(0)
            .interact()
            .unwrap_or(0);
        servers[selection].clone()
    };

    let url = match selected_server["url"].as_str() {
        Some(url) => {
            if url.ends_with('/') {
                println!(" ! URL ends with a trailing slash, please remove it.");
                std::process::exit(1);
            } else {
                url.to_string()
            }
        }
        None => {
            println!(" ! Selected server does not have a URL configured");
            std::process::exit(1);
        }
    };
    let name = match selected_server["name"].as_str() {
        Some(name) => name.to_string(),
        None => {
            println!(" ! Selected server does not have a name configured");
            std::process::exit(1);
        }
    };
    let username = match selected_server["username"].as_str() {
        Some(username) => username.to_string(),
        None => {
            println!(" ! Selected server does not have a username configured");
            std::process::exit(1);
        }
    };
    let password = match selected_server["password"].as_str() {
        Some(password) => password.to_string(),
        None => {
            println!(" ! Selected server does not have a password configured");
            std::process::exit(1);
        }
    };
    Some(SelectedServer {
        url, name, username, password
    })
}

pub fn initialize_config() {
    let config_dir = match config_dir() {
        Some(dir) => dir,
        None => {
            println!(" ! Could not find config directory");
            std::process::exit(1);
        }
    };
    let cache_dir = match cache_dir() {
        Some(dir) => dir,
        None => {
            println!(" ! Could not find cache directory");
            std::process::exit(1);
        }
    };

    let config_file = config_dir.join("jellyfin-tui").join("config.yaml");
    let mapping_file = cache_dir.join("jellyfin-tui").join("server_map.json");

    if config_file.exists() {
        println!(
            " - Using configuration file: {}",
            config_file
                .to_str()
                .expect(" ! Could not convert config path to string")
        );
        return;
    }

    let mut server_name = String::new();
    let mut server_url = String::new();
    let mut username = String::new();
    let mut password = String::new();
    let mut server_id = String::new();

    println!(" - Thank you for trying out jellyfin-tui! <3\n");
    println!(" - This version introduces a new (complicated) offline mode, so please report any issues you find or ideas you have here:");
    println!(" - https://github.com/dhonus/jellyfin-tui/issues\n");
    println!(" ! The configuration file does not exist. Please fill in the following details:\n");

    let http_client = reqwest::blocking::Client::new();

    let mut ok = false;
    let mut counter = 0;
    while !ok {
        server_url = Input::with_theme(&DialogTheme::default())
            .with_prompt("Server URL")
            .with_initial_text("https://")
            .validate_with({
                move |input: &String| -> Result<(), &str> {
                    if input.starts_with("http://") || input.starts_with("https://") && input != "http://" && input != "https://" {
                        Ok(())
                    } else {
                        Err("Please enter a valid URL including http or https")
                    }
                }
            })
            .interact_text()
            .unwrap();

        if server_url.ends_with('/') {
            server_url.pop();
        }

        server_name = Input::with_theme(&DialogTheme::default())
            .with_prompt("Server name")
            .with_initial_text("Home Server")
            .interact_text()
            .unwrap();

        username = Input::with_theme(&DialogTheme::default())
            .with_prompt("Username")
            .interact_text()
            .unwrap();

        password = Password::with_theme(&DialogTheme::default())
            .allow_empty_password(true)
            .with_prompt("Password")
            .with_confirmation("Repeat password", "Error: the passwords don't match.")
            .interact()
            .unwrap();

        {
            let url: String = String::new() + &server_url + "/Users/authenticatebyname";
            match http_client
                .post(url)
                .header("Content-Type", "text/json")
                .header("Authorization", format!("MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"jellyfin-tui\", Version=\"{}\"", env!("CARGO_PKG_VERSION")))
                .json(&serde_json::json!({
                    "Username": &username,
                    "Pw": &password,
                }))
                .send() {
                Ok(response) => {
                    if !response.status().is_success() {
                        println!(" ! Error authenticating: {}", response.status());
                        continue;
                    }
                    let value = match response.json::<serde_json::Value>() {
                        Ok(v) => v,
                        Err(e) => {
                            println!(" ! Error authenticating: {}", e);
                            continue;
                        }
                    };
                    match value["AccessToken"].as_str() {
                        Some(_) => {}
                        None => {
                            println!(" ! Error authenticating: No access token received");
                            continue;
                        }
                    }
                    match value["ServerId"].as_str() {
                        Some(id) => {
                            server_id = id.to_string();
                        }
                        None => {
                            println!(" ! Error authenticating: No server ID received");
                            continue;
                        }
                    }
                }
                Err(e) => {
                    println!(" ! Error authenticating: {}", e);
                    continue;
                }
            }
        }

        match Confirm::with_theme(&DialogTheme::default())
            .with_prompt(format!("Success! Use server '{}' ({}) Username: '{}'?", server_name.trim(), server_url.trim(), username.trim()))
            .default(true)
            .wait_for_newline(true)
            .interact_opt()
            .unwrap()
        {
            Some(true) => {
                ok = true;
            }
            _ => {
                counter += 1;
                if counter >= 3 {
                    println!(" 𝄆 I believe in you! You can do it! 𝄆");
                } else {
                    println!(" ! Let's try again.\n");
                }
            }
        }
    }

    let default_config = serde_yaml::to_string(&serde_json::json!({
        "servers": [
            {
                "name": server_name.trim(),
                "url": server_url.trim(),
                "username": username.trim(),
                "password": password.trim(),
            }
        ],
    })).expect(" ! Could not serialize default configuration");

    // TODO: make sure these are first deleted
    match std::fs::create_dir_all(config_dir.join("jellyfin-tui")) {
        Ok(_) => {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&config_file)
                .expect(" ! Could not create config file");
            file.write_all(default_config.as_bytes())
                .expect(" ! Could not write default config");

            println!(
                "\n - Created default config file at: {}",
                config_file
                    .to_str()
                    .expect(" ! Could not convert config path to string")
            );
        }
        Err(_) => {
            println!(" ! Could not create config directory");
            std::process::exit(1);
        }
    }
}

/// This is called after a successful connection.
/// Writes a mapping of (Server from config.yaml) -> (ServerId from Jellyfin) to a file.
/// This is later used to show the server name when choosing an offline database.
pub fn write_selected_server(selected_server: &SelectedServer, server_id: &str, config: &serde_yaml::Value) -> Result<(), Box<dyn std::error::Error>> {
    let cache_dir = cache_dir().ok_or("Could not find cache directory")?.join("jellyfin-tui");
    let mapping_file = cache_dir.join("server_map.json");

    std::fs::create_dir_all(&cache_dir)?;

    let mut map: HashMap<String, String> = if mapping_file.exists() {
        let content = std::fs::read_to_string(&mapping_file)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    };

    map.insert(selected_server.url.clone(), server_id.to_string());

    // remove servers not in the config file anymore
    if let Some(servers) = config["servers"].as_sequence() {
        let server_urls: Vec<String> = servers.iter()
            .filter_map(|s| s.get("url").and_then(|v| v.as_str()).map(String::from))
            .collect();
        map.retain(|url, _| server_urls.contains(url));
    }

    let json = serde_json::to_string_pretty(&map)?;
    std::fs::write(&mapping_file, json)?;

    Ok(())
}