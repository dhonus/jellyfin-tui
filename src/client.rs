/* --------------------------
HTTP client for Jellyfin API
    - This file contains all HTTP related functions. It defines the Client struct which is used to interact with the Jellyfin API.
    - All the types used in the client are defined at the end of the file.
-------------------------- */

// https://gist.github.com/nielsvanvelzen/ea047d9028f676185832e51ffaf12a6f

use crate::database::extension::DownloadStatus;
use crate::keyboard::Searchable;
use dirs::data_dir;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::Row;
use std::error::Error;

use crate::config::AuthEntry;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub struct Client {
    pub base_url: String,
    pub server_id: String,
    http_client: reqwest::Client,
    pub access_token: String,
    pub(crate) user_id: String,
    pub user_name: String,
    pub authorization_header: (String, String),
    pub device_id: String,
}

#[derive(Debug, Clone)]
pub enum AuthMethod {
    UserPass { username: String, password: String },
    QuickConnect,
}

#[derive(Debug, Clone)]
pub struct SelectedServer {
    // pub name: String,
    pub url: String,
    pub auth: AuthMethod,
}

#[derive(Debug)]
pub struct Transcoding {
    pub enabled: bool,
    pub bitrate: u32,
    pub container: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkQuality {
    Normal,
    Slow,
    CzechTrain,
}
impl NetworkQuality {
    pub fn classify(ms: u128) -> Self {
        match ms {
            0..=300 => NetworkQuality::Normal,
            301..=1200 => NetworkQuality::Slow,
            _ => NetworkQuality::CzechTrain,
        }
    }
}

impl Client {
    /// Creates a new client with the given base URL
    /// If the configuration file does not exist, it will be created with stdin input
    ///
    pub async fn new(
        server_url: &String,
        username: &String,
        password: &String,
    ) -> Option<Arc<Self>> {
        let http_client = reqwest::Client::new();
        let device_id = random_string();

        let url: String = String::new() + &server_url + "/Users/authenticatebyname";
        let response = http_client
            .post(&url)
            .timeout(Duration::from_secs(5))
            .header("Content-Type", "text/json")
            .header("Authorization", format!("MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"{}\", Version=\"{}\"", &device_id, env!("CARGO_PKG_VERSION")))
            .json(&serde_json::json!({
                "Username": &username,
                "Pw": &password,
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
                    base_url: server_url.clone(),
                    server_id: server_id.to_string(),
                    http_client,
                    access_token: access_token.to_string(),
                    user_id: user_id.to_string(),
                    user_name: username.clone(),
                    authorization_header: Self::generate_authorization_header(
                        &device_id,
                        access_token,
                    ),
                    device_id,
                }))
            }
            Err(e) => {
                println!(" ! Error authenticating: {}", e);
                None
            }
        }
    }

    pub async fn from_cache(base_url: &str, server_id: &String, entry: &AuthEntry) -> Arc<Self> {
        let authorization_header =
            Self::generate_authorization_header(&entry.device_id, &entry.access_token);

        Arc::new(Self {
            base_url: base_url.to_string(),
            server_id: server_id.to_string(),
            http_client: reqwest::Client::new(),
            access_token: entry.access_token.clone(),
            user_id: entry.user_id.clone(),
            user_name: entry.username.clone(),
            authorization_header,
            device_id: entry.device_id.clone(),
        })
    }

    pub async fn quick_connect(base_url: &str) -> Arc<Self> {
        let client = reqwest::Client::new();
        let device_id = random_string();

        let auth_header = format!(
            "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"{}\", Version=\"{}\"",
            device_id,
            env!("CARGO_PKG_VERSION")
        );

        let qc = client
            .post(format!("{}/QuickConnect/Initiate", base_url))
            .header("Authorization", &auth_header)
            .json(&serde_json::json!({
                "AppName": "jellyfin-tui",
                "AppVersion": env!("CARGO_PKG_VERSION"),
                "DeviceId": device_id,
                "DeviceName": "jellyfin-tui",
            }))
            .send()
            .await
            .unwrap()
            .json::<QuickConnectState>()
            .await
            .unwrap();

        println!(" - Quick Connect: To authenticate, open Jellyfin on another device");
        println!(" - Quick Connect: Enter code {}", qc.code);

        loop {
            let state = client
                .get(format!("{}/QuickConnect/Connect?secret={}", base_url, qc.secret))
                .header("Authorization", &auth_header)
                .send()
                .await
                .unwrap()
                .json::<QuickConnectState>()
                .await
                .unwrap();

            if state.authenticated {
                break;
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        let auth: QuickConnectAuth = client
            .post(format!("{}/Users/AuthenticateWithQuickConnect", base_url))
            .header("Authorization", &auth_header)
            .json(&serde_json::json!({
                "Secret": qc.secret
            }))
            .send()
            .await
            .unwrap()
            .json::<QuickConnectAuth>()
            .await
            .unwrap();

        Arc::new(Self {
            base_url: base_url.to_string(),
            server_id: auth.server_id,
            http_client: client,
            access_token: auth.access_token.clone(),
            user_id: auth.user.id.clone(),
            user_name: auth.user.name.clone(),
            authorization_header: Self::generate_authorization_header(
                &device_id,
                &auth.access_token,
            ),
            device_id,
        })
    }

    pub async fn validate_token(&self) -> bool {
        let url = format!("{}/Users/Me", self.base_url);
        match self
            .http_client
            .get(url)
            .timeout(Duration::from_secs(5))
            .header(self.authorization_header.0.clone(), self.authorization_header.1.clone())
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    let user: serde_json::Value = response.json().await.unwrap_or_default();
                    if user["Id"].as_str() == Some(&self.user_id) {
                        return true;
                    }
                }
                false
            }
            Err(_) => false,
        }
    }

    pub async fn get_network_quality(
        http_client: &reqwest::Client,
        base_url: &String,
    ) -> NetworkQuality {
        let url = format!("{}/System/Info/Public", base_url);
        let start = std::time::Instant::now();
        let response = http_client.get(url).timeout(Duration::from_secs(10)).send().await;

        match response {
            Ok(_) => {
                let duration = start.elapsed();
                NetworkQuality::classify(duration.as_millis())
            }
            Err(_) => NetworkQuality::CzechTrain,
        }
    }

    // returns the key/value pair for the authorization header
    pub fn generate_authorization_header(
        device_id: &String,
        access_token: &str,
    ) -> (String, String) {
        (
            "Authorization".into(),
            format!(
                "MediaBrowser Client=\"{}\", Device=\"{}\", DeviceId=\"{}\", Version=\"{}\", Token=\"{}\"",
                "jellyfin-tui", "jellyfin-tui", device_id, env!("CARGO_PKG_VERSION"), access_token
            )
        )
    }

    /// Returns available music libraries
    ///
    pub async fn music_libraries(&self) -> Result<Vec<LibraryView>, reqwest::Error> {
        let url = format!("{}/Users/{}/Views", self.base_url, self.user_id);

        let req = self
            .http_client
            .get(&url)
            .header("X-MediaBrowser-Token", &self.access_token)
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str());

        let views: ViewsResponse = match self.get_json_with_retry(req).await {
            Ok(v) => v,
            Err(e) => {
                log::error!("Failed to fetch library views: {}", e);
                return Ok(vec![]);
            }
        };

        let music_libs: Vec<LibraryView> = views
            .items
            .into_iter()
            .filter(|v| {
                v.collection_type
                    .as_deref()
                    .map(|t| t.eq_ignore_ascii_case("music"))
                    .unwrap_or(false)
            })
            .collect();

        log::debug!("Found {} music libraries", music_libs.len());

        Ok(music_libs)
    }

    /// Produces a list of artists, called by the main function before initializing the app
    ///
    pub async fn artists(&self, search_term: String) -> Result<Vec<Artist>, reqwest::Error> {
        let url = format!("{}/Artists/AlbumArtists", self.base_url);

        let req = self
            .http_client
            .get(&url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "text/json")
            .query(&[
                ("SearchTerm", search_term.as_str()),
                ("SortBy", "Name,DateCreated"),
                ("SortOrder", "Ascending"),
                ("Recursive", "true"),
                ("ImageTypeLimit", "1"),
                ("Fields", "DateCreated"),
                ("StartIndex", "0"),
            ]);

        let mut artists: Artists = match self.get_json_with_retry(req).await {
            Ok(a) => a,
            Err(e) => {
                log::error!("Failed to fetch artists: {}", e);
                return Ok(vec![]);
            }
        };

        // temporary jellyfin bug, doesn't return anything for UserData. Remove once this works!
        let favorite_req = self
            .http_client
            .get(&url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "text/json")
            .query(&[("Filters", "IsFavorite"), ("StartIndex", "0")]);

        let favorite_artists = match self.get_json_with_retry::<Artists>(favorite_req).await {
            Ok(a) => a.items,
            Err(e) => {
                log::warn!("Failed to fetch favorite artists: {}", e);
                vec![]
            }
        };

        for artist in artists.items.iter_mut() {
            artist.user_data.is_favorite = favorite_artists.iter().any(|fa| fa.id == artist.id);
        }

        log::debug!("Loaded {} artists total", artists.items.len());

        Ok(artists.items)
    }

    /// Produces a list of all albums
    ///
    pub async fn albums(&self, library_id: Option<&String>) -> Result<Vec<Album>, reqwest::Error> {
        const LIMIT: usize = 200;

        let mut all_albums = Vec::new();
        let mut start_index = 0;

        loop {
            let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

            let mut req = self
                .http_client
                .get(&url)
                .header("X-MediaBrowser-Token", self.access_token.to_string())
                .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
                .query(&[
                    ("SortBy", "DateCreated,SortName"),
                    ("SortOrder", "Ascending"),
                    ("Recursive", "true"),
                    ("IncludeItemTypes", "MusicAlbum"),
                    ("Fields", "DateCreated,ParentId,ProductionYear,PremiereDate"),
                    ("StartIndex", &start_index.to_string()),
                    ("Limit", &LIMIT.to_string()),
                ]);

            if let Some(lib) = library_id {
                req = req.query(&[("ParentId", lib)]);
            }

            let parsed: Albums = self.get_json_with_retry(req).await?;

            let count = parsed.items.len();

            log::debug!("Fetched {} albums at offset {}", count, start_index);

            if count == 0 {
                break;
            }

            all_albums.extend(parsed.items);

            if count < LIMIT {
                break;
            }

            start_index += LIMIT;
        }

        log::info!("Loaded {} albums total", all_albums.len());

        Ok(all_albums)
    }

    /// Produces a list of songs in an album
    ///
    pub async fn album_tracks(&self, id: &str) -> Result<Vec<DiscographySong>, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let req = self
            .http_client
            .get(&url)
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
                ("ParentId", id),
                ("StartIndex", "0"),
            ]);

        let mut discog: Discography = match self.get_json_with_retry(req).await {
            Ok(d) => d,
            Err(e) => {
                log::error!("Failed to fetch album_tracks for album {}: {}", id, e);
                return Ok(vec![]);
            }
        };

        for song in discog.items.iter_mut() {
            song.name.retain(|c| c != '\t' && c != '\n');
            song.name = song.name.trim().to_string();
        }

        log::debug!("Loaded {} tracks for album {}", discog.items.len(), id);

        Ok(discog.items)
    }

    /// Produces a list of songs by an artist sorted by album and index
    ///
    pub async fn discography(&self, id: &str) -> Result<Vec<DiscographySong>, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let req = self
            .http_client
            .get(&url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "text/json")
            .query(&[
                ("Recursive", "true"),
                ("IncludeItemTypes", "Audio"),
                ("Fields", "Genres, DateCreated, MediaSources, ParentId"),
                ("ImageTypeLimit", "1"),
                ("ArtistIds", id),
                ("StartIndex", "0"),
            ]);

        let discog: Discography = match self.get_json_with_retry(req).await {
            Ok(d) => d,
            Err(e) => {
                log::error!("Failed to fetch discography for artist {}: {}", id, e);
                return Ok(vec![]);
            }
        };

        log::debug!("Loaded {} tracks for artist {}", discog.items.len(), id);

        Ok(discog.items)
    }

    /// This for the search functionality, it will poll songs based on the search term
    ///
    pub async fn search_tracks(
        &self,
        search_term: String,
    ) -> Result<Vec<DiscographySong>, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let req = self
            .http_client
            .get(&url)
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
                ("IncludeItemTypes", "Audio"),
                ("StartIndex", "0"),
            ]);

        let discog: Discography = match self.get_json_with_retry(req).await {
            Ok(d) => d,
            Err(e) => {
                log::error!("Search tracks failed for '{}': {}", search_term, e);
                return Ok(vec![]);
            }
        };

        let songs: Vec<DiscographySong> =
            discog.items.into_iter().filter(|s| !s.album_artists.is_empty()).collect();

        log::debug!("Search '{}' returned {} tracks", search_term, songs.len());

        Ok(songs)
    }

    /// Returns a randomized list of tracks based on the preferences
    ///
    pub async fn random_tracks(
        &self,
        tracks_n: usize,
        only_played: bool,
        only_unplayed: bool,
        only_favorite: bool,
    ) -> Result<Vec<DiscographySong>, Box<dyn Error>> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let filters = match (only_played, only_unplayed, only_favorite) {
            (true, false, true) => "IsPlayed,IsFavorite",
            (true, false, false) => "IsPlayed",
            (false, true, true) => "IsUnplayed,IsFavorite",
            (false, true, false) => "IsUnplayed",
            (false, false, true) => "IsFavorite",
            _ => "",
        };

        let req = self
            .http_client
            .get(&url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "Random"),
                ("SortOrder", "Ascending"),
                ("Recursive", "true"),
                ("Fields", "Genres, DateCreated, MediaSources, ParentId"),
                ("IncludeItemTypes", "Audio"),
                ("EnableTotalRecordCount", "true"),
                ("ImageTypeLimit", "1"),
                ("Limit", &tracks_n.to_string()),
                ("StartIndex", "0"),
                ("Filters", filters),
            ]);

        let discog: Discography = match self.get_json_with_retry(req).await {
            Ok(d) => d,
            Err(e) => {
                log::error!("Random tracks request failed: {}", e);
                return Ok(vec![]);
            }
        };

        let songs: Vec<DiscographySong> =
            discog.items.into_iter().filter(|s| !s.album_artists.is_empty()).collect();

        log::debug!("Random tracks returned {} tracks (requested {})", songs.len(), tracks_n);

        Ok(songs)
    }

    /// Returns a list of lyrics lines for a song
    ///
    pub async fn lyrics(&self, song_id: &String) -> Result<Vec<Lyric>, reqwest::Error> {
        let url = format!("{}/Audio/{}/Lyrics", self.base_url, song_id);

        let req = self
            .http_client
            .get(&url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "application/json");

        let lyrics: Lyrics = match self.get_json_with_retry(req).await {
            Ok(l) => l,
            Err(e) => {
                log::debug!("No lyrics found for song {}: {}", song_id, e);
                return Ok(vec![]);
            }
        };

        log::debug!("Track {} has lyrics with {} lines", song_id, lyrics.lyrics.len());

        Ok(lyrics.lyrics)
    }

    /// Downloads cover art for an album and saves it as cover.* in the data_dir, filename is returned
    ///
    pub async fn download_cover_art(
        &self,
        album_id: &String,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/Items/{}/Images/Primary?fillHeight=512&fillWidth=512&quality=96&tag=be2a8642e97e2151ef0580fc72f3505a", self.base_url, album_id);
        let response = self
            .http_client
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

        let bytes = response.bytes().await?.to_vec();

        let cover_dir = data_dir().unwrap().join("jellyfin-tui").join("covers");
        tokio::fs::create_dir_all(&cover_dir).await?;

        let final_path = cover_dir.join(format!("{}.{}", album_id, extension));
        let tmp_path = cover_dir.join(format!("{}.{}.part", album_id, extension));

        tokio::fs::write(&tmp_path, &bytes).await?;

        tokio::fs::rename(&tmp_path, &final_path).await?;

        Ok(format!("{}.{}", album_id, extension))
    }

    /// Produces URL of a song from its ID
    pub fn song_url_sync(&self, song_id: &String, transcoding: &Transcoding) -> String {
        let mut url = format!("{}/Audio/{}/universal", self.base_url, song_id);
        url += &format!(
            "?UserId={}&api_key={}&StartTimeTicks=0&EnableRedirection=true&EnableRemoteMedia=false",
            self.user_id, self.access_token
        );
        url += "&container=opus,webm|opus,mp3,aac,m4a|aac,m4a|alac,m4b|aac,flac,webma,webm|webma,wav,ogg,wv|wavpack";

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
        let url = format!("{}/Users/{}/FavoriteItems/{}", self.base_url, self.user_id, id);
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
        const LIMIT: usize = 200;

        let mut all_playlists = Vec::new();
        let mut start_index = 0;

        loop {
            let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

            let req = self
                .http_client
                .get(&url)
                .header("X-MediaBrowser-Token", self.access_token.to_string())
                .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
                .query(&[
                    ("SortBy", "Name"),
                    ("SortOrder", "Ascending"),
                    ("SearchTerm", search_term.as_str()),
                    ("Fields", "ChildCount,Genres,DateCreated,ParentId,Overview"),
                    ("IncludeItemTypes", "Playlist"),
                    ("Recursive", "true"),
                    ("StartIndex", &start_index.to_string()),
                    ("Limit", &LIMIT.to_string()),
                ]);

            let parsed: Playlists = match self.get_json_with_retry(req).await {
                Ok(p) => p,
                Err(e) => {
                    log::error!("Failed to fetch playlists at offset {}: {}", start_index, e);
                    break;
                }
            };

            let count = parsed.items.len();

            log::debug!("Fetched {} playlists at offset {}", count, start_index);

            if count == 0 {
                break;
            }

            all_playlists.extend(parsed.items);

            if count < LIMIT {
                break;
            }

            start_index += LIMIT;
        }

        log::info!("Loaded {} playlists total", all_playlists.len());

        Ok(all_playlists)
    }

    /// Gets a single playlist
    ///
    pub async fn playlist(
        &self,
        playlist_id: &String,
        limit: Option<usize>,
    ) -> Result<Discography, reqwest::Error> {
        let url = format!("{}/Playlists/{}/Items", self.base_url, playlist_id);

        let mut all_items = Vec::new();
        let mut start_index = 0usize;
        let mut total_record_count: Option<usize> = None;

        loop {
            let mut query_params: Vec<(String, String)> = vec![
                ("Fields".into(), "Genres, DateCreated, MediaSources, UserData, ParentId".into()),
                ("IncludeItemTypes".into(), "Audio".into()),
                ("EnableTotalRecordCount".into(), "true".into()),
                ("SortOrder".into(), "Ascending".into()),
                ("SortBy".into(), "IndexNumber".into()),
                ("StartIndex".into(), start_index.to_string()),
                ("UserId".into(), self.user_id.clone()),
            ];

            if let Some(limit) = limit {
                query_params.push(("Limit".into(), limit.to_string()));
            }

            let req = self
                .http_client
                .get(&url)
                .header("X-MediaBrowser-Token", self.access_token.to_string())
                .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
                .header("Content-Type", "text/json")
                .query(&query_params);

            let mut page: Discography = match self.get_json_with_retry(req).await {
                Ok(p) => p,
                Err(e) => {
                    log::error!(
                        "Failed to fetch playlist {} at start_index {}: {}",
                        playlist_id,
                        start_index,
                        e
                    );
                    break;
                }
            };

            if total_record_count.is_none() {
                total_record_count = Some(page.total_record_count as usize);
            }

            let fetched = page.items.len();

            log::debug!(
                "Fetched playlist page playlist={} start_index={} fetched={} accumulated={} total={}",
                playlist_id,
                start_index,
                fetched,
                all_items.len(),
                total_record_count.unwrap_or(0),
            );

            if fetched == 0 {
                break;
            }

            all_items.append(&mut page.items);

            if limit.is_some() {
                break;
            }

            if all_items.len() >= total_record_count.unwrap_or(0) {
                break;
            }

            start_index += fetched;
        }

        let total = total_record_count.unwrap_or(all_items.len());

        log::debug!(
            "Finished playlist {} total_expected={} total_fetched={} limit={:?}",
            playlist_id,
            total,
            all_items.len(),
            limit
        );

        Ok(Discography { items: all_items, total_record_count: total as u64 })
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

        let response = self
            .http_client
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

        let playlist_id =
            response?.json::<serde_json::Value>().await?["Id"].as_str().unwrap_or("").to_string();
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
        let response = self
            .http_client
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
            .query(&[("ids", track_id), ("userId", self.user_id.as_str())])
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
            .query(&[("EntryIds", track_id)])
            .send()
            .await
    }
    // POST /Playlists/{playlistId}/Items/{itemId}/Move/{newIndex}
    pub async fn move_playlist_item(
        &self,
        track_id: &String,
        playlist_id: &String,
        new_index: usize,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!(
            "{}/Playlists/{}/Items/{}/Move/{}",
            self.base_url, playlist_id, track_id, new_index
        );

        self.http_client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "application/json")
            .send()
            .await
    }

    /// Returns a list of all server tasks
    ///
    pub async fn scheduled_tasks(&self) -> Result<Vec<ScheduledTask>, reqwest::Error> {
        let url = format!("{}/ScheduledTasks", self.base_url);

        let req = self
            .http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "application/json")
            .query(&[("isHidden", "false")]);

        match self.get_json_with_retry(req).await {
            Ok(tasks) => Ok(tasks),
            Err(e) => {
                log::error!("Failed to fetch scheduled tasks: {}", e);
                Ok(vec![])
            }
        }
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
        let _response = self
            .http_client
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
        song_id: Option<String>,
        position_ticks: Option<u64>,
    ) -> Result<(), reqwest::Error> {
        let url = format!("{}/Sessions/Playing/Stopped", self.base_url);
        let mut body = serde_json::Map::new();

        if let Some(id) = song_id {
            body.insert("ItemId".into(), serde_json::Value::String(id.to_string()));
        }
        if let Some(ticks) = position_ticks {
            body.insert("PositionTicks".into(), serde_json::Value::Number(ticks.into()));
        }

        let _ = self
            .http_client
            .post(url)
            .timeout(Duration::from_millis(300))
            .header("X-MediaBrowser-Token", &self.access_token)
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        Ok(())
    }

    /// Reports progress to the server using the info we have from mpv
    ///
    pub async fn report_progress(&self, pr: &ProgressReport) -> Result<(), reqwest::Error> {
        let url = format!("{}/Sessions/Playing/Progress", self.base_url);
        // new http client, this is a pure function so we can create a new one
        let client = reqwest::Client::new();
        let _response = client
            .post(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header(self.authorization_header.0.as_str(), self.authorization_header.1.as_str())
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

    /// A helper function to retry a request in case of failure, with a maximum number of retries and a delay between retries
    ///
    async fn get_json_with_retry<T: serde::de::DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
    ) -> Result<T, reqwest::Error> {
        const MAX_RETRIES: usize = 3;

        let mut attempt = 0;

        loop {
            match req.try_clone().unwrap().send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        return resp.json::<T>().await;
                    }

                    attempt += 1;

                    log::warn!("HTTP {} (attempt {}/{})", resp.status(), attempt, MAX_RETRIES);
                }

                Err(e) => {
                    attempt += 1;

                    log::warn!("Network error (attempt {}/{}): {}", attempt, MAX_RETRIES, e);
                }
            }

            if attempt >= MAX_RETRIES {
                log::error!("Request failed after {} attempts", MAX_RETRIES);
                return req.send().await?.json::<T>().await;
            }

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
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
    #[serde(rename = "Name", default)]
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
    #[serde(rename = "DateCreated", default)]
    pub date_created: String,
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
    #[serde(default)]
    pub disliked: bool,
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
            album_artists: serde_json::from_str(row.get::<&str, _>("album_artists"))
                .unwrap_or_default(),
            artists: serde_json::from_str(row.get::<&str, _>("artists")).unwrap_or_default(),
            backdrop_image_tags: serde_json::from_str(row.get::<&str, _>("backdrop_image_tags"))
                .unwrap_or_default(),
            genres: serde_json::from_str(row.get::<&str, _>("genres")).unwrap_or_default(),
            media_sources: serde_json::from_str(row.get::<&str, _>("media_sources"))
                .unwrap_or_default(),

            // Handle JSON user_data with a default fallback
            user_data: serde_json::from_str(row.get::<&str, _>("user_data")).unwrap_or_else(|_| {
                DiscographySongUserData {
                    playback_position_ticks: 0,
                    play_count: 0,
                    is_favorite: false,
                    played: false,
                    key: "".to_string(),
                }
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
            download_status: serde_json::from_str(row.get::<&str, _>("download_status"))
                .unwrap_or(DownloadStatus::NotDownloaded),
            disliked: row.get::<i32, _>("disliked") != 0,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LibraryView {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "CollectionType")]
    pub collection_type: Option<String>,
    // internal value to whether the library is enabled internally
    #[serde(skip)]
    pub selected: bool,
}

