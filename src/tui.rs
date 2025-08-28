/* --------------------------
The main struct of the program. Holds the state and main logic.
    - Gets created in main.rs and the run() function is called in the main loop.
Notable fields:
    - state = main persistent state object (gets deserialized and loaded when you reopen the app)
    - client = HTTP client (client.rs)
    - mpv_thread = MPV thread handle. We use MPV for audio playback.
    - mpv_state = Shared state for controlling MPV. We update this state every frame using a channel from the MPV thread.
        - sender = Sender for the MPV channel.
        - receiver = Receiver for the MPV channel.
    - controls = MPRIS controls. We use MPRIS for media controls.
-------------------------- */
use crate::client::{Album, Artist, Client, DiscographySong, Lyric, Playlist, ProgressReport, TempDiscographyAlbum, Transcoding};
use crate::database::extension::{
    get_album_tracks, get_albums_with_tracks, get_all_albums, get_all_artists, get_all_playlists, get_artists_with_tracks, get_discography, get_lyrics, get_playlist_tracks, get_playlists_with_tracks, insert_lyrics
};
use crate::helpers::{Preferences, State};
use crate::{helpers, mpris, sort};
use crate::popup::PopupState;
use crate::{database, keyboard::*};

use chrono::NaiveDate;
use libmpv2::*;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use tokio::sync::mpsc;

use std::io::Stdout;
use std::collections::HashMap;

use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPosition};

use dirs::data_dir;
use std::path::PathBuf;

use ratatui::{prelude::*, widgets::*, Frame, Terminal};

use ratatui_image::{picker::Picker, protocol::StatefulProtocol};

use std::time::Duration;

use rand::seq::SliceRandom;

/// A type alias for the terminal type used in this application
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use std::thread;
use dialoguer::Select;
use tokio::time::Instant;
use crate::database::database::{Command, DownloadItem, JellyfinCommand, UpdateCommand};
use crate::themes::dialoguer::DialogTheme;

/// This represents the playback state of MPV
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MpvPlaybackState {
    #[serde(default)]
    pub position: f64,
    pub duration: f64,
    pub current_index: i64,
    pub last_index: i64,
    pub volume: i64,
    pub audio_bitrate: i64,
    pub audio_samplerate: i64,
    pub hr_channels: String,
    pub file_format: String,
}

impl Default for MpvPlaybackState {
    fn default() -> Self {
        MpvPlaybackState {
            position: 0.0,
            duration: 0.0,
            current_index: 0,
            last_index: -1,
            volume: 100,
            audio_bitrate: 0,
            audio_samplerate: 0,
            file_format: String::from(""),
            hr_channels: String::from(""),
        }
    }
}

/// Internal song representation. Used in the queue and passed to MPV
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Song {
    pub id: String,
    pub url: String,
    pub name: String,
    pub artist: String,
    pub artist_items: Vec<Artist>,
    pub album: String,
    pub parent_id: String,
    pub production_year: u64,
    pub is_in_queue: bool,
    pub is_transcoded: bool,
    pub is_favorite: bool,
    pub original_index: i64,
    #[serde(default)]
    pub run_time_ticks: u64,
}
#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum Repeat {
    None,
    One,
    #[default]
    All,
}

#[derive(PartialEq, Serialize, Deserialize, Default)]
pub enum Filter {
    Normal,
    #[default]
    FavoritesFirst,
}

#[derive(PartialEq, Serialize, Deserialize, Default)]
pub enum Sort {
    #[default]
    Ascending,
    Descending,

    DateCreated,
    DateCreatedInverse,

    Random,
    PlayCount,

    Duration,
    DurationDesc,

    Title,
    TitleDesc
}

pub struct DatabaseWrapper {
    pub pool: Arc<Pool<Sqlite>>,
    pub cmd_tx: mpsc::Sender<database::database::Command>,
    pub status_rx: mpsc::Receiver<database::database::Status>,
    pub status_tx: mpsc::Sender<database::database::Status>,
}

pub struct App {
    pub exit: bool,
    pub dirty: bool,       // dirty flag for rendering
    pub dirty_clear: bool, // dirty flag for clearing the screen
    pub db_updating: bool, // flag to show if db is processing data
    pub transcoding: Transcoding,

    pub state: State, // main persistent state
    pub preferences: Preferences, // user preferences
    pub server_id: String,

    pub primary_color: Color,              // primary color
    pub config: serde_yaml::Value, // config
    pub auto_color: bool,                  // grab color from cover art (coolest feature ever omg)

    pub original_artists: Vec<Artist>,      // all artists
    pub original_albums: Vec<Album>,        // all albums
    pub original_playlists: Vec<Playlist>,  // playlists

    pub artists: Vec<Artist>,               // all artists
    pub albums: Vec<Album>,                 // all albums
    pub album_tracks: Vec<DiscographySong>, // current album's tracks
    pub playlists: Vec<Playlist>,           // playlists
    pub tracks: Vec<DiscographySong>,       // current artist's tracks
    pub playlist_tracks: Vec<DiscographySong>, // current playlist tracks

    pub lyrics: Option<(String, Vec<Lyric>, bool)>, // ID, lyrics, time_synced
    pub previous_song_parent_id: String,
    pub active_song_id: String,

    pub cover_art: Option<StatefulProtocol>,
    pub cover_art_path: String,
    cover_art_dir: String,
    pub picker: Option<Picker>,

    pub paused: bool,
    pending_seek: Option<f64>, // pending seek
    pub buffering: bool,       // buffering state (spinner)
    pub download_item: Option<DownloadItem>,

    pub spinner: usize, // spinner for buffering
    spinner_skipped: u8,
    pub spinner_stages: Vec<&'static str>,

    pub searching: bool,
    pub show_help: bool,
    pub search_term: String,

    pub locally_searching: bool,

    // this means some new data has been fetched
    pub artists_stale: bool,
    pub albums_stale: bool,
    pub playlists_stale: bool,
    pub discography_stale: bool,
    pub playlist_incomplete: bool,          // we fetch 300 first, and fill the DB with the rest. Speeds up load times of HUGE playlists :)

    // dynamic frame bound heights for page up/down
    pub left_list_height: usize,
    pub track_list_height: usize,

    pub search_result_artists: Vec<Artist>,
    pub search_result_albums: Vec<Album>,
    pub search_result_tracks: Vec<DiscographySong>,

    pub popup: PopupState,
    pub popup_search_term: String, // this is here because popup isn't persisted

    pub client: Option<Arc<Client>>, // jellyfin http client
    pub downloads_dir: PathBuf,

    // mpv is run in a separate thread, this is the handle
    mpv_thread: Option<thread::JoinHandle<()>>,
    pub mpv_state: Arc<Mutex<MpvState>>, // shared mutex for controlling mpv
    pub song_changed: bool,

    pub mpris_paused: bool,
    pub mpris_active_song_id: String,

    // every second, we get the playback state from the mpv thread
    sender: Sender<MpvPlaybackState>,
    pub receiver: Receiver<MpvPlaybackState>,
    // and to avoid a jumpy tui we throttle this update to fast changing values
    pub last_meta_update: Instant,
    last_position_secs: f64,
    scrobble_this: (String, u64), // an id of the previous song we want to scrobble when it ends, and the position in jellyfin ticks
    pub controls: Option<MediaControls>,
    pub db: DatabaseWrapper,
}

