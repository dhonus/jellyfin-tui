use crate::client::{AuthMethod, SelectedServer};
use crate::themes::dialoguer::DialogTheme;
use dialoguer::{Confirm, Input, Password};
use dirs::{cache_dir, config_dir, data_dir};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct AuthEntry {
    pub known_urls: Vec<String>,
    pub device_id: String,
    pub access_token: String,
    pub user_id: String,
    pub username: String,
}
// ServerId -> AuthEntry
pub type AuthCache = HashMap<String, AuthEntry>;

#[derive(Debug, Clone, Copy)]
pub enum LyricsVisibility {
    Always,
    Auto,
    Never,
}
impl LyricsVisibility {
    pub fn from_config(val: &str) -> Self {
        match val {
            "auto" => Self::Auto,
            "never" => Self::Never,
            _ => Self::Always,
        }
    }
}

/// This makes sure all dirs are created before we do anything.
/// Also makes unwraps on dirs::data_dir and config_dir safe to do. In theory ;)
pub fn prepare_directories() -> Result<(), Box<dyn std::error::Error>> {
    // these are the system-wide dirs like ~/.cache ~/.local/share and ~/config
    let cache_dir = cache_dir().expect(" ! Failed getting cache directory");
    let data_dir = data_dir().expect(" ! Failed getting data directory");
    let config_dir = config_dir().expect(" ! Failed getting config directory");

    let j_cache_dir = cache_dir.join("jellyfin-tui");
    let j_data_dir = data_dir.join("jellyfin-tui");
    let j_config_dir = config_dir.join("jellyfin-tui");

    std::fs::create_dir_all(&j_data_dir)?;
    std::fs::create_dir_all(&j_config_dir)?;

    // try to move existing files in cache to the data directory
    // it errors if nothing is in cache, so we explicitly ignore that
    // remove this and references to the cache dir at some point!
    match std::fs::rename(&j_cache_dir, &j_data_dir) {
        Ok(_) => (),
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(ref e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
            println!(
                " ! Cache directory is not empty, please remove it manually: {}",
                j_cache_dir.display()
            );
            return Err(Box::new(std::io::Error::new(e.kind(), e.to_string())));
        }
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            if std::fs::metadata(&j_cache_dir).is_ok() == true {
                fs_extra::dir::copy(
                    &j_cache_dir,
                    &j_data_dir,
                    &fs_extra::dir::CopyOptions::new().content_only(true),
                )?;
                std::fs::remove_dir_all(&j_cache_dir)?;
            } else {
                return Ok(());
            }
        }
        Err(e) => return Err(Box::new(e)),
    };

    std::fs::create_dir_all(j_data_dir.join("log"))?;
    std::fs::create_dir_all(j_data_dir.join("covers"))?;
    std::fs::create_dir_all(j_data_dir.join("states"))?;
    std::fs::create_dir_all(j_data_dir.join("downloads"))?;
    std::fs::create_dir_all(j_data_dir.join("databases"))?;

    // deprecated files, remove this at some point!
    let _ = std::fs::remove_file(j_data_dir.join("state.json"));
    let _ = std::fs::remove_file(j_data_dir.join("offline_state.json"));
    let _ = std::fs::remove_file(j_data_dir.join("seen_artists"));
    let _ = std::fs::remove_file(j_data_dir.join("server_map.json"));

    Ok(())
}

pub fn get_config() -> Result<(PathBuf, serde_yaml::Value), Box<dyn std::error::Error>> {
    let config_dir = match config_dir() {
        Some(dir) => dir,
        None => {
            return Err("Could not find config directory".into());
        }
    };

    let config_file: PathBuf = config_dir.join("jellyfin-tui").join("config.yaml").into();

    let f = std::fs::File::open(&config_file)?;
    let d = serde_yaml::from_reader(f)?;

    Ok((config_file, d))
}

pub fn select_server(
    config: &serde_yaml::Value,
    force_server_select: bool,
) -> Option<SelectedServer> {
    let servers = config["servers"]
        .as_sequence()
        .expect(" ! Could not find servers in config file");

    if servers.is_empty() {
        println!(" ! No servers configured in config file");
        std::process::exit(1);
    }

    let server = if servers.len() == 1 {
        &servers[0]
    } else if let Some(default) = servers
        .iter()
        .find(|s| s.get("default").and_then(|v| v.as_bool()).unwrap_or(false))
    {
        if !force_server_select {
            println!(
                " - Server: {} [{}] — use --select-server to switch.",
                default["name"].as_str().unwrap_or("Unnamed"),
                default["url"].as_str().unwrap_or("Unknown")
            );
            default
        } else {
            select_server_interactively(servers)?
        }
    } else {
        select_server_interactively(servers)?
    };

    Some(parse_server(server))
}

