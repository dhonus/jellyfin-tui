/* --------------------------
HTTP client for Jellyfin API
    - This file contains all HTTP related functions. It defines the Client struct which is used to interact with the Jellyfin API.
    - All the types used in the client are defined at the end of the file.
-------------------------- */

use crate::keyboard::Searchable;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use dirs::config_dir;
use dirs::cache_dir;
use std::io::Write;
use std::path::PathBuf;
use std::io::Cursor;
use std::error::Error;
use chrono::NaiveDate;
use std::fs::File;
use std::io::Read;
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;

#[derive(Debug)]
pub struct Client {
    pub base_url: String,
    http_client: reqwest::Client,
    pub access_token: String,
    user_id: String,
    pub user_name: String,
    pub transcoding: Transcoding,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Credentials {
    #[serde(rename = "Username")]
    username: String,
    #[serde(rename = "Pw")]
    password: String,
}

#[derive(Debug)]
pub struct Transcoding {
    pub enabled: bool,
    pub bitrate: u32,
    pub container: String,
}

impl Client {
    /// Creates a new client with the given base URL
    /// If the configuration file does not exist, it will be created with stdin input
    ///
    pub async fn new(quiet: bool) -> Self {

        let config_dir = match config_dir() {
            Some(dir) => dir,
            None => {
                println!(" ! Could not find config directory");
                std::process::exit(1);
            }
        };

        let config_file = config_dir.join("jellyfin-tui").join("config.yaml");

        if !config_file.exists() {
            let mut server = String::new();
            let mut username = String::new();
            let mut password = String::new();

            println!("");
            println!(" - <3 Thank you for trying out jellyfin-tui! It is still beta-quality software, so please report any issues you find or ideas you have here:");
            println!(" - https://github.com/dhonus/jellyfin-tui/issues");
            println!("\n ! The configuration file does not exist. Please fill in the following details:\n");
            println!("--- Jellyfin TUI Configuration ---");
            println!("The expected format is:");
            println!(" - server: http://localhost:8096");
            println!(" - username: admin");
            println!(" - password: password\n");
            let mut ok = false;
            while !ok {
                while server.is_empty() || !server.contains("http") {
                    println!("server: ");
                    server = "".to_string();
                    std::io::stdin().read_line(&mut server).unwrap();
                    server = server.trim().to_string();
                    if server.ends_with("/") {
                        server.pop();
                    }
                    if server.is_empty() {
                        println!(" ! Host cannot be empty");
                    } else if !server.starts_with("http") {
                        println!(" ! Host must be a valid URL including http or https");
                    }
                }
                println!("username: ");
                std::io::stdin().read_line(&mut username).expect("[XX] Failed to read username");
                println!("password: ");
                std::io::stdin().read_line(&mut password).expect("[XX] Failed to read password");

                println!("\nHost: '{}' Username: '{}' Password: '{}'", server.trim(), username.trim(), password.trim());
                println!(" ? Is this correct? (Y/n)");
                let mut confirm = String::new();
                std::io::stdin().read_line(&mut confirm).expect("[XX] Failed to read confirmation");
                // y is default
                if confirm.contains("n") || confirm.contains("N") {
                    server = "".to_string();
                    username = "".to_string();
                    password = "".to_string();
                } else {
                    ok = true;
                }
            }

            // create the config file
            let default_config = serde_yaml::to_string(&serde_json::json!({
                "server": server.trim(),
                "username": username.trim(),
                "password": password.trim(),
            })).expect(" ! Could not serialize default config");

            match std::fs::create_dir_all(config_dir.join("jellyfin-tui")) {
                Ok(_) => {
                    let mut file = OpenOptions::new()
                        .write(true).create_new(true).mode(0o600)
                        .open(config_file.clone())
                        .expect(" ! Could not create config file");
                    file.write_all(default_config.as_bytes())
                        .expect(" ! Could not write default config");

                    println!("\n - Created default config file at: {}", config_file.to_str().expect(" ! Could not convert config path to string"));
                },
                Err(_) => {
                    println!(" ! Could not create config directory");
                    std::process::exit(1);
                }
            }
        } else if !quiet {
            println!(" - Found config file at: {}", config_file.to_str().expect(" ! Could not convert config path to string"));
        }

        let config = crate::config::get_config();
        if let Err(e) = config {
            println!(" ! Could not get config: {}", e);
            std::process::exit(1);
        }
        let d = config.unwrap();

        let http_client = reqwest::Client::new();
        let _credentials: Credentials = {
            let username = match d["username"].as_str() {
                Some(s) => String::from(s),
                None => {
                    println!(" ! Could not find username in config file");
                    std::process::exit(1);
                }
            };
            let password = match d["password"].as_str() {
                Some(s) => String::from(s),
                None => {
                    println!(" ! Could not find password in config file");
                    std::process::exit(1);
                }
            };
            Credentials {
                username, password
            }
        };

        let server = match d["server"].as_str() {
            Some(s) => s,
            None => {
                println!(" ! Could not find server in config file");
                std::process::exit(1);
            }
        };

        if !quiet {
            println!(" - Using {} as the server.", server);
        }

        let transcoding = Transcoding {
            enabled: d["transcoding"]["enabled"].as_bool().unwrap_or(false),
            bitrate: d["transcoding"]["bitrate"].as_u64().unwrap_or(320) as u32,
            container: d["transcoding"]["container"].as_str().unwrap_or("mp3").to_string(),
        };

        let url: String = String::new() + server + "/Users/authenticatebyname";
        let response = http_client
            .post(url)
            .header("Content-Type", "text/json")
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .json(&serde_json::json!({
                "Username": _credentials.username,
                "Pw": _credentials.password,
            }))
            .send()
            .await;

        // TODO: some offline state handling. Implement when adding offline caching
        match response {
            Ok(json) => {
                let value = match json.json::<Value>().await {
                    Ok(v) => v,
                    Err(e) => {
                        println!(" ! Error authenticating: {}", e);
                        std::process::exit(1);
                    }
                };
                let access_token = value["AccessToken"].as_str().unwrap_or_else(|| {
                    println!(" ! Could not get access token");
                    std::process::exit(1);
                });
                let user_id = value["User"]["Id"].as_str().unwrap_or_else(|| {
                    println!(" ! Could not get user id");
                    std::process::exit(1);
                });
                Self {
                    base_url: server.to_string(),
                    http_client,
                    access_token: access_token.to_string(),
                    user_id: user_id.to_string(),
                    user_name: _credentials.username.to_string(),
                    transcoding,
                }
            },
            Err(e) => {
                println!(" ! Error authenticating: {}", e);
                std::process::exit(1);
            }
        }
    }

