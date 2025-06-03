use core::panic;
use std::{path::Path, time::Duration};
use std::collections::VecDeque;
use std::sync::{Arc};
use reqwest::header::CONTENT_LENGTH;
use sqlx::{Pool, Sqlite, SqlitePool};
use tokio::{fs, io::AsyncWriteExt, sync::mpsc::{Receiver, Sender}, sync::Mutex};
use tokio::sync::broadcast;
use tokio::time::Instant;
use crate::{client::{Album, Artist, Client, DiscographySong}, database::extension::{remove_track_download, remove_tracks_downloads, query_download_tracks, DownloadStatus}};
use crate::client::Transcoding;
use super::extension::{insert_lyrics, query_download_track};

#[derive(Debug)]
pub enum Command {
    Download(DownloadCommand),
    Update(UpdateCommand),
    Delete(DeleteCommand),
    CancelDownloads,
}

pub enum Status {
    TrackQueued { id: String },
    TrackDownloading { track: DiscographySong },
    TrackDownloaded { id: String },
    TrackDeleted { id: String },

    ArtistsUpdated,
    AlbumsUpdated,
    PlaylistsUpdated,

    DiscographyUpdated { id: String },
    PlaylistUpdated { id: String },

    UpdateStarted,
    UpdateFinished,
    UpdateFailed { error: String },

    ProgressUpdate { progress: f32 },
    AllDownloaded,
    
    Error { error: String },
}

#[derive(Debug)]
pub struct DownloadItem {
    pub name: String,
    pub progress: f32,
}

#[derive(Debug)]
pub enum DownloadCommand {
    Track { track: DiscographySong, playlist_id: Option<String> },
    Tracks { tracks: Vec<DiscographySong> },
}

#[derive(Debug)]
pub enum UpdateCommand {
    SongPlayed { track_id: String },
    Discography { artist_id: String },
    Playlist { playlist_id: String },
    Library,
}

#[derive(Debug)]
pub enum DeleteCommand {
    Track { track: DiscographySong },
    Tracks { tracks: Vec<DiscographySong> },
}

