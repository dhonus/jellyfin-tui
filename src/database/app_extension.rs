use std::{fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

use sqlx::{migrate::MigrateDatabase, Row, FromRow, Sqlite, SqlitePool};

use crate::{client::{DiscographySong, Lyric}, tui};

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
        }
    }

    /// Create a database if it doesn't exist. Perform any necessary initialization / migrations etc
    /// 
    pub async fn init_db(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let path = "sqlite://music.db";
        if !Sqlite::database_exists(path).await.unwrap_or(false) {
            println!(" ! Creating database {}", path);
            match Sqlite::create_database(path).await {
                Ok(_) => println!(" - Create db success."),
                // TODO
                Err(error) => panic!("error: {}", error),
            }
            let pool = SqlitePool::connect(path).await?;
            create_tracks_table(&pool).await?;
            create_artists_table(&pool).await?;
            create_albums_table(&pool).await?;
            create_lyrics_table(&pool).await?;
        }
        Ok(())
    }
}

/// ------------ helpers ------------
/// 
pub async fn create_tracks_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tracks (
            -- Basic text fields
            id TEXT PRIMARY KEY,
            album_id TEXT NOT NULL,
            server_id TEXT NOT NULL,

            -- Fields stored as JSON (Vec<T> or custom types)
            artist_items TEXT NOT NULL,   -- JSON array of Artist objects
            download_status TEXT NOT NULL, -- JSON object of DownloadStatus
            
            -- DiscographySong
            track TEXT NOT NULL
        );
        "#
    )
    .execute(pool)
    .await.unwrap();

    Ok(())
}

pub async fn create_artists_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS artists (
            -- Primary key (adjust if needed)
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,

            -- JSON fields stored as TEXT
            artist TEXT NOT NULL
        );
        "#
    )
    .execute(pool)
    .await?;
    
    Ok(())
}

pub async fn create_albums_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS albums (
            -- Primary key for the album
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,

            -- JSON fields stored as TEXT
            artist_items TEXT NOT NULL
        );
        "#
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn create_lyrics_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS lyrics (
            -- Primary key for the lyric
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,

            -- JSON fields stored as TEXT
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
    track: &DiscographySong
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
        "#
    )
    .bind(&track.id)
    .bind(&track.album_id)
    .bind(&track.server_id)
    .bind(serde_json::to_string(&track.artist_items)?)
    .bind(DownloadStatus::Queued.to_string())
    .bind(serde_json::to_string(&track)?)
    .execute(pool)
    .await.unwrap();

    Ok(())
}

pub async fn insert_tracks(
    pool: &SqlitePool,
    tracks: &[DiscographySong]
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
            "#
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
pub async fn delete_track(pool: &SqlitePool, track: &DiscographySong, cache_dir: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = pool.begin().await?;
    let id: (String,) = sqlx::query_as("UPDATE tracks SET download_status = 'NotDownloaded' WHERE id = ? RETURNING id")
        .bind(&track.id)
        .fetch_one(&mut *tx)
        .await?;

    let file_path = std::path::Path::new(&cache_dir).join(&track.server_id).join(&track.album_id).join(&track.id);
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
    cache_dir: &PathBuf
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = pool.begin().await?;
    for track in tracks {
        let id: (String,) = sqlx::query_as("UPDATE tracks SET download_status = 'NotDownloaded' WHERE id = ? RETURNING id")
            .bind(&track.id)
            .fetch_one(&mut *tx)
            .await?;

        let file_path = std::path::Path::new(&cache_dir).join(&track.server_id).join(&track.album_id).join(&track.id);
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

pub async fn insert_lyrics(pool: &SqlitePool, track_id: &str, server_id: &str, lyrics: &[Lyric]) -> Result<(), Box<dyn std::error::Error>> {
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
        "#
    )
    .bind(track_id)
    .bind(server_id)
    .bind(serde_json::to_string(&lyrics)?)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_lyrics(pool: &SqlitePool, id: &str) -> Result<Vec<Lyric>, Box<dyn std::error::Error>> {
    let record: (String,) = sqlx::query_as("SELECT lyric FROM lyrics WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    let lyrics: Vec<Lyric> = serde_json::from_str(&record.0)?;
    Ok(lyrics)
}