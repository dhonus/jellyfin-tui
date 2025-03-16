use std::{fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

use sqlx::{migrate::MigrateDatabase, FromRow, Row, Sqlite, SqlitePool};

use crate::{
    client::{Album, Artist, DiscographySong, Lyric, Playlist}, database::database::data_updater, tui
};

use super::database::Status;

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
        let db = match self.db {
            Some(ref mut db) => db,
            None => return Ok(()),
        };

        let status = db.status_rx.try_recv();
        match status {
            Ok(status) => self.handle_database_status(status).await,
            Err(_) => return Ok(()),
        }
        Ok(())
    }

    async fn handle_database_status(&mut self, status: Status) {
        match status {
            Status::TrackQueued { id } => {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::Queued;
                }
            }
            Status::TrackDownloaded { id } => {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::Downloaded;
                }
            }
            Status::TrackDownloading { id } => {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::Downloading;
                }
            }
            Status::TrackDeleted { id } => {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::NotDownloaded;
                }
            }
            // TODO: notify user of stale data
            Status::ArtistsUpdated => {
                println!(" ! Artists updated");
                // self.artists = get_artists(&db.pool).await.unwrap();
            }
            Status::AlbumsUpdated => {
                println!(" ! Albums updated");
                // self.albums = get_albums(&db.pool).await.unwrap();
            }
            Status::PlaylistsUpdated => {
                println!(" ! Playlists updated");
                // self.playlists = get_playlists(&db.pool).await.unwrap();
            }
            Status::UpdateFailed { error } => {
                // TODO add into popup
                println!(" ! Update failed: {}", error);
            }
        }
    }

    /// Create a database if it doesn't exist. Perform any necessary initialization / migrations etc
    ///
    pub async fn init_db(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let path = "sqlite://music.db";
        if !Sqlite::database_exists(path).await.unwrap_or(false) {
            println!(" ! Creating database {}", path);
            Sqlite::create_database(path).await?;
            let pool = SqlitePool::connect(path).await?;
            create_tables(&pool).await?;

            println!(" - Database created. Fetching data...");

            if let Err(e) = data_updater(None).await {
                return Err(e);
            }
        }
        Ok(())
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
            server_id TEXT NOT NULL,
            artist_items TEXT NOT NULL,
            download_status TEXT NOT NULL,
            track TEXT NOT NULL
        );
        "#
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS artists (
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,
            artist TEXT NOT NULL
        );
        "#
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS albums (
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,
            album TEXT NOT NULL
        );
        "#
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS playlists (
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,
            playlist TEXT NOT NULL
        );
        "#
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS lyrics (
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,
            lyric TEXT NOT NULL
        );
        "#
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn insert_track(
    pool: &SqlitePool,
    track: &DiscographySong,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        r#"
        INSERT INTO tracks (
            id,
            album_id,
            server_id,
            artist_items,
            download_status,
            track
        ) VALUES (?, ?, ?, ?, ?, ?);
        "#,
    )
    .bind(&track.id)
    .bind(&track.album_id)
    .bind(&track.server_id)
    .bind(serde_json::to_string(&track.artist_items)?)
    .bind(DownloadStatus::Queued.to_string())
    .bind(serde_json::to_string(&track)?)
    .execute(pool)
    .await
    .unwrap();

    Ok(())
}

pub async fn insert_tracks(
    pool: &SqlitePool,
    tracks: &[DiscographySong],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = pool.begin().await?;
    for track in tracks {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO tracks (
                id,
                album_id,
                server_id,
                artist_items,
                download_status,
                track
            ) VALUES (?, ?, ?, ?, ?, ?);
            "#,
        )
        .bind(&track.id)
        .bind(&track.album_id)
        .bind(&track.server_id)
        .bind(serde_json::to_string(&track.artist_items)?)
        .bind(DownloadStatus::Queued.to_string())
        .bind(serde_json::to_string(&track)?)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(())
}

