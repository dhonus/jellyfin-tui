use serde::{Deserialize, Serialize};

use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};

use crate::{client::DiscographySong, tui};

use super::database::Status;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum DownloadStatus {
    Downloaded,
    Queued,
    Downloading { progress: f32 },
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
            Status::TrackDownloaded { id } => {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                    track.download_status = DownloadStatus::Downloaded;
                } else {
                    panic!("Track not found: {}", id);
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
            album TEXT NOT NULL,
            album_artist TEXT NOT NULL,
            album_id TEXT NOT NULL,
            date_created TEXT NOT NULL,
            media_type TEXT NOT NULL,
            name TEXT NOT NULL,
            parent_id TEXT NOT NULL,
            premiere_date TEXT NOT NULL,
            server_id TEXT NOT NULL,
            
            -- Fields stored as JSON (Vec<T> or custom types)
            album_artists TEXT NOT NULL,  -- JSON array of Artist objects
            artist_items TEXT NOT NULL,   -- JSON array of Artist objects
            artists TEXT NOT NULL,        -- JSON array of strings
            backdrop_image_tags TEXT NOT NULL,  -- JSON array of strings
            genres TEXT NOT NULL,         -- JSON array of strings
            media_sources TEXT NOT NULL,  -- JSON array of MediaSource objects
            user_data TEXT NOT NULL,      -- JSON object for DiscographySongUserData

            -- Optional field
            channel_id TEXT,

            -- Boolean values stored as INTEGER (0/1)
            has_lyrics INTEGER NOT NULL,
            is_folder INTEGER NOT NULL,

            -- Numeric fields
            index_number INTEGER NOT NULL,
            parent_index_number INTEGER NOT NULL,
            normalization_gain REAL NOT NULL,
            production_year INTEGER NOT NULL,
            run_time_ticks INTEGER NOT NULL,
            
            -- Other text fields
            playlist_item_id TEXT NOT NULL,
            download_status TEXT NOT NULL
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
            
            -- Basic text fields
            name TEXT NOT NULL,
            type_ TEXT NOT NULL,
            location_type TEXT NOT NULL,
            media_type TEXT NOT NULL,
            
            -- Numeric field
            run_time_ticks INTEGER NOT NULL,
            
            -- JSON fields stored as TEXT
            user_data TEXT NOT NULL,
            image_tags TEXT NOT NULL,
            image_blur_hashes TEXT NOT NULL,
            
            -- Boolean stored as INTEGER (0 for false, 1 for true)
            jellyfintui_recently_added INTEGER NOT NULL
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
            
            -- Basic text fields
            name TEXT NOT NULL,
            date_created TEXT NOT NULL,
            parent_id TEXT NOT NULL,
            
            -- JSON fields stored as TEXT
            album_artists TEXT NOT NULL,
            user_data TEXT NOT NULL,
            
            -- Numeric field
            run_time_ticks INTEGER NOT NULL
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
            album,
            album_artist,
            album_id,
            date_created,
            media_type,
            name,
            parent_id,
            premiere_date,
            server_id,
            album_artists,
            artist_items,
            artists,
            backdrop_image_tags,
            genres,
            media_sources,
            user_data,
            channel_id,
            has_lyrics,
            is_folder,
            index_number,
            parent_index_number,
            normalization_gain,
            production_year,
            run_time_ticks,
            playlist_item_id,
            download_status
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#
    )
    .bind(&track.id)
    .bind(&track.album)
    .bind(&track.album_artist)
    .bind(&track.album_id)
    .bind(&track.date_created)
    .bind(&track.media_type)
    .bind(&track.name)
    .bind(&track.parent_id)
    .bind(&track.premiere_date)
    .bind(&track.server_id)
    // Serialize JSON fields
    .bind(serde_json::to_string(&track.album_artists)?)
    .bind(serde_json::to_string(&track.artist_items)?)
    .bind(serde_json::to_string(&track.artists)?)
    .bind(serde_json::to_string(&track.backdrop_image_tags)?)
    .bind(serde_json::to_string(&track.genres)?)
    .bind(serde_json::to_string(&track.media_sources)?)
    .bind(serde_json::to_string(&track.user_data)?)
    // Optional channel_id
    .bind(track.channel_id.as_ref())
    // Booleans as integers
    .bind(track.has_lyrics as i32)
    .bind(if track.is_folder { 1 } else { 0 })
    // Numeric fields
    .bind(track.index_number as i64)
    .bind(track.parent_index_number as i64)
    .bind(track.normalization_gain)
    .bind(track.production_year as i64)
    .bind(track.run_time_ticks as i64)
    .bind(&track.playlist_item_id)
    .bind(serde_json::to_string(&track.download_status)?)
    .execute(pool)
    .await.unwrap();

    Ok(())
}