    /// Produces a list of artists, called by the main function before initializing the app
    ///
    pub async fn artists(&self, search_term: String) -> Result<Vec<Artist>, reqwest::Error> {
        let url = format!("{}/Artists", self.base_url);

        let response: Result<reqwest::Response, reqwest::Error> = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .query(&[
                ("SearchTerm", search_term.as_str()),
                ("SortBy", "Name"),
                ("SortOrder", "Ascending"),
                ("Recursive", "true"),
                ("ImageTypeLimit", "-1")
            ])
            .query(&[("StartIndex", "0")])
            .send()
            .await;

        let artists = match response {
            Ok(json) => {
                let artists: Artists = json.json().await.unwrap_or_else(|_| Artists {
                    items: vec![],
                    start_index: 0,
                    total_record_count: 0,
                });
                artists
            },
            Err(_) => {
                return Ok(vec![]);
            }
        };

        Ok(artists.items)
    }

    /// Produces a list of songs by an artist sorted by album and index
    ///
    pub async fn discography(&self, id: &str, recently_added: bool) -> Result<Discography, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "Album"),
                ("SortOrder", "Descending"),
                ("Recursive", "true"),
                ("IncludeItemTypes", "Audio"),
                ("Fields", "Genres, DateCreated, MediaSources, ParentId"),
                ("StartIndex", "0"),
                ("ImageTypeLimit", "1"),
                ("ArtistIds", id)
            ])
            .query(&[("StartIndex", "0")])
            .send()
            .await;

        let discog = match response {
            Ok(json) => {
                let discog: Discography = json.json().await.unwrap_or_else(|_| Discography {
                    items: vec![],
                });

                // group the songs by album
                let mut albums: Vec<DiscographyAlbum> = vec![];
                let mut current_album = DiscographyAlbum { songs: vec![] };
                for song in discog.items {
                    // push songs until we find a different album
                    if current_album.songs.is_empty() {
                        current_album.songs.push(song);
                        continue;
                    }
                    if current_album.songs[0].album_id == song.album_id {
                        current_album.songs.push(song);
                        continue;
                    }
                    albums.push(current_album);
                    current_album = DiscographyAlbum { songs: vec![song] };
                }
                albums.push(current_album);

                // sort the songs within each album by indexnumber
                for album in albums.iter_mut() {
                    album.songs.sort_by(|a, b| a.index_number.cmp(&b.index_number));
                }

                albums.sort_by(|a, b| {
                    // sort albums by release date, if that fails fall back to just the year. Albums with no date will be at the end
                    match (NaiveDate::parse_from_str(&a.songs[0].premiere_date, "%Y-%m-%dT%H:%M:%S.%fZ"), NaiveDate::parse_from_str(&b.songs[0].premiere_date, "%Y-%m-%dT%H:%M:%S.%fZ")) {
                        (Ok(a_date), Ok(b_date)) => b_date.cmp(&a_date),
                        _ => b.songs[0].production_year.cmp(&a.songs[0].production_year),
                    }
                });

                // sort over parent_index_number to separate into separate disks
                for album in albums.iter_mut() {
                    album.songs.sort_by(|a, b| a.parent_index_number.cmp(&b.parent_index_number));
                }

                // now we flatten the albums back into a list of songs
                let mut songs: Vec<DiscographySong> = vec![];
                for (i, album) in albums.iter().enumerate() {
                    if album.songs.is_empty() {
                        continue;
                    }

                    // push a dummy song with the album name
                    let mut album_song = album.songs[0].clone();
                    // let name be Artist - Album - Year
                    album_song.name = format!("{} ({})", album.songs[0].album, album.songs[0].production_year);
                    album_song.id = format!("_album_{}", i);
                    album_song.album_artists = album.songs[0].album_artists.clone();
                    album_song.album_id = "".to_string();
                    album_song.album_artists = vec![];
                    album_song.run_time_ticks = 0;
                    for song in album.songs.iter() {
                        album_song.run_time_ticks += song.run_time_ticks;
                    }
                    songs.push(album_song);

                    for song in album.songs.iter() {
                        songs.push(song.clone());
                    }
                }

                // now we've seen this artist, so let's mark it in the cache
                let cache_dir = match cache_dir() {
                    Some(dir) => dir,
                    None => {
                        return Ok(Discography { items: songs });
                    }
                };

                if !recently_added {
                    return Ok(Discography { items: songs });
                }

                // first check if it's not already in the file
                let seen_artists_file = cache_dir.join("jellyfin-tui").join("seen_artists");
                if seen_artists_file.exists() {
                    if let Ok(mut file) = File::open(seen_artists_file.clone()) {
                        let mut contents = String::new();
                        if let Err(_e) = file.read_to_string(&mut contents) {
                            return Ok(Discography { items: songs });
                        }
                        if contents.contains(id) {
                            return Ok(Discography { items: songs });
                        }
                    }
                }

                match OpenOptions::new()
                    .write(true)
                    .append(true)
                    .open(cache_dir.join("jellyfin-tui").join("seen_artists"))
                {
                    Ok(mut file) => {
                        if let Err(e) = writeln!(file, "{}", id) {
                            _ = e;
                        }
                    },
                    Err(_) => {
                        return Ok(Discography { items: songs });
                    }
                }

                Discography { items: songs }
            },
            Err(_) => {
                return Ok(Discography { items: vec![] });
            }
        };

        Ok(discog)
    }

    /// This for the search functionality, it will poll albums based on the search term
    ///
    pub async fn search_albums(&self, search_term: String) -> Result<Vec<Album>, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "SortName"),
                ("SortOrder", "Ascending"),
                ("searchTerm", search_term.as_str()),
                ("Fields", "PrimaryImageAspectRatio, CanDelete, MediaSourceCount"),
                ("Recursive", "true"),
                ("EnableTotalRecordCount", "false"),
                ("ImageTypeLimit", "1"),
                ("IncludePeople", "false"),
                ("IncludeMedia", "true"),
                ("IncludeGenres", "false"),
                ("IncludeStudios", "false"),
                ("IncludeArtists", "false"),
                ("IncludeItemTypes", "MusicAlbum")
            ])
            .query(&[("StartIndex", "0")])
            .send()
            .await;

        let albums = match response {
            Ok(json) => {
                let albums: SearchAlbums = json.json().await.unwrap_or_else(|_| SearchAlbums {
                    items: vec![],
                });
                albums.items
            },
            Err(_) => {
                return Ok(vec![]);
            }
        };

        Ok(albums)
    }

    /// This for the search functionality, it will poll songs based on the search term
    ///
    pub async fn search_tracks(&self, search_term: String) -> Result<Vec<DiscographySong>, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "Name"),
                ("SortOrder", "Ascending"),
                ("searchTerm", search_term.as_str()),
                ("Fields", "PrimaryImageAspectRatio, CanDelete, MediaSourceCount"),
                ("Recursive", "true"),
                ("EnableTotalRecordCount", "false"),
                ("ImageTypeLimit", "1"),
                ("IncludePeople", "false"),
                ("IncludeMedia", "true"),
                ("IncludeGenres", "false"),
                ("IncludeStudios", "false"),
                ("IncludeArtists", "false"),
                ("IncludeItemTypes", "Audio")
            ])
            .query(&[("StartIndex", "0")])
            .send()
            .await;

        let songs = match response {
            Ok(json) => {
                let songs: Discography = json.json().await.unwrap_or_else(|_| Discography {
                    items: vec![],
                });
                // remove those where album_artists is empty
                let songs: Vec<DiscographySong> = songs.items.into_iter().filter(|s| !s.album_artists.is_empty()).collect();
                songs
            },
            Err(_) => {
                return Ok(vec![]);
            }
        };

        Ok(songs)
    }

    /// Returns a randomized list of tracks based on the preferences
    /// 
    pub async fn random_tracks(&self, tracks_n: usize, only_played: bool, only_unplayed: bool) -> Result<Vec<DiscographySong>, Box<dyn Error>> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "Random"),
                ("StartIndex", "0"),
                ("SortOrder", "Ascending"),
                ("Recursive", "true"),
                ("Fields", "Genres, DateCreated, MediaSources, ParentId"),
                ("IncludeItemTypes", "Audio"),
                ("Recursive", "true"),
                ("EnableTotalRecordCount", "false"),
                ("ImageTypeLimit", "1"),
                ("Limit", &tracks_n.to_string()),
                ("Filters", match (only_played, only_unplayed) {
                    (true, false) => "IsPlayed",
                    (false, true) => "IsUnplayed",
                    _ => "",
                })
            ])
            .query(&[("StartIndex", "0")])
            .send()
            .await;

        let songs = match response {
            Ok(json) => {
                let songs: Discography = json.json().await.unwrap_or_else(|_| Discography {
                    items: vec![],
                });
                // remove those where album_artists is empty
                let songs: Vec<DiscographySong> = songs.items.into_iter().filter(|s| !s.album_artists.is_empty()).collect();
                songs
            },
            Err(_) => {
                return Ok(vec![]);
            }
        };

        Ok(songs)
    }

    /// Returns a list of artists with recently added albums
    /// 
    pub async fn new_artists(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let url = format!("{}/Artists", self.base_url);

        let response: Result<reqwest::Response, reqwest::Error> = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "DateCreated"),
                ("SortOrder", "Descending"),
                ("Recursive", "true"),
                ("ImageTypeLimit", "-1")
            ])
            .query(&[("StartIndex", "0")])
            .query(&[("Limit", "50")])
            .send()
            .await;

        let artists = match response {
            Ok(json) => {
                let artists: Artists = json.json().await.unwrap_or_else(|_| Artists {
                    items: vec![],
                    start_index: 0,
                    total_record_count: 0,
                });
                artists
            },
            Err(_) => {
                return Ok(vec![]);
            }
        };

        // we will have a file in the cache directory with artists that are new,but we have already seen them
        let cache_dir = match cache_dir() {
            Some(dir) => dir,
            None => {
                return Ok(vec![]);
            }
        };

        // The process is as follows:
        // 1. We get a list of artists that have had albums added recently (var artists)
        // 2. We read the file with the artists we have seen (var seen_artists)
        // 3. If we've seen this artist, we're fine
        // 4. The length of the newly added will be 50. If we go over this, it won't have an artist that we've seen before and we can REMOVE it from the file
        // 5. The next time the artist has something new, we will see it again and write it back to the file

        let mut new_artists: Vec<String> = vec![];
        let seen_artists: Vec<String>;
        // store it as IDs on each line
        let seen_artists_file = cache_dir.join("jellyfin-tui").join("seen_artists");

        // if new we just throw everything in, makes no sense initially
        if !seen_artists_file.exists() {
            let _ = File::create(&seen_artists_file);
            // write all the artists to the file
            let mut file = OpenOptions::new().append(true).open(&seen_artists_file)?;
            for artist in artists.items.iter() {
                writeln!(file, "{}", artist.id)?;
            }
            return Ok(vec![]);
        }

        if seen_artists_file.exists() {
            { // read the file
                let mut file = File::open(&seen_artists_file)?;
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                seen_artists = contents.lines().map(|s| s.to_string()).collect();
            }
            { // wipe it and write the new artists
                let mut file = OpenOptions::new().write(true).open(&seen_artists_file)?;
                for artist in artists.items.iter() {
                    if seen_artists.contains(&artist.id) {
                        continue;
                    }
                    new_artists.push(artist.id.clone());
                    writeln!(file, "{}", artist.id)?;
                }
            }
        }

        Ok(new_artists)
    }

    /// Returns a list of lyrics lines for a song
    ///
    pub async fn lyrics(&self, song_id: &String) -> Result<Vec<Lyric>, reqwest::Error> {
        let url = format!("{}/Audio/{}/Lyrics", self.base_url, song_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .send()
            .await;

        match response {
            Ok(_) => {},
            Err(_) => {
                return Ok(vec![]);
            }
        }

        let lyric = match response {
            Ok(json) => {
                let lyrics: Lyrics = json.json().await.unwrap_or_else(|_| Lyrics {
                    metadata: serde_json::Value::Null,
                    lyrics: vec![],
                });
                lyrics
            },
            Err(_) => {
                return Ok(vec![]);
            }
        }.lyrics;

        Ok(lyric)
    }

    /// Returns media info for a song
    ///
    pub async fn metadata(&self, song_id: &String) -> Result<MediaStream, Box<dyn Error>> {
        let url = format!("{}/Users/{}/Items/{}", self.base_url, self.user_id, song_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .send()
            .await?;

        // check if response is ok
        let song: Value = response.json().await?;
        let media_sources: Vec<MediaSource> = serde_json::from_value(song["MediaSources"].clone())?;

        for m in media_sources {
            for ms in m.media_streams {
                if ms.type_ == "Audio" {
                    return Ok(ms);
                }
            }
        }

        Ok(MediaStream {
            codec: "".to_string(),
            bit_rate: 0,
            channels: 0,
            sample_rate: 0,
            type_: "".to_string(),
        })
    }

    /// Downloads cover art for an album and saves it as cover.* in the cache_dir, filename is returned
    ///
    pub async fn download_cover_art(&self, album_id: String) -> Result<String, Box<dyn Error>> {
        let url = format!("{}/Items/{}/Images/Primary?fillHeight=512&fillWidth=512&quality=96&tag=be2a8642e97e2151ef0580fc72f3505a", self.base_url, album_id);
        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .send()
            .await?;

        // we need to get the file extension
        let content_type = match response.headers().get("Content-Type") {
            Some(c) => c.to_str()?,
            None => "",
        };
        let extension = match content_type {
            "image/png" => "png",
            "image/jpeg" => "jpeg",
            "image/jpg" => "jpg",
            "image/webp" => "webp",
            _ => "png",
        };

        let cache_dir = match cache_dir() {
            Some(dir) => dir,
            None => {
                PathBuf::from("./")
            }
        };

        if !cache_dir.join("jellyfin-tui").exists() {
            std::fs::create_dir_all(cache_dir.join("jellyfin-tui"))?;
            std::fs::create_dir_all(cache_dir.join("jellyfin-tui").join("covers"))?;
        } else {
            // TODO: maybe cache these images?
            let files = std::fs::read_dir(cache_dir.join("jellyfin-tui").join("covers"))?;
            for file in files {
                let file = file?;
                std::fs::remove_file(file.path())?;
            }
        }

        let mut file = std::fs::File::create(
            cache_dir
            .join("jellyfin-tui")
            .join("covers")
            .join(album_id.to_string() + "." + extension)
        )?;
        let mut content =  Cursor::new(response.bytes().await?);
        std::io::copy(&mut content, &mut file)?;

        Ok(album_id.to_string() + "." + extension)
    }

    /// Produces URL of a song from its ID
    pub fn song_url_sync(&self, song_id: String) -> String {
        let mut url = format!("{}/Audio/{}/universal", self.base_url, song_id);
        url += &format!("?UserId={}&api_key={}&StartTimeTicks=0&EnableRedirection=true&EnableRemoteMedia=false", self.user_id, self.access_token);
        url += &format!("&container=opus,webm|opus,mp3,aac,m4a|aac,m4b|aac,flac,webma,webm|webma,wav,ogg");

        if self.transcoding.enabled {
            url += &format!("&transcodingContainer={}&transcodingProtocol=http&audioCodec={}", self.transcoding.container, self.transcoding.container);
            if self.transcoding.bitrate > 0 {
                url += &format!("&maxStreamingBitrate={}", self.transcoding.bitrate * 1000);
            } else {
                url += &format!("&MaxStreamingBitrate=320000");
            }
        }
        url
    }

    /// Sends an update to favorite of a track. POST is true, DELETE is false
    ///
    pub async fn set_favorite(&self, song_id: &String, favorite: bool) -> Result<(), reqwest::Error> {
        let url = format!("{}/Users/{}/FavoriteItems/{}", self.base_url, self.user_id, song_id);
        let response = if favorite {
            self.http_client
                .post(url)
                .header("X-MediaBrowser-Token", self.access_token.to_string())
                .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
                .header("Content-Type", "application/json")
                .send()
                .await
        } else {
            self.http_client
                .delete(url)
                .header("X-MediaBrowser-Token", self.access_token.to_string())
                .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
                .header("Content-Type", "application/json")
                .send()
                .await
        };

        match response {
            Ok(_) => {},
            Err(_) => {
                return Ok(());
            }
        }

        Ok(())
    }

    /// Produces a list of all playlists
    /// 
    pub async fn playlists(&self, search_term: String) -> Result<Vec<Playlist>, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);
        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "Name"),
                ("SortOrder", "Ascending"),
                ("SearchTerm", search_term.as_str()),
                ("Fields", "ChildCount, Genres, DateCreated, ParentId, Overview"),
                ("IncludeItemTypes", "Playlist"),
                ("Recursive", "true"),
                ("StartIndex", "0")
            ])
            .send()
            .await;

        let playlists = match response {
            Ok(json) => {
                let playlists: Playlists = json.json().await.unwrap_or_else(|_| Playlists {
                    items: vec![],
                });
                playlists.items
            },
            Err(_) => {
                return Ok(vec![]);
            }
        };

        Ok(playlists)

    }

    /// Gets a single playlist
    /// 
    /// https://jelly.danielhonus.com/playlists/636d3c3e246dc4f24718480d4316ef2d/items?Fields=Genres%2C%20DateCreated%2C%20MediaSources%2C%20UserData%2C%20ParentId&IncludeItemTypes=Audio&Limit=300&SortOrder=Ascending&StartIndex=0&UserId=aca06460269248d5bbe12e5ae7ceac8b
    pub async fn playlist(&self, playlist_id: &String) -> Result<Discography, reqwest::Error> {
        let url = format!("{}/Playlists/{}/Items", self.base_url, playlist_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .query(&[
                ("Fields", "Genres, DateCreated, MediaSources, UserData, ParentId"),
                ("IncludeItemTypes", "Audio"),
                ("Limit", "300"),
                ("SortOrder", "Ascending"),
                ("SortBy", "IndexNumber"),
                ("StartIndex", "0"),
                ("UserId", self.user_id.as_str())
            ])
            .send()
            .await;

        let playlist = match response {
            Ok(json) => {
                let playlist: Discography = json.json().await.unwrap_or_else(|_| Discography {
                    items: vec![],
                });
                playlist
            },
            Err(_) => {
                return Ok(Discography { items: vec![] });
            }
        };

        Ok(playlist)
    }

    /// Creates a new playlist on the server
    /// 
    /// We can pass Ids[] to add songs to the playlist as well! Todo
    pub async fn create_playlist(&self, playlist_name: &String, is_public: bool) -> Result<String, reqwest::Error> {
        let url = format!("{}/Playlists", self.base_url);

        let response = self.http_client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "Ids": [],
                "Name": playlist_name,
                "IsPublic": is_public,
                "UserId": self.user_id
            }))
            .send()
            .await;

        let playlist_id = response?.json::<serde_json::Value>().await?["Id"].as_str().unwrap_or("").to_string();
        Ok(playlist_id)
    }

    /// Deletes a playlist on the server
    /// 
    pub async fn delete_playlist(&self, playlist_id: &String) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/Items/{}", self.base_url, playlist_id);

        self.http_client
            .delete(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .send()
            .await
    }

    /// Updates a playlist on the server by sending the full definition
    /// 
    pub async fn update_playlist(&self, playlist: &Playlist) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/Items/{}", self.base_url, playlist.id);

        // i do this because my Playlist struct is not the full playlist and i don't want to lose data :)
        // so GET -> modify -> POST
        let response = self.http_client
        .get(url.clone())
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .send()
            .await;

        let mut full_playlist = response?.json::<serde_json::Value>().await?;
        // so far we only have rename
        full_playlist["Name"] = serde_json::Value::String(playlist.name.clone());

        self.http_client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .json(&full_playlist)
            .send()
            .await
    }

    /// Adds a track to a playlist
    /// 
    /// https://jelly.danielhonus.com/Playlists/60efcb22e97a01f2b2a59f4d7b4a48ee/Items?ids=818923889708a83351a8a381af78310b&userId=aca06460269248d5bbe12e5ae7ceac8b
    pub async fn add_to_playlist(&self, track_id: &String, playlist_id: &String) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/Playlists/{}/Items", self.base_url, playlist_id);

        self.http_client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .query(&[
                ("ids", track_id.as_str()),
                ("userId", self.user_id.as_str())
            ])
            .send()
            .await
    }

    /// Removes a track from a playlist
    ///
    pub async fn remove_from_playlist(&self, track_id: &String, playlist_id: &String) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/Playlists/{}/Items", self.base_url, playlist_id);

        self.http_client
            .delete(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .query(&[
                ("EntryIds", track_id)
            ])
            .send()
            .await
    }

    /// Returns a list of all server tasks
    /// 
    pub async fn scheduled_tasks(&self) -> Result<Vec<ScheduledTask>, reqwest::Error> {
        let url = format!("{}/ScheduledTasks", self.base_url);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .query(&[
                ("isHidden", "false")
            ])
            .send()
            .await;

        let tasks = match response {
            Ok(json) => {
                let tasks: Vec<ScheduledTask> = json.json().await.unwrap_or_else(|_| vec![]);
                tasks
            },
            Err(_) => {
                return Ok(vec![]);
            }
        };

        Ok(tasks)
    }

    /// Runs a scheduled task
    /// 
    pub async fn run_scheduled_task(&self, task_id: &String) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/ScheduledTasks/Running/{}", self.base_url, task_id);

        self.http_client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .send()
            .await
    }

    /// Sends a 'playing' event to the server
    ///
    pub async fn playing(&self, song_id: &String) -> Result<(), reqwest::Error> {
        let url = format!("{}/Sessions/Playing", self.base_url);
        let _response = self.http_client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "ItemId": song_id,
                "PositionTicks": 0
            }))
            .send()
            .await;

        Ok(())
    }

    /// Sends a 'stopped' event to the server. Needed for scrobbling
    ///
    pub async fn stopped(&self, song_id: &String, position_ticks: u64) -> Result<(), reqwest::Error> {
        let url = format!("{}/Sessions/Playing/Stopped", self.base_url);
        let _response = self.http_client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "ItemId": song_id,
                "PositionTicks": position_ticks
            }))
            .send()
            .await;

        Ok(())
    }
}

