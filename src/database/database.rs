use core::panic;
use std::{path::Path, time::Duration};

use sqlx::{SqlitePool, sqlite::SqliteTransaction};
use tokio::{fs, io::AsyncWriteExt, sync::mpsc::{error::TryRecvError, Receiver, Sender}, time::Interval};

use crate::{client::{Album, Artist, Client, DiscographySong}, database::extension::{delete_track, delete_tracks, insert_tracks, DownloadStatus}, playlists};

use super::extension::insert_track;

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

    ArtistsUpdated,
    AlbumsUpdated,
    PlaylistsUpdated,
    UpdateFailed { error: String },
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

/// This is a thread that gets spawned at the start of the application to fetch all artists/playlists and update them
/// in the DB and also emit the status to the UI to reload the data.
/// 
pub async fn t_data_updater(
    tx: Sender<Status>,
) {
    loop {
        match data_updater(Some(tx.clone())).await {
            Ok(_) => {}
            Err(e) => {
                let _ = tx.send(Status::UpdateFailed { error: e.to_string() }).await;
            }
        }
        tokio::time::sleep(Duration::from_secs(60 * 10)).await;
    }
}

pub async fn data_updater(
    tx: Option<Sender<Status>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    let pool = SqlitePool::connect("sqlite://music.db").await.unwrap();

    let client = match Client::new(true, true).await {
        Some(client) => client,
        None => {
            return Err("Failed to create client".into());
        }
    };

    if client.access_token.is_empty() {
        return Err("No access token found".into());
    }
    let artists: Vec<Artist> = match client.artists(String::from("")).await {
        Ok(artists) => artists,
        Err(_) => return Err("Failed to fetch artists".into()),
    };
    let albums: Vec<Album> = match client.albums().await {
        Ok(albums) => albums,
        Err(_) => return Err("Failed to fetch albums".into()),
    };
    let playlists = match client.playlists(String::from("")).await {
        Ok(playlists) => playlists,
        Err(_) => return Err("Failed to fetch playlists".into()),
    };

    let mut tx_db = pool.begin().await?;
    let mut changes_occurred = false;

    for artist in &artists {
        let artist_json = serde_json::to_string(&artist)?;

        let result = sqlx::query(
            r#"
            INSERT INTO artists (id, server_id, artist)
            VALUES (?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                artist = excluded.artist,
                server_id = excluded.server_id
            WHERE artists.artist != excluded.artist;
            "#
        )
        .bind(&artist.id)
        .bind(&client.server_id)
        .bind(&artist_json)
        .execute(&mut *tx_db)
        .await?;

        if result.rows_affected() > 0 {
            changes_occurred = true;
        }
    }

    tx_db.commit().await?;

    let remote_artist_ids: Vec<String> = artists.iter().map(|artist| artist.id.clone()).collect();
    let rows_deleted = delete_missing_artists(&pool, &client.server_id, &remote_artist_ids).await?;
    if rows_deleted > 0 {
        changes_occurred = true;
    }

    if changes_occurred {
        if let Some(tx) = &tx {
            tx.send(Status::ArtistsUpdated).await?;
        }
    }

    changes_occurred = false;
    let mut tx_db = pool.begin().await?;

    for album in &albums {
        let album_json = serde_json::to_string(&album)?;

        let result = sqlx::query(
            r#"
            INSERT INTO albums (id, server_id, album)
            VALUES (?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                album = excluded.album,
                server_id = excluded.server_id
            WHERE albums.album != excluded.album;
            "#
        )
        .bind(&album.id)
        .bind(&client.server_id)
        .bind(&album_json)
        .execute(&mut *tx_db)
        .await?;

        if result.rows_affected() > 0 {
            changes_occurred = true;
        }
    }

    tx_db.commit().await?;

    let remote_album_ids: Vec<String> = albums.iter().map(|album| album.id.clone()).collect();
    let rows_deleted = delete_missing_albums(&pool, &client.server_id, &remote_album_ids).await?;
    if rows_deleted > 0 {
        changes_occurred = true;
    }

    if changes_occurred {
        if let Some(tx) = &tx {
            tx.send(Status::AlbumsUpdated).await?;
        }
    }

    changes_occurred = false;
    let mut tx_db = pool.begin().await?;

    for playlist in &playlists {
        let playlist_json = serde_json::to_string(&playlist)?;

        let result = sqlx::query(
            r#"
            INSERT INTO playlists (id, server_id, playlist)
            VALUES (?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                playlist = excluded.playlist,
                server_id = excluded.server_id
            WHERE playlists.playlist != excluded.playlist;
            "#
        )
        .bind(&playlist.id)
        .bind(&client.server_id)
        .bind(&playlist_json)
        .execute(&mut *tx_db)
        .await?;

        if result.rows_affected() > 0 {
            changes_occurred = true;
        }
    }

    tx_db.commit().await?;

    let remote_playlist_ids: Vec<String> = playlists.iter().map(|playlist| playlist.id.clone()).collect();
    let rows_deleted = delete_missing_playlists(&pool, &client.server_id, &remote_playlist_ids).await?;
    if rows_deleted > 0 {
        changes_occurred = true;
    }

    if changes_occurred {
        if let Some(tx) = &tx {
            tx.send(Status::PlaylistsUpdated).await?;
        }
    }

    Ok(())
}

/// Deletes local artists for the given server that are not present in the remote list.
/// Uses a temporary table to store remote artist IDs.
/// 
/// Returns the number of rows affected.
async fn delete_missing_artists(
    pool: &SqlitePool,
    server_id: &str,
    remote_artist_ids: &[String],
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let mut tx = pool.begin().await?;

    sqlx::query("CREATE TEMPORARY TABLE tmp_remote_artist_ids (id TEXT PRIMARY KEY);")
        .execute(&mut *tx)
        .await?;

    for artist_id in remote_artist_ids {
        sqlx::query("INSERT INTO tmp_remote_artist_ids (id) VALUES (?);")
            .bind(artist_id)
            .execute(&mut *tx)
            .await?;
    }

    let result = sqlx::query(
        "DELETE FROM artists
         WHERE server_id = ?
         AND id NOT IN (SELECT id FROM tmp_remote_artist_ids);",
    )
    .bind(server_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(result.rows_affected())
}

/// Deletes local albums for the given server that are not present in the remote list.
/// Uses a temporary table to store remote album IDs.
/// 
/// Returns the number of rows affected.
async fn delete_missing_albums(
    pool: &SqlitePool,
    server_id: &str,
    remote_album_ids: &[String],
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let mut tx = pool.begin().await?;

    sqlx::query("CREATE TEMPORARY TABLE tmp_remote_album_ids (id TEXT PRIMARY KEY);")
        .execute(&mut *tx)
        .await?;

    for album_id in remote_album_ids {
        sqlx::query("INSERT INTO tmp_remote_album_ids (id) VALUES (?);")
            .bind(album_id)
            .execute(&mut *tx)
            .await?;
    }

    let result = sqlx::query(
        "DELETE FROM albums
         WHERE server_id = ?
         AND id NOT IN (SELECT id FROM tmp_remote_album_ids);",
    )
    .bind(server_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(result.rows_affected())
}

/// Deletes local playlists for the given server that are not present in the remote list.
/// Uses a temporary table to store remote playlist IDs.
/// 
/// Returns the number of rows affected.
async fn delete_missing_playlists(
    pool: &SqlitePool,
    server_id: &str,
    remote_playlist_ids: &[String],
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let mut tx = pool.begin().await?;

    sqlx::query("CREATE TEMPORARY TABLE tmp_remote_playlist_ids (id TEXT PRIMARY KEY);")
        .execute(&mut *tx)
        .await?;

    for playlist_id in remote_playlist_ids {
        sqlx::query("INSERT INTO tmp_remote_playlist_ids (id) VALUES (?);")
            .bind(playlist_id)
            .execute(&mut *tx)
            .await?;
    }

    let result = sqlx::query(
        "DELETE FROM playlists
         WHERE server_id = ?
         AND id NOT IN (SELECT id FROM tmp_remote_playlist_ids);",
    )
    .bind(server_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(result.rows_affected())
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

                    tx.send(Status::TrackDownloaded { id: track.id.to_string() })
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

    Ok(())
}