use std::{fmt, path::PathBuf};
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use sqlx::{migrate::MigrateDatabase, FromRow, Pool, Row, Sqlite, SqlitePool};
use crate::{
    client::{Album, Artist, Client, DiscographySong, Lyric, Playlist},
    database::database::data_updater,
    keyboard::ActiveSection,
    popup::PopupMenu,
    tui
};

use super::database::{DownloadItem, Status};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum DownloadStatus {
    Downloaded,
    Queued,
    Downloading,
    #[default]
    NotDownloaded,
}

impl fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DownloadStatus::Downloaded => "Downloaded",
            DownloadStatus::Queued => "Queued",
            DownloadStatus::Downloading => "Downloading",
            DownloadStatus::NotDownloaded => "NotDownloaded",
        };
        write!(f, "{}", s)
    }
}

impl<'r> FromRow<'r, sqlx::sqlite::SqliteRow> for DownloadStatus {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        let status: String = row.get(0);
        match status.as_str() {
            "Downloaded" => Ok(DownloadStatus::Downloaded),
            "Queued" => Ok(DownloadStatus::Queued),
            "Downloading" => Ok(DownloadStatus::Downloading),
            _ => Ok(DownloadStatus::NotDownloaded),
        }
    }
}

impl tui::App {
    pub async fn handle_database_events(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let status = self.db.status_rx.try_recv();
        match status {
            Ok(status) => self.handle_database_status(status).await,
            Err(_) => return Ok(()),
        }
        Ok(())
    }