/// Reports progress to the server using the info we have from mpv
/// 
pub async fn report_progress(base_url: String, access_token: String, pr: ProgressReport) -> Result<(), reqwest::Error> {
    let url = format!("{}/Sessions/Playing/Progress", base_url);
    // new http client, this is a pure function so we can create a new one
    let client = reqwest::Client::new();
    let _response = client
        .post(url)
        .header("X-MediaBrowser-Token", access_token.to_string())
        .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "VolumeLevel": pr.volume_level,
            "IsMuted": false,
            "IsPaused": pr.is_paused,
            "ShuffleMode": "Sorted",
            "PositionTicks": pr.position_ticks,
            // "PlaybackStartTimeTicks": pr.playback_start_time_ticks,
            "PlaybackRate": 1,
            "SecondarySubtitleStreamIndex": -1,
            // "BufferedRanges": [{"start": 0, "end": 1457709999.9999998}],
            "MediaSourceId": pr.media_source_id,
            "CanSeek": pr.can_seek,
            "ItemId": pr.item_id,
            "EventName": "timeupdate"
        }))
        .send()
        .await;

        Ok(())
}

/// TYPES ///
///
/// All the jellyfin types will be defined here. These types will be used to interact with the jellyfin server.

#[derive(Debug, Serialize, Deserialize)]
pub struct Artists {
    #[serde(rename = "Items")]
    items: Vec<Artist>,
    #[serde(rename = "StartIndex")]
    start_index: u64,
    #[serde(rename = "TotalRecordCount")]
    total_record_count: u64,
}
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Artist {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Id", default)]
    pub id: String,
    #[serde(rename = "RunTimeTicks", default)]
    run_time_ticks: u64,
    #[serde(rename = "Type", default)]
    type_: String,
    #[serde(rename = "UserData", default)]
    pub user_data: UserData,
    #[serde(rename = "ImageTags", default)]
    image_tags: serde_json::Value,
    #[serde(rename = "ImageBlurHashes", default)]
    image_blur_hashes: serde_json::Value,
    #[serde(rename = "LocationType", default)]
    location_type: String,
    #[serde(rename = "MediaType", default)]
    media_type: String,
    // our own fields
    #[serde(rename = "JellyfinTuiRecentlyAdded", default)]
    pub jellyfintui_recently_added: bool,
}