impl App {
    pub async fn new(offline: bool, force_server_select: bool) -> Self {

        let config = match crate::config::get_config() {
            Ok(config) => Some(config),
            Err(_) => None,
        }.expect(" ! Failed to load config");

        let (sender, receiver) = channel();
        let (cmd_tx, cmd_rx) = mpsc::channel::<database::database::Command>(100);
        let (status_tx, status_rx) = mpsc::channel::<database::database::Status>(100);

        // try to go online, construct the http client
        let mut client: Option<Arc<Client>> = None;
        let successfully_online = if !offline {
            match App::init_online(&config, force_server_select).await {
                Some(c) => {
                    client = Some(c);
                    true
                }
                None => { false }
            }
        } else {
            false
        };
        if !successfully_online && !offline {
            println!(" ! Connection failed. Running in offline mode.")
        }

        // db init
        let (db_path, server_id) = Self::get_database_file(&config, &client);
        let pool = Self::init_db(&client, &db_path).await
            .unwrap_or_else(|e| {
                println!(" ! Failed to connect to database {}. Error: {}", db_path, e);
                log::error!("Failed to connect to database {}. Error: {}", db_path, e);
                std::process::exit(1);
            });
        let db = DatabaseWrapper {
            pool, cmd_tx, status_tx: status_tx.clone(), status_rx,
        };

        let ( // load initial data
            original_artists, original_albums, original_playlists
        ) = Self::init_library(&db.pool, successfully_online).await;

        // this is the main background thread
        tokio::spawn(database::database::t_database(Arc::clone(&db.pool), cmd_rx, status_tx, successfully_online, client.clone(), server_id.clone()));

        // connect to mpv, set options and default properties
        let mpv_state = Arc::new(Mutex::new(MpvState::new(&config)));

        // mpris
        let controls = match mpris::mpris() {
            Ok(mut controls) => {
                Self::register_controls(&mut controls, mpv_state.clone());
                Some(controls)
            }
            Err(_) => None,
        };

        let (primary_color, picker) = Self::init_theme_and_picker(&config);

        let preferences = Preferences::load().unwrap_or_else(|_| Preferences::new());

        App {
            exit: false,
            dirty: true,
            dirty_clear: false,
            db_updating: false,
            transcoding: Transcoding {
                enabled: preferences.transcoding,
                bitrate: config["transcoding"]["bitrate"]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .unwrap_or(320),
                container: config["transcoding"]["container"]
                    .as_str()
                    .unwrap_or("mp3")
                    .to_string(),
            },
            state: State::new(),
            preferences,
            server_id,
            primary_color,
            config: config.clone(),
            auto_color: config
                .get("auto_color")
                .and_then(|a| a.as_bool())
                .unwrap_or(true),

            original_artists,
            original_albums,
            original_playlists,

            artists: vec![],
            albums: vec![],
            album_tracks: vec![],
            playlists: vec![],
            tracks: vec![],
            playlist_tracks: vec![],

            lyrics: None,
            previous_song_parent_id: String::from(""),
            active_song_id: String::from(""),
            cover_art: None,
            cover_art_path: String::from(""),
            cover_art_dir: data_dir().unwrap_or_else(|| PathBuf::from("./"))
            .join("jellyfin-tui")
            .join("covers")
            .to_str()
            .unwrap_or("")
            .to_string(),
            picker,
            paused: true,

            pending_seek: None,
            buffering: false,
            download_item: None,
            spinner: 0,
            spinner_skipped: 0,
            spinner_stages: vec!["◰", "◳", "◲", "◱"],
            searching: false,
            show_help: false,
            search_term: String::from(""),

            locally_searching: false,

            artists_stale: false,
            albums_stale: false,
            playlists_stale: false,
            discography_stale: false,
            playlist_incomplete: false,

            // these get overwritten in the first run loop
            left_list_height: 0,
            track_list_height: 0,

            search_result_artists: vec![],
            search_result_albums: vec![],
            search_result_tracks: vec![],

            popup: PopupState::default(),
            popup_search_term: String::from(""),

            client,
            downloads_dir: data_dir().unwrap().join("jellyfin-tui").join("downloads"),
            mpv_thread: None,
            mpris_paused: true,
            mpris_active_song_id: String::from(""),
            mpv_state,
            song_changed: false,

            sender,
            receiver,
            last_meta_update: Instant::now(),

            last_position_secs: 0.0,
            scrobble_this: (String::from(""), 0),
            controls,

            db,
        }
    }
}

pub struct MpvState {
    pub mpris_events: Vec<MediaControlEvent>,
    pub mpv: Mpv,
}

impl MpvState {
    fn new(config: &serde_yaml::Value) -> Self {
        let mpv = Mpv::with_initializer(|mpv| {
            mpv.set_option("msg-level", "ffmpeg/demuxer=no").unwrap();
            Ok(())
        })
        .expect(" [XX] Failed to initiate mpv context");
        mpv.set_property("vo", "null").unwrap();
        mpv.set_property("volume", 100).unwrap();
        mpv.set_property("prefetch-playlist", "yes").unwrap(); // gapless playback

        // no console output (it shifts the tui around)
        mpv.set_property("quiet", "yes").ok();
        mpv.set_property("really-quiet", "yes").ok();

        // optional mpv options (hah...)
        if let Some(mpv_config) = config.get("mpv") {
            if let Some(mpv_config) = mpv_config.as_mapping() {
                for (key, value) in mpv_config {
                    if let (Some(key), Some(value)) = (key.as_str(), value.as_str()) {
                        mpv.set_property(key, value).unwrap_or_else(|e| {
                            panic!("This is not a valid mpv property {key}: {:?}", e)
                        });
                        log::info!("Set mpv property: {} = {}", key, value);
                    }
                }
            } else {
                log::error!("mpv config is not a mapping");
            }
        }

        mpv.disable_deprecated_events().unwrap();
        mpv.observe_property("volume", Format::Int64, 0).unwrap();
        mpv.observe_property("demuxer-cache-state", Format::Node, 0).unwrap();
        MpvState {
            mpris_events: vec![],
            mpv,
        }
    }
}

impl App {
    async fn init_online(config: &serde_yaml::Value, force_server_select: bool) -> Option<Arc<Client>> {
        let selected_server = crate::config::select_server(&config, force_server_select)?;
        let mut auth_cache = crate::config::load_auth_cache().unwrap_or_default();
        let maybe_cached = crate::config::find_cached_auth_by_url(&auth_cache, &selected_server.url);
        if let Some((server_id, cached_entry)) = maybe_cached {
            let client = Client::from_cache(
                &selected_server.url,
                server_id,
                cached_entry,
            );
            if client.validate_token().await {
                return Some(client);
            }
            println!(" - Expired auth token, re-authenticating...");
        }
        let client = Client::new(&selected_server).await?;
        if client.access_token.is_empty() {
            println!(" ! Failed to authenticate. Please check your credentials and try again.");
            return None;
        }

        println!(" - Authenticated as {}.", client.user_name);

        auth_cache = crate::config::update_cache_with_new_auth(auth_cache, &selected_server, &client);
        if let Err(e) = crate::config::save_auth_cache(&auth_cache) {
            println!(" ! Failed to update auth cache: {}", e);
        }

        Some(client)
    }

