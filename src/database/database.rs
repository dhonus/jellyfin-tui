use core::panic;
use std::{path::Path, time::Duration};

use sqlx::{SqlitePool, sqlite::SqliteTransaction};
use tokio::{fs, io::AsyncWriteExt, sync::mpsc::{error::TryRecvError, Receiver, Sender}, time::Interval};

use crate::{client::{Client, DiscographySong}, database::app_extension::{delete_track, delete_tracks, insert_tracks, DownloadStatus}};

use super::app_extension::insert_track;

#[derive(Debug)]
pub enum Command {
    Download(DownloadCommand),
    Update(UpdateCommand),
    Delete(DeleteCommand),
}

pub enum Status {
    TrackQueued { id: String },
    TrackDownloading { id: String },
    TrackDownloaded { id: String },
    TrackDeleted { id: String },
}

#[derive(Debug)]
pub enum DownloadCommand {
    Track { track: DiscographySong },
    Album { tracks: Vec<DiscographySong> },
    Playlist { id: String, playlist_url: String },
}

#[derive(Debug)]
pub enum UpdateCommand {
    Track { id: String, url: String },
    Album { id: String, album_url: String },
    Playlist { id: String, playlist_url: String },
}

#[derive(Debug)]
pub enum DeleteCommand {
    Track { track: DiscographySong },
    Album { tracks: Vec<DiscographySong> },
    Playlist { id: String },
}

pub async fn t_database(
    mut rx: Receiver<Command>,
    tx: Sender<Status>,
) {

    let pool = SqlitePool::connect("sqlite://music.db").await.unwrap();

    let client = match Client::new(true, true).await {
        Some(client) => client,
        None => {
            return;
        }
    };

    if client.access_token.is_empty() {
        return;
    }

    let cache_dir = match dirs::cache_dir() {
        Some(dir) => dir.join("jellyfin-tui").join("downloads"),
        None => return,
    };
    if !cache_dir.exists() {
        if fs::create_dir_all(&cache_dir).await.is_err() {
            return;
        }
    }

    let mut db_interval = tokio::time::interval(Duration::from_secs(5));
    let mut active_download: Option<tokio::task::JoinHandle<()>> = None;

    loop {
        tokio::select! {
            Some(cmd) = rx.recv() => {
                match cmd {
                    Command::Download(download_cmd) => {
                        match download_cmd {
                            DownloadCommand::Track { track } => {
                                let _ = insert_track(&pool, &track).await;
                                let _ = tx.send(Status::TrackQueued { id: track.id }).await;
                            }
                            DownloadCommand::Album { tracks } => {
                                let _ = insert_tracks(&pool, &tracks).await;
                                for track in tracks {
                                    if !matches!(track.download_status, DownloadStatus::NotDownloaded) {
                                        continue;
                                    }
                                    let _ = tx.send(Status::TrackQueued { id: track.id }).await;
                                }
                            }
                            _ => {}
                        }
                    },
                    Command::Delete(delete_cmd) => {
                        match delete_cmd {
                            DeleteCommand::Track { track } => {
                                let _ = delete_track(&pool, &track, &cache_dir).await;
                                let _ = tx.send(Status::TrackDeleted { id: track.id }).await;
                            }
                            DeleteCommand::Album { tracks } => {
                                let _ = delete_tracks(&pool, &tracks, &cache_dir).await;
                                for track in tracks {
                                    let _ = tx.send(Status::TrackDeleted { id: track.id }).await;
                                }
                            }
                            _ => {}
                        }
                    },
                    _ => {}
                }
            },
            _ = db_interval.tick() => {
                if active_download.is_none() {
                    active_download = track_process_queued_download(&pool, &tx, &client, &cache_dir).await;
                }
            },
            _ = async {
                if let Some(handle) = &mut active_download {
                    handle.await.ok();
                }
            }, if active_download.is_some() => {
                active_download = None;
            },
        }
    }
}

async fn track_process_queued_download(
    pool: &SqlitePool,
    tx: &Sender<Status>,
    client: &Client,
    cache_dir: &std::path::PathBuf,
) -> Option<tokio::task::JoinHandle<()>> {
    if let Ok(record) = sqlx::query_as::<_, (String, String, String, String)>(
        "
        SELECT id, server_id, album_id, track 
            FROM tracks WHERE download_status = 'Queued' OR download_status = 'Downloading'
            ORDER BY download_status ASC LIMIT 1
        "
    )
    .fetch_optional(pool)
    .await {
        if let Some((id, server_id, album_id, track_str)) = record {
            let track: DiscographySong = match serde_json::from_str(&track_str) {
                Ok(track) => track,
                Err(_) => {
                    println!("Failed to parse track JSON: {}", track_str);
                    return None;
                }
            };

            let pool = pool.clone();
            let tx = tx.clone();
            let url = client.song_url_sync(track.id.clone());
            let file_dir = cache_dir.join(server_id).join(album_id);
            if !file_dir.exists() {
                if fs::create_dir_all(&file_dir).await.is_err() {
                    println!("Failed to create directory: {}", file_dir.display());
                    return None;
                }
            }
            return Some(tokio::spawn(async move {
                if let Err(e) =
                    track_download_and_update(&pool, &id, &url, &file_dir, &track, &tx).await
                {
                    println!("Download process failed for track {}: {:?}", track.id, e);
                }
            }));
        }
    }
    None
}

async fn track_download_and_update(
    pool: &SqlitePool,
    id: &str,
    url: &str,
    file_dir: &Path,
    track: &DiscographySong,
    tx: &Sender<Status>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // T1 set Downloading status
    {
        let mut tx_db = pool.begin().await?;
        sqlx::query("UPDATE tracks SET download_status = 'Downloading' WHERE id = ?")
            .bind(id)
            .execute(&mut *tx_db)
            .await?;
        tx_db.commit().await?;

        tx.send(Status::TrackDownloading { id: track.id.to_string() }).await?;
    }

    // Download a song
    let download_result = async {
        let mut response = reqwest::get(url).await?;
        let file_path = file_dir.join(format!("{}", track.id));
        let mut file = fs::File::create(&file_path).await?;
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;

    // T2 update final status
    {
        let mut tx_db = pool.begin().await?;
        match download_result {
            Ok(_) => {
                let record = sqlx::query_as::<_, DownloadStatus>(
                    "SELECT download_status FROM tracks WHERE id = ?"
                )
                .bind(id)
                .fetch_one(&mut *tx_db)
                .await;
                if let Ok(record) = record {
                    if !matches!(record, DownloadStatus::Downloading) {
                        fs::remove_file(file_dir.join(format!("{}", track.id))).await.ok();
                        return Ok(());
                    }
                    sqlx::query("UPDATE tracks SET download_status = 'Downloaded' WHERE id = ?")
                        .bind(id)
                        .execute(&mut *tx_db)
                        .await?;
                } else {
                    fs::remove_file(file_dir.join(format!("{}", track.id))).await.ok();
                }
            }
            Err(e) => {
                sqlx::query("UPDATE tracks SET download_status = 'Queued' WHERE id = ?")
                    .bind(id)
                    .execute(&mut *tx_db)
                    .await?;
                tx_db.commit().await?;
                return Err(e);
            }
        }
        tx_db.commit().await?;
    }

    tx.send(Status::TrackDownloaded { id: track.id.to_string() })
        .await?;
    Ok(())
}