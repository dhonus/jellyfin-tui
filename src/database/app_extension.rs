use serde::{Deserialize, Serialize};

use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};

use crate::{client::DiscographySong, tui};

use super::database::Status;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum DownloadStatus {
    Downloaded,
    Queued,
    Downloading,
    #[default]
    NotDownloaded,
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

pub async fn insert_track(pool: &SqlitePool, track: &DiscographySong) -> Result<(), Box<dyn std::error::Error>> {
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
    .bind(serde_json::to_string(&track.download_status)?)
    .bind(serde_json::to_string(&track)?)
    .execute(pool)
    .await.unwrap();

    Ok(())
}