    /// This will return the database path.
    /// If online, it will return the path to the database for the current server.
    /// If offline, it let the user choose which server's database to use.
    fn get_database_file(config: &serde_yaml::Value, client: &Option<Arc<Client>>) -> (String, String) {

        let data_dir = data_dir().unwrap().join("jellyfin-tui");
        let db_directory = data_dir.join("databases");

        if let Some(client) = client {
            return (
                db_directory.join(format!("{}.db", client.server_id)).to_string_lossy().into_owned(),
                client.server_id.clone(),
            )
        }

        let servers = config["servers"]
            .as_sequence()
            .expect(" ! Could not find servers in config file");

        let auth_cache = crate::config::load_auth_cache().unwrap_or_default();

        let available = servers.iter()
            .filter_map(|server| {
                let name = server.get("name")?.as_str()?;
                let url = server.get("url")?.as_str()?;

                let (server_id, _) = auth_cache
                    .iter()
                    .find(|(_, entry)| entry.known_urls.contains(&url.to_string()))?;

                let db_path = format!("{}.db", server_id);
                if db_directory.join(&db_path).exists() {
                    Some((name.to_string(), url.to_string(), db_path, server_id.clone()))
                } else {
                    None
                }
            })
            .collect::<Vec<(String, String, String, String)>>();


        match available.len() {
            0 => {
                println!(" ! There are no offline databases available.");
                std::process::exit(1);
            }
            _ => {
                let choices: Vec<String> = available.iter()
                    .map(|(name, url, _, _)| format!("{} ({})", name, url))
                    .collect();

                let selection = Select::with_theme(&DialogTheme::default())
                    .with_prompt("The following servers are available offline. Select one to use:")
                    .default(0)
                    .items(&choices)
                    .interact()
                    .unwrap();

                let (_, _, db_path, server_id) = &available[selection];
                (
                    db_directory.join(db_path).to_string_lossy().into_owned(),
                    server_id.to_string().replace(".db", "")
                )

            }
        }
    }

    fn init_theme_and_picker(config: &serde_yaml::Value) -> (Color, Option<Picker>) {
        let primary_color = crate::config::get_primary_color(&config);

        let is_art_enabled = config.get("art")
            .and_then(|a| a.as_bool())
            .unwrap_or(true);
        let picker = if is_art_enabled {
            match Picker::from_query_stdio() {
                Ok(picker) => Some(picker),
                Err(_) => {
                    let picker = Picker::from_fontsize((8, 12));
                    Some(picker)
                }
            }
        } else {
            None
        };

        (primary_color, picker)
    }

    async fn init_library(pool: &sqlx::SqlitePool, online: bool) -> (Vec<Artist>, Vec<Album>, Vec<Playlist>) {
        if online {
            let artists = get_all_artists(pool).await.unwrap_or_default();
            let albums = get_all_albums(pool).await.unwrap_or_default();
            let playlists = get_all_playlists(pool).await.unwrap_or_default();
            (artists, albums, playlists)
        } else {
            let artists = get_artists_with_tracks(pool).await.unwrap_or_default();
            let albums = get_albums_with_tracks(pool).await.unwrap_or_default();
            let playlists = get_playlists_with_tracks(pool).await.unwrap_or_default();
            (artists, albums, playlists)
        }
    }