impl Searchable for Artist {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
}
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UserData {
    #[serde(rename = "PlaybackPositionTicks", default)]
    playback_position_ticks: u64,
    #[serde(rename = "PlayCount", default)]
    play_count: u64,
    #[serde(rename = "IsFavorite", default)]
    pub is_favorite: bool,
    #[serde(rename = "Played", default)]
    played: bool,
    #[serde(rename = "Key", default)]
    key: String,
}

/// DISCOGRAPHY
///
/// The goal here is to mimic behavior of CMUS and get the whole discography of an artist.
/// We query jellyfin for all songs by an artist sorted by album and sort name.
/// Later we group them nicely by album.

#[derive(Debug, Serialize, Deserialize)]
pub struct Discography {
    #[serde(rename = "Items")]
    pub items: Vec<DiscographySong>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscographyAlbum {
    songs: Vec<DiscographySong>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DiscographySongUserData {
    #[serde(rename = "PlaybackPositionTicks", default)]
    playback_position_ticks: u64,
    #[serde(rename = "PlayCount", default)]
    pub play_count: u64,
    #[serde(rename = "IsFavorite", default)]
    pub is_favorite: bool,
    #[serde(rename = "Played", default)]
    played: bool,
    #[serde(rename = "Key", default)]
    key: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiscographySong {
    #[serde(rename = "Album", default)]
    pub album: String,
    #[serde(rename = "AlbumArtist", default)]
    pub album_artist: String,
    #[serde(rename = "AlbumArtists", default)]
    pub album_artists: Vec<Artist>,
    #[serde(rename = "AlbumId", default)]
    pub album_id: String,
    // #[serde(rename = "AlbumPrimaryImageTag")]
    // album_primary_image_tag: String,
    #[serde(rename = "ArtistItems", default)]
    pub artist_items: Vec<Artist>,
    #[serde(rename = "Artists", default)]
    artists: Vec<String>,
    #[serde(rename = "BackdropImageTags", default)]
    backdrop_image_tags: Vec<String>,
    #[serde(rename = "ChannelId", default)]
    channel_id: Option<String>,
    #[serde(rename = "DateCreated", default)]
    date_created: String,
    // #[serde(rename = "GenreItems")]
    // genre_items: Vec<Genre>,
    #[serde(rename = "Genres", default)]
    genres: Vec<String>,
    #[serde(rename = "HasLyrics", default)]
    pub has_lyrics: bool,
    #[serde(rename = "Id", default)]
    pub id: String,
    #[serde(rename = "PlaylistItemId", default)]
    pub playlist_item_id: String,
    // #[serde(rename = "ImageBlurHashes")]
    // image_blur_hashes: ImageBlurHashes,
    // #[serde(rename = "ImageTags")]
    // image_tags: ImageTags,
    #[serde(rename = "IndexNumber", default = "index_default")]
    pub index_number: u64,
    #[serde(rename = "IsFolder", default)]
    is_folder: bool,
    // #[serde(rename = "LocationType")]
    // location_type: String,
    #[serde(rename = "MediaSources", default)]
    media_sources: Vec<MediaSource>,
    #[serde(rename = "MediaType", default)]
    media_type: String,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "NormalizationGain", default)]
    normalization_gain: f64,
    // #[serde(rename = "ParentBackdropImageTags")]
    // parent_backdrop_image_tags: Vec<String>,
    // #[serde(rename = "ParentBackdropItemId")]
    // parent_backdrop_item_id: String,
    #[serde(rename = "ParentId", default)]
    pub parent_id: String,
    #[serde(rename = "ParentIndexNumber", default = "index_default")]
    pub parent_index_number: u64,
    #[serde(rename = "PremiereDate", default)]
    premiere_date: String,
    #[serde(rename = "ProductionYear", default)]
    pub production_year: u64,
    #[serde(rename = "RunTimeTicks", default)]
    pub run_time_ticks: u64,
    #[serde(rename = "ServerId", default)]
    server_id: String,
    // #[serde(rename = "Type")]
    // type_: String,
    #[serde(rename = "UserData", default)]
    pub user_data: DiscographySongUserData,
}

impl Searchable for DiscographySong {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
}

fn index_default() -> u64 {
    1
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MediaSource {
    #[serde(rename = "Container", default)]
    container: String,
    #[serde(rename = "Size", default)]
    size: u64,
    #[serde(rename = "MediaStreams", default)]
    media_streams: Vec<MediaStream>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MediaStream {
    #[serde(rename = "Codec", default)]
    pub codec: String,
    #[serde(rename = "BitRate", default)]
    pub bit_rate: u64,
    #[serde(rename = "Channels", default)]
    pub channels: u64,
    #[serde(rename = "SampleRate", default)]
    pub sample_rate: u64,
    #[serde(rename = "Type", default)]
    type_: String,
}

/// Lyrics
/*
{
    "Metadata": {},
    "Lyrics": [
        {
            "Text": "Inside you\u0027re pretending"
            "Start": 220300000,
        },
        {
            "Text": "Crimes have been swept aside"
            "Start": 225000000,
        },
    ]
}
*/

#[derive(Debug, Serialize, Deserialize)]
pub struct Lyrics {
    #[serde(rename = "Metadata", default)]
    metadata: serde_json::Value,
    #[serde(rename = "Lyrics", default)]
    lyrics: Vec<Lyric>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Lyric {
    #[serde(rename = "Text", default)]
    pub text: String,
    #[serde(rename = "Start", default)]
    pub start: u64,
}

/// {"VolumeLevel":94,"IsMuted":true,"IsPaused":false,"RepeatMode":"RepeatNone","ShuffleMode":"Sorted","MaxStreamingBitrate":4203311,"PositionTicks":31637660,"PlaybackStartTimeTicks":17171041814570000,"PlaybackRate":1,"SecondarySubtitleStreamIndex":-1,"BufferedRanges":[{"start":0,"end":1457709999.9999998}],"PlayMethod":"Transcode","PlaySessionId":"1717104167942","PlaylistItemId":"playlistItem0","MediaSourceId":"77fb3ec1b0c2a027c2651771c7268e79","CanSeek":true,"ItemId":"77fb3ec1b0c2a027c2651771c7268e79","EventName":"timeupdate"}
#[derive(Debug, Serialize, Deserialize)]
pub struct ProgressReport {
    #[serde(rename = "VolumeLevel")]
    pub volume_level: u64,
    // #[serde(rename = "IsMuted")]
    // is_muted: bool,
    #[serde(rename = "IsPaused")]
    pub is_paused: bool,
    // #[serde(rename = "RepeatMode")]
    // repeat_mode: String,
    // #[serde(rename = "ShuffleMode")]
    // shuffle_mode: String,
    // #[serde(rename = "MaxStreamingBitrate")]
    // max_streaming_bitrate: u64,
    #[serde(rename = "PositionTicks")]
    pub position_ticks: u64,
    #[serde(rename = "PlaybackStartTimeTicks")]
    pub playback_start_time_ticks: u64,
    // #[serde(rename = "PlaybackRate")]
    // playback_rate: u64,
    // #[serde(rename = "SecondarySubtitleStreamIndex")]
    // secondary_subtitle_stream_index: i64,
    // #[serde(rename = "PlayMethod")]
    // play_method: String,
    // #[serde(rename = "PlaySessionId")]
    // pub play_session_id: String,
    // #[serde(rename = "PlaylistItemId")]
    // pub playlist_item_id: String,
    #[serde(rename = "MediaSourceId")]
    pub media_source_id: String,
    #[serde(rename = "CanSeek")]
    pub can_seek: bool,
    #[serde(rename = "ItemId")]
    pub item_id: String,
    #[serde(rename = "EventName")]
    pub event_name: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchAlbums {
    #[serde(rename = "Items", default)]
    pub items: Vec<Album>,
}

#[derive(Debug, Deserialize)]
pub struct Album {
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Id",default )]
    pub id: String,
    #[serde(rename = "AlbumArtists")]
    pub album_artists: Vec<Artist>,
}

impl Searchable for Album {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Deserialize)]
pub struct Playlists {
    #[serde(rename = "Items")]
    pub items: Vec<Playlist>,
    // #[serde(rename = "TotalRecordCount")]
    // pub total_record_count: u64,
    // #[serde(rename = "StartIndex")]
    // pub start_index: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Playlist {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "ServerId")]
    pub server_id: String,
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "DateCreated")]
    pub date_created: String,
    #[serde(rename = "ChannelId")]
    pub channel_id: Option<String>,
    #[serde(rename = "Genres")]
    pub genres: Vec<String>,
    #[serde(rename = "RunTimeTicks")]
    pub run_time_ticks: u64,
    #[serde(rename = "IsFolder")]
    pub is_folder: bool,
    #[serde(rename = "ParentId")]
    pub parent_id: String,
    #[serde(rename = "Type")]
    pub type_: String,
    #[serde(rename = "UserData")]
    pub user_data: UserData,
    #[serde(rename = "ChildCount")]
    pub child_count: u64,
    #[serde(rename = "LocationType")]
    pub location_type: String,
}

impl Searchable for Playlist {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScheduledTask {
    #[serde(rename = "Name")]
    pub name: String,
    // #[serde(rename = "State")]
    // pub state: String,
    #[serde(rename = "Id")]
    pub id: String,
    // #[serde(rename = "LastExecutionResult")]
    // pub last_execution_result: LastExecutionResult,
    // #[serde(rename = "Triggers")]
    // pub triggers: Vec<Trigger>,
    #[serde(rename = "Description")]
    pub description: String,
    #[serde(rename = "Category")]
    pub category: String,
    // #[serde(rename = "IsHidden")]
    // pub is_hidden: bool,
    // #[serde(rename = "Key")]
    // pub key: String,
}