#[derive(Debug, Deserialize)]
struct ViewsResponse {
    #[serde(rename = "Items")]
    pub items: Vec<LibraryView>,
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
    // #[serde(rename = "ProductionYear")]
    // pub production_year: Option<String>,
    #[serde(rename = "PremiereDate", default)]
    pub premiere_date: String,
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
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "ServerId", default)]
    pub server_id: String,
    #[serde(rename = "Id", default)]
    pub id: String,
    #[serde(rename = "DateCreated", default)]
    pub date_created: String,
    #[serde(rename = "ChannelId", default)]
    pub channel_id: Option<String>,
    #[serde(rename = "Genres", default)]
    pub genres: Vec<String>,
    #[serde(rename = "RunTimeTicks", default)]
    pub run_time_ticks: u64,
    #[serde(rename = "IsFolder", default)]
    pub is_folder: bool,
    #[serde(rename = "ParentId", default)]
    pub parent_id: String,
    #[serde(rename = "Type", default)]
    pub type_: String,
    #[serde(rename = "UserData", default)]
    pub user_data: UserData,
    #[serde(rename = "ChildCount", default)]
    pub child_count: u64,
    #[serde(rename = "LocationType", default)]
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

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct QuickConnectState {
    authenticated: bool,
    secret: String,
    code: String,
    // user_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct QuickConnectAuth {
    access_token: String,
    user: UserDto,
    server_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UserDto {
    id: String,
    name: String,
}
