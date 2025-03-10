use sqlx::SqlitePool;
use tokio::sync::mpsc::{error::TryRecvError, Receiver, Sender};

use crate::client::DiscographySong;

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

    loop {
        let status = rx.try_recv();
        match status {
            Ok(Command::Download(cmd)) => {
                match cmd {
                    DownloadCommand::Track { track } => {
                        let _ = insert_track(&pool, &track).await;
                        tx.send(Status::TrackDownloaded { id: track.id }).await.unwrap();
                    }
                    _ => {}
                }
            }
            Err(e) => {
                if e != TryRecvError::Empty {
                    println!("{:?}", e);
                }
            },
            _ => {},
        }
        tokio::time::sleep(std::time::Duration::from_secs_f32(0.2)).await;
    }
}