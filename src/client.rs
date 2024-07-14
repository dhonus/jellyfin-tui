use reqwest;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_yaml;
use dirs::config_dir;
use std::io::Cursor;
use std::io;
use std::error::Error;
use chrono::NaiveDate;

#[derive(Debug)]
pub struct Client {
    pub base_url: String,
    http_client: reqwest::Client,
    pub access_token: String,
    user_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Credentials {
    #[serde(rename = "Username")]
    username: String,
    #[serde(rename = "Pw")]
    password: String,
}

impl Client {
    /// Creates a new client with the given base URL
    /// If the configuration file does not exist, it will be created with stdin input
    /// 
    pub async fn new() -> Self {

        let config_dir = match config_dir() {
            Some(dir) => dir,
            None => {
                println!("[!!] Could not find config directory");
                std::process::exit(1);
            }
        };

        let config_file = config_dir.join("jellyfin-tui").join("config.yaml");
        
        if !config_file.exists() {
            let mut server = String::new();
            let mut username = String::new();
            let mut password = String::new();

            println!("\n[!!] The configuration file does not exist. Please fill in the following details:");
            println!("--- Jellyfin TUI Configuration ---");
            println!("The expected format is:");
            println!("- server: http://localhost:8096");
            println!("- username: admin");
            println!("- password: password\n");
            let mut ok = false;
            while !ok {
                while server.is_empty() || !server.contains("http") {
                    println!("host: ");
                    io::stdin().read_line(&mut server).unwrap();
                    if server.is_empty() {
                        println!("[!!] Host cannot be empty");
                    } else if !server.contains("http") {
                        println!("[!!] Host must be a valid URL including http or https");
                    }
                    server = "".to_string();
                }
                println!("username: ");
                io::stdin().read_line(&mut username).expect("Failed to read username");
                println!("password: ");
                io::stdin().read_line(&mut password).expect("Failed to read password");

                println!("\nHost: '{}' Username: '{}' Password: '{}'", server.trim(), username.trim(), password.trim());
                println!("[!!] Is this correct? (Y/n)");
                let mut confirm = String::new();
                io::stdin().read_line(&mut confirm).expect("Failed to read confirmation");
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
            })).unwrap();

            match std::fs::create_dir_all(config_dir.join("jellyfin-tui")) {
                Ok(_) => {
                    std::fs::write(config_file.clone(), default_config).expect("[!!] Could not write default config");
                    println!("\n[OK] Created default config file at: {}", config_file.to_str().unwrap());
                },
                Err(_) => {
                    println!("[!!] Could not create config directory");
                    std::process::exit(1);
                }
            }
        } else {
            println!("[OK] Found config file at: {}", config_file.to_str().unwrap());
        }

        let f = std::fs::File::open(config_file).unwrap();
        let d: Value = serde_yaml::from_reader(f).unwrap();

        let http_client = reqwest::Client::new();
        let _credentials: Credentials = {
            let username = match d["username"].as_str() {
                Some(s) => String::from(s),
                None => {
                    println!("[!!] Could not find username in config file");
                    std::process::exit(1);
                }
            };
            let password = match d["password"].as_str() {
                Some(s) => String::from(s),
                None => {
                    println!("[!!] Could not find password in config file");
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
                println!("[!!] Could not find server in config file");
                std::process::exit(1);
            }
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

        match response {
            Ok(json) => {
                let value = match json.json::<Value>().await {
                    Ok(v) => v,
                    Err(e) => {
                        println!("[!!] Error authenticating: {}", e);
                        std::process::exit(1);
                    }
                };
                let access_token = value["AccessToken"].as_str().unwrap_or_else(|| {
                    println!("[!!] Could not get access token");
                    std::process::exit(1);
                });
                let user_id = value["User"]["Id"].as_str().unwrap_or_else(|| {
                    println!("[!!] Could not get user id");
                    std::process::exit(1);
                });
                return Self {
                    base_url: server.to_string(),
                    http_client,
                    access_token: access_token.to_string(),
                    user_id: user_id.to_string(),
                }
            },
            Err(e) => {
                println!("[!!] Error authenticating: {}", e);
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
                ("SortBy", "SortName"),
                ("SortOrder", "Ascending"), 
                ("Recursive", "true"), 
                ("Fields", "SortName"), 
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
    pub async fn discography(&self, id: &str) -> Result<Discography, reqwest::Error> {
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
                    if current_album.songs.len() == 0 {
                        current_album.songs.push(song);
                    } else {
                        if current_album.songs[0].album == song.album {
                            current_album.songs.push(song);
                        } else {
                            albums.push(current_album);
                            current_album = DiscographyAlbum { songs: vec![song] };
                        }
                    }
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

                // now we flatten the albums back into a list of songs
                let mut songs: Vec<DiscographySong> = vec![];
                for album in albums.iter() {
                    for song in album.songs.iter() {
                        songs.push(song.clone());
                    }
                }

                Discography { items: songs }
            },
            Err(_) => {
                return Ok(Discography { items: vec![] });
            }
        };

        return Ok(discog);
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

    /// Returns a list of lyrics lines for a song
    ///
    pub async fn lyrics(&self, song_id: String) -> Result<Vec<String>, reqwest::Error> {
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
        }.lyrics.iter().map(|l| format!(" {}", l.text)).collect();

        return Ok(lyric);
    }

    /// Returns media info for a song
    /// 
    pub async fn metadata(&self, song_id: String) -> Result<MediaStream, Box<dyn Error>> {
        let url = format!("{}/Users/{}/Items/{}", self.base_url, self.user_id, song_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .send()
            .await?;

        // check status without moving
        // let status = response.as_ref().unwrap().status();

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

        return Ok(MediaStream {
            codec: "".to_string(),
            bit_rate: 0,
            channels: 0,
            sample_rate: 0,
            type_: "".to_string(),
        });

        // artists is the json string of all artists

    }

    /// Downloads cover art for an album and saves it as cover.*, filename is returned
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
        // let content_type = response.headers().get("Content-Type").unwrap().to_str().unwrap();
        let content_type = match response.headers().get("Content-Type") {
            Some(c) => c.to_str().unwrap(),
            None => "",
        };
        // if content_type.is_empty() {
        //     return Ok("".to_string());
        // }
        let extension = match content_type {
            "image/png" => "png",
            "image/jpeg" => "jpeg",
            "image/jpg" => "jpg",
            "image/webp" => "webp",
            _ => "png",
        };

        std::fs::create_dir_all("covers")?;

        let mut file = std::fs::File::create("covers/cover.".to_string() + extension)?;
        let mut content =  Cursor::new(response.bytes().await?);
        std::io::copy(&mut content, &mut file)?;

        Ok("cover.".to_string() + extension)
    }

    /// Produces URL of a song from its ID
    pub fn song_url_sync(&self, song_id: String) -> String {
        let url = format!("{}/Audio/{}/universal", self.base_url, song_id);
        let url = url + &format!("?UserId={}&Container=opus,webm|opus,mp3,aac,m4a|aac,m4b|aac,flac,webma,webm|webma,wav,ogg&TranscodingContainer=mp4&TranscodingProtocol=hls&AudioCodec=aac&api_key={}&StartTimeTicks=0&EnableRedirection=true&EnableRemoteMedia=false", self.user_id, self.access_token);
        url
    }
    /// Sends a 'playing' event to the server
    /// 
    pub async fn playing(&self, song_id: String) -> Result<(), reqwest::Error> {
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
    pub async fn stopped(&self, song_id: String, position_ticks: u64) -> Result<(), reqwest::Error> {
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

        // println!("stopped: {:?}", position_ticks);

        Ok(())
    }
}

/// {"VolumeLevel":94,"IsMuted":true,"IsPaused":false,"RepeatMode":"RepeatNone","ShuffleMode":"Sorted","MaxStreamingBitrate":4203311,"PositionTicks":31637660,"PlaybackStartTimeTicks":17171041814570000,"PlaybackRate":1,"SecondarySubtitleStreamIndex":-1,"BufferedRanges":[{"start":0,"end":1457709999.9999998}],"PlayMethod":"Transcode","PlaySessionId":"1717104167942","PlaylistItemId":"playlistItem0","MediaSourceId":"77fb3ec1b0c2a027c2651771c7268e79","CanSeek":true,"ItemId":"77fb3ec1b0c2a027c2651771c7268e79","EventName":"timeupdate"}
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

        // match response {
        //     Ok(_) => {
        //     },
        //     Err(_) => {
        //         return Ok(());
        //     }
        // }

        Ok(())
}

/// TYPES ///
///
/// All the jellyfin types will be defined here. These types will be used to interact with the jellyfin server.

/// ARTIST
/* {
  "Name": "Flam",
  "ServerId": "97a9003303d7461395074680d9046935",
  "Id": "a9b08901ce0884038ef2ab824e4783b5",
  "SortName": "flam",
  "ChannelId": null,
  "RunTimeTicks": 4505260770,
  "Type": "MusicArtist",
  "UserData": {
    "PlaybackPositionTicks": 0,
    "PlayCount": 0,
    "IsFavorite": false,
    "Played": false,
    "Key": "Artist-Musicbrainz-622c87fa-dc5e-45a3-9693-76933d4c6619"
  },
  "ImageTags": {},
  "BackdropImageTags": [],
  "ImageBlurHashes": {},
  "LocationType": "FileSystem",
  "MediaType": "Unknown"
} */
#[derive(Debug, Serialize, Deserialize)]
pub struct Artists {
    #[serde(rename = "Items")]
    items: Vec<Artist>,
    #[serde(rename = "StartIndex")]
    start_index: u64,
    #[serde(rename = "TotalRecordCount")]
    total_record_count: u64,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Artist {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "SortName", default)]
    sort_name: String,
    #[serde(rename = "RunTimeTicks", default)]
    run_time_ticks: u64,
    #[serde(rename = "Type", default)]
    type_: String,
    #[serde(rename = "UserData", default)]
    user_data: UserData,
    #[serde(rename = "ImageTags", default)]
    image_tags: serde_json::Value,
    #[serde(rename = "ImageBlurHashes", default)]
    image_blur_hashes: serde_json::Value,
    #[serde(rename = "LocationType", default)]
    location_type: String,
    #[serde(rename = "MediaType", default)]
    media_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UserData {
    #[serde(rename = "PlaybackPositionTicks")]
    playback_position_ticks: u64,
    #[serde(rename = "PlayCount")]
    play_count: u64,
    #[serde(rename = "IsFavorite")]
    is_favorite: bool,
    #[serde(rename = "Played")]
    played: bool,
    #[serde(rename = "Key")]
    key: String,
}

/// DISCOGRAPHY
///
/// The goal here is to mimic behavior of CMUS and get the whole discography of an artist.
/// We query jellyfin for all songs by an artist sorted by album and sort name.
/// Later we group them nicely by album.

/*
Object {
    "Album": String("Cardan [EP]"),
    "AlbumArtist": String("Agar Agar"),
    "AlbumArtists": Array [
        Object {
            "Id": String("c910b835045265897c9b1e30417937c8"),
            "Name": String("Agar Agar"),
        },
    ],
    "AlbumId": String("e66386bd52e9e13bcd53fefbe4dbfe80"),
    "AlbumPrimaryImageTag": String("728e73b82a9103d8d3bd46615f7c0786"),
    "ArtistItems": Array [
        Object {
            "Id": String("c910b835045265897c9b1e30417937c8"),
            "Name": String("Agar Agar"),
        },
    ],
    "Artists": Array [
        String("Agar Agar"),
    ],
    "BackdropImageTags": Array [],
    "ChannelId": Null,
    "DateCreated": String("2024-03-12T12:41:07.2583951Z"),
    "GenreItems": Array [
        Object {
            "Id": String("5897c94bfe512270b15fa7e6088e94d0"),
            "Name": String("Synthpop"),
        },
    ],
    "Genres": Array [
        String("Synthpop"),
    ],
    "HasLyrics": Bool(true),
    "Id": String("b26c12ffca74316396cb3d366a7f09f5"),
    "ImageBlurHashes": Object {
        "Backdrop": Object {
            "ea9ad04d014bd8317aa784ffb5676eac": String("W797hQ?bf7ofxuWU?b~qxut6t7M|-;xu%Mayj[xu-:j[xuRjRjt7"),
        },
        "Primary": Object {
            "222d9d1264b6994621fe99bb78047348": String("eQG*]WD+VD=|H?CmIoIotlM|Q,n%R*oeozVXjY$$n%WBMds.tRW=ni"),
            "728e73b82a9103d8d3bd46615f7c0786": String("eQG*]WD+VD=|H?CmIoIotlM|Q,n%R*oeozVXjY$$n%WBMds.tRW=ni"),
        },
    },
    "ImageTags": Object {
        "Primary": String("222d9d1264b6994621fe99bb78047348"),
    },
    "IndexNumber": Number(3),
    "IsFolder": Bool(false),
    "LocationType": String("FileSystem"),
    "MediaSources": Array [
        Object {
            "Bitrate": Number(321847),
            "Container": String("mp3"),
            "DefaultAudioStreamIndex": Number(0),
            "ETag": String("23dab11df466604c0b0cade1f8f814da"),
            "Formats": Array [],
            "GenPtsInput": Bool(false),
            "Id": String("b26c12ffca74316396cb3d366a7f09f5"),
            "IgnoreDts": Bool(false),
            "IgnoreIndex": Bool(false),
            "IsInfiniteStream": Bool(false),
            "IsRemote": Bool(false),
            "MediaAttachments": Array [],
            "MediaStreams": Array [
                Object {
                    "AudioSpatialFormat": String("None"),
                    "BitRate": Number(320000),
                    "ChannelLayout": String("stereo"),
                    "Channels": Number(2),
                    "Codec": String("mp3"),
                    "DisplayTitle": String("MP3 - Stereo"),
                    "Index": Number(0),
                    "IsAVC": Bool(false),
                    "IsDefault": Bool(false),
                    "IsExternal": Bool(false),
                    "IsForced": Bool(false),
                    "IsHearingImpaired": Bool(false),
                    "IsInterlaced": Bool(false),
                    "IsTextSubtitleStream": Bool(false),
                    "Level": Number(0),
                    "SampleRate": Number(44100),
                    "SupportsExternalStream": Bool(false),
                    "TimeBase": String("1/14112000"),
                    "Type": String("Audio"),
                    "VideoRange": String("Unknown"),
                    "VideoRangeType": String("Unknown"),
                },
                Object {
                    "AspectRatio": String("1:1"),
                    "AudioSpatialFormat": String("None"),
                    "BitDepth": Number(8),
                    "Codec": String("mjpeg"),
                    "ColorSpace": String("bt470bg"),
                    "Comment": String("Cover (front)"),
                    "Height": Number(500),
                    "Index": Number(1),
                    "IsAVC": Bool(false),
                    "IsAnamorphic": Bool(false),
                    "IsDefault": Bool(false),
                    "IsExternal": Bool(false),
                    "IsForced": Bool(false),
                    "IsHearingImpaired": Bool(false),
                    "IsInterlaced": Bool(false),
                    "IsTextSubtitleStream": Bool(false),
                    "Level": Number(-99),
                    "PixelFormat": String("yuvj420p"),
                    "Profile": String("Baseline"),
                    "RealFrameRate": Number(90000),
                    "RefFrames": Number(1),
                    "SupportsExternalStream": Bool(false),
                    "TimeBase": String("1/90000"),
                    "Type": String("EmbeddedImage"),
                    "VideoRange": String("Unknown"),
                    "VideoRangeType": String("Unknown"),
                    "Width": Number(500),
                },
                Object {
                    "AudioSpatialFormat": String("None"),
                    "Index": Number(2),
                    "IsDefault": Bool(false),
                    "IsExternal": Bool(false),
                    "IsForced": Bool(false),
                    "IsHearingImpaired": Bool(false),
                    "IsInterlaced": Bool(false),
                    "IsTextSubtitleStream": Bool(false),
                    "Path": String("/data/music/Agar Agar/Cardan/03 - Cuidado, Peligro, Eclipse.txt"),
                    "SupportsExternalStream": Bool(false),
                    "Type": String("Lyric"),
                    "VideoRange": String("Unknown"),
                    "VideoRangeType": String("Unknown"),
                },
            ],
            "Name": String("03 - Cuidado, Peligro, Eclipse"),
            "Path": String("/data/music/Agar Agar/Cardan/03 - Cuidado, Peligro, Eclipse.mp3"),
            "Protocol": String("File"),
            "ReadAtNativeFramerate": Bool(false),
            "RequiredHttpHeaders": Object {},
            "RequiresClosing": Bool(false),
            "RequiresLooping": Bool(false),
            "RequiresOpening": Bool(false),
            "RunTimeTicks": Number(3600979590),
            "Size": Number(14487065),
            "SupportsDirectPlay": Bool(true),
            "SupportsDirectStream": Bool(true),
            "SupportsProbing": Bool(true),
            "SupportsTranscoding": Bool(true),
            "TranscodingSubProtocol": String("http"),
            "Type": String("Default"),
        },
    ],
    "MediaType": String("Audio"),
    "Name": String("Cuidado, Peligro, Eclipse"),
    "NormalizationGain": Number(-10.45),
    "ParentBackdropImageTags": Array [
        String("ea9ad04d014bd8317aa784ffb5676eac"),
    ],
    "ParentBackdropItemId": String("c910b835045265897c9b1e30417937c8"),
    "ParentId": String("e66386bd52e9e13bcd53fefbe4dbfe80"),
    "ParentIndexNumber": Number(0),
    "PremiereDate": String("2016-01-01T00:00:00.0000000Z"),
    "ProductionYear": Number(2016),
    "RunTimeTicks": Number(3600979590),
    "ServerId": String("97a9003303d7461395074680d9046935"),
    "Type": String("Audio"),
    "UserData": Object {
        "IsFavorite": Bool(false),
        "Key": String("Agar Agar-Cardan [EP]-0000-0003Cuidado, Peligro, Eclipse"),
        "PlayCount": Number(0),
        "PlaybackPositionTicks": Number(0),
        "Played": Bool(false),
    },
}, */

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
    #[serde(rename = "PlaybackPositionTicks")]
    playback_position_ticks: u64,
    #[serde(rename = "PlayCount")]
    play_count: u64,
    #[serde(rename = "IsFavorite")]
    is_favorite: bool,
    #[serde(rename = "Played")]
    played: bool,
    #[serde(rename = "Key")]
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
    // #[serde(rename = "ArtistItems")]
    // artist_items: Vec<Artist>,
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
    // #[serde(rename = "ImageBlurHashes")]
    // image_blur_hashes: ImageBlurHashes,
    // #[serde(rename = "ImageTags")]
    // image_tags: ImageTags,
    #[serde(rename = "IndexNumber")]
    index_number: u64,
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
    #[serde(rename = "ParentIndexNumber", default)]
    parent_index_number: u64,
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
    user_data: DiscographySongUserData,
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
        },
        {
            "Text": "Crimes have been swept aside"
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
    text: String,
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