fn select_server_interactively(servers: &[serde_yaml::Value]) -> Option<&serde_yaml::Value> {
    let mut names: Vec<String> = servers
        .iter()
        .map(|s| {
            format!(
                "{} ({})",
                s["name"].as_str().unwrap_or("Unnamed"),
                s["url"].as_str().unwrap_or("Unknown")
            )
        })
        .collect();
    names.push("Offline Library".to_string());

    let selection = dialoguer::Select::with_theme(&DialogTheme::default())
        .with_prompt("Which server would you like to use?")
        .items(&names)
        .default(0)
        .interact()
        .unwrap_or(0);

    if selection == names.len() - 1 {
        return None;
    }

    Some(&servers[selection])
}

fn parse_server(server: &serde_yaml::Value) -> SelectedServer {
    let url = match server["url"].as_str() {
        Some(url) if !url.ends_with('/') => url.to_string(),
        Some(_) => {
            println!(" ! Server URL must not end with a trailing slash");
            std::process::exit(1);
        }
        None => {
            println!(" ! Selected server does not have a URL configured");
            std::process::exit(1);
        }
    };

    if let None = server["name"].as_str() {
        println!(" ! Selected server does not have a name configured");
        std::process::exit(1);
    }

    let auth = match server["username"].as_str() {
        Some(username) => {
            let password = match (
                server["password"].as_str(),
                server["password_file"].as_str(),
            ) {
                (None, Some(password_file)) => std::fs::read_to_string(password_file)
                    .unwrap_or_else(|e| {
                        println!(" ! Error reading password file '{}': {}", password_file, e);
                        std::process::exit(1);
                    })
                    .trim_matches(&['\n', '\r'])
                    .to_string(),
                (Some(p), None) => p.to_string(),
                (Some(_), Some(_)) => {
                    println!(
                        " ! Selected server has password and password_file configured, only choose one"
                    );
                    std::process::exit(1);
                }
                (None, None) => {
                    println!(" ! Selected server does not have a password configured");
                    std::process::exit(1);
                }
            };

            AuthMethod::UserPass {
                username: username.to_string(),
                password,
            }
        }
        None => {
            if server["quick_connect"].as_bool().unwrap_or(false) {
                AuthMethod::QuickConnect
            } else {
                println!(" ! Selected server does not have a username configured");
                std::process::exit(1);
            }
        }
    };

    SelectedServer { url, auth }
}