    /// This will re-compute the order of any list that allows sorting and filtering
    pub fn reorder_lists(&mut self) {
        self.artists = self.original_artists.clone();
        self.albums = self.original_albums.clone();
        self.playlists = self.original_playlists.clone();

        self.artists.sort_by(|a, b| {
            sort::compare(&a.name.to_ascii_lowercase(), &b.name.to_ascii_lowercase())
        });
        self.albums.sort_by(|a, b| {
            sort::compare(&a.name.to_ascii_lowercase(), &b.name.to_ascii_lowercase())
        });
        self.playlists.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        });

        match self.preferences.artist_filter {
            Filter::FavoritesFirst => {
                let mut favorites: Vec<_> = self
                    .artists
                    .iter()
                    .filter(|a| a.user_data.is_favorite)
                    .cloned()
                    .collect();
                let mut non_favorites: Vec<_> = self
                    .artists
                    .iter()
                    .filter(|a| !a.user_data.is_favorite)
                    .cloned()
                    .collect();
                match self.preferences.artist_sort {
                    Sort::Ascending => {
                        // this is the default
                    }
                    Sort::Descending => {
                        favorites.reverse();
                        non_favorites.reverse();
                    }
                    Sort::Random => {
                        let mut rng = rand::rng();
                        favorites.shuffle(&mut rng);
                        non_favorites.shuffle(&mut rng);
                    }
                    _ => {}
                }
                self.artists = favorites.into_iter().chain(non_favorites).collect();
            }
            Filter::Normal => {
                match self.preferences.artist_sort {
                    Sort::Ascending => {
                        // this is the default
                    }
                    Sort::Descending => {
                        self.artists.reverse();
                    }
                    Sort::Random => {
                        let mut rng = rand::rng();
                        self.artists.shuffle(&mut rng);
                    }
                    _ => {}
                }
            }
        }
        match self.preferences.album_filter {
            Filter::FavoritesFirst => {
                let mut favorites: Vec<_> = self
                    .albums
                    .iter()
                    .filter(|a| a.user_data.is_favorite)
                    .cloned()
                    .collect();
                let mut non_favorites: Vec<_> = self
                    .albums
                    .iter()
                    .filter(|a: &&Album| !a.user_data.is_favorite)
                    .cloned()
                    .collect();

                // sort by preference
                match self.preferences.album_sort {
                    Sort::Ascending => {
                        // this is the default
                    }
                    Sort::Descending => {
                        favorites.reverse();
                        non_favorites.reverse();
                    }
                    Sort::DateCreated => {
                        favorites.sort_by(|a, b| b.date_created.cmp(&a.date_created));
                        non_favorites.sort_by(|a, b| b.date_created.cmp(&a.date_created));
                    }
                    Sort::Random => {
                        let mut rng = rand::rng();
                        favorites.shuffle(&mut rng);
                        non_favorites.shuffle(&mut rng);
                    }
                    _ => {}
                }
                self.albums = favorites.into_iter().chain(non_favorites).collect();
            }
            Filter::Normal => {
                match self.preferences.album_sort {
                    Sort::Ascending => {
                        // this is the default
                    }
                    Sort::Descending => {
                        self.albums.reverse();
                    }
                    Sort::DateCreated => {
                        self.albums.sort_by(|a, b| b.date_created.cmp(&a.date_created));
                    }
                    Sort::Random => {
                        let mut rng = rand::rng();
                        self.albums.shuffle(&mut rng);
                    }
                    _ => {}
                }
            }
        }
        match self.preferences.playlist_filter {
            Filter::FavoritesFirst => {
                let mut favorites: Vec<_> = self
                    .playlists
                    .iter()
                    .filter(|a| a.user_data.is_favorite)
                    .cloned()
                    .collect();
                let mut non_favorites: Vec<_> = self
                    .playlists
                    .iter()
                    .filter(|a| !a.user_data.is_favorite)
                    .cloned()
                    .collect();
                match self.preferences.playlist_sort {
                    Sort::Ascending => {
                        // this is the default
                    }
                    Sort::Descending => {
                        favorites.reverse();
                        non_favorites.reverse();
                    }
                    Sort::DateCreated => {
                        favorites.sort_by(|a, b| b.date_created.cmp(&a.date_created));
                        non_favorites.sort_by(|a, b| b.date_created.cmp(&a.date_created));
                    }
                    Sort::Random => {
                        let mut rng = rand::rng();
                        favorites.shuffle(&mut rng);
                        non_favorites.shuffle(&mut rng);
                    }
                    _ => {}
                }
                self.playlists = favorites.into_iter().chain(non_favorites).collect();
            }
            Filter::Normal => {
                match self.preferences.playlist_sort {
                    Sort::Ascending => {
                        // this is the default
                    }
                    Sort::Descending => {
                        self.playlists.reverse();
                    }
                    Sort::DateCreated => {
                        self.playlists.sort_by(|a, b| b.date_created.cmp(&a.date_created));
                    }
                    Sort::Random => {
                        let mut rng = rand::rng();
                        self.playlists.shuffle(&mut rng);
                    }
                    _ => {}
                }
            }
        }
    }

    /// This will regroup the tracks into albums
    pub fn group_tracks_into_albums(&mut self, mut tracks: Vec<DiscographySong>, album_order: Option<Vec<String>>) -> Vec<DiscographySong> {
        tracks.retain(|s| !s.id.starts_with("_album_"));
        if tracks.is_empty() {
            return vec![];
        }

        // first we sort the songs by album
        tracks.sort_by(|a, b| a.album_id.cmp(&b.album_id));

        // group the songs by album
        let mut albums: Vec<TempDiscographyAlbum> = vec![];
        let mut current_album = TempDiscographyAlbum {
            songs: vec![],
            id: "".to_string(),
        };

        for mut song in tracks {
            // you wouldn't believe the kind of things i have to deal with
            song.name.retain(|c| c != '\t' && c != '\n');
            song.name = song.name.trim().to_string();

            if current_album.id.is_empty() {
                current_album.id = song.album_id.clone();
            }

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
            current_album = TempDiscographyAlbum {
                id: song.album_id.clone(),
                songs: vec![song],
            };
        }
        albums.push(current_album);

        // sort the songs within each album by indexnumber
        for album in albums.iter_mut() {
            album
                .songs
                .sort_by(|a, b| a.index_number.cmp(&b.index_number));
        }

        if let Some(order) = album_order {
            let order_map: HashMap<&str, usize> = order
                .iter()
                .enumerate()
                .map(|(i, id)| (id.as_str(), i))
                .collect();

            albums.sort_by(|a, b| {
                let ai = order_map.get(a.id.as_str()).copied().unwrap_or(usize::MAX);
                let bi = order_map.get(b.id.as_str()).copied().unwrap_or(usize::MAX);
                ai.cmp(&bi)
            });
        } else {
            albums.sort_by(|a, b| {
                match (
                    NaiveDate::parse_from_str(&a.songs[0].premiere_date, "%Y-%m-%dT%H:%M:%S.%fZ"),
                    NaiveDate::parse_from_str(&b.songs[0].premiere_date, "%Y-%m-%dT%H:%M:%S.%fZ"),
                ) {
                    (Ok(a_date), Ok(b_date)) => b_date.cmp(&a_date),
                    _ => b.songs[0].production_year.cmp(&a.songs[0].production_year),
                }
            });

            match self.preferences.tracks_sort {
                Sort::Ascending => {
                    albums.reverse();
                }
                Sort::Descending => {
                    // default
                }
                Sort::Random => {
                    let mut rng = rand::rng();
                    albums.shuffle(&mut rng);
                }
                Sort::Title => {
                    albums.sort_by(|a, b| a.songs[0].album.cmp(&b.songs[0].album));
                }
                Sort::TitleDesc => {
                    albums.sort_by(|a, b| b.songs[0].album.cmp(&a.songs[0].album));
                }
                Sort::Duration => {
                    albums.sort_by_key(|al| {
                        al.songs.iter().map(|s| s.run_time_ticks).sum::<u64>()
                    });
                }
                Sort::DurationDesc => {
                    albums.sort_by_key(|al| {
                        std::cmp::Reverse(
                            al.songs.iter().map(|s| s.run_time_ticks).sum::<u64>(),
                        )
                    });
                }
                Sort::DateCreated => {
                    albums.sort_by(|a, b| {
                        let parse = |s: &str| {
                            NaiveDate::parse_from_str(s, "%Y-%m-%dT%H:%M:%S.%fZ").ok()
                        };
                        let amax = a.songs.iter().filter_map(|s| parse(&s.date_created)).max();
                        let bmax = b.songs.iter().filter_map(|s| parse(&s.date_created)).max();
                        match (amax, bmax) {
                            (Some(ad), Some(bd)) => bd.cmp(&ad),
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (None, None) => std::cmp::Ordering::Equal,
                        }
                    });
                }
                Sort::DateCreatedInverse => {
                    albums.sort_by(|a, b| {
                        let parse = |s: &str| {
                            NaiveDate::parse_from_str(s, "%Y-%m-%dT%H:%M:%S.%fZ").ok()
                        };
                        let amin = a.songs.iter().filter_map(|s| parse(&s.date_created)).min();
                        let bmin = b.songs.iter().filter_map(|s| parse(&s.date_created)).min();
                        match (amin, bmin) {
                            (Some(ad), Some(bd)) => ad.cmp(&bd),
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (None, None) => std::cmp::Ordering::Equal,
                        }
                    });
                }
                _ => {}
            }
        }

        // sort over parent_index_number to separate into separate disks
        for album in albums.iter_mut() {
            album
                .songs
                .sort_by(|a, b| a.parent_index_number.cmp(&b.parent_index_number));
        }

        // now we flatten the albums back into a list of songs
        let mut songs: Vec<DiscographySong> = vec![];
        for album in albums.into_iter() {
            if album.songs.is_empty() {
                continue;
            }

            // push a dummy song with the album name
            let mut album_song = album.songs[0].clone();
            // let name be Artist - Album - Year
            album_song.name = format!(
                "{} ({})",
                album.songs[0].album, album.songs[0].production_year
            );
            album_song.id = format!("_album_{}", album.id);
            album_song.album_artists = album.songs[0].album_artists.clone();
            album_song.album_id = "".to_string();
            album_song.album_artists = vec![];
            album_song.run_time_ticks = 0;
            album_song.user_data.is_favorite = self.original_albums
                .iter()
                .any(|a| a.id == album.id && a.user_data.is_favorite);
            for song in album.songs.iter() {
                album_song.run_time_ticks += song.run_time_ticks;
            }
            songs.push(album_song);

            for song in album.songs {
                songs.push(song);
            }
        }

        songs
    }

    pub async fn run<'a>(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // startup: we have to wait for mpv to be ready before seeking to previously saved position
        self.handle_pending_seek();

        // get playback state from the mpv thread
        self.receive_mpv_state().ok();

        let current_song = self.state.queue
            .get(self.state.current_playback_state.current_index as usize)
            .cloned()
            .unwrap_or_default();

        if let Ok(mpv) = self.mpv_state.lock() {
            let paused_for_cache = mpv.mpv.get_property("paused-for-cache").unwrap_or(false);
            let seeking = mpv.mpv.get_property("seeking").unwrap_or(false);
            self.buffering = paused_for_cache || seeking;
        }

        self.report_progress_if_needed(&current_song).await?;
        self.handle_song_change(current_song).await?;

        self.handle_database_events().await?;

        self.handle_events().await?;

        self.handle_mpris_events().await;

        Ok(())
    }

    fn handle_pending_seek(&mut self) {
        if let Some(seek) = self.pending_seek {
            if let Ok(mpv) = self.mpv_state.lock() {
                if mpv.mpv.get_property("seekable").unwrap_or(false) {
                    match mpv.mpv.command("seek", &[&seek.to_string(), "absolute"]) {
                        Ok(_) => {
                            self.pending_seek = None;
                            self.dirty = true;
                        }
                        Err(e) => {
                            log::error!(" ! Failed to seek to {}: {}", seek, e);
                        }
                    }
                }
            }
        }
    }

    fn receive_mpv_state(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let state = self.receiver.try_recv()?;
        self.update_playback_state(&state);
        self.update_mpris_metadata();
        self.update_selected_queue_item(&state);
        self.cleanup_played_tracks(&state);
        Ok(())
    }

    fn update_playback_state(&mut self, state: &MpvPlaybackState) {
        self.dirty = true;
        let playback = &mut self.state.current_playback_state;

        playback.position = state.position;
        playback.current_index = state.current_index;
        playback.duration = state.duration;
        playback.volume = state.volume;
        if self.last_meta_update.elapsed() >= Duration::from_secs_f64(2.0) {
            playback.audio_bitrate = state.audio_bitrate / 1000;
            self.last_meta_update = Instant::now();
        }
        playback.hr_channels = state.hr_channels.clone();
        playback.audio_samplerate = state.audio_samplerate;

        if !state.file_format.is_empty() {
            playback.file_format = state.file_format.clone();
        }
    }

    fn update_mpris_metadata(&mut self) {
        if self.active_song_id != self.mpris_active_song_id
            && self.state.current_playback_state.current_index
            != self.state.current_playback_state.last_index
            && self.state.current_playback_state.duration > 0.0
        {
            self.mpris_active_song_id = self.active_song_id.clone();
            let cover_url = format!("file://{}", self.cover_art_path);
            let metadata = match self
                .state
                .queue
                .get(self.state.current_playback_state.current_index as usize)
            {
                Some(song) => {
                    let metadata = MediaMetadata {
                        title: Some(song.name.as_str()),
                        artist: Some(song.artist.as_str()),
                        album: Some(song.album.as_str()),
                        cover_url: Some(cover_url.as_str()),
                        duration: Some(Duration::from_secs(
                            (self.state.current_playback_state.duration) as u64,
                        )),
                    };
                    metadata
                }
                None => MediaMetadata {
                    title: None,
                    artist: None,
                    album: None,
                    cover_url: None,
                    duration: None,
                },
            };

            if let Some(ref mut controls) = self.controls {
                let _ = controls.set_metadata(metadata);
            }
        }

        if self.paused != self.mpris_paused && self.state.current_playback_state.duration > 0.0 {
            self.mpris_paused = self.paused;
            if let Some(ref mut controls) = self.controls {
                let _ = controls.set_playback(if self.paused {
                    souvlaki::MediaPlayback::Paused {
                        progress: Some(MediaPosition(Duration::from_secs_f64(self.state.current_playback_state.position))),
                    }
                } else {
                    souvlaki::MediaPlayback::Playing {
                        progress: Some(MediaPosition(Duration::from_secs_f64(self.state.current_playback_state.position))),
                    }
                });
            }
        }
    }

    fn update_selected_queue_item(&mut self, state: &MpvPlaybackState) {
        if !self.state.selected_queue_item_manual_override {
            self.state
                .selected_queue_item
                .select(Some(state.current_index as usize));
        }
    }

    // temporary queue: remove previously played track(s) (should be just one :))
    fn cleanup_played_tracks(&mut self, state: &MpvPlaybackState) {
        if let Ok(mpv) = self.mpv_state.lock() {
            for i in (0..state.current_index).rev() {
                if let Some(song) = self.state.queue.get(i as usize) {
                    if song.is_in_queue {
                        self.state.queue.remove(i as usize);
                        let _ = mpv.mpv.command("playlist_remove", &[&i.to_string()]);

                        if let Some(selected) = self.state.selected_queue_item.selected() {
                            self.state.selected_queue_item.select(Some(selected - 1));
                            self.state.current_playback_state.current_index -= 1;
                        }
                    }
                }
            }
        }
    }

    async fn report_progress_if_needed(&mut self, song: &Song) -> Result<()> {
        let playback = &self.state.current_playback_state;

        if (self.last_position_secs + 5.0) < playback.position {
            self.last_position_secs = playback.position;

            // every 5 seconds report progress to jellyfin
            self.scrobble_this = (
                song.id.clone(),
                (playback.position * 10_000_000.0) as u64,
            );

            if self.client.is_some() {
                let _ = self.db.cmd_tx.send(Command::Jellyfin(JellyfinCommand::ReportProgress {
                    progress_report: ProgressReport {
                        volume_level: playback.volume as u64,
                        is_paused: self.paused,
                        position_ticks: self.scrobble_this.1,
                        media_source_id: self.active_song_id.clone(),
                        playback_start_time_ticks: 0,
                        can_seek: false,
                        item_id: self.active_song_id.clone(),
                        event_name: "timeupdate".into(),
                    },
                })).await;
            }
        } else if self.last_position_secs > playback.position {
            self.last_position_secs = playback.position;
        }

        Ok(())
    }

    async fn handle_song_change(&mut self, song: Song) -> Result<()> {
        if song.id == self.active_song_id && !self.song_changed {
            return Ok(()); // song hasn't changed since last run
        }

        self.song_changed = false;
        self.active_song_id = song.id.clone();
        self.state.selected_lyric_manual_override = false;
        self.state.selected_lyric.select(None);
        self.state.current_lyric = 0;

        self.set_lyrics().await?;
        let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::SongPlayed {
            track_id: song.id.clone(),
        })).await;

        if self.client.is_some() {
            // Scrobble. The way to do scrobbling in jellyfin is using the last.fm jellyfin plugin.
            // Essentially, this event should be sent either way, the scrobbling is purely server side and not something we need to worry about.
            if !self.scrobble_this.0.is_empty() {
                let _ = self.db.cmd_tx.send(Command::Jellyfin(JellyfinCommand::Stopped {
                    id: self.scrobble_this.0.clone(),
                    position_ticks: self.scrobble_this.1.clone()
                })).await;
                self.scrobble_this = (String::new(), 0);
            }
            let _ = self.db.cmd_tx.send(Command::Jellyfin(JellyfinCommand::Playing {
                id: self.active_song_id.clone(),
            })).await;
        }

        self.update_cover_art(&song).await;

        Ok(())
    }

    async fn set_lyrics(&mut self) -> Result<()> {
        if self.active_song_id.is_empty() {
            return Ok(());
        }
        if let Some(client) = self.client.as_mut() {
            self.lyrics = client.lyrics(&self.active_song_id).await.ok().map(|lyrics| {
                let time_synced = lyrics.iter().all(|l| l.start != 0);
                (self.active_song_id.clone(), lyrics, time_synced)
            });
            if let Some((_, lyrics, _)) = &self.lyrics {
                let _ = insert_lyrics(&self.db.pool, &self.active_song_id, lyrics).await;
            }
            return Ok(());
        }

        self.lyrics = None;
        if let Ok(lyrics) = get_lyrics(&self.db.pool, &self.active_song_id).await {
            let time_synced = lyrics.iter().all(|l| l.start != 0);
            self.lyrics = Some((self.active_song_id.clone(), lyrics, time_synced));
            self.state.selected_lyric.select(None);
        }

        Ok(())
    }

    async fn update_cover_art(&mut self, song: &Song) {
        if self.previous_song_parent_id != song.parent_id || self.cover_art.is_none() {
            self.previous_song_parent_id = song.parent_id.clone();
            self.cover_art = None;
            self.cover_art_path.clear();

            if let Ok(cover_image) = self.get_cover_art(&song.parent_id).await {
                let p = format!("{}/{}", self.cover_art_dir, cover_image);

                if let Ok(reader) = image::ImageReader::open(&p) {
                    if let Ok(img) = reader.decode() {
                        if let Some(picker) = &mut self.picker {
                            let image_fit_state = picker.new_resize_protocol(img.clone());
                            self.cover_art = Some(image_fit_state);
                            self.cover_art_path = p.clone();
                        }
                        if self.auto_color {
                            self.grab_primary_color(&p);
                        }
                    } else {
                        self.primary_color = crate::config::get_primary_color(&self.config);
                    }
                }
            } else {
                self.primary_color = crate::config::get_primary_color(&self.config);
            }
        }
    }

    pub async fn draw<'a>(
        &mut self,
        terminal: &'a mut Tui,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        if self.dirty_clear {
            terminal.clear()?;
            self.dirty_clear = false;
            self.dirty = true;
        }

        // let the rats take over
        if self.dirty {
            terminal.draw(|frame: &mut Frame| {
                self.render_frame(frame);
            })?;
            self.dirty = false;
        }

        // ratatui is an immediate mode tui which is cute, but it will be heavy on the cpu
        // we use a dirty draw flag and thread::sleep to throttle the bool check a bit
        tokio::time::sleep(Duration::from_millis(2)).await;

        Ok(())
    }

    /// This is the main render function for rataui. It's called every frame.
    pub fn render_frame<'a>(&mut self, frame: &'a mut Frame) {
        let app_container = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Min(1), Constraint::Percentage(100)])
            .split(frame.area());

        // render tabs
        self.render_tabs(app_container[0], frame.buffer_mut());

        match self.state.active_tab {
            ActiveTab::Library => {
                if self.show_help {
                    self.render_home_help(app_container[1], frame);
                } else {
                    self.render_home(app_container[1], frame);
                }
            }
            ActiveTab::Albums => {
                if self.show_help {
                    self.render_home_help(app_container[1], frame);
                } else {
                    self.render_home(app_container[1], frame);
                }
            }
            ActiveTab::Playlists => {
                if self.show_help {
                    self.render_playlists_help(app_container[1], frame);
                } else {
                    self.render_playlists(app_container[1], frame);
                }
            }
            ActiveTab::Search => {
                self.render_search(app_container[1], frame);
            }
        }

        self.spinner_skipped += 1;
        if self.spinner_skipped > 5 {
            self.spinner_skipped = 0;
            self.spinner += 1;
            if self.spinner > self.spinner_stages.len() - 1 {
                self.spinner = 0;
            }
        }
    }

    fn render_tabs(&self, area: Rect, buf: &mut Buffer) {
        // split the area into left and right
        let tabs_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(70),
                Constraint::Percentage(30),
                Constraint::Min(15),
            ])
            .split(area);

        Tabs::new(vec!["Library", "Albums", "Playlists", "Search"])
            .style(Style::default().white().dim())
            .highlight_style(Style::default().white().not_dim())
            .select(self.state.active_tab as usize)
            .divider(symbols::DOT)
            .padding(" ", " ")
            .render(tabs_layout[0], buf);

        let mut status_bar: Vec<Span> = vec![];

        if self.client.is_none() {
            status_bar.push(Span::raw("(offline)").white());
        }

        let updating = format!(
            "{} Updating",
            &self.spinner_stages[self.spinner],
        );
        if self.db_updating {
            status_bar.push(Span::raw(updating).fg(self.primary_color));
        }

        status_bar.push(Span::from(
            match self.preferences.repeat {
                Repeat::None => "",
                Repeat::One => "R1",
                Repeat::All => "R*",
            }
        ).white());

        let transcoding = if self.transcoding.enabled {
            format!(
                "[{}@{}]",
                self.transcoding.container, self.transcoding.bitrate
            )
        } else {
            String::new()
        };
        if !transcoding.is_empty() {
            status_bar.push(Span::raw(&transcoding).white());
        }

        let volume_color = match self.state.current_playback_state.volume {
            0..=100 => Color::White,
            101..=120 => Color::Yellow,
            _ => Color::Red,
        };

        let mut spaced = Vec::new();
        let mut iterator = status_bar.into_iter();
        if let Some(first) = iterator.next() {
            spaced.push(first);
            for span in iterator {
                if span.content.is_empty() {
                    continue;
                }
                spaced.push(Span::raw(" ").white());
                spaced.push(span);
            }
        }

        Paragraph::new(Line::from(spaced))
            .alignment(Alignment::Right)
            .wrap(Wrap { trim: false })
            .render(tabs_layout[1], buf);

        LineGauge::default()
            .block(Block::default().padding(Padding::horizontal(1)))
            .filled_style(
                Style::default()
                    .fg(volume_color)
                    .add_modifier(Modifier::BOLD),
            )
            .label(
                Line::from(format!("{}%", self.state.current_playback_state.volume))
                    .style(Style::default().fg(volume_color)),
            )
            .unfilled_style(
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .line_set(symbols::line::ROUNDED)
            .ratio((self.state.current_playback_state.volume as f64 / 100_f64).min(1.0))
            .render(tabs_layout[2], buf);
    }

    /// Fetch the discography of an artist
    /// This will change the active section to tracks
    pub async fn discography(&mut self, id: &str) {
        self.discography_stale = false;
        if id.is_empty() {
            return;
        }
        self.tracks = vec![];

        // we first try the database. If there are no tracks, or an error, we try the online route.
        // after an offline pull, we query for updates in the background
        // TODO: this can be compacted
        match get_discography(&self.db.pool, id, self.client.as_ref()).await {
            Ok(tracks) if !tracks.is_empty() => {
                self.state.active_section = ActiveSection::Tracks;
                self.tracks = self.group_tracks_into_albums(tracks, None);
                // run the update query in the background
                let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::Discography {
                    artist_id: id.to_string(),
                })).await;
            }
            // if we get here, it means the DB call returned either
            // empty tracks, or an error. We'll try the pure online route next.
            _ => {
                if let Some(client) = self.client.as_ref() {
                    if let Ok(tracks) = client.discography(id).await {
                        self.state.active_section = ActiveSection::Tracks;
                        self.tracks = self.group_tracks_into_albums(tracks, None);
                        let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::Discography {
                            artist_id: id.to_string(),
                        })).await;
                    }
                } else {
                    // a catch-all for db errors
                    let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::OfflineRepair)).await;
                }
            }
        }
        self.state.tracks_scroll_state = ScrollbarState::new(
            std::cmp::max(0, self.tracks.len() as i32 - 1) as usize
        );
        self.state.current_artist = self
            .artists
            .iter()
            .find(|a| a.id == id)
            .cloned()
            .unwrap_or_default();
    }

    pub async fn album_tracks(&mut self, album_id: &String) {
        self.album_tracks = vec![];

        let album = match self
            .albums
            .iter()
            .find(|a| a.id == *album_id)
            .cloned() {
            Some(album) => album,
            None => {
                return;
            }
        };
        // we first try the database. If there are no tracks, or an error, we try the online route.
        // after an offline pull, we query for updates in the background
        match get_album_tracks(&self.db.pool, &album.id, self.client.as_ref()).await {
            Ok(tracks) if !tracks.is_empty() => {
                self.state.active_section = ActiveSection::Tracks;
                self.album_tracks = tracks;
            }
            _ => {
                if let Some(client) = self.client.as_ref() {
                    if let Ok(tracks) = client.album_tracks(&album.id).await {
                        self.state.active_section = ActiveSection::Tracks;
                        self.album_tracks = tracks;
                    }
                } else {
                    let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::OfflineRepair)).await;
                }
            }
        }
        self.state.album_tracks_scroll_state =
            ScrollbarState::new(
                std::cmp::max(0, self.album_tracks.len() as i32 - 1) as usize
            );
        self.state.current_album = self
            .albums
            .iter()
            .find(|a| a.id == *album.id)
            .cloned()
            .unwrap_or_default();

        if self.client.is_none() {
            return;
        }

        for artist in &album.album_artists {
            let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::Discography {
                artist_id: artist.id.clone(),
            })).await;
        }
    }

    pub async fn playlist(&mut self, album_id: &String, limit: bool) {
        self.playlist_incomplete = false;
        let playlist = match self.playlists.iter().find(|a| a.id == *album_id).cloned() {
            Some(playlist) => playlist,
            None => {
                return;
            }
        };
        self.playlist_tracks = vec![];
        // we first try the database. If there are no tracks, or an error, we try the online route.
        // after an offline pull, we query for updates in the background
        match get_playlist_tracks(&self.db.pool, &playlist.id, self.client.as_ref()).await {
            Ok(tracks) if !tracks.is_empty() => {
                self.state.active_section = ActiveSection::Tracks;
                self.playlist_tracks = tracks;
            }
            _ => {
                if let Some(client) = self.client.as_ref() {
                    if let Ok(tracks) = client.playlist(&playlist.id, limit).await {
                        self.state.active_section = ActiveSection::Tracks;
                        self.playlist_tracks = tracks.items;
                        if self.playlist_tracks.len() != tracks.total_record_count as usize {
                            self.playlist_incomplete = true;
                        }
                    }
                } else {
                    let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::OfflineRepair)).await;
                }
            }
        }
        self.state.playlist_tracks_scroll_state =
            ScrollbarState::new(
                std::cmp::max(0, self.playlist_tracks.len() as i32 - 1) as usize
            );
        self.state.current_playlist = self
            .playlists
            .iter()
            .find(|a| a.id == *playlist.id)
            .cloned()
            .unwrap_or_default();

        if self.client.is_none() {
            return;
        }

        let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::Playlist {
            playlist_id: playlist.id.clone(),
        })).await;
    }

    pub async fn mpv_start_playlist(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let sender = self.sender.clone();
        let songs = self.state.queue.clone();

        if self.mpv_thread.is_some() {
            if let Ok(mpv) = self.mpv_state.lock() {
                let _ = mpv.mpv.command("stop", &[]);
                for song in &songs {
                    match helpers::normalize_mpvsafe_url(&song.url) {
                        Ok(safe_url) => {
                            let _ = mpv.mpv.command("loadfile", &[safe_url.as_str(), "append-play"]);
                        }
                        Err(e) => {
                            log::error!("Failed to normalize URL '{}': {:?}", song.url, e);
                            if e.to_string().contains("No such file or directory") {
                                let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::OfflineRepair)).await;
                            }
                        }
                    }
                }
                let _ = mpv.mpv.set_property("pause", false);
                self.paused = false;
                self.song_changed = true;
            }
            return Ok(());
        }

        let mpv_state = self.mpv_state.clone();
        if let Some(ref mut controls) = self.controls {
            if controls.detach().is_ok() {
                App::register_controls(controls, mpv_state.clone());
            }
        }

        let repeat = self.preferences.repeat.clone();

        let mut state = MpvPlaybackState::default();
        state.current_index = self.state.current_playback_state.current_index;
        state.volume = self.state.current_playback_state.volume;
        state.last_index = self.state.current_playback_state.last_index;
        state.position = self.state.current_playback_state.position;
        state.duration = self.state.current_playback_state.duration;

        self.mpv_thread = Some(thread::spawn(move || {
            if let Err(e) = Self::t_playlist(songs, mpv_state, sender, state, repeat) {
                log::error!("Error in mpv playlist thread: {}", e);
            }
        }));

        self.paused = false;

        Ok(())
    }

    /// The thread that keeps in sync with the mpv thread
    fn t_playlist(
        songs: Vec<Song>,
        mpv_state: Arc<Mutex<MpvState>>,
        sender: Sender<MpvPlaybackState>,
        state: MpvPlaybackState,
        repeat: Repeat,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mpv = mpv_state
            .lock()
            .map_err(|e| format!("Failed to lock mpv_state: {:?}", e))?;

        let _ = mpv.mpv.command("playlist_clear", &["force"]);

        for song in songs {
            match helpers::normalize_mpvsafe_url(&song.url) {
                Ok(safe_url) => {
                    let _ = mpv.mpv.command("loadfile", &[safe_url.as_str(), "append-play"]);
                }
                Err(e) => log::error!("Failed to normalize URL '{}': {:?}", song.url, e),
            }
        }

        mpv.mpv.set_property("volume", state.volume)?;
        mpv.mpv.set_property("playlist-pos", state.current_index)?;

        match repeat {
            Repeat::None => {
                let _ = mpv.mpv.set_property("loop-file", "no");
                let _ = mpv.mpv.set_property("loop-playlist", "no");
            }
            Repeat::All => {
                let _ = mpv.mpv.set_property("loop-playlist", "inf");
            }
            Repeat::One => {
                let _ = mpv.mpv.set_property("loop-playlist", "no");
                let _ = mpv.mpv.set_property("loop-file", "inf");
            }
        }

        drop(mpv);

        loop {
            // main mpv loop
            let mpv = mpv_state
                .lock()
                .map_err(|e| format!("Failed to lock mpv_state: {:?}", e))?;

            let position = mpv.mpv.get_property("time-pos").unwrap_or(0.0);
            let current_index: i64 = mpv.mpv.get_property("playlist-pos").unwrap_or(0);
            let duration = mpv.mpv.get_property("duration").unwrap_or(0.0);
            let volume = mpv.mpv.get_property("volume").unwrap_or(0);
            let audio_bitrate = mpv.mpv.get_property("audio-bitrate").unwrap_or(0);
            let audio_samplerate = mpv.mpv.get_property("audio-params/samplerate").unwrap_or(0);
            // let audio_channels = mpv.mpv.get_property("audio-params/channel-count").unwrap_or(0);
            // let audio_format: String = mpv.mpv.get_property("audio-params/format").unwrap_or_default();
            let hr_channels: String = mpv.mpv.get_property("audio-params/hr-channels").unwrap_or_default();

            let file_format: String = mpv
                .mpv.get_property("file-format")
                .unwrap_or_default();
            drop(mpv);

            let _ = sender.send({
                MpvPlaybackState {
                    position,
                    duration,
                    current_index,
                    last_index: state.last_index,
                    volume,
                    audio_bitrate,
                    audio_samplerate,
                    hr_channels,
                    file_format: file_format.to_string(),
                }
            });

            thread::sleep(Duration::from_secs_f32(0.2));
        }
    }

    async fn get_cover_art(&mut self, album_id: &String) -> std::result::Result<String, Box<dyn std::error::Error>> {
        if album_id.is_empty() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Album ID is empty",
            )));
        }
        let data_dir = data_dir().unwrap();

        // check if the file already exists
        let files = std::fs::read_dir(data_dir.join("jellyfin-tui").join("covers"))?;
        for file in files {
            if let Ok(entry) = file {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if file_name.contains(album_id) {
                    return Ok(file_name);
                }
            }
        }

        if let Some(client) = &self.client {
            if let Ok(cover_art) = client.download_cover_art(&album_id).await {
                return Ok(cover_art);
            }
        }

        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Cover art not found",
        )))
    }

    pub fn get_image_buffer(img: image::DynamicImage) -> (Vec<u8>, color_thief::ColorFormat) {
        let rgba = img.to_rgba8();
        (rgba.to_vec(), color_thief::ColorFormat::Rgba)
    }

    fn grab_primary_color(&mut self, p: &str) {
        let img = match image::open(p) {
            Ok(img) => img,
            Err(_) => return,
        };
        let (buffer, color_type) = Self::get_image_buffer(img);
        if let Ok(colors) = color_thief::get_palette(&buffer, color_type, 10, 8) {
            let mut prominent_color = colors
                .iter()
                .filter(|color| {
                    // filter out too dark or light colors
                    let brightness =
                        0.299 * color.r as f32 + 0.587 * color.g as f32 + 0.114 * color.b as f32;
                    brightness > 50.0 && brightness < 200.0
                })
                .max_by_key(|color| {
                    let maxc = color.r.max(color.g).max(color.b) as i32;
                    let minc = color.r.min(color.g).min(color.b) as i32;
                    let contrast = maxc - minc;

                    // saturation = (contrast / maxc) in 0..1 range
                    let saturation = if maxc == 0 { 0.0 } else { (maxc - minc) as f32 / maxc as f32 };
                    let sat_bonus = (saturation * 100.0) as i32;

                    // penalize mid-tone orange (r > g > b) a bit (I'm an orange hater)
                    let brightness =
                        0.299 * color.r as f32 + 0.587 * color.g as f32 + 0.114 * color.b as f32;
                    let orangey = color.r > color.g && color.g > color.b && (color.r as i32 - color.b as i32) > 40;
                    let midtone = brightness > 80.0 && brightness < 180.0;
                    let penalty = if orangey && midtone { -50 } else { 0 };
                    let near_white_penalty = if brightness > 200.0 && saturation < 0.118 { -180 } else { 0 };

                    contrast + penalty + sat_bonus + near_white_penalty
                })
                .unwrap_or(&colors[0]);

            // last ditch effort to avoid gray colors
            let maxc = prominent_color.r.max(prominent_color.g).max(prominent_color.b) as i32;
            let minc = prominent_color.r.min(prominent_color.g).min(prominent_color.b) as i32;
            let contrast = maxc - minc;
            let near_gray = (prominent_color.r as i32 - prominent_color.g as i32).abs() < 15
                && (prominent_color.g as i32 - prominent_color.b as i32).abs() < 15
                || (maxc > 0 && (contrast as f32 / maxc as f32) < 0.20);

            if near_gray {
                if let Some(c) = colors.iter().max_by_key(|c| {
                    let maxc = c.r.max(c.g).max(c.b) as i32;
                    let minc = c.r.min(c.g).min(c.b) as i32;
                    maxc - minc
                }) {
                    prominent_color = c;
                }
            }

            let max_chan = prominent_color.r.max(prominent_color.g).max(prominent_color.b);
            let scale = if max_chan == 0 { 1.0 } else { 255.0 / max_chan as f32 };
            let mut r = (prominent_color.r as f32 * scale) as u8;
            let mut g = (prominent_color.g as f32 * scale) as u8;
            let mut b = (prominent_color.b as f32 * scale) as u8;

            // enhance contrast against black and white
            let brightness = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
            if brightness < 80.0 {
                r = r.saturating_add(50);
                g = g.saturating_add(50);
                b = b.saturating_add(50);
            } else if brightness > 200.0 {
                r = r.saturating_sub(50);
                g = g.saturating_sub(50);
                b = b.saturating_sub(50);
            }

            self.primary_color = Color::Rgb(r, g, b);
        }
    }

    pub fn save_state(&self) {
        let persist = self
            .config
            .get("persist")
            .and_then(|a| a.as_bool())
            .unwrap_or(true);
        if !persist {
            return;
        }
        if let Err(e) = self.state.save(&self.server_id, self.client.is_none()) {
            log::error!(
                "[XX] Failed to save state This is most likely a bug: {:?}",
                e
            );
        }
    }

    pub async fn load_state(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {

        self.state.artists_scroll_state = ScrollbarState::new(self.artists.len().saturating_sub(1));
        self.state.active_section = ActiveSection::List;
        self.state.selected_artist.select_first();
        self.state.selected_album.select_first();
        self.state.selected_playlist.select_first();

        let persist = self.config
            .get("persist")
            .and_then(|a| a.as_bool())
            .unwrap_or(true);
        if !persist {
            self.reorder_lists();
            return Ok(());
        }

        let offline = self.client.is_none();
        self.state = State::load(&self.server_id, offline)?;

        let mut needs_repair = false;
        self.state.queue.retain(|song| {
            match helpers::normalize_mpvsafe_url(&song.url) {
                Ok(_) => true,
                Err(e) => {
                    log::warn!("Removed song with invalid URL '{}': {:?}", song.url, e);
                    if e.to_string().contains("No such file or directory") {
                        needs_repair = true;
                    }
                    false
                }
            }
        });
        if needs_repair {
            let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::OfflineRepair)).await;
        }

        self.reorder_lists();

        // set the previous song as current
        if let Some(current_song) = self.state.queue.get(self.state.current_playback_state.current_index as usize).cloned() {
            self.active_song_id = current_song.id.clone();
            let _ = self.db.cmd_tx.send(Command::Update(UpdateCommand::SongPlayed {
                track_id: current_song.id.clone(),
            })).await;
            self.update_cover_art(&current_song).await;
        }
        // load lyrics
        self.set_lyrics().await?;

        self.buffering = true;

        let current_artist_id = self.state.current_artist.id.clone();
        let current_album_id = self.state.current_album.id.clone();
        let current_playlist_id = self.state.current_playlist.id.clone();

        let active_section = self.state.active_section;

        self.discography(&current_artist_id).await;
        self.album_tracks(&current_album_id).await;
        self.playlist(&current_playlist_id, true).await;

        self.state.active_section = active_section;

        // Ensure correct scrollbar state and selection
        let index = self.state.selected_artist.selected().unwrap_or(0);
        self.artist_select_by_index(index);
        let index = self.state.selected_playlist.selected().unwrap_or(0);
        self.playlist_select_by_index(index);
        let index = self.state.selected_track.selected().unwrap_or(0);
        self.track_select_by_index(index);
        let index = self.state.selected_playlist_track.selected().unwrap_or(0);
        self.playlist_track_select_by_index(index);
        let index = self.state.selected_album.selected().unwrap_or(0);
        self.album_select_by_index(index);
        let index = self.state.selected_album_track.selected().unwrap_or(0);
        self.album_track_select_by_index(index);

        #[cfg(target_os = "linux")]
        {
            if let Some(ref mut controls) = self.controls {
                let _ = controls.set_volume(self.state.current_playback_state.volume as f64 / 100.0);
            }
        }

        // handle expired session token in urls
        if let Some(client) = self.client.as_mut() {
            for song in &mut self.state.queue {
                song.url = client.song_url_sync(&song.id, &self.transcoding);
            }
        }

        let _ = self.mpv_start_playlist().await;

        if let Ok(mpv) = self.mpv_state.lock() {
            let _ = mpv.mpv.set_property("pause", true);
            self.paused = true;
        }

        // unfortunately while transcoding it doesn't know the duration immediately and stalls
        if self.state.current_playback_state.position > 0.1 && !self.transcoding.enabled {
            self.pending_seek = Some(self.state.current_playback_state.position);
        }

        println!(" - Session restored");
        Ok(())
    }

    pub async fn exit(&mut self) {
        self.save_state();
        if let Err(e) = self.preferences.save() {
            log::error!("Failed to save preferences: {:?}", e);
        }
        self.exit = true;
    }
}