    async fn handle_database_status(&mut self, status: Status) {
        match status {
            Status::AllDownloaded => {
                // pretty nifty huh
                if let Some(popup) = &mut self.popup.current_menu {
                    if let PopupMenu::GlobalRoot { downloading, .. } = popup {
                        *downloading = false;
                    }
                }
                self.download_item = None;
            }
            Status::ProgressUpdate { progress } => {
                 if let Some(download_item) = &mut self.download_item {
                    download_item.progress = progress;
                }
            }
            Status::TrackQueued { id } => {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::Queued;
                }
                if let Some(track) = self.album_tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::Queued;
                }
                if let Some(track) = self.playlist_tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::Queued;
                }
            }
            Status::TrackDownloaded { id } => {
                if let Some(download_item) = &mut self.download_item {
                    download_item.progress = 100.0;
                }
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::Downloaded;
                }
                if let Some(track) = self.album_tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::Downloaded;
                }
                if let Some(track) = self.playlist_tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::Downloaded;
                }
            }
            Status::TrackDownloading { track } => {
                self.download_item = Some(DownloadItem {
                    name: track.name,
                    progress: 0.0,
                });
                if let Some(popup) = &mut self.popup.current_menu {
                    if let PopupMenu::GlobalRoot { downloading, .. } = popup {
                        *downloading = true;
                    }
                }
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == track.id) {
                    track.download_status = DownloadStatus::Downloading;
                }
                if let Some(track) = self.album_tracks.iter_mut().find(|t| t.id == track.id) {
                    track.download_status = DownloadStatus::Downloading;
                }
                if let Some(track) = self.playlist_tracks.iter_mut().find(|t| t.id == track.id) {
                    track.download_status = DownloadStatus::Downloading;
                }
            }
            Status::TrackDeleted { id } => {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::NotDownloaded;
                }
                if let Some(track) = self.album_tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::NotDownloaded;
                }
                if let Some(track) = self.playlist_tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::NotDownloaded;
                }

                if self.client.is_some() {
                    return;
                }

                // if we are offline, we of course don't want to see deleted tracks
                // some may call me lazy, i call it being efficient
                if self.tracks.is_empty() || self.album_tracks.is_empty() || self.playlist_tracks.is_empty() {
                    self.original_artists = get_artists_with_tracks(&self.db.pool).await.unwrap_or_default();
                    self.original_albums = get_albums_with_tracks(&self.db.pool).await.unwrap_or_default();
                    self.original_playlists = get_playlists_with_tracks(&self.db.pool).await.unwrap_or_default();
                    self.reorder_lists();
                }
            }
            Status::ArtistsUpdated => {
                self.artists_stale = true;
            }
            Status::AlbumsUpdated => {
                self.albums_stale = true;
            }
            Status::PlaylistsUpdated => {
                self.playlists_stale = true;
            }
            Status::DiscographyUpdated { id } => {
                if self.state.current_artist.id == id {
                    match get_discography(&self.db.pool, self.state.current_artist.id.as_str(), self.client.as_ref())
                        .await
                    {
                        Ok(tracks) if !tracks.is_empty() => {
                            self.tracks = self.group_tracks_into_albums(tracks);
                        }
                        _ => {}
                    }
                }
                if self.state.current_album.album_artists.iter().any(|a| a.id == id) {
                    match get_album_tracks(&self.db.pool, self.state.current_album.id.as_str(), self.client.as_ref())
                        .await
                    {
                        Ok(tracks) if !tracks.is_empty() => {
                            self.album_tracks = tracks;
                        }
                        _ => {}
                    }
                }
            }
            Status::PlaylistUpdated { id } => {
                if self.state.current_playlist.id == id {
                    if let Ok(tracks) = get_playlist_tracks(&self.db.pool, self.state.current_playlist.id.as_str(), self.client.as_ref()).await {
                        if !tracks.is_empty() {
                            self.playlist_tracks = tracks;
                        }
                    }
                    self.playlist_incomplete = false;
                }
            }
            Status::UpdateStarted => { 
                self.db_updating = true;
            }
            Status::UpdateFinished => {
                if self.client.is_none() {
                    self.original_artists = get_artists_with_tracks(&self.db.pool).await.unwrap_or_default();
                    self.original_albums = get_albums_with_tracks(&self.db.pool).await.unwrap_or_default();
                    self.original_playlists = get_playlists_with_tracks(&self.db.pool).await.unwrap_or_default();
                    self.reorder_lists();
                }
                self.db_updating = false;
            }
            Status::UpdateFailed { error } => {
                self.state.last_section = self.state.active_section;
                self.state.active_section = ActiveSection::Popup;
                self.set_generic_message(
                    "Background update failed, please restart the app", &error,
                );
                self.db_updating = false;
            }
            Status::Error { error } => {
                self.state.last_section = self.state.active_section;
                self.state.active_section = ActiveSection::Popup;
                self.set_generic_message(
                    "Background Error (please report)", &error,
                );
            }
        }
    }

    /// Create a database if it doesn't exist. Perform any necessary initialization / migrations etc
    ///
    /// TODO: change to migrations - https://david.rothlis.net/declarative-schema-migration-for-sqlite/
    pub async fn init_db(
        client: &Option<Arc<Client>>,
        db_path: &String,
    ) -> Result<Arc<Pool<Sqlite>>, Box<dyn std::error::Error>> {
        if !Sqlite::database_exists(db_path).await.unwrap_or(false) {
            if client.is_none() {
                return Err("Database does not exist and you are offline. Please connect to the internet and try again.".into());
            }
            let client = client.as_ref().unwrap().clone();

            println!(" ! Creating database {}", db_path);
            Sqlite::create_database(db_path).await?;

            let pool = Arc::new(SqlitePool::connect(db_path)
                    .await
                    .unwrap_or_else(|_| core::panic!("Fatal error, failed to connect to new database. Please remove it and try again: {}", db_path)));

            create_tables(&pool).await?;

            println!(" - Database created. Fetching data...");

            if let Err(e) = data_updater(Arc::clone(&pool), None, client).await {
                return Err(e);
            }
            pool.close().await;
        }

        let pool = Arc::new(
            SqlitePool::connect(db_path)
                .await
                .unwrap_or_else(|_| core::panic!("Fatal error, failed to connect to database: {}", db_path)),
        );
        sqlx::query("PRAGMA journal_mode = WAL;").execute(&*pool).await.unwrap();

        log::info!(" - Database connected: {}", db_path);

        let total_download_size: i64 = sqlx::query_scalar(
            "SELECT SUM(download_size_bytes) FROM tracks WHERE download_status = 'Downloaded'",
        ).fetch_one(&*pool).await.unwrap_or(0);

        if total_download_size > 0 {
            let total_download_size_human = if total_download_size < 1024 {
                format!("{} B", total_download_size)
            } else if total_download_size < 1024 * 1024 {
                format!("{:.2} KB", total_download_size as f64 / 1024.0)
            } else if total_download_size < 1024 * 1024 * 1024 {
                format!("{:.2} MB", total_download_size as f64 / (1024.0 * 1024.0))
            } else {
                format!("{:.2} GB", total_download_size as f64 / (1024.0 * 1024.0 * 1024.0))
            };
            println!(" - Total download size for this server: {}", total_download_size_human);
        }

        Ok(pool)
    }
}

