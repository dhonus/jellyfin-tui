use std::time::Duration;

use sqlx::SqlitePool;
use tokio::{sync::mpsc::{error::TryRecvError, Receiver, Sender}, time::Interval};

use crate::client::{Client, DiscographySong};

use super::app_extension::insert_track;

#[derive(Debug)]
pub enum Command {
    Download(DownloadCommand),
    Update(UpdateCommand),
    Delete(DeleteCommand),
}

pub enum Status {
    TrackDownloaded { id: String },
}

#[derive(Debug)]
pub enum DownloadCommand {
    Track { track: DiscographySong },
    Album { id: u32, album_url: String },
    Playlist { id: u32, playlist_url: String },
}

#[derive(Debug)]
enum UpdateCommand {
    Song { id: u32, url: String },
    Album { id: u32, album_url: String },
    Playlist { id: u32, playlist_url: String },
}

#[derive(Debug)]
enum DeleteCommand {
    Song { id: u32 },
    Album { id: u32 },
    Playlist { id: u32 },
}

pub async fn t_database(
    mut rx: Receiver<Command>,
    tx: Sender<Status>,
) {

    let pool = SqlitePool::connect("sqlite://music.db").await.unwrap();

    let client = Client::new(false, true).await;
    if client.access_token.is_empty() {
        println!("[XX] Failed to authenticate. Exiting...");
        return;
    }

    // Set up an interval for checking the database periodically.
    let mut db_interval = tokio::time::interval(Duration::from_secs(5));
    // Hold the handle for an active download task.
    let mut active_download: Option<tokio::task::JoinHandle<()>> = None;

    loop {
        tokio::select! {
            // Process new incoming commands.
            Some(cmd) = rx.recv() => {
                match cmd {
                    Command::Download(download_cmd) => {
                        match download_cmd {
                            DownloadCommand::Track { track } => {
                                let _ = insert_track(&pool, &track).await;
                                tx.send(Status::TrackDownloaded { id: track.id }).await.unwrap();
                            }
                            _ => {
                                // Handle other download types as needed.
                            }
                        }
                    },
                    // Add handling for Update and Delete commands if needed.
                    _ => {}
                }
            },
            // Periodically check the database for queued downloads.
            _ = db_interval.tick() => {
                // Only start a new download if one isn't already in progress.
                if active_download.is_none() {
                    active_download = process_queued_download(&pool, &tx).await;
                }
            },
            // Await completion of the active download task if one is running.
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

async fn process_queued_download(
    pool: &SqlitePool,
    tx: &Sender<Status>,
) -> Option<tokio::task::JoinHandle<()>> {
    if let Ok(record) = sqlx::query(
        "SELECT id, track FROM tracks WHERE download_status = 'Queued' LIMIT 1"
    )
    .fetch_optional(pool)
    .await {
        if let Some((id, track)) = record {
            let pool = pool.clone();
            let tx = tx.clone();
            return Some(tokio::spawn(async move {
                // Replace the following with your actual download logic.
                println!("Downloading track with id: {}", track.id);
                tokio::time::sleep(Duration::from_secs(3)).await;

                // Update the database to mark the track as downloaded.
                let _ = sqlx::query(
                    "UPDATE tracks SET download_status = 'downloaded' WHERE id = ?",
                    track.id
                )
                .execute(&pool)
                .await;

                // Emit a status update upward.
                tx.send(Status::TrackDownloaded { id: track.id.to_string() })
                    .await
                    .unwrap();
                println!("Track {} downloaded and updated.", track.id);
            }));
        }
    }
    None
}