/// This is the main background thread. It queues and processes downloads and background updates.
///
pub async fn t_database<'a>(
    pool: Arc<Pool<Sqlite>>,
    mut rx: Receiver<Command>,
    tx: Sender<Status>,
    online: bool,
    client: Option<Arc<Client>>,
) {

    let cache_dir = dirs::cache_dir().unwrap()
        .join("jellyfin-tui")
        .join("downloads");

    let mut db_interval = tokio::time::interval(Duration::from_secs(1));
    let mut large_update_interval = tokio::time::interval(Duration::from_secs(60 * 10));

    if !online || client.is_none() {
        loop {
            match rx.try_recv() {
                Ok(cmd) => {
                    match cmd {
                        Command::Delete(delete_cmd) => {
                            match delete_cmd {
                                DeleteCommand::Track { track } => {
                                    let _ = remove_track_download(&pool, &track, &cache_dir).await;
                                    let _ = tx.send(Status::TrackDeleted { id: track.id }).await;
                                }
                                DeleteCommand::Tracks { tracks } => {
                                    let _ = remove_tracks_downloads(&pool, &tracks, &cache_dir).await;
                                    for track in tracks {
                                        let _ = tx.send(Status::TrackDeleted { id: track.id }).await;
                                    }
                                }
                            }
                        }
                        Command::Update(update_cmd) => {
                            match update_cmd {
                                UpdateCommand::SongPlayed {
                                    track_id,
                                } => {
                                    let _ = sqlx::query("UPDATE tracks SET last_played = CURRENT_TIMESTAMP WHERE id = ?")
                                        .bind(&track_id)
                                        .execute(&*pool)
                                        .await;
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        return;
    }

    let client = client.unwrap();

    // queue for managing discography updates with priority
    // the first task run is the complete Library update, to see changes made while the app was closed
    let task_queue: Arc<Mutex<VecDeque<UpdateCommand>>> = Arc::new(Mutex::new(VecDeque::new()));
    let mut active_task: Option<tokio::task::JoinHandle<()>> = Some(tokio::spawn(t_data_updater(Arc::clone(&pool), tx.clone(), client.clone())));

    // rx/tx to stop downloads in progress
    let (cancel_tx, _) = broadcast::channel::<String>(16);

    loop {
        tokio::select! {
            Some(cmd) = rx.recv() => {
                match cmd {
                    Command::Download(download_cmd) => {
                        match download_cmd {
                            DownloadCommand::Track { mut track, playlist_id } => {
                                let _ = query_download_track(&pool, &mut track, &playlist_id).await;
                                let _ = tx.send(Status::TrackQueued { id: track.id }).await;
                            }
                            DownloadCommand::Tracks { mut tracks } => {
                                let _ = query_download_tracks(&pool, &mut tracks).await;
                                for track in tracks {
                                    let _ = tx.send(Status::TrackQueued { id: track.id }).await;
                                }
                            }
                        }
                    },
                    Command::Delete(delete_cmd) => {
                        match delete_cmd {
                            DeleteCommand::Track { track } => {
                                let _ = remove_track_download(&pool, &track, &cache_dir).await;
                                let _ = tx.send(Status::TrackDeleted { id: track.id.clone() }).await;
                                let _ = cancel_tx.send(track.id);
                            }
                            DeleteCommand::Tracks { tracks } => {
                                let _ = remove_tracks_downloads(&pool, &tracks, &cache_dir).await;
                                for track in tracks {
                                    let _ = tx.send(Status::TrackDeleted { id: track.id.clone() }).await;
                                    let _ = cancel_tx.send(track.id);
                                }
                            }
                        }
                    },
                    Command::Update(update_cmd) => {
                        let (should_start, next_update) = {
                            let mut queue = task_queue.lock().await;
                            queue.push_front(update_cmd);

                            if active_task.is_none() {
                                (true, queue.pop_back())
                            } else {
                                (false, None)
                            }
                        };

                        if should_start {
                            if let Some(update_cmd) = next_update {
                                active_task = handle_update(update_cmd, Arc::clone(&pool), tx.clone(), client.clone()).await;
                            }
                        }
                    }
                    Command::CancelDownloads => {
                        if let Err(e) = cancel_all_downloads(&pool, tx.clone(), &cancel_tx).await {
                            let _ = tx.send(Status::Error { error: e.to_string() }).await;
                        }
                    }
                }
            },
            _ = db_interval.tick() => {
                if active_task.is_none() {
                    // queue updates have priority here
                    let next_update = {
                        let mut queue = task_queue.lock().await;
                        queue.pop_back()
                    };

                    if let Some(update_cmd) = next_update {
                        active_task = handle_update(update_cmd, Arc::clone(&pool), tx.clone(), client.clone()).await;
                    } else {
                        active_task = track_process_queued_download(&pool, &tx, &client, &cache_dir, &cancel_tx).await;
                    }
                }
            },
            _ = large_update_interval.tick() => {
                if active_task.is_none() {
                    active_task = Some(tokio::spawn(t_data_updater(Arc::clone(&pool), tx.clone(), client.clone())));
                }
            },
            _ = async {
                if let Some(handle) = &mut active_task {
                    match handle.await {
                        Ok(_) => {},
                        Err(e) => {
                            let _ = tx.send(Status::Error { error: e.to_string() }).await;
                        }
                    }
                }
            }, if active_task.is_some() => {
                active_task = None;
            },
        }
    }
}

// If an update has been requested, we process it here.
// The t_functions are expected to send the status to the UI themselves.
async fn handle_update(
    update_cmd: UpdateCommand,
    pool: Arc<Pool<Sqlite>>,
    tx: Sender<Status>,
    client: Arc<Client>,
) -> Option<tokio::task::JoinHandle<()>> {
    match update_cmd {
        UpdateCommand::Discography { artist_id } => {
            Some(tokio::spawn(async move {
                if let Err(e) = t_discography_updater(pool, artist_id, tx.clone(), client).await {
                    // TODO: add logging
                    let _ = tx.send(Status::UpdateFailed { error: e.to_string() }).await;
                }
            }))
        }
        UpdateCommand::SongPlayed { track_id } => {
            let _ = sqlx::query("UPDATE tracks SET last_played = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(&track_id)
                .execute(&*pool)
                .await;
            None
        }
        UpdateCommand::Library => {
            Some(tokio::spawn(t_data_updater(Arc::clone(&pool), tx.clone(), client)))
        }
        UpdateCommand::Playlist { playlist_id } => { 
            Some(tokio::spawn(async move {
                if let Err(e) = t_playlist_updater(pool, playlist_id, tx.clone(), client).await {
                    // TODO: add logging
                    let _ = tx.send(Status::UpdateFailed { error: e.to_string() }).await;
                }
            }))
        }
    }
}

/// This is a thread that gets spawned at the start of the application to fetch all artists/playlists and update them
/// in the DB and also emit the status to the UI to reload the data.
///
pub async fn t_data_updater(
    pool: Arc<Pool<Sqlite>>,
    tx: Sender<Status>,
    client: Arc<Client>,
) {
    let _ = tx.send(Status::UpdateStarted).await;
    match data_updater(pool, Some(tx.clone()), client).await {
        Ok(_) => {
            let _ = tx.send(Status::UpdateFinished).await;
        }
        Err(e) => {
            // TODO: add logging
            let _ = tx.send(Status::UpdateFailed { error: e.to_string() }).await;
        }
    }
}

pub async fn data_updater(
    pool: Arc<Pool<Sqlite>>,
    tx: Option<Sender<Status>>,
    client: Arc<Client>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

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

    let batch_size = 250;

    for (i, artist) in artists.iter().enumerate() {

        if i != 0 && i % batch_size == 0 {
            tx_db.commit().await?;
            tx_db = pool.begin().await?;
            tokio::task::yield_now().await;
        }

        let artist_json = serde_json::to_string(&artist)?;

        let result = sqlx::query(
            r#"
            INSERT INTO artists (id, artist)
            VALUES (?, ?)
            ON CONFLICT(id) DO UPDATE SET artist = excluded.artist
            WHERE artists.artist != excluded.artist;
            "#
        )
        .bind(&artist.id)
        .bind(&artist_json)
        .execute(&mut *tx_db)
        .await?;

        if result.rows_affected() > 0 {
            changes_occurred = true;
        }
    }

    tx_db.commit().await?;

    let remote_artist_ids: Vec<String> = artists.iter().map(|artist| artist.id.clone()).collect();
    let rows_deleted = delete_missing_artists(&pool, &remote_artist_ids).await?;
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

    for (i, album) in albums.iter().enumerate() {
        if i != 0 && i % batch_size == 0 {
            tx_db.commit().await?;
            tx_db = pool.begin().await?;
            tokio::task::yield_now().await;
        }

        let album_json = serde_json::to_string(&album)?;

        let result = sqlx::query(
            r#"
            INSERT INTO albums (id, album)
            VALUES (?, ?)
            ON CONFLICT(id) DO UPDATE SET album = excluded.album
            WHERE albums.album != excluded.album;
            "#
        )
        .bind(&album.id)
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

    for (i, playlist) in playlists.iter().enumerate() {

        if i != 0 && i % batch_size == 0 {
            tx_db.commit().await?;
            tx_db = pool.begin().await?;
            tokio::task::yield_now().await;
        }

        let playlist_json = serde_json::to_string(&playlist)?;

        let result = sqlx::query(
            r#"
            INSERT INTO playlists (id, playlist)
            VALUES (?, ?)
            ON CONFLICT(id) DO UPDATE SET playlist = excluded.playlist
            WHERE playlists.playlist != excluded.playlist;
            "#
        )
        .bind(&playlist.id)
        .bind(&playlist_json)
        .execute(&mut *tx_db)
        .await?;

        if result.rows_affected() > 0 {
            changes_occurred = true;
        }
    }

    tx_db.commit().await?;

    let remote_playlist_ids: Vec<String> = playlists.iter().map(|playlist| playlist.id.clone()).collect();
    let rows_deleted = delete_missing_playlists(&pool, &remote_playlist_ids).await?;
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

/// Similar updater function to the data_updater, but for an individual artist's discography.
/// All tracks pulled into the tracks table and their download_status is set to NotDownloaded.
///
pub async fn t_discography_updater(
    pool: Arc<Pool<Sqlite>>,
    artist_id: String,
    tx: Sender<Status>,
    client: Arc<Client>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    let cache_dir = match dirs::cache_dir() {
        Some(dir) => dir.join("jellyfin-tui").join("downloads"),
        None => return Ok(()),
    };

    let discography = match client.discography(&artist_id).await {
        Ok(discography) => discography,
        Err(_) => return Ok(()),
    };

    let mut dirty = false;

    let mut tx_db = pool.begin().await?;

    // first we need to delete tracks that are not in the remote discography anymore
    let server_ids: Vec<String> = discography.iter().map(|track| track.id.clone()).collect();
    let rows = sqlx::query_as::<_, (String,)>(
        "SELECT track_id FROM artist_membership WHERE artist_id = ?"
    ).bind(&artist_id).fetch_all(&mut *tx_db).await?;
    for track_id in rows {
        if !server_ids.contains(&track_id.0) {
            sqlx::query(
                "DELETE FROM artist_membership WHERE artist_id = ? AND track_id = ?",
            )
                .bind(&artist_id)
                .bind(&track_id.0)
                .execute(&mut *tx_db)
                .await?;
            sqlx::query(
                "DELETE FROM playlist_membership WHERE track_id = ?"
            )
                .bind(&track_id.0)
                .execute(&mut *tx_db)
                .await?;

            let album_row = sqlx::query_as::<_, (String,)>(
                "SELECT album_id FROM tracks WHERE id = ?"
            )
                .bind(&track_id.0)
                .fetch_optional(&mut *tx_db)
                .await?;

            sqlx::query("DELETE FROM tracks WHERE id = ?")
                .bind(&track_id.0)
                .execute(&mut *tx_db)
                .await?;

            sqlx::query("DELETE FROM albums WHERE id = ?")
                .bind(&track_id.0)
                .execute(&mut *tx_db)
                .await?;

            // remove the file from filesystem if need be
            if let Some(album) = album_row {
                let file_path = std::path::Path::new(&cache_dir)
                    .join(&client.server_id)
                    .join(&album.0)
                    .join(&track_id.0);
                let _ = tokio::fs::remove_file(&file_path).await;
            }

            dirty = true;
        }
    }

    let cache_dir = match dirs::cache_dir() {
        Some(dir) => dir.join("jellyfin-tui").join("downloads").join(&client.server_id),
        None => return Ok(()),
    };

    for track in discography {

        let result = sqlx::query(
        r#"
            INSERT OR REPLACE INTO tracks (
                id,
                album_id,
                artist_items,
                download_status,
                track
            ) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                album_id = excluded.album_id,
                artist_items = excluded.artist_items,
                track = json_set(excluded.track, '$.download_status', tracks.download_status)
            WHERE tracks.track != excluded.track;
            "#,
        )
        .bind(&track.id)
        .bind(&track.album_id)
        .bind(serde_json::to_string(&track.artist_items)?)
        .bind(track.download_status.to_string())
        .bind(serde_json::to_string(&track)?)
        .execute(&mut *tx_db)
        .await?;

        if result.rows_affected() > 0 {
            dirty = true;
        }

        // if Downloaded is true, let's check if the file exists. In case the user deleted it, NotDownloaded is set
        if let Some(download_status) = sqlx::query_as::<_, DownloadStatus>(
            "SELECT download_status FROM tracks WHERE id = ?"
        ).bind(&track.id)
        .fetch_optional(&mut *tx_db)
        .await? {
            let file_path = cache_dir.join(&track.album_id).join(&track.id);
            if matches!(download_status, DownloadStatus::Downloaded) && !file_path.exists() {
                // if the user deleted the file, we set the download status to NotDownloaded
                sqlx::query("UPDATE tracks SET download_status = 'NotDownloaded' WHERE id = ?")
                    .bind(&track.id)
                    .execute(&mut *tx_db)
                    .await?;
                dirty = true;
            }
            if !matches!(download_status, DownloadStatus::Downloaded) && file_path.exists() {
                // conversely, if i made a mistake we can recover here
                sqlx::query("UPDATE tracks SET download_status = 'Downloaded' WHERE id = ?")
                    .bind(&track.id)
                    .execute(&mut *tx_db)
                    .await?;
                dirty = true;
            }
        }

        let result = sqlx::query(
            r#"
            INSERT OR REPLACE INTO artist_membership (
                artist_id,
                track_id
            ) VALUES (?, ?)
            "#,
        )
        .bind(&artist_id)
        .bind(&track.id)
        .execute(&mut *tx_db)
        .await?;

        if result.rows_affected() > 0 {
            dirty = true;
        }
    }

    tx_db.commit().await.ok();

    if dirty {
        tx.send(Status::DiscographyUpdated { id: artist_id }).await.ok();
    }

    Ok(())
}

/// Very similar idea here, but here we only manage the playlist_membership table. If a song disappears from the remote playlist, it doesn't necessarily mean it should be deleted from the local database.
pub async fn t_playlist_updater(
    pool: Arc<Pool<Sqlite>>,
    playlist_id: String,
    tx: Sender<Status>,
    client: Arc<Client>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let playlist = match client.playlist(&playlist_id).await {
        Ok(playlist) => playlist,
        Err(_) => return Ok(()),
    };

    let mut dirty = false;

    let mut tx_db = pool.begin().await?;

    // the strategy for playlists is not removing, but only dealing with playlist_membership table
    let server_ids: Vec<String> = playlist.items.iter().map(|track| track.id.clone()).collect();
    let rows = sqlx::query_as::<_, (String,)>(
        "SELECT track_id FROM playlist_membership WHERE playlist_id = ?"
    ).bind(&playlist_id).fetch_all(&mut *tx_db).await?;

    for track_id in rows {
        if !server_ids.contains(&track_id.0) {
            sqlx::query(
                "DELETE FROM playlist_membership WHERE playlist_id = ? AND track_id = ?",
            )
                .bind(&playlist_id)
                .bind(&track_id.0)
                .execute(&mut *tx_db)
                .await?;
            dirty = true;
        }
    }

    let cache_dir = match dirs::cache_dir() {
        Some(dir) => dir.join("jellyfin-tui").join("downloads").join(&client.server_id),
        None => return Ok(()),
    };

    for (i, track) in playlist.items.iter().enumerate() {
        let result = sqlx::query(
            r#"
            INSERT OR REPLACE INTO tracks (
                id,
                album_id,
                artist_items,
                download_status,
                track
            ) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                album_id = excluded.album_id,
                artist_items = excluded.artist_items,
                track = json_set(excluded.track, '$.download_status', tracks.download_status)
            WHERE tracks.track != excluded.track;
            "#,
        )
            .bind(&track.id)
            .bind(&track.album_id)
            .bind(serde_json::to_string(&track.artist_items)?)
            .bind(track.download_status.to_string())
            .bind(serde_json::to_string(&track)?)
            .execute(&mut *tx_db)
            .await?;

        if result.rows_affected() > 0 {
            dirty = true;
        }

        // if Downloaded is true, let's check if the file exists. In case the user deleted it, NotDownloaded is set
        if let Some(download_status) = sqlx::query_as::<_, DownloadStatus>(
            "SELECT download_status FROM tracks WHERE id = ?"
        ).bind(&track.id).fetch_optional(&mut *tx_db).await? {
            let file_path = cache_dir.join(&track.album_id).join(&track.id);
            if matches!(download_status, DownloadStatus::Downloaded) && !file_path.exists() {
                // if the user deleted the file, we set the download status to NotDownloaded
                sqlx::query("UPDATE tracks SET download_status = 'NotDownloaded' WHERE id = ?")
                    .bind(&track.id)
                    .execute(&mut *tx_db)
                    .await?;
                dirty = true;
            }
            if !matches!(download_status, DownloadStatus::Downloaded) && file_path.exists() {
                // conversely, if i made a mistake we can recover here
                sqlx::query("UPDATE tracks SET download_status = 'Downloaded' WHERE id = ?")
                    .bind(&track.id)
                    .execute(&mut *tx_db)
                    .await?;
                dirty = true;
            }
        }

        let result = sqlx::query(
            r#"
            INSERT OR REPLACE INTO playlist_membership (
                playlist_id,
                track_id,
                position
            ) VALUES (?, ?, ?)
            "#,
        )
            .bind(&playlist_id)
            .bind(&track.id)
            .bind(i as i64)
            .execute(&mut *tx_db)
            .await?;

        if result.rows_affected() > 0 {
            dirty = true;
        }
    }

    tx_db.commit().await.ok();

    if dirty {
        let _ = tx.send(Status::PlaylistUpdated { id: playlist_id }).await;
    }

    Ok(())

}
/// Deletes local artists for the given server that are not present in the remote list.
/// Uses a temporary table to store remote artist IDs.
/// Do NOT call this concurrently unless you rework the temp table creation (sqlite isolates temp tables per connection).
/// TODO: add file removal process
///
/// Returns the number of rows affected.
async fn delete_missing_artists(
    pool: &SqlitePool,
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
         WHERE id NOT IN (SELECT id FROM tmp_remote_artist_ids);",
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query("DROP TABLE IF EXISTS tmp_remote_artist_ids;")
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
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
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

    let deleted_albums: Vec<(String,)> = sqlx::query_as(
        "DELETE FROM albums
         WHERE id NOT IN (SELECT id FROM tmp_remote_album_ids)
         RETURNING id;",
    )
    .fetch_all(&mut *tx)
    .await?;
    
    sqlx::query("DROP TABLE IF EXISTS tmp_remote_album_ids;")
        .execute(&mut *tx)
        .await?;

    let cache_dir = match dirs::cache_dir() {
        Some(dir) => dir.join("jellyfin-tui").join("downloads").join(&server_id),
        None => return Ok(deleted_albums.len())
    };

    for (album,) in &deleted_albums {
        match std::fs::exists(cache_dir.join(&album)) {
            Ok(true) => {
                let _ = std::fs::remove_dir_all(cache_dir.join(album));
            }
            _ => {}
        }
    }

    tx.commit().await?;
    Ok(deleted_albums.len())
}

/// Deletes local playlists for the given server that are not present in the remote list.
/// Uses a temporary table to store remote playlist IDs.
///
/// Returns the number of rows affected.
async fn delete_missing_playlists(
    pool: &SqlitePool,
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
         WHERE id NOT IN (SELECT id FROM tmp_remote_playlist_ids);",
    )
    .execute(&mut *tx)
    .await?;
    
    sqlx::query("DROP TABLE IF EXISTS tmp_remote_playlist_ids;")
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
    cancel_tx: &broadcast::Sender<String>,
) -> Option<tokio::task::JoinHandle<()>> {
    if let Ok(record) = sqlx::query_as::<_, (String, String, String)>(
        "
        SELECT id, album_id, track
            FROM tracks WHERE download_status = 'Queued' OR download_status = 'Downloading'
            ORDER BY download_status ASC LIMIT 1
        "
    )
    .fetch_optional(pool)
    .await {
       
        // downloads using transcoded files not implemented yet. Future me problem?
        let transcoding_off = Transcoding {
            enabled: false,
            bitrate: 0,
            container: String::from("")
        };
        
        if let Some((id, album_id, track_str)) = record {
            let track: DiscographySong = match serde_json::from_str(&track_str) {
                Ok(track) => track,
                Err(_) => {
                    println!("Failed to parse track JSON: {}", track_str);
                    return None;
                }
            };

            let pool = pool.clone();
            let tx = tx.clone();
            let url = client.song_url_sync(&track.id, &transcoding_off);
            let file_dir = cache_dir.join(&track.server_id).join(album_id);
            if !file_dir.exists() {
                if fs::create_dir_all(&file_dir).await.is_err() {
                    println!("Failed to create directory: {}", file_dir.display());
                    return None;
                }
            }

            // this will pull it if it doesn't exist already
            let _ = client.download_cover_art(&track.parent_id).await;
            let lyrics = client.lyrics(&track.id).await;
            if let Ok(lyrics) = lyrics.as_ref() {
                let _ = insert_lyrics(&pool, &track.id, lyrics).await;
            }

            let mut cancel_rx = cancel_tx.subscribe();

            return Some(tokio::spawn(async move {
                if let Err(_) =
                    track_download_and_update(&pool, &id, &url, &file_dir, &track, &tx, &mut cancel_rx).await
                {
                    // TODO: log
                    // println!("Download process failed for track {}: {:?}", track.id, e);
                }
            }));
        } else {
            // totally nothing to download anymore, let's send an end query thing
            let _ = tx.send(Status::AllDownloaded).await;
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
    cancel_rx: &mut broadcast::Receiver<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // T1 set Downloading status
    {
        let mut tx_db = pool.begin().await?;
        sqlx::query("UPDATE tracks SET download_status = 'Downloading' WHERE id = ?")
            .bind(id)
            .execute(&mut *tx_db)
            .await?;
        tx_db.commit().await?;

        tx.send(Status::TrackDownloading { track: track.clone() }).await?;
    }

    // Download a song
    let mut total_size: i64 = 0;
    let download_result = async {
        let mut downloaded: u64 = 0;
        let mut response = reqwest::get(url).await?;
        if let Some(content_length) = response.headers().get(CONTENT_LENGTH) {
            total_size = content_length.to_str()?.parse()?;
        }
        // TODO: download into a temporary file and then rename it to the final name
        let mut last_update = Instant::now();
        let file_path = file_dir.join(format!("{}", track.id));
        let mut file = fs::File::create(&file_path).await?;
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            if last_update.elapsed() >= Duration::from_secs_f64(0.2) {
                // this lets the user cancel a download in progress
                match cancel_rx.try_recv() {
                    Ok(to_cancel) if to_cancel == track.id || to_cancel == "all" => {
                        let _ = tx.send(Status::UpdateFinished).await;
                        return Ok(());
                    }
                    _ => {} // let's keep going, this should be fine :3
                }
                let progress = if total_size > 0 {
                    downloaded as f32 / total_size as f32 * 100.0
                } else {
                    0.0
                };
                let _ = tx
                    .send(Status::ProgressUpdate { progress })
                    .await;
                last_update = Instant::now();
            }
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;

    let _ = tx.send(Status::ProgressUpdate { progress: 99.9 }).await;

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
                    sqlx::query(
                    r#"
                        UPDATE tracks
                        SET download_status = 'Downloaded',
                            download_size_bytes = ?,
                            downloaded_at = CURRENT_TIMESTAMP
                        WHERE id = ?
                        "#
                    )
                    .bind(total_size)
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

async fn cancel_all_downloads(
    pool: &SqlitePool,
    tx: Sender<Status>,
    cancel_tx: &broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut tx_db = pool.begin().await?;
    let rows = sqlx::query_as::<_, (String,)>(
        "UPDATE tracks SET download_status = 'NotDownloaded' 
     WHERE download_status = 'Queued' OR download_status = 'Downloading'
     RETURNING id"
    )
        .fetch_all(&mut *tx_db)
        .await?;

    let affected_ids: Vec<String> = rows.into_iter().map(|row| row.0).collect();

    tx_db.commit().await?;

    // send a cancel signal to all downloads
    let _ = cancel_tx.send("all".to_string());
    let _ = tx.send(Status::AllDownloaded).await;
    
    for id in affected_ids {
        let _ = tx.send(Status::TrackDeleted { id }).await;
    }

    Ok(())
}