/// ------------ helpers ------------
///
async fn create_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tracks (
            id TEXT PRIMARY KEY,
            album_id TEXT NOT NULL,
            artist_items TEXT NOT NULL,
            download_status TEXT NOT NULL,
            download_size_bytes INTEGER,
            track TEXT NOT NULL,
            last_played TIMESTAMP,
            downloaded_at TIMESTAMP
        );
        "#,
    )
    .execute(pool)
    .await?;

    // this client uses DiscographySong structs everywhere (track)
    // to avoid dealing with json_set in every GET function, we update the JSON download_status
    // at every change, avoiding inconsistent data
    sqlx::query(
        r#"
        CREATE TRIGGER update_json_download_status
        AFTER UPDATE OF download_status ON tracks
        FOR EACH ROW
        BEGIN
            UPDATE tracks
            SET track = json_set(track, '$.download_status', NEW.download_status)
            WHERE id = NEW.id;
        END;
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS artists (
            id TEXT PRIMARY KEY,
            artist TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS albums (
            id TEXT PRIMARY KEY,
            album TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS playlists (
            id TEXT PRIMARY KEY,
            playlist TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS artist_membership (
            artist_id TEXT NOT NULL,
            track_id TEXT NOT NULL,
            PRIMARY KEY (artist_id, track_id)
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS playlist_membership (
            playlist_id TEXT NOT NULL,
            track_id TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (playlist_id, track_id)
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS lyrics (
            id TEXT PRIMARY KEY,
            lyric TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn query_download_track(
    pool: &SqlitePool,
    track: &DiscographySong,
    playlist_id: &Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        r#"
        INSERT INTO tracks (
            id,
            album_id,
            artist_items,
            download_status,
            track
        ) VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE
          SET download_status = excluded.download_status;
        "#,
    )
    .bind(&track.id)
    .bind(&track.album_id)
    .bind(serde_json::to_string(&track.album_artists)?)
    .bind(DownloadStatus::Queued.to_string())
    .bind(serde_json::to_string(track)?)
    .execute(pool)
    .await?;

    for artist in &track.album_artists {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO artist_membership (
                artist_id,
                track_id
            ) VALUES (?, ?);
            "#,
        )
        .bind(&artist.id)
        .bind(&track.id)
        .execute(pool)
        .await?;
    }

    if let Some(pl_id) = playlist_id {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO playlist_membership (
                playlist_id,
                track_id,
                position
            ) VALUES (?, ?, ?);
            "#,
        )
        .bind(pl_id)
        .bind(&track.id)
        .bind(0) // this gets overwritten later
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub async fn query_download_tracks(
    pool: &SqlitePool,
    tracks: &mut [DiscographySong],
) -> Result<(), Box<dyn std::error::Error>> {
    tracks.iter_mut().for_each(|track| {
        track.download_status = DownloadStatus::Queued;
    });

    let mut tx = pool.begin().await?;

    for track in tracks {
        sqlx::query(
            r#"
            INSERT INTO tracks (
                id,
                album_id,
                artist_items,
                download_status,
                track
            ) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE
              SET download_status = excluded.download_status;
            "#,
        )
        .bind(&track.id)
        .bind(&track.album_id)
        .bind(serde_json::to_string(&track.album_artists)?)
        .bind(DownloadStatus::Queued.to_string())
        .bind(serde_json::to_string(&track)?)
        .execute(&mut *tx)
        .await?;

        for artist in &track.album_artists {
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO artist_membership (
                    artist_id,
                    track_id
                ) VALUES (?, ?);
                "#,
            )
            .bind(&artist.id)
            .bind(&track.id)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    Ok(())
}


/// Delete a track from the database and the filesystem
///
pub async fn remove_track_download(
    pool: &SqlitePool,
    track: &DiscographySong,
    data_dir: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = pool.begin().await?;
    let _: (String,) = sqlx::query_as(
        "UPDATE tracks SET download_status = 'NotDownloaded' WHERE id = ? RETURNING id",
    )
    .bind(&track.id)
    .fetch_one(&mut *tx)
    .await?;

    let file_path = std::path::Path::new(&data_dir)
        .join(&track.server_id)
        .join(&track.album_id)
        .join(&track.id);
    if file_path.exists() {
        tokio::fs::remove_file(&file_path).await?;

        if let Some(parent_dir) = file_path.parent() {
            let mut entries = tokio::fs::read_dir(parent_dir).await?;
            if entries.next_entry().await?.is_none() {
                tokio::fs::remove_dir(parent_dir).await?;
            }
        }
    }

    tx.commit().await?;

    Ok(())
}

pub async fn remove_tracks_downloads(
    pool: &SqlitePool,
    tracks: &[DiscographySong],
    data_dir: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = pool.begin().await?;
    for track in tracks {
        sqlx::query(
            "UPDATE tracks SET download_status = 'NotDownloaded' WHERE id = ?",
        )
        .bind(&track.id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    for track in tracks {
        let file_path = std::path::Path::new(&data_dir)
            .join(&track.server_id)
            .join(&track.album_id)
            .join(&track.id);
        if file_path.exists() {
            tokio::fs::remove_file(&file_path).await?;

            if let Some(parent_dir) = file_path.parent() {
                let mut entries = tokio::fs::read_dir(parent_dir).await?;
                if entries.next_entry().await?.is_none() {
                    tokio::fs::remove_dir(parent_dir).await?;
                }
            }
        }
    }

    Ok(())
}

pub async fn insert_lyrics(
    pool: &SqlitePool,
    track_id: &str,
    lyrics: &[Lyric],
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("DELETE FROM lyrics WHERE id = ?")
        .bind(track_id)
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO lyrics (
            id, lyric
        ) VALUES (?, ?);
        "#,
    )
    .bind(track_id)
    .bind(serde_json::to_string(&lyrics)?)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_lyrics(
    pool: &SqlitePool,
    id: &str,
) -> Result<Vec<Lyric>, Box<dyn std::error::Error>> {
    let record: (String,) = sqlx::query_as("SELECT lyric FROM lyrics WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    let lyrics: Vec<Lyric> = serde_json::from_str(&record.0)?;
    Ok(lyrics)
}

/// Query for all artists that have at least one track in the database
///
pub async fn get_all_artists(
    pool: &SqlitePool,
) -> Result<Vec<Artist>, Box<dyn std::error::Error>> {
    // artist items is a JSON array of Artist objects
    let records: Vec<(String,)> = sqlx::query_as("SELECT artist FROM artists")
        .fetch_all(pool)
        .await?;

    let artists: Vec<Artist> = records
        .iter()
        .map(|r| serde_json::from_str(&r.0).unwrap())
        .collect();

    Ok(artists)
}

pub async fn get_discography(
    pool: &SqlitePool,
    artist_id: &str,
    client: Option<&Arc<Client>>,
) -> Result<Vec<DiscographySong>, Box<dyn std::error::Error>> {
    let records: Vec<(String, String)> = if client.is_some() {
        sqlx::query_as(
            r#"
            SELECT t.track, t.download_status
            FROM tracks t
            JOIN artist_membership am ON t.id = am.track_id
            WHERE am.artist_id = ?
            "#,
        )
        .bind(artist_id)
        .fetch_all(pool)
        .await?
    } else {
        // when client is not present (offline), we only fetch downloaded tracks
        sqlx::query_as(
            r#"
            SELECT t.track, t.download_status
            FROM tracks t
            JOIN artist_membership am ON t.id = am.track_id
            WHERE am.artist_id = ?
            AND t.download_status = 'Downloaded'
            "#,
        )
        .bind(artist_id)
        .fetch_all(pool)
        .await?
    };

    let mut tracks = Vec::new();
    for (json_str, download_status) in records {
        let mut track: DiscographySong = serde_json::from_str(&json_str).unwrap();
        track.download_status = match download_status.as_str() {
            "Downloaded" => DownloadStatus::Downloaded,
            "Queued" => DownloadStatus::Queued,
            "Downloading" => DownloadStatus::Downloading,
            _ => DownloadStatus::NotDownloaded,
        };
        tracks.push(track);
    }

    Ok(tracks)
}

pub async fn get_album_tracks(
    pool: &SqlitePool,
    album_id: &str,
    client: Option<&Arc<Client>>,
) -> Result<Vec<DiscographySong>, Box<dyn std::error::Error>> {
    let records: Vec<(String,)> = if client.is_some() {
        sqlx::query_as(
            r#"
            SELECT track
            FROM tracks
            WHERE album_id = ?
            "#,
        )
        .bind(album_id)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            r#"
            SELECT track
            FROM tracks
            WHERE album_id = ? AND download_status = 'Downloaded'
            "#,
        )
        .bind(album_id)
        .fetch_all(pool)
        .await?
    };

    let mut tracks: Vec<DiscographySong> = records
        .iter()
        .map(|r| serde_json::from_str(&r.0).unwrap())
        .collect();

    tracks.sort_by(|a, b| a.index_number.cmp(&b.index_number));
    tracks.sort_by(|a, b| a.parent_index_number.cmp(&b.parent_index_number));

    Ok(tracks)
}

pub async fn get_playlist_tracks(
    pool: &SqlitePool,
    playlist_id: &str,
    client: Option<&Arc<Client>>,
) -> Result<Vec<DiscographySong>, Box<dyn std::error::Error>> {
    let records: Vec<(String,)> = if client.is_some() {
        sqlx::query_as(
            r#"
            SELECT track
            FROM tracks t
            JOIN playlist_membership pm ON t.id = pm.track_id
            WHERE pm.playlist_id = ?
            ORDER BY pm.position
            "#,
        )
        .bind(playlist_id)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            r#"
            SELECT track
            FROM tracks t
            JOIN playlist_membership pm ON t.id = pm.track_id
            WHERE pm.playlist_id = ? AND t.download_status = 'Downloaded'
            ORDER BY pm.position
            "#,
        )
        .bind(playlist_id)
        .fetch_all(pool)
        .await?
    };

    let tracks: Vec<DiscographySong> = records
        .iter()
        .map(|r| serde_json::from_str(&r.0).unwrap())
        .collect();

    Ok(tracks)
}

pub async fn get_all_albums(
    pool: &SqlitePool,
) -> Result<Vec<Album>, Box<dyn std::error::Error>> {
    let records: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT album FROM albums
        "#,
    )
    .fetch_all(pool)
    .await?;

    let albums: Vec<Album> = records
        .iter()
        .map(|r| serde_json::from_str(&r.0).unwrap())
        .collect();

    Ok(albums)
}

pub async fn get_all_playlists(
    pool: &SqlitePool,
) -> Result<Vec<Playlist>, Box<dyn std::error::Error>> {
    let records: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT playlist FROM playlists
        "#,
    )
    .fetch_all(pool)
    .await?;

    let playlists: Vec<Playlist> = records
        .iter()
        .filter_map(|r| serde_json::from_str(&r.0).ok())
        .collect();

    Ok(playlists)
}

/// Query for all artists that have at least one track in the database
///
pub async fn get_artists_with_tracks(
    pool: &SqlitePool,
) -> Result<Vec<Artist>, Box<dyn std::error::Error>> {
    let records: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT a.artist
        FROM artists a
        JOIN artist_membership am ON a.id = am.artist_id
        JOIN tracks t ON t.id = am.track_id
        WHERE t.download_status = 'Downloaded'
        "#,
    )
    .fetch_all(pool)
    .await?;

    let artists: Vec<Artist> = records
        .iter()
        .map(|r| serde_json::from_str(&r.0).unwrap())
        .collect();

    Ok(artists)
}

/// Query for all albums that have at least one track in the database
///
pub async fn get_albums_with_tracks(
    pool: &SqlitePool,
) -> Result<Vec<Album>, Box<dyn std::error::Error>> {
    let records: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT a.album
        FROM albums a
        JOIN tracks t ON t.album_id = a.id
        WHERE t.download_status = 'Downloaded'
        "#,
    )
    .fetch_all(pool)
    .await?;

    let albums: Vec<Album> = records
        .iter()
        .map(|r| serde_json::from_str(&r.0).unwrap())
        .collect();

    Ok(albums)
}

/// Query for all playlists that have at least one track in the database
///
pub async fn get_playlists_with_tracks(
    pool: &SqlitePool,
) -> Result<Vec<Playlist>, Box<dyn std::error::Error>> {
    let records: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT p.playlist
        FROM playlists p
        JOIN playlist_membership pm ON p.id = pm.playlist_id
        JOIN tracks t ON t.id = pm.track_id
        WHERE t.download_status = 'Downloaded'
        "#,
    )
    .fetch_all(pool)
    .await?;

    let playlists: Vec<Playlist> = records
        .iter()
        .map(|r| serde_json::from_str(&r.0).unwrap())
        .collect();

    Ok(playlists)
}

pub async fn get_tracks(
    pool: &SqlitePool,
    search_term: &str,
) -> Result<Vec<DiscographySong>, Box<dyn std::error::Error>> {
    let records: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT track
        FROM tracks
        WHERE track LIKE ? AND download_status = 'Downloaded'
        "#,
    )
    .bind(format!("%{}%", search_term))
    .fetch_all(pool)
    .await?;

    let tracks: Vec<DiscographySong> = records
        .iter()
        .map(|r| serde_json::from_str(&r.0).unwrap())
        .collect();

    Ok(tracks)
}


/// Favorite toggles
///
pub async fn set_favorite_track(
    pool: &SqlitePool,
    track_id: &String, favorite: bool
) -> Result<(), sqlx::Error> {
    let mut tx_db = pool.begin().await?;
    sqlx::query(
        r#"
            UPDATE tracks
            SET track = json_set(track, '$.UserData.IsFavorite', json(?))
            WHERE id = ?
        "#)
        .bind(favorite.to_string())
        .bind(track_id)
        .execute(&mut *tx_db)
        .await?;

    tx_db.commit().await?;

    Ok(())
}

pub async fn set_favorite_album(
    pool: &SqlitePool,
    album_id: &String, favorite: bool
) -> Result<(), sqlx::Error> {
    let mut tx_db = pool.begin().await?;
    sqlx::query(
        r#"
            UPDATE album
            SET album = json_set(album, '$.UserData.IsFavorite', json(?))
            WHERE id = ?
        "#)
        .bind(favorite.to_string())
        .bind(album_id)
        .execute(&mut *tx_db)
        .await?;

    tx_db.commit().await?;

    Ok(())
}

pub async fn set_favorite_artist(
    pool: &SqlitePool,
    artist_id: &String, favorite: bool
) -> Result<(), sqlx::Error> {
    let mut tx_db = pool.begin().await?;
    sqlx::query(
        r#"
            UPDATE artists
            SET artist = json_set(artist, '$.UserData.IsFavorite', json(?))
            WHERE id = ?
        "#)
        .bind(favorite.to_string())
        .bind(artist_id)
        .execute(&mut *tx_db)
        .await?;

    tx_db.commit().await?;

    Ok(())
}

pub async fn set_favorite_playlist(
    pool: &SqlitePool,
    playlist_id: &String, favorite: bool
) -> Result<(), sqlx::Error> {
    let mut tx_db = pool.begin().await?;
    sqlx::query(
        r#"
            UPDATE playlists
            SET playlist = json_set(playlist, '$.UserData.IsFavorite', json(?))
            WHERE id = ?
        "#)
        .bind(favorite.to_string())
        .bind(playlist_id)
        .execute(&mut *tx_db)
        .await?;

    tx_db.commit().await?;

    Ok(())
}