enum OnboardingAuth {
    UserPass,
    QuickConnect,
}
pub fn initialize_config() {
    let config_dir = match config_dir() {
        Some(dir) => dir,
        None => {
            println!(" ! Could not find config directory");
            std::process::exit(1);
        }
    };

    let config_file = config_dir.join("jellyfin-tui").join("config.yaml");

    let mut updating = false;
    if config_file.exists() {
        // the config file changed this version. Let's check for a servers array, if it doesn't exist we do the following
        // 1. rename old config
        // 2. run the rest of this function to create a new config file and tell the user about it
        if let Ok(content) = std::fs::read_to_string(&config_file) {
            if !content.contains("servers:") && content.contains("server:") {
                updating = true;
                let old_config_file = config_file.with_extension("_old");
                std::fs::rename(&config_file, &old_config_file)
                    .expect(" ! Could not rename old config file");
                println!(
                    " ! Your config file is outdated and has been backed up to: config_old.yaml"
                );
                println!(" ! A new config will now be created. Please go through the setup again.");
                println!(" ! This is done to support the new offline mode and multiple servers.\n");
            }
        }
        if !updating {
            println!(" - Config loaded: {}", config_file.display());
            return;
        }
    }

    let mut auth_method = OnboardingAuth::UserPass;
    let mut server_name = String::new();
    let mut server_url = String::new();
    let mut username = String::new();
    let mut password = String::new();

    println!(" - Thank you for trying jellyfin-tui! <3\n");
    println!(" - If you encounter issues or missing features, please report them here:");
    println!(" - https://github.com/dhonus/jellyfin-tui/issues\n");
    println!(" ! Configuration file not found. Please enter the following details:\n");

    let http_client = reqwest::blocking::Client::new();

    let mut ok = false;
    let mut counter = 0;
    while !ok {
        server_url = Input::with_theme(&DialogTheme::default())
            .with_prompt("Server URL")
            .with_initial_text("https://")
            .validate_with({
                move |input: &String| -> Result<(), &str> {
                    if input.starts_with("http://")
                        || input.starts_with("https://")
                            && input != "http://"
                            && input != "https://"
                    {
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

        let auth_choice = dialoguer::Select::with_theme(&DialogTheme::default())
            .with_prompt("How would you like to authenticate?")
            .items(&[
                "Username & password",
                "Quick Connect (authorize from another device)",
            ])
            .default(0)
            .interact()
            .unwrap();

        auth_method = match auth_choice {
            0 => OnboardingAuth::UserPass,
            _ => OnboardingAuth::QuickConnect,
        };

        match auth_method {
            OnboardingAuth::UserPass => {
                username = Input::with_theme(&DialogTheme::default())
                    .with_prompt("Username")
                    .interact_text()
                    .unwrap();

                password = Password::with_theme(&DialogTheme::default())
                    .allow_empty_password(true)
                    .with_prompt("Password")
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
                                println!(" ! Connection failed: {}", response.status());
                                continue;
                            }
                            let value = match response.json::<serde_json::Value>() {
                                Ok(v) => v,
                                Err(e) => {
                                    println!(" ! Error authenticating: {}", e);
                                    continue;
                                }
                            };
                            if value["AccessToken"].is_null() {
                                println!(" ! Error authenticating: No access token received");
                                continue;
                            }
                            if value["ServerId"].is_null() {
                                println!(" ! Error authenticating: No server ID received");
                                continue;
                            }
                        }
                        Err(e) => {
                            println!(" ! Error authenticating: {}", e);
                            continue;
                        }
                    }
                }
            }
            OnboardingAuth::QuickConnect => {
                username.clear();
                password.clear();
                println!(" - Quick Connect selected.");
                println!(" - You will authorize this device later from another Jellyfin client.");
            }
        }

        let confirm_prompt = match auth_method {
            OnboardingAuth::UserPass => format!(
                "Success! Use server '{}' ({}) as user '{}'?",
                server_name.trim(),
                server_url.trim(),
                username.trim(),
            ),
            OnboardingAuth::QuickConnect => format!(
                "Use server '{}' ({}) with Quick Connect?",
                server_name.trim(),
                server_url.trim(),
            ),
        };

        match Confirm::with_theme(&DialogTheme::default())
            .with_prompt(&confirm_prompt)
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

    let server_entry = match auth_method {
        OnboardingAuth::UserPass => serde_json::json!({
            "name": server_name.trim(),
            "url": server_url.trim(),
            "username": username.trim(),
            "password": password.trim(),
        }),
        OnboardingAuth::QuickConnect => serde_json::json!({
            "name": server_name.trim(),
            "url": server_url.trim(),
            "quick_connect": true,
        }),
    };

    let default_config = serde_yaml::to_string(&serde_json::json!({
        "servers": [ server_entry ]
    }))
    .expect(" ! Could not serialize default configuration");

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&config_file)
        .expect(" ! Could not create config file");
    file.write_all(default_config.as_bytes())
        .expect(" ! Could not write default config");

    println!(
        " - Created default config file at: {}",
        config_file
            .to_str()
            .expect(" ! Could not convert config path to string.")
    );
}

pub fn load_auth_cache() -> Result<AuthCache, Box<dyn std::error::Error>> {
    let path = dirs::data_dir()
        .unwrap()
        .join("jellyfin-tui")
        .join("auth_cache.json");
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(path)?;
    let cache: AuthCache = serde_json::from_str(&content)?;
    Ok(cache)
}

pub fn save_auth_cache(cache: &AuthCache) -> Result<(), Box<dyn std::error::Error>> {
    let path = dirs::data_dir()
        .unwrap()
        .join("jellyfin-tui")
        .join("auth_cache.json");
    let json = serde_json::to_string_pretty(cache)?;

    let mut file = {
        let mut opts = OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        opts.mode(0o600);
        opts.open(&path)?
    };

    file.write_all(json.as_bytes())?;
    Ok(())
}

pub fn find_cached_auth_by_url<'a>(
    cache: &'a AuthCache,
    url: &str,
) -> Option<(&'a String, &'a AuthEntry)> {
    for (server_id, entry) in cache {
        if entry.known_urls.contains(&url.to_string()) {
            return Some((server_id, entry));
        }
    }
    None
}

/// This is called after a successful connection.
/// Writes a mapping of (Server from config.yaml) -> (ServerId from Jellyfin), among other things, to a file.
/// This is later used to show the server name when choosing an offline database.
pub fn update_cache_with_new_auth(
    mut cache: AuthCache,
    selected_server: &SelectedServer,
    client: &crate::client::Client,
) -> AuthCache {
    let server_id = &client.server_id;

    let entry = cache.entry(server_id.clone()).or_insert(AuthEntry {
        known_urls: vec![],
        device_id: client.device_id.clone(),
        access_token: client.access_token.clone(),
        user_id: client.user_id.clone(),
        username: client.user_name.clone(),
    });

    if !entry.known_urls.contains(&selected_server.url) {
        entry.known_urls.push(selected_server.url.clone());
    }

    entry.access_token = client.access_token.clone();
    entry.user_id = client.user_id.clone();
    entry.username = client.user_name.clone();

    cache
}