/// Delete a track from the database and the filesystem
///
pub async fn delete_track(
    pool: &SqlitePool,
    track: &DiscographySong,
    cache_dir: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = pool.begin().await?;
    let id: (String,) = sqlx::query_as(
        "UPDATE tracks SET download_status = 'NotDownloaded' WHERE id = ? RETURNING id",
    )
    .bind(&track.id)
    .fetch_one(&mut *tx)
    .await?;

    let file_path = std::path::Path::new(&cache_dir)
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
    sqlx::query("DELETE FROM tracks WHERE id = ?")
        .bind(&id.0)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(())
}

pub async fn delete_tracks(
    pool: &SqlitePool,
    tracks: &[DiscographySong],
    cache_dir: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = pool.begin().await?;
    for track in tracks {
        let id: (String,) = sqlx::query_as(
            "UPDATE tracks SET download_status = 'NotDownloaded' WHERE id = ? RETURNING id",
        )
        .bind(&track.id)
        .fetch_one(&mut *tx)
        .await?;

        let file_path = std::path::Path::new(&cache_dir)
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
        sqlx::query("DELETE FROM tracks WHERE id = ?")
            .bind(&id.0)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    Ok(())
}

pub async fn insert_lyrics(
    pool: &SqlitePool,
    track_id: &str,
    server_id: &str,
    lyrics: &[Lyric],
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("DELETE FROM lyrics WHERE id = ? AND server_id = ?")
        .bind(track_id)
        .bind(server_id)
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO lyrics (
            id,
            server_id,
            lyric
        ) VALUES (?, ?, ?);
        "#,
    )
    .bind(track_id)
    .bind(server_id)
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
    server_id: &String,
) -> Result<Vec<Artist>, Box<dyn std::error::Error>> {
    // artist items is a JSON array of Artist objects
    let records: Vec<(String,)> = sqlx::query_as("SELECT artist FROM artists WHERE server_id = ?")
        .bind(server_id)
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
) -> Result<Vec<DiscographySong>, Box<dyn std::error::Error>> {
    let records: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT t.track
        FROM tracks t, json_each(t.artist_items)
        WHERE json_extract(json_each.value, '$.Id') = ?
        "#,
    )
    .bind(artist_id)
    .fetch_all(pool)
    .await?;

    let tracks: Vec<DiscographySong> = records
        .iter()
        .map(|r| serde_json::from_str(&r.0).unwrap())
        .collect();

    Ok(tracks)
}

pub async fn get_album_tracks(
    pool: &SqlitePool,
    album_id: &str,
) -> Result<Vec<DiscographySong>, Box<dyn std::error::Error>> {
    let records: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT track
        FROM tracks
        WHERE album_id = ?
        "#,
    )
    .bind(album_id)
    .fetch_all(pool)
    .await?;

    let tracks: Vec<DiscographySong> = records
        .iter()
        .map(|r| serde_json::from_str(&r.0).unwrap())
        .collect();

    Ok(tracks)
}

pub async fn get_all_albums(
    pool: &SqlitePool,
    server_id: &String,
) -> Result<Vec<Album>, Box<dyn std::error::Error>> {
    let records: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT album
        FROM albums
        WHERE server_id = ?
        "#,
    )
    .bind(server_id)
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
    server_id: &String,
) -> Result<Vec<Playlist>, Box<dyn std::error::Error>> {
    let records: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT playlist
        FROM playlists
        WHERE server_id = ?
        "#,
    )
    .bind(server_id)
    .fetch_all(pool)
    .await?;

    let playlists: Vec<Playlist> = records
        .iter()
        .map(|r| serde_json::from_str(&r.0).unwrap())
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
        WHERE EXISTS (
            SELECT 1
            FROM tracks t, json_each(t.artist_items)
            WHERE json_extract(json_each.value, '$.Id') = a.id
        )
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
        WHERE EXISTS (
            SELECT 1
            FROM tracks t
            WHERE t.album_id = a.id
        )
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
        WHERE EXISTS (
            SELECT 1
            FROM tracks t
            WHERE t.album_id = p.id
        )
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