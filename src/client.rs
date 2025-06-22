/* --------------------------
HTTP client for Jellyfin API
    - This file contains all HTTP related functions. It defines the Client struct which is used to interact with the Jellyfin API.
    - All the types used in the client are defined at the end of the file.
-------------------------- */

use crate::database::extension::DownloadStatus;
use crate::keyboard::Searchable;
use dirs::data_dir;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::Row;
use std::error::Error;
use std::io::Cursor;

use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
pub struct Client {
    pub base_url: String,
    pub server_id: String,
    http_client: reqwest::Client,
    pub access_token: String,
    user_id: String,
    pub user_name: String,
    pub authorization_header: (String, String),
}

pub struct SelectedServer {
    #[allow(dead_code)]
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: String,
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
    pub async fn new(server: &SelectedServer) -> Option<Arc<Self>> {

        let http_client = reqwest::Client::new();
        let device_id = random_string();

        let url: String = String::new() + &server.url + "/Users/authenticatebyname";
        let response = http_client
            .post(url)
            .header("Content-Type", "text/json")
            .header("Authorization", format!("MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"{}\", Version=\"{}\"", &device_id, env!("CARGO_PKG_VERSION")))
            .json(&serde_json::json!({
                "Username": &server.username,
                "Pw": &server.password,
            }))
            .send()
            .await;

        match response {
            Ok(json) => {
                let value = match json.json::<serde_json::Value>().await {
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
                let server_id = value["ServerId"].as_str().unwrap_or_else(|| {
                    println!(" ! Could not get server id");
                    std::process::exit(1);
                });
                Some(Arc::new(Self {
                    base_url: server.url.clone(),
                    server_id: server_id.to_string(),
                    http_client,
                    access_token: access_token.to_string(),
                    user_id: user_id.to_string(),
                    user_name: server.username.clone(),
                    authorization_header: Self::generate_authorization_header(&device_id, access_token),
                }))
            }
            Err(e) => {
                println!(" ! Error authenticating: {}", e);
                None
            }
        }
    }
    // returns the key/value pair for the authorization header
    pub fn generate_authorization_header(device_id: &String, access_token: &str) -> (String, String) {
        (
            "Authorization".into(),
            format!(
                "MediaBrowser Client=\"{}\", Device=\"{}\", DeviceId=\"{}\", Version=\"{}\", Token=\"{}\"",
                env!("CARGO_PKG_VERSION"), "jellyfin-tui", "jellyfin-tui", device_id, access_token
            )
        )
    }

    /// Produces a list of artists, called by the main function before initializing the app
    ///
    pub async fn artists(&self, search_term: String) -> Result<Vec<Artist>, reqwest::Error> {
        let url = format!("{}/Artists/AlbumArtists", self.base_url);

        let response: Result<reqwest::Response, reqwest::Error> = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())

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
            }
            Err(_) => {
                return Ok(vec![]);
            }
        };

        Ok(artists.items)
    }

    /// Produces a list of all albums
    ///
    pub async fn albums(&self) -> Result<Vec<Album>, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "DateCreated,SortName"),
                ("SortOrder", "Ascending"),
                ("Recursive", "true"),
                ("IncludeItemTypes", "MusicAlbum"),
                ("Fields", "DateCreated, ParentId"),
                ("ImageTypeLimit", "1")
            ])
            .query(&[("StartIndex", "0")])
            .send()
            .await;

        let albums = match response {
            Ok(json) => {
                let albums: Albums = json
                    .json()
                    .await
                    .unwrap_or_else(|_| Albums { items: vec![] });
                albums
            }
            Err(_) => {
                return Ok(vec![]);
            }
        };

        Ok(albums.items)
    }

    /// Produces a list of songs in an album
    ///
    pub async fn album_tracks(&self, id: &str) -> Result<Vec<DiscographySong>, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "ParentIndexNumber,IndexNumber,SortName"),
                ("SortOrder", "Ascending"),
                ("Recursive", "true"),
                ("IncludeItemTypes", "Audio"),
                ("Fields", "Genres, DateCreated, MediaSources, ParentId"),
                ("ImageTypeLimit", "1"),
                ("ParentId", id)
            ])
            .query(&[("StartIndex", "0")])
            .send()
            .await;

        let mut songs = match response {
            Ok(json) => {
                let songs: Discography = json
                    .json()
                    .await
                    .unwrap_or_else(|_| Discography { items: vec![], total_record_count: 0 });
                songs.items
            }
            Err(_) => {
                return Ok(vec![]);
            }
        };

        for song in songs.iter_mut() {
            song.name.retain(|c| c != '\t' && c != '\n');
            song.name = song.name.trim().to_string();
        }

        Ok(songs)
    }

    /// Produces a list of songs by an artist sorted by album and index
    ///
    pub async fn discography(
        &self,
        id: &str,
    ) -> Result<Vec<DiscographySong>, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "text/json")
            .query(&[
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

        match response {
            Ok(json) => {
                let discog: Discography = json
                    .json()
                    .await
                    .unwrap_or_else(|_| Discography { items: vec![], total_record_count: 0 });

                Ok(discog.items)
            }
            Err(_) => {
                Ok(vec![])
            }
        }
    }

    /// This for the search functionality, it will poll albums based on the search term
    ///
    // pub async fn search_albums(&self, search_term: String) -> Result<Vec<Album>, reqwest::Error> {
    //     let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);
    //
    //     let response = self.http_client
    //         .get(url)
    //         .header("X-MediaBrowser-Token", self.access_token.to_string())
    //         .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
    //         .header("Content-Type", "text/json")
    //         .query(&[
    //             ("SortBy", "SortName"),
    //             ("SortOrder", "Ascending"),
    //             ("searchTerm", search_term.as_str()),
    //             ("Fields", "PrimaryImageAspectRatio, CanDelete, MediaSourceCount"),
    //             ("Recursive", "true"),
    //             ("EnableTotalRecordCount", "false"),
    //             ("ImageTypeLimit", "1"),
    //             ("IncludePeople", "false"),
    //             ("IncludeMedia", "true"),
    //             ("IncludeGenres", "false"),
    //             ("IncludeStudios", "false"),
    //             ("IncludeArtists", "false"),
    //             ("IncludeItemTypes", "MusicAlbum")
    //         ])
    //         .query(&[("StartIndex", "0")])
    //         .send()
    //         .await;
    //
    //     let albums = match response {
    //         Ok(json) => {
    //             let albums: Albums = json
    //                 .json()
    //                 .await
    //                 .unwrap_or_else(|_| Albums { items: vec![] });
    //             albums.items
    //         }
    //         Err(_) => {
    //             return Ok(vec![]);
    //         }
    //     };
    //
    //     Ok(albums)
    // }

    /// This for the search functionality, it will poll songs based on the search term
    ///
    pub async fn search_tracks(
        &self,
        search_term: String,
    ) -> Result<Vec<DiscographySong>, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "Name"),
                ("SortOrder", "Ascending"),
                ("searchTerm", search_term.as_str()),
                ("Fields", "PrimaryImageAspectRatio, CanDelete, MediaSourceCount"),
                ("Recursive", "true"),
                ("EnableTotalRecordCount", "true"),
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
                let songs: Discography = json
                    .json()
                    .await
                    .unwrap_or_else(|_| Discography { items: vec![], total_record_count: 0 });
                // remove those where album_artists is empty
                let songs: Vec<DiscographySong> = songs
                    .items
                    .into_iter()
                    .filter(|s| !s.album_artists.is_empty())
                    .collect();
                songs
            }
            Err(_) => {
                return Ok(vec![]);
            }
        };

        Ok(songs)
    }

    /// Returns a randomized list of tracks based on the preferences
    ///
    pub async fn random_tracks(
        &self,
        tracks_n: usize,
        only_played: bool,
        only_unplayed: bool,
    ) -> Result<Vec<DiscographySong>, Box<dyn Error>> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "Random"),
                ("StartIndex", "0"),
                ("SortOrder", "Ascending"),
                ("Recursive", "true"),
                ("Fields", "Genres, DateCreated, MediaSources, ParentId"),
                ("IncludeItemTypes", "Audio"),
                ("Recursive", "true"),
                ("EnableTotalRecordCount", "true"),
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
                let songs: Discography = json
                    .json()
                    .await
                    .unwrap_or_else(|_| Discography { items: vec![], total_record_count: 0 });
                // remove those where album_artists is empty
                let songs: Vec<DiscographySong> = songs
                    .items
                    .into_iter()
                    .filter(|s| !s.album_artists.is_empty())
                    .collect();
                songs
            }
            Err(_) => {
                return Ok(vec![]);
            }
        };

        Ok(songs)
    }

    /// Returns a list of artists with recently added albums
    ///
    // pub async fn new_artists(&self) -> Result<Vec<String>, Box<dyn Error>> {
    //     let url = format!("{}/Artists", self.base_url);
    //
    //     let response: Result<reqwest::Response, reqwest::Error> = self.http_client
    //         .get(url)
    //         .header("X-MediaBrowser-Token", self.access_token.to_string())
    //         .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
    //         .header("Content-Type", "text/json")
    //         .query(&[
    //             ("SortBy", "DateCreated"),
    //             ("SortOrder", "Descending"),
    //             ("Recursive", "true"),
    //             ("ImageTypeLimit", "-1")
    //         ])
    //         .query(&[("StartIndex", "0")])
    //         .query(&[("Limit", "50")])
    //         .send()
    //         .await;
    //
    //     let artists = match response {
    //         Ok(json) => {
    //             let artists: Artists = json.json().await.unwrap_or_else(|_| Artists {
    //                 items: vec![],
    //                 start_index: 0,
    //                 total_record_count: 0,
    //             });
    //             artists
    //         }
    //         Err(_) => {
    //             return Ok(vec![]);
    //         }
    //     };
    //
    //     // we will have a file in the data directory with artists that are new,but we have already seen them
    //     let data_dir = match data_dir() {
    //         Some(dir) => dir,
    //         None => {
    //             return Ok(vec![]);
    //         }
    //     };
    //
    //     // The process is as follows:
    //     // 1. We get a list of artists that have had albums added recently (var artists)
    //     // 2. We read the file with the artists we have seen (var seen_artists)
    //     // 3. If we've seen this artist, we're fine
    //     // 4. The length of the newly added will be 50. If we go over this, it won't have an artist that we've seen before and we can REMOVE it from the file
    //     // 5. The next time the artist has something new, we will see it again and write it back to the file
    //
    //     let mut new_artists: Vec<String> = vec![];
    //     let seen_artists: Vec<String>;
    //     // store it as IDs on each line
    //     let seen_artists_file = data_dir.join("jellyfin-tui").join("seen_artists");
    //
    //     // if new we just throw everything in, makes no sense initially
    //     if !seen_artists_file.exists() {
    //         let _ = File::create(&seen_artists_file);
    //         // write all the artists to the file
    //         let mut file = OpenOptions::new().append(true).open(&seen_artists_file)?;
    //         for artist in artists.items.iter() {
    //             writeln!(file, "{}", artist.id)?;
    //         }
    //         return Ok(vec![]);
    //     }
    //
    //     if seen_artists_file.exists() {
    //         {
    //             // read the file
    //             let mut file = File::open(&seen_artists_file)?;
    //             let mut contents = String::new();
    //             file.read_to_string(&mut contents)?;
    //             seen_artists = contents.lines().map(|s| s.to_string()).collect();
    //         }
    //         {
    //             // wipe it and write the new artists
    //             let mut file = OpenOptions::new().write(true).open(&seen_artists_file)?;
    //             for artist in artists.items.iter() {
    //                 if seen_artists.contains(&artist.id) {
    //                     continue;
    //                 }
    //                 new_artists.push(artist.id.clone());
    //                 writeln!(file, "{}", artist.id)?;
    //             }
    //         }
    //     }
    //
    //     Ok(new_artists)
    // }

    /// Returns a list of lyrics lines for a song
    ///
    pub async fn lyrics(&self, song_id: &String) -> Result<Vec<Lyric>, reqwest::Error> {
        let url = format!("{}/Audio/{}/Lyrics", self.base_url, song_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "application/json")
            .send()
            .await;

        match response {
            Ok(_) => {}
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
            }
            Err(_) => {
                return Ok(vec![]);
            }
        }
        .lyrics;

        Ok(lyric)
    }

    /// Downloads cover art for an album and saves it as cover.* in the data_dir, filename is returned
    ///
    pub async fn download_cover_art(&self, album_id: &String) -> Result<String, Box<dyn Error>> {
        let url = format!("{}/Items/{}/Images/Primary?fillHeight=512&fillWidth=512&quality=96&tag=be2a8642e97e2151ef0580fc72f3505a", self.base_url, album_id);
        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
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

        let data_dir = data_dir().unwrap_or_else(|| PathBuf::from("./"));

        let mut file = std::fs::File::create(
            data_dir
                .join("jellyfin-tui")
                .join("covers")
                .join(album_id.to_string() + "." + extension),
        )?;
        let mut content = Cursor::new(response.bytes().await?);
        std::io::copy(&mut content, &mut file)?;

        Ok(album_id.to_string() + "." + extension)
    }

    /// Produces URL of a song from its ID
    pub fn song_url_sync(&self, song_id: &String, transcoding: &Transcoding) -> String {
        let mut url = format!("{}/Audio/{}/universal", self.base_url, song_id);
        url += &format!(
            "?UserId={}&api_key={}&StartTimeTicks=0&EnableRedirection=true&EnableRemoteMedia=false",
            self.user_id, self.access_token
        );
        url += "&container=opus,webm|opus,mp3,aac,m4a|aac,m4b|aac,flac,webma,webm|webma,wav,ogg";

        if transcoding.enabled {
            url += &format!(
                "&transcodingContainer={}&transcodingProtocol=http&audioCodec={}",
                transcoding.container, transcoding.container
            );
            if transcoding.bitrate > 0 {
                url += &format!("&maxStreamingBitrate={}", transcoding.bitrate * 1000);
            } else {
                url += "&MaxStreamingBitrate=320000";
            }
        }
        url
    }

    /// Sends an update to favorite of a track. POST is true, DELETE is false
    ///
    pub async fn set_favorite(&self, item_id: &str, favorite: bool) -> Result<(), reqwest::Error> {
        let id = item_id.replace("_album_", "");
        let url = format!(
            "{}/Users/{}/FavoriteItems/{}",
            self.base_url, self.user_id, id
        );
        let response = if favorite {
            self.http_client
                .post(url)
                .header("X-MediaBrowser-Token", self.access_token.to_string())
                .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
                .header("Content-Type", "application/json")
                .send()
                .await
        } else {
            self.http_client
                .delete(url)
                .header("X-MediaBrowser-Token", self.access_token.to_string())
                .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
                .header("Content-Type", "application/json")
                .send()
                .await
        };

        match response {
            Ok(_) => {}
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
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
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
                let playlists: Playlists = json
                    .json()
                    .await
                    .unwrap_or_else(|_| Playlists { items: vec![] });
                playlists.items
            }
            Err(_) => {
                return Ok(vec![]);
            }
        };

        Ok(playlists)
    }

    /// Gets a single playlist
    ///
    /// /playlists/636d3c3e246dc4f24718480d4316ef2d/items?Fields=Genres%2C%20DateCreated%2C%20MediaSources%2C%20UserData%2C%20ParentId&IncludeItemTypes=Audio&Limit=300&SortOrder=Ascending&StartIndex=0&UserId=aca06460269248d5bbe12e5ae7ceac8b
    pub async fn playlist(&self, playlist_id: &String, limit: bool) -> Result<Discography, reqwest::Error> {
        let url = format!("{}/Playlists/{}/Items", self.base_url, playlist_id);

        let mut query_params = vec![
            ("Fields", "Genres, DateCreated, MediaSources, UserData, ParentId"),
            ("IncludeItemTypes", "Audio"),
            ("EnableTotalRecordCount", "true"),
            ("SortOrder", "Ascending"),
            ("SortBy", "IndexNumber"),
            ("StartIndex", "0"),
            ("UserId", self.user_id.as_str())
        ];

        if limit {
            query_params.push(("Limit", "200"));
        }

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "text/json")
            .query(&query_params)
            .send()
            .await;

        let playlist = match response {
            Ok(json) => {
                let playlist: Discography = json
                    .json()
                    .await
                    .unwrap_or_else(|_| Discography { items: vec![], total_record_count: 0 });
                playlist
            }
            Err(_) => {
                return Ok(Discography { items: vec![], total_record_count: 0 });
            }
        };

        Ok(playlist)
    }

    /// Creates a new playlist on the server
    ///
    /// We can pass Ids[] to add songs to the playlist as well! Todo
    pub async fn create_playlist(
        &self,
        playlist_name: &String,
        is_public: bool,
    ) -> Result<String, reqwest::Error> {
        let url = format!("{}/Playlists", self.base_url);

        let response = self.http_client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "Ids": [],
                "Name": playlist_name,
                "IsPublic": is_public,
                "UserId": self.user_id
            }))
            .send()
            .await;

        let playlist_id = response?.json::<serde_json::Value>().await?["Id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(playlist_id)
    }

    /// Deletes a playlist on the server
    ///
    pub async fn delete_playlist(
        &self,
        playlist_id: &String,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/Items/{}", self.base_url, playlist_id);

        self.http_client
            .delete(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "application/json")
            .send()
            .await
    }

    /// Updates a playlist on the server by sending the full definition
    ///
    pub async fn update_playlist(
        &self,
        playlist: &Playlist,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/Items/{}", self.base_url, playlist.id);

        // i do this because my Playlist struct is not the full playlist and i don't want to lose data :)
        // so GET -> modify -> POST
        let response = self.http_client
        .get(url.clone())
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "application/json")
            .send()
            .await;

        let mut full_playlist = response?.json::<serde_json::Value>().await?;
        // so far we only have rename
        full_playlist["Name"] = serde_json::Value::String(playlist.name.clone());

        self.http_client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "application/json")
            .json(&full_playlist)
            .send()
            .await
    }

    /// Adds a track to a playlist
    ///
    /// /Playlists/60efcb22e97a01f2b2a59f4d7b4a48ee/Items?ids=818923889708a83351a8a381af78310b&userId=aca06460269248d5bbe12e5ae7ceac8b
    pub async fn add_to_playlist(
        &self,
        track_id: &str,
        playlist_id: &String,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/Playlists/{}/Items", self.base_url, playlist_id);

        self.http_client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "application/json")
            .query(&[
                ("ids", track_id),
                ("userId", self.user_id.as_str())
            ])
            .send()
            .await
    }

    /// Removes a track from a playlist
    ///
    pub async fn remove_from_playlist(
        &self,
        track_id: &String,
        playlist_id: &String,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/Playlists/{}/Items", self.base_url, playlist_id);

        self.http_client
            .delete(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
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
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
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
            }
            Err(_) => {
                return Ok(vec![]);
            }
        };

        Ok(tasks)
    }

    /// Runs a scheduled task
    ///
    pub async fn run_scheduled_task(
        &self,
        task_id: &String,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/ScheduledTasks/Running/{}", self.base_url, task_id);

        self.http_client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
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
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
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
    pub async fn stopped(
        &self,
        song_id: &String,
        position_ticks: u64,
    ) -> Result<(), reqwest::Error> {
        let url = format!("{}/Sessions/Playing/Stopped", self.base_url);
        let _response = self.http_client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
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
pub async fn report_progress(base_url: String, access_token: String, pr: ProgressReport, authorization_header: (String, String)) -> Result<(), reqwest::Error> {
    let url = format!("{}/Sessions/Playing/Progress", base_url);
    // new http client, this is a pure function so we can create a new one
    let client = reqwest::Client::new();
    let _response = client
        .post(url)
        .header("X-MediaBrowser-Token", access_token.to_string())
        .header(authorization_header.0.as_str(), authorization_header.1.as_str())
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

fn random_string() -> String {
    let charset = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    random_string::generate(10, charset)
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
    pub run_time_ticks: u64,
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
    #[serde(rename = "TotalRecordCount", default)]
    pub total_record_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscographyAlbum {
    pub id: String,
    pub songs: Vec<DiscographySong>,
}

pub struct TempDiscographyAlbum {
    pub id: String,
    pub songs: Vec<DiscographySong>,
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
    // #[serde(rename = "ArtistItems", default)]
    // pub artist_items: Vec<Artist>,
    #[serde(rename = "Artists", default)]
    pub artists: Vec<String>,
    #[serde(rename = "BackdropImageTags", default)]
    pub backdrop_image_tags: Vec<String>,
    #[serde(rename = "ChannelId", default)]
    pub channel_id: Option<String>,
    #[serde(rename = "DateCreated", default)]
    pub date_created: String,
    // #[serde(rename = "GenreItems")]
    // genre_items: Vec<Genre>,
    #[serde(rename = "Genres", default)]
    pub genres: Vec<String>,
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
    pub is_folder: bool,
    // #[serde(rename = "LocationType")]
    // location_type: String,
    #[serde(rename = "MediaSources", default)]
    pub media_sources: Vec<MediaSource>,
    #[serde(rename = "MediaType", default)]
    pub media_type: String,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "NormalizationGain", default)]
    pub normalization_gain: f64,
    // #[serde(rename = "ParentBackdropImageTags")]
    // parent_backdrop_image_tags: Vec<String>,
    // #[serde(rename = "ParentBackdropItemId")]
    // parent_backdrop_item_id: String,
    #[serde(rename = "ParentId", default)]
    pub parent_id: String,
    #[serde(rename = "ParentIndexNumber", default = "index_default")]
    pub parent_index_number: u64,
    #[serde(rename = "PremiereDate", default)]
    pub premiere_date: String,
    #[serde(rename = "ProductionYear", default)]
    pub production_year: u64,
    #[serde(rename = "RunTimeTicks", default)]
    pub run_time_ticks: u64,
    #[serde(rename = "ServerId", default)]
    pub server_id: String,
    // #[serde(rename = "Type")]
    // type_: String,
    #[serde(rename = "UserData", default)]
    pub user_data: DiscographySongUserData,
    /// our own fields
    #[serde(default)]
    pub download_status: DownloadStatus,
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

impl<'r> FromRow<'r, sqlx::sqlite::SqliteRow> for DiscographySong {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.get("id"),
            album: row.get("album"),
            album_artist: row.get("album_artist"),
            album_id: row.get("album_id"),
            date_created: row.get("date_created"),
            media_type: row.get("media_type"),
            name: row.get("name"),
            parent_id: row.get("parent_id"),
            premiere_date: row.get("premiere_date"),
            server_id: row.get("server_id"),

            // Deserialize JSON fields, using `unwrap_or_default()` to avoid panics
            album_artists: serde_json::from_str(row.get::<&str, _>("album_artists")).unwrap_or_default(),
            artists: serde_json::from_str(row.get::<&str, _>("artists")).unwrap_or_default(),
            backdrop_image_tags: serde_json::from_str(row.get::<&str, _>("backdrop_image_tags")).unwrap_or_default(),
            genres: serde_json::from_str(row.get::<&str, _>("genres")).unwrap_or_default(),
            media_sources: serde_json::from_str(row.get::<&str, _>("media_sources")).unwrap_or_default(),

            // Handle JSON user_data with a default fallback
            user_data: serde_json::from_str(row.get::<&str, _>("user_data")).unwrap_or_else(|_| DiscographySongUserData {
                playback_position_ticks: 0,
                play_count: 0,
                is_favorite: false,
                played: false,
                key: "".to_string(),
            }),

            // Handle `Option<String>` safely
            channel_id: row.try_get("channel_id").ok(),

            // Convert integer values to booleans
            has_lyrics: row.get::<i32, _>("has_lyrics") != 0,
            is_folder: row.get::<i32, _>("is_folder") != 0,

            // Numeric fields
            index_number: row.get("index_number"),
            parent_index_number: row.get("parent_index_number"),
            normalization_gain: row.get("normalization_gain"),
            production_year: row.get("production_year"),
            run_time_ticks: row.get("run_time_ticks"),
            playlist_item_id: row.get("playlist_item_id"),

            // Deserialize JSON for download_status
            download_status: serde_json::from_str(row.get::<&str, _>("download_status")).unwrap_or(DownloadStatus::NotDownloaded),
        })
    }
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
pub struct Albums {
    #[serde(rename = "Items", default)]
    pub items: Vec<Album>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Album {
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Id", default)]
    pub id: String,
    #[serde(rename = "AlbumArtists", default)]
    pub album_artists: Vec<Artist>,
    // #[serde(rename = "ArtistItems", default)]
    // pub artist_items: Vec<Artist>,
    #[serde(rename = "UserData", default)]
    pub user_data: UserData,
    #[serde(rename = "DateCreated", default)]
    pub date_created: String,
    #[serde(rename = "ParentId", default)]
    pub parent_id: String,
    #[serde(rename = "RunTimeTicks", default)]
    pub run_time_ticks: u64,
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
