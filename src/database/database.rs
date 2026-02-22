use super::extension::{
    get_last_library_update, insert_lyrics, query_download_track, set_last_library_update,
};
use crate::client::{NetworkQuality, ProgressReport, Transcoding};
use crate::{
    client::{Artist, Client, DiscographySong},
    database::extension::{
        query_download_tracks, remove_track_download, remove_tracks_downloads, DownloadStatus,
    },
};
use core::panic;
use reqwest::header::CONTENT_LENGTH;
use sqlx::{Pool, Sqlite, SqlitePool};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{path::Path, time::Duration};
use tokio::sync::broadcast;
use tokio::time::Instant;
use tokio::{
    fs,
    io::AsyncWriteExt,
    sync::mpsc::{Receiver, Sender},
    sync::Mutex,
};

#[derive(Debug)]
pub enum Command {
    Download(DownloadCommand),
    Update(UpdateCommand),
    Remove(RemoveCommand), // remove local files
    Rename(RenameCommand),
    Delete(DeleteCommand), // delete on the jellyfin server
    CancelDownloads,
    Jellyfin(JellyfinCommand),
    DislikeTrack { track_id: String, disliked: bool },
}

pub enum Status {
    TrackQueued { id: String },
    TrackDownloading { track: DiscographySong },
    TrackDownloaded { id: String },
    TrackDeleted { id: String },
    CoverArtDownloaded { item_id: Option<String> },

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

    NetworkQualityChanged(NetworkQuality),

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
    CoverArt { item_id: String },
}

#[derive(Debug)]
pub enum UpdateCommand {
    SongPlayed { track_id: String },
    Discography { artist_id: String },
    Playlist { playlist_id: String },
    Library,
    OfflineRepair,
}

#[derive(Debug)]
pub enum RemoveCommand {
    Track { track: DiscographySong },
    Tracks { tracks: Vec<DiscographySong> },
}

#[derive(Debug)]
pub enum DeleteCommand {
    Playlist { id: String },
}

#[derive(Debug)]
pub enum RenameCommand {
    Playlist { id: String, new_name: String },
}

#[derive(Debug)]
pub enum JellyfinCommand {
    Stopped { id: Option<String>, position_ticks: Option<u64> },
    Playing { id: String },
    ReportProgress { progress_report: ProgressReport },
}

/// This is the main background thread. It queues and processes downloads and background updates.
///
pub async fn t_database<'a>(
    pool: Arc<Pool<Sqlite>>,
    mut rx: Receiver<Command>,
    tx: Sender<Status>,
    online: bool,
    client: Option<Arc<Client>>,
    server_id: String,
    network_quality: NetworkQuality,
) {
    let data_dir = dirs::data_dir().unwrap().join("jellyfin-tui").join("downloads");

    let mut db_interval = tokio::time::interval(Duration::from_secs(1));
    let mut large_update_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(60 * 10),
        Duration::from_secs(60 * 10),
    );

    if !online || client.is_none() {
        let mut active_task: Option<tokio::task::JoinHandle<()>> = None;

        loop {
            tokio::select! {
                received = rx.recv() => {
                    match received {
                        Some(cmd) => {
                            match cmd {
                                Command::Remove(delete_cmd) => {
                                    match delete_cmd {
                                        RemoveCommand::Track { track } => {
                                            if let Err(e) = remove_track_download(&pool, &track, &data_dir).await {
                                                log::error!("Failed to remove track download: {}", e);
                                            }
                                            let _ = tx.send(Status::TrackDeleted { id: track.id }).await;
                                        }
                                        RemoveCommand::Tracks { tracks } => {
                                            if let Err(e) = remove_tracks_downloads(&pool, &tracks, &data_dir).await {
                                                log::error!("Failed to remove tracks downloads: {}", e);
                                            }
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
                                        UpdateCommand::OfflineRepair { .. } => {
                                            let should_spawn = match &active_task {
                                                Some(handle) if !handle.is_finished() => false,
                                                _ => true,
                                            };

                                            if should_spawn {
                                                log::info!("Spawning offline track checker...");
                                                let handle = tokio::spawn(t_offline_tracks_checker(
                                                    Arc::clone(&pool),
                                                    tx.clone(),
                                                    data_dir.clone(),
                                                    server_id.clone(),
                                                ));
                                                active_task = Some(handle);
                                            } else {
                                                log::debug!("Offline track checker is already running.");
                                            }
                                        }

                                        _ => {}
                                    }
                                }
                                Command::Rename(rename_cmd) => {
                                    match rename_cmd {
                                        RenameCommand::Playlist { id, new_name } => {
                                            if let Err(e) = rename_playlist(&pool, &id, &new_name).await {
                                                log::error!("Failed to rename playlist {}: {}", id, e);
                                            }
                                        }
                                    }
                                }
                                Command::Delete(delete_cmd) => {
                                    match delete_cmd {
                                        DeleteCommand::Playlist { id } => {
                                            if let Err(e) = delete_playlist(&pool, &id).await {
                                                log::error!("Failed to delete playlist {}: {}", id, e);
                                            }
                                        }
                                    }
                                }
                                Command::DislikeTrack { track_id, disliked } => {
                                    if let Err(e) = mark_track_as_disliked(&pool, &track_id, disliked).await {
                                        log::error!("Failed to mark track {} as disliked: {}", track_id, e);
                                    }
                                }
                                _ => {
                                    log::warn!("Received unsupported command: {:?}", cmd);
                                }
                            }
                        }
                        None => {
                            log::info!("Command channel closed, exiting database thread.");
                            return;
                        }
                    }
                }
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

    let client = client.unwrap();

    // queue for managing discography updates with priority
    let task_queue: Arc<Mutex<VecDeque<UpdateCommand>>> = Arc::new(Mutex::new(VecDeque::new()));
    let mut active_task = None;

    // The first task run is the complete Library update, to see changes made while the app was closed
    // Only do it every 10 minutes by default including across restarts.
    if network_quality == NetworkQuality::Normal {
        if let Some(last) = get_last_library_update(&pool).await {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
            if now - last >= 600 {
                active_task = Some(tokio::spawn(t_data_updater(
                    Arc::clone(&pool),
                    tx.clone(),
                    client.clone(),
                )));
            } else {
                log::debug!("skipping library update on startup");
            }
        } else {
            active_task =
                Some(tokio::spawn(t_data_updater(Arc::clone(&pool), tx.clone(), client.clone())));
        }
    }

    // rx/tx to stop downloads in progress
    let (cancel_tx, _) = broadcast::channel::<Vec<String>>(4);

    // intervals for checking network quality
    let mut netcheck_interval = tokio::time::interval(Duration::from_secs(120));
    let mut last_quality = network_quality; // or NetworkQuality::Normal

    loop {
        tokio::select! {
            Some(cmd) = rx.recv() => {
                match cmd {
                    Command::Download(download_cmd) => {
                        match download_cmd {
                            DownloadCommand::Track { mut track, playlist_id } => {
                                if let Err(e) = query_download_track(&pool, &mut track, &playlist_id).await {
                                    log::error!("Failed to query download track: {}", e);
                                }
                                let _ = tx.send(Status::TrackQueued { id: track.id }).await;
                            }
                            DownloadCommand::Tracks { mut tracks } => {
                                if let Err(e) = query_download_tracks(&pool, &mut tracks).await {
                                    log::error!("Failed to query download tracks: {}", e);
                                }
                                for track in tracks {
                                    let _ = tx.send(Status::TrackQueued { id: track.id }).await;
                                }
                            }
                            DownloadCommand::CoverArt { item_id } => {
                                if let Err(e) = client.download_cover_art(&item_id).await {
                                    let _ = tx.send(Status::CoverArtDownloaded { item_id: None }).await;
                                    log::error!("Failed to download cover art for {}: {}", item_id, e);
                                } else {
                                    let _ = tx.send(Status::CoverArtDownloaded { item_id: Some(item_id) }).await;
                                }
                            }
                        }
                    },
                    Command::Remove(delete_cmd) => {
                        match delete_cmd {
                            RemoveCommand::Track { track } => {
                                let _ = cancel_tx.send(Vec::from([track.id.clone()]));
                                let _ = tx.send(Status::TrackDeleted { id: track.id.clone() }).await;
                                if let Err(e) = remove_track_download(&pool, &track, &data_dir).await {
                                    log::error!("Failed to remove track download: {}", e);
                                }
                            }
                            RemoveCommand::Tracks { tracks } => {
                                let _ = cancel_tx.send(tracks.iter().map(|t| t.id.clone()).collect());
                                if let Err(e) = remove_tracks_downloads(&pool, &tracks, &data_dir).await {
                                    log::error!("Failed to remove tracks downloads: {}", e);
                                }
                                for track in &tracks {
                                    let _ = tx.send(Status::TrackDeleted { id: track.id.clone() }).await;
                                }
                            }
                        }
                    },
                    Command::Update(update_cmd) => {
                        let (should_start, next_update) = {
                            let mut queue = task_queue.lock().await;
                            queue.push_front(update_cmd);
                            prune_update_queue(&mut queue);

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
                    Command::Rename(rename_cmd) => {
                        match rename_cmd {
                            RenameCommand::Playlist { id, new_name } => {
                                if let Err(e) = rename_playlist(&pool, &id, &new_name).await {
                                    log::error!("Failed to rename playlist {}: {}", id, e);
                                }
                            }
                        }
                    }
                    Command::Delete(delete_cmd) => {
                        match delete_cmd {
                            DeleteCommand::Playlist { id } => {
                                if let Err(e) = delete_playlist(&pool, &id).await {
                                    log::error!("Failed to delete playlist {}: {}", id, e);
                                }
                            }
                        }
                    }
                    Command::Jellyfin(jellyfin_cmd) => {
                        match jellyfin_cmd {
                            JellyfinCommand::Stopped { id, position_ticks } => {
                                if let Err(e) = client.stopped(id, position_ticks).await {
                                    log::error!("Failed to send stopped report to jellyfin: {}", e);
                                }
                            }
                            JellyfinCommand::Playing { id } => {
                                if let Err(e) = client.playing(&id).await {
                                    log::error!("Failed to send playing report to jellyfin: {}", e);
                                }
                            }
                            JellyfinCommand::ReportProgress { progress_report } => {
                                if let Err(e) = client.report_progress(&progress_report).await {
                                    log::error!("Failed to report progress to jellyfin: {}", e);
                                }
                            }
                        }
                    }
                    Command::CancelDownloads => {
                        if let Err(e) = cancel_all_downloads(&pool, tx.clone(), &cancel_tx).await {
                            let _ = tx.send(Status::Error { error: e.to_string() }).await;
                        }
                    }
                    Command::DislikeTrack { track_id, disliked } => {
                        if let Err(e) = mark_track_as_disliked(&pool, &track_id, disliked).await {
                            log::error!("Failed to mark track {} as disliked: {}", track_id, e);
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
                    } else if last_quality != NetworkQuality::CzechTrain {
                        active_task = track_process_queued_download(&pool, &tx, &client, &data_dir, &cancel_tx).await;
                    }
                }
            },
            _ = large_update_interval.tick() => {
                if last_quality == NetworkQuality::Normal {
                    if active_task.is_none() {
                        active_task = Some(tokio::spawn(t_data_updater(Arc::clone(&pool), tx.clone(), client.clone())));
                    }
                }
            },
            // this is here to adjust the network quality checking interval dynamically
            // for example, if you're on a train we want to disable auto updates and downloads
            // and when we enter a good lte zone we can pick up again
            _ = netcheck_interval.tick() => {
                let new_quality = Client::get_network_quality(
                    &reqwest::Client::new(),
                    &client.base_url,
                ).await;
                if new_quality != last_quality {
                    last_quality = new_quality;
                    // notify UI
                    let _ = tx.send(Status::NetworkQualityChanged(new_quality)).await;
                    match new_quality {
                        NetworkQuality::Normal => {
                            netcheck_interval = tokio::time::interval(Duration::from_secs(180));
                        }
                        NetworkQuality::Slow => {
                            netcheck_interval = tokio::time::interval(Duration::from_secs(90));
                        }
                        NetworkQuality::CzechTrain => {
                            netcheck_interval = tokio::time::interval(Duration::from_secs(30));
                        }
                    }
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
        UpdateCommand::Discography { artist_id } => Some(tokio::spawn(async move {
            if let Err(e) = t_discography_updater(pool, artist_id.clone(), tx.clone(), client).await
            {
                let _ = tx.send(Status::UpdateFailed { error: e.to_string() }).await;
                log::error!("Failed to update discography for artist {}: {}", artist_id, e);
            }
        })),
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
        UpdateCommand::Playlist { playlist_id } => Some(tokio::spawn(async move {
            if let Err(e) = t_playlist_updater(pool, playlist_id.clone(), tx.clone(), client).await
            {
                let _ = tx.send(Status::UpdateFailed { error: e.to_string() }).await;
                log::error!("Failed to update playlist {}: {}", playlist_id, e);
            }
        })),
        UpdateCommand::OfflineRepair => {
            let data_dir = match dirs::data_dir() {
                Some(dir) => dir.join("jellyfin-tui").join("downloads"),
                None => {
                    log::error!("Could not find data directory for offline repair");
                    return None;
                }
            };
            Some(tokio::spawn(t_offline_tracks_checker(
                Arc::clone(&pool),
                tx.clone(),
                data_dir,
                client.server_id.clone(),
            )))
        }
    }
}

/// This is a thread that gets spawned at the start of the application to fetch all artists/playlists and update them
/// in the DB and also emit the status to the UI to reload the data.
///
pub async fn t_data_updater(pool: Arc<Pool<Sqlite>>, tx: Sender<Status>, client: Arc<Client>) {
    let _ = tx.send(Status::UpdateStarted).await;
    match data_updater(pool, Some(tx.clone()), client).await {
        Ok(_) => {
            let _ = tx.send(Status::UpdateFinished).await;
        }
        Err(e) => {
            let _ = tx.send(Status::UpdateFailed { error: e.to_string() }).await;
            log::error!("Background updater task failed. This is a major bug: {}", e);
        }
    }
}

/// This fixes offline tracks, checking if they are still present on the filesystem and updating their status in the DB. Sometimes necessary to run
/// when the user deletes files manually or moves them around. Auto-triggered if something weird is detected, runnable by user.
async fn t_offline_tracks_checker(
    pool: Arc<Pool<Sqlite>>,
    tx: Sender<Status>,
    data_dir: std::path::PathBuf,
    server_id: String,
) {
    let _ = tx.send(Status::UpdateStarted).await;
    match offline_tracks_checker(pool, tx.clone(), data_dir, server_id).await {
        Ok(_) => {
            let _ = tx.send(Status::UpdateFinished).await;
        }
        Err(e) => {
            let _ = tx.send(Status::UpdateFailed { error: e.to_string() }).await;
            log::error!("Offline tracks checker failed: {}", e);
        }
    }
}

pub async fn data_updater(
    pool: Arc<Pool<Sqlite>>,
    tx: Option<Sender<Status>>,
    client: Arc<Client>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::info!("Starting global data updater...");

    let start_time = Instant::now();

    let music_libs = client.music_libraries().await?;
    if music_libs.is_empty() {
        return Err("No music libraries returned".into());
    }

    let artists: Vec<Artist> = client.artists(String::from("")).await?;
    let playlists = client.playlists(String::from("")).await?;

    log::info!(
        "Fetched {} artists and {} playlists in {:.2}s",
        artists.len(),
        playlists.len(),
        start_time.elapsed().as_secs_f32()
    );

    let mut albums_complete = true;

    let batch_size = 250;

    // save our libs first
    {
        let mut tx_db = pool.begin().await?;
        for lib in &music_libs {
            sqlx::query(
                r#"
                INSERT INTO libraries (id, name, collection_type, last_seen, selected)
                VALUES (?, ?, ?, CURRENT_TIMESTAMP, 1)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    last_seen = CURRENT_TIMESTAMP;
                "#,
            )
            .bind(&lib.id)
            .bind(&lib.name)
            .bind(&lib.collection_type)
            .execute(&mut *tx_db)
            .await?;
        }
        tx_db.commit().await?;
    }

    let mut tx_db = pool.begin().await?;

    for (i, artist) in artists.iter().enumerate() {
        if i != 0 && i % batch_size == 0 {
            tokio::task::yield_now().await;
        }

        let artist_json = serde_json::to_string(&artist)?;

        sqlx::query(
            r#"
            INSERT INTO artists (id, artist)
            VALUES (?, ?)
            ON CONFLICT(id) DO UPDATE SET artist = excluded.artist
            WHERE artists.artist != excluded.artist;
            "#,
        )
        .bind(&artist.id)
        .bind(&artist_json)
        .execute(&mut *tx_db)
        .await?;
    }

    tx_db.commit().await?;

    if let Some(tx) = &tx {
        log::info!("Artists updated, sending notification to UI");
        tx.send(Status::ArtistsUpdated).await?;
    }

    let artist_ids: Vec<String> = artists.iter().map(|a| a.id.clone()).collect();
    let remote_json = serde_json::to_string(&artist_ids)?;

    let mut tx_db = pool.begin().await?;
    let mut remote_album_ids: Vec<String> = vec![];

    for lib in &music_libs {
        log::info!("Fetching albums for library '{}' (id={})", lib.name, lib.id);
        let albums = match client.albums(Some(&lib.id)).await {
            Ok(albums) => {
                log::info!(
                    "Fetched {} albums for library '{}' (id={})",
                    albums.len(),
                    lib.name,
                    lib.id
                );
                albums
            }
            Err(e) => {
                albums_complete = false;
                log::warn!("Failed to fetch albums for library {}: {}", lib.id, e);
                continue; // keep local state until we get a clean run
            }
        };

        if albums.is_empty() {
            log::warn!("Library '{}' (id={}) returned ZERO albums from Jellyfin", lib.name, lib.id);
        }

        for (i, album) in albums.iter().enumerate() {
            if i != 0 && i % batch_size == 0 {
                tokio::task::yield_now().await;
            }

            let album_json = serde_json::to_string(&album)?;

            let result = sqlx::query(
                r#"
                INSERT INTO albums (id, album, library_id)
                VALUES (?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    album = excluded.album,
                    library_id = excluded.library_id
                WHERE albums.album != excluded.album
                   OR albums.library_id IS NULL
                   OR albums.library_id != excluded.library_id;
                "#,
            )
            .bind(&album.id)
            .bind(&album_json)
            .bind(&lib.id)
            .execute(&mut *tx_db)
            .await?;

            if result.rows_affected() > 0 {
                log::debug!("Album updated: {:?}", album);
            }

            remote_album_ids.push(album.id.clone());

            sqlx::query("DELETE FROM album_artist WHERE album_id = ?")
                .bind(&album.id)
                .execute(&mut *tx_db)
                .await?;

            for artist in &album.album_artists {
                let canonical = sqlx::query_scalar::<_, Option<String>>(
                    "SELECT id FROM artists WHERE json_extract(artist, '$.Name') = ?",
                )
                .bind(&artist.name)
                .fetch_optional(&mut *tx_db)
                .await?
                .flatten();

                let canonical_id = canonical.unwrap_or_else(|| artist.id.clone());

                sqlx::query(
                    r#"
                    INSERT OR IGNORE INTO album_artist (album_id, artist_id)
                    VALUES (?, ?)
                    "#,
                )
                .bind(&album.id)
                .bind(&canonical_id)
                .execute(&mut *tx_db)
                .await?;
            }
        }
        log::info!(
            "Finished processing library '{}' (id={}), total albums processed={}",
            lib.name,
            lib.id,
            albums.len()
        );
    }

    tx_db.commit().await?;

    if albums_complete {
        let mut tx_db = pool.begin().await?;
        sqlx::query(
            r#"
        DELETE FROM album_artist
        WHERE artist_id NOT IN (
            SELECT value FROM json_each(json(?))
        );
        "#,
        )
        .bind(&remote_json)
        .execute(&mut *tx_db)
        .await?;
        tx_db.commit().await?;
    }

    mark_missing(&pool, &tx, "artist", &artist_ids, &client.server_id, 4).await?;

    tx_db = pool.begin().await?;
    sqlx::query(
        r#"
        UPDATE tracks
        SET library_id = (
            SELECT library_id FROM albums WHERE albums.id = tracks.album_id
        )
        WHERE library_id IS NULL
          AND EXISTS (
              SELECT 1 FROM albums WHERE albums.id = tracks.album_id
          )
        "#,
    )
    .execute(&mut *tx_db)
    .await?;

    tx_db.commit().await?;

    if let Some(tx) = &tx {
        tx.send(Status::AlbumsUpdated).await?;
    }

    if albums_complete {
        mark_missing(&pool, &tx, "album", &remote_album_ids, &client.server_id, 3).await?;
    } else {
        log::warn!("skipping album deletion pass: album list incomplete (some libraries failed).");
    }

    let mut tx_db = pool.begin().await?;

    for (i, playlist) in playlists.iter().enumerate() {
        if i != 0 && i % batch_size == 0 {
            tokio::task::yield_now().await;
        }

        let playlist_json = serde_json::to_string(&playlist)?;

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO playlists (id, playlist)
            VALUES (?, ?)
            ON CONFLICT(id) DO UPDATE SET playlist = excluded.playlist
            WHERE playlists.playlist != excluded.playlist;
            "#,
        )
        .bind(&playlist.id)
        .bind(&playlist_json)
        .execute(&mut *tx_db)
        .await?;
    }

    tx_db.commit().await?;

    let remote_playlist_ids: Vec<String> = playlists.iter().map(|p| p.id.clone()).collect();
    mark_missing(&pool, &tx, "playlist", &remote_playlist_ids, &client.server_id, 3).await?;

    if let Some(tx) = &tx {
        tx.send(Status::PlaylistsUpdated).await?;
    }

    log::info!("Global data updater took {:.2}s", start_time.elapsed().as_secs_f32());

    set_last_library_update(&pool).await;

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
    let data_dir = match dirs::data_dir() {
        Some(dir) => dir.join("jellyfin-tui").join("downloads").join(&client.server_id),
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
        "SELECT track_id FROM artist_membership WHERE artist_id = ?",
    )
    .bind(&artist_id)
    .fetch_all(&mut *tx_db)
    .await?;

    for (track_id,) in rows {
        if !server_ids.contains(&track_id) {
            // Remove memberships
            sqlx::query("DELETE FROM artist_membership WHERE artist_id = ? AND track_id = ?")
                .bind(&artist_id)
                .bind(&track_id)
                .execute(&mut *tx_db)
                .await?;
            sqlx::query("DELETE FROM playlist_membership WHERE track_id = ?")
                .bind(&track_id)
                .execute(&mut *tx_db)
                .await?;

            dirty = true;
        }
    }

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
        .bind(serde_json::to_string(&track.album_artists)?)
        .bind(track.download_status.to_string())
        .bind(serde_json::to_string(&track)?)
        .execute(&mut *tx_db)
        .await?;

        if result.rows_affected() > 0 {
            dirty = true;
        }

        if let Some(lib_id) =
            sqlx::query_scalar::<_, Option<String>>(r#"SELECT library_id FROM albums WHERE id = ?"#)
                .bind(&track.album_id)
                .fetch_optional(&mut *tx_db)
                .await?
        {
            sqlx::query(r#"UPDATE tracks SET library_id = ? WHERE id = ?"#)
                .bind(lib_id)
                .bind(&track.id)
                .execute(&mut *tx_db)
                .await?;
        }

        // if Downloaded is true, let's check if the file exists. In case the user deleted it, NotDownloaded is set
        if let Some(download_status) =
            sqlx::query_as::<_, DownloadStatus>("SELECT download_status FROM tracks WHERE id = ?")
                .bind(&track.id)
                .fetch_optional(&mut *tx_db)
                .await?
        {
            let file_path = data_dir.join(&track.album_id).join(&track.id);
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
    let playlist = match client.playlist(&playlist_id, None).await {
        Ok(playlist) => playlist,
        Err(_) => return Ok(()),
    };

    let mut dirty = false;

    let mut tx_db = pool.begin().await?;

    // the strategy for playlists is not removing, but only dealing with playlist_membership table
    let server_ids: Vec<String> = playlist.items.iter().map(|track| track.id.clone()).collect();
    let rows = sqlx::query_as::<_, (String,)>(
        "SELECT track_id FROM playlist_membership WHERE playlist_id = ?",
    )
    .bind(&playlist_id)
    .fetch_all(&mut *tx_db)
    .await?;

    for track_id in rows {
        if !server_ids.contains(&track_id.0) {
            sqlx::query("DELETE FROM playlist_membership WHERE playlist_id = ? AND track_id = ?")
                .bind(&playlist_id)
                .bind(&track_id.0)
                .execute(&mut *tx_db)
                .await?;
            dirty = true;
        }
    }

    let data_dir = match dirs::data_dir() {
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
        .bind(serde_json::to_string(&track.album_artists)?)
        .bind(track.download_status.to_string())
        .bind(serde_json::to_string(&track)?)
        .execute(&mut *tx_db)
        .await?;

        if result.rows_affected() > 0 {
            dirty = true;
        }

        if let Some(lib_id) =
            sqlx::query_scalar::<_, Option<String>>(r#"SELECT library_id FROM albums WHERE id = ?"#)
                .bind(&track.album_id)
                .fetch_optional(&mut *tx_db)
                .await?
        {
            sqlx::query(r#"UPDATE tracks SET library_id = ? WHERE id = ?"#)
                .bind(lib_id)
                .bind(&track.id)
                .execute(&mut *tx_db)
                .await?;
        } else {
            log::warn!(
                "Album {} for track {} in playlist {} not found in local DB",
                track.album_id,
                track.id,
                playlist_id
            );
        }

        // if Downloaded is true, let's check if the file exists. In case the user deleted it, NotDownloaded is set
        if let Some(download_status) =
            sqlx::query_as::<_, DownloadStatus>("SELECT download_status FROM tracks WHERE id = ?")
                .bind(&track.id)
                .fetch_optional(&mut *tx_db)
                .await?
        {
            let file_path = data_dir.join(&track.album_id).join(&track.id);
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
            // log::debug!("Updated playlist membership for track: {}", track.id);
            dirty = true;
        }
    }

    tx_db.commit().await.ok();

    if dirty {
        let _ = tx.send(Status::PlaylistUpdated { id: playlist_id }).await;
    }

    Ok(())
}

/// This will go over all downloaded tracks, make sure they exist (if not, set their status to NotDownloaded), and emit the correct status updates to the UI. Also, make sure it won't block the db while checking the files. It takes a long time
async fn offline_tracks_checker(
    pool: Arc<Pool<Sqlite>>,
    tx: Sender<Status>,
    data_dir: std::path::PathBuf,
    server_id: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();

    let mut tx_db = pool.begin().await?;

    // Fetch track IDs and album IDs
    let tracks: Vec<(String, String)> =
        sqlx::query_as("SELECT id, album_id FROM tracks WHERE download_status = 'Downloaded';")
            .fetch_all(&mut *tx_db)
            .await?;
    tx_db.commit().await?;

    // Group tracks by album_id
    let mut grouped_tracks: HashMap<String, Vec<String>> = HashMap::new();
    for (id, album_id) in tracks {
        grouped_tracks.entry(album_id).or_default().push(id);
    }

    let mut missing_ids = Vec::new();

    // Check file existence per album
    for (album_id, track_ids) in &grouped_tracks {
        let album_path = data_dir.join(&server_id).join(&album_id);
        for id in track_ids {
            let file_path = album_path.join(&id);
            if tokio::fs::metadata(&file_path).await.is_err() {
                missing_ids.push(id.clone());
                let _ = tx.send(Status::TrackDeleted { id: id.clone() }).await;
            }
        }
    }

    // Update DB only if there are missing files
    if !missing_ids.is_empty() {
        let mut tx_db = pool.begin().await?;
        for id in missing_ids {
            sqlx::query("UPDATE tracks SET download_status = 'NotDownloaded' WHERE id = ?")
                .bind(&id)
                .execute(&mut *tx_db)
                .await?;
        }
        tx_db.commit().await?;
    }

    let elapsed_time = start_time.elapsed();
    log::info!(
        "Offline tracks checker finished. Checked {} tracks in {:.2}s.",
        grouped_tracks.iter().map(|(_, v)| v.len()).sum::<usize>(),
        elapsed_time.as_secs_f32()
    );

    Ok(())
}

/// Deletes local albums for the given server that are not present in the remote list.
/// Uses a temporary table to store remote album IDs.
///
/// Returns the number of rows affected.
// async fn delete_missing_albums(
//     pool: &SqlitePool,
//     server_id: &str,
//     remote_album_ids: &[String],
// ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
//     let mut tx = pool.begin().await?;
//
//     sqlx::query("CREATE TEMPORARY TABLE tmp_remote_album_ids (id TEXT PRIMARY KEY);")
//         .execute(&mut *tx)
//         .await?;
//
//     for album_id in remote_album_ids {
//         sqlx::query("INSERT INTO tmp_remote_album_ids (id) VALUES (?);")
//             .bind(album_id)
//             .execute(&mut *tx)
//             .await?;
//     }
//
//     let deleted_albums: Vec<(String,)> = sqlx::query_as(
//         "DELETE FROM albums
//          WHERE id NOT IN (SELECT id FROM tmp_remote_album_ids)
//          RETURNING id;",
//     )
//     .fetch_all(&mut *tx)
//     .await?;
//
//     sqlx::query("DROP TABLE IF EXISTS tmp_remote_album_ids;")
//         .execute(&mut *tx)
//         .await?;
//
//     tx.commit().await?;
//     Ok(deleted_albums.len())
// }

async fn track_process_queued_download(
    pool: &SqlitePool,
    tx: &Sender<Status>,
    client: &Client,
    data_dir: &std::path::PathBuf,
    cancel_tx: &broadcast::Sender<Vec<String>>,
) -> Option<tokio::task::JoinHandle<()>> {
    let mut cancel_rx = cancel_tx.subscribe();

    if let Ok(record) = sqlx::query_as::<_, (String, String, String)>(
        "
        SELECT id, album_id, track
        FROM tracks
        WHERE download_status = 'Queued' OR download_status = 'Downloading'
        ORDER BY
            COALESCE(CAST(json_extract(track, '$.IndexNumber') AS INTEGER), 999999) ASC,
            CASE download_status
                WHEN 'Downloading' THEN 0
                WHEN 'Queued' THEN 1
                ELSE 2
           END ASC
        LIMIT 1
        ",
    )
    .fetch_optional(pool)
    .await
    {
        // downloads using transcoded files not implemented yet. Future me problem?
        let transcoding_off =
            Transcoding { enabled: false, bitrate: 0, container: String::from("") };

        if let Some((id, album_id, track_str)) = record {
            let track: DiscographySong = match serde_json::from_str(&track_str) {
                Ok(track) => track,
                Err(_) => {
                    log::error!("Failed to deserialize track: {}", track_str);
                    return None;
                }
            };

            let pool = pool.clone();
            let tx = tx.clone();
            let url = client.song_url_sync(&track.id, &transcoding_off);
            let file_dir = data_dir.join(&track.server_id).join(album_id);
            if !file_dir.exists() {
                if fs::create_dir_all(&file_dir).await.is_err() {
                    log::error!("Failed to create directory for track: {}", file_dir.display());
                    return None;
                }
            }

            // this will pull it if it doesn't exist already. // TODO: use the cache...
            let _ = client.download_cover_art(&track.parent_id).await;
            let lyrics = client.lyrics(&track.id).await;
            if let Ok(lyrics) = lyrics.as_ref() {
                let _ = insert_lyrics(&pool, &track.id, lyrics).await;
            }

            return Some(tokio::spawn(async move {
                if let Err(e) = track_download_and_update(
                    &pool,
                    &id,
                    &url,
                    &file_dir,
                    &track,
                    &tx,
                    &mut cancel_rx,
                )
                .await
                {
                    let _ = sqlx::query(
                        "UPDATE tracks SET download_status = 'NotDownloaded' WHERE id = ?",
                    )
                    .bind(&id)
                    .execute(&pool)
                    .await;
                    log::error!("Failed to download track {}: {} Error: {}", id, url, e);
                    let _ = tx.send(Status::TrackDeleted { id: track.id }).await;
                }
            }));
        } else {
            // that's all folks!
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
    cancel_rx: &mut broadcast::Receiver<Vec<String>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = dirs::data_dir().unwrap().join("jellyfin-tui").join("downloads");
    let temp_file = path.join("jellyfin-tui-track.part");
    if temp_file.exists() {
        let _ = fs::remove_file(&temp_file).await;
    }
    if let Ok(cancelled_ids) = cancel_rx.try_recv() {
        if cancelled_ids.contains(&track.id) {
            return Ok(());
        }
    }

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
        let mut last_update = Instant::now();
        let mut file = fs::File::create(&temp_file).await?;
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            if last_update.elapsed() >= Duration::from_secs_f64(0.2) {
                // this lets the user cancel a download in progress
                match cancel_rx.try_recv() {
                    Ok(to_cancel) if to_cancel.contains(&track.id) => {
                        let _ = tx.send(Status::TrackDeleted { id: track.id.to_string() }).await?;
                        sqlx::query(
                            "UPDATE tracks SET download_status = 'NotDownloaded' WHERE id = ?",
                        )
                        .bind(id)
                        .execute(pool)
                        .await?;
                        return Ok(());
                    }
                    _ => {} // let's keep going, this should be fine :3
                }
                let progress = if total_size > 0 {
                    downloaded as f32 / total_size as f32 * 100.0
                } else {
                    0.0
                };
                let _ = tx.send(Status::ProgressUpdate { progress }).await;
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
                    "SELECT download_status FROM tracks WHERE id = ?",
                )
                .bind(id)
                .fetch_one(&mut *tx_db)
                .await;

                let file_path = file_dir.join(format!("{}", track.id));
                if let Err(e) = fs::rename(&temp_file, file_path).await {
                    return Err(Box::new(e));
                }

                if let Ok(record) = record {
                    if !matches!(record, DownloadStatus::Downloading) {
                        let _ = fs::remove_file(&temp_file).await;
                        return Ok(());
                    }
                    sqlx::query(
                        r#"
                        UPDATE tracks
                        SET download_status = 'Downloaded',
                            download_size_bytes = ?,
                            downloaded_at = CURRENT_TIMESTAMP
                        WHERE id = ?
                        "#,
                    )
                    .bind(total_size)
                    .bind(id)
                    .execute(&mut *tx_db)
                    .await?;

                    tx.send(Status::TrackDownloaded { id: track.id.to_string() }).await?;
                } else {
                    let _ = fs::remove_file(&temp_file).await;
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
    cancel_tx: &broadcast::Sender<Vec<String>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut tx_db = pool.begin().await?;
    let rows = sqlx::query_as::<_, (String,)>(
        "UPDATE tracks SET download_status = 'NotDownloaded'
     WHERE download_status = 'Queued' OR download_status = 'Downloading'
     RETURNING id",
    )
    .fetch_all(&mut *tx_db)
    .await?;

    let affected_ids: Vec<String> = rows.into_iter().map(|row| row.0).collect();

    tx_db.commit().await?;

    // send a cancel signal to all downloads
    let _ = cancel_tx.send(affected_ids.clone()).unwrap_or_default();
    let _ = tx.send(Status::AllDownloaded).await;

    for id in affected_ids {
        let _ = tx.send(Status::TrackDeleted { id }).await;
    }

    Ok(())
}

async fn rename_playlist(
    pool: &SqlitePool,
    playlist_id: &str,
    new_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut tx_db = pool.begin().await?;
    sqlx::query("UPDATE playlists SET playlist = json_set(playlist, '$.Name', ?) WHERE id = ?")
        .bind(&new_name)
        .bind(&playlist_id)
        .execute(&mut *tx_db)
        .await?;
    tx_db.commit().await?;

    Ok(())
}

async fn delete_playlist(
    pool: &SqlitePool,
    playlist_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut tx_db = pool.begin().await?;

    sqlx::query("DELETE FROM playlist_membership WHERE playlist_id = ?")
        .bind(playlist_id)
        .execute(&mut *tx_db)
        .await?;

    sqlx::query("DELETE FROM playlists WHERE id = ?")
        .bind(playlist_id)
        .execute(&mut *tx_db)
        .await?;

    tx_db.commit().await?;

    Ok(())
}

async fn mark_track_as_disliked(
    pool: &SqlitePool,
    track_id: &str,
    disliked: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut tx_db = pool.begin().await?;
    sqlx::query("UPDATE tracks SET disliked = ? WHERE id = ?")
        .bind(disliked as i64)
        .bind(track_id)
        .execute(&mut *tx_db)
        .await?;
    tx_db.commit().await?;

    Ok(())
}

/// This cleans up duplicates in the queue. Avoids the user triggering N wasteful updates that produce identical output.
fn prune_update_queue(queue: &mut VecDeque<UpdateCommand>) {
    let mut seen = Vec::new();
    let mut discography_count = 0;

    queue.retain(|cmd| match cmd {
        UpdateCommand::Discography { artist_id } => {
            if seen.contains(artist_id) {
                return false;
            }
            discography_count += 1;
            if discography_count > 3 {
                return false;
            }
            seen.push(artist_id.clone());
            true
        }
        _ => true,
    });

    let mut seen_library = false;
    queue.retain(|cmd| match cmd {
        UpdateCommand::Library => {
            if seen_library {
                false
            } else {
                seen_library = true;
                true
            }
        }
        _ => true,
    });
}

pub async fn mark_missing(
    pool: &SqlitePool,
    db_thread_tx: &Option<Sender<Status>>,
    entity_type: &str,
    remote_ids: &Vec<String>,
    server_id: &String,
    threshold: i64,
) -> sqlx::Result<()> {
    if !matches!(entity_type, "artist" | "album" | "playlist") {
        return Ok(());
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

    let remote_json = serde_json::to_string(remote_ids).unwrap();

    // flags for UI update after deletions
    let mut deleted_albums = false;
    let mut deleted_artists = false;
    let mut deleted_playlists = false;
    let mut album_paths_to_delete: Vec<PathBuf> = Vec::new();
    let data_dir = dirs::data_dir().unwrap().join("jellyfin-tui").join("downloads").join(server_id);

    let mut tx = pool.begin().await?;

    // insert or update missing counters
    match entity_type {
        "artist" => {
            sqlx::query(
                r#"
                    UPDATE missing_counters
                    SET missing_seen_count = missing_seen_count + 1,
                        last_checked_at = ?
                    WHERE entity_type = 'artist'
                      AND id IN (
                          SELECT id FROM artists
                          WHERE id NOT IN (SELECT value FROM json_each(json(?)))
                            AND id NOT IN (SELECT artist_id FROM album_artist)
                      );
                    "#,
            )
            .bind(now)
            .bind(&remote_json)
            .execute(&mut *tx)
            .await?;

            let new_missing_count = sqlx::query(
                r#"
                    INSERT INTO missing_counters (entity_type, id, missing_seen_count, last_checked_at)
                    SELECT 'artist', id, 1, ?
                    FROM artists
                    WHERE id NOT IN (SELECT value FROM json_each(json(?)))
                      AND id NOT IN (SELECT artist_id FROM album_artist)
                      AND NOT EXISTS (
                          SELECT 1 FROM missing_counters mc
                          WHERE mc.entity_type = 'artist' AND mc.id = artists.id
                      );
                    "#
            )
                .bind(now)
                .bind(&remote_json)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            if new_missing_count > 0 {
                deleted_artists = true;
            }
        }

        "album" => {
            sqlx::query(
                r#"
                UPDATE missing_counters
                SET missing_seen_count = missing_seen_count + 1,
                    last_checked_at = ?
                WHERE entity_type = 'album'
                  AND id IN (
                      SELECT id FROM albums
                      WHERE id NOT IN (SELECT value FROM json_each(json(?)))
                  );
                "#,
            )
            .bind(now)
            .bind(&remote_json)
            .execute(&mut *tx)
            .await?;

            let new_missing_count = sqlx::query(
                r#"
                INSERT INTO missing_counters (entity_type, id, missing_seen_count, last_checked_at)
                SELECT 'album', id, 1, ?
                FROM albums
                WHERE id NOT IN (SELECT value FROM json_each(json(?)))
                  AND NOT EXISTS (
                      SELECT 1 FROM missing_counters mc
                      WHERE mc.entity_type = 'album' AND mc.id = albums.id
                  );
                "#,
            )
            .bind(now)
            .bind(&remote_json)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if new_missing_count > 0 {
                deleted_albums = true;
            }
        }

        "playlist" => {
            sqlx::query(
                r#"
                UPDATE missing_counters
                SET missing_seen_count = missing_seen_count + 1,
                    last_checked_at = ?
                WHERE entity_type = 'playlist'
                  AND id IN (
                      SELECT id FROM playlists
                      WHERE id NOT IN (SELECT value FROM json_each(json(?)))
                  );
                "#,
            )
            .bind(now)
            .bind(&remote_json)
            .execute(&mut *tx)
            .await?;

            let new_missing_count = sqlx::query(
                r#"
                INSERT INTO missing_counters (entity_type, id, missing_seen_count, last_checked_at)
                SELECT 'playlist', id, 1, ?
                FROM playlists
                WHERE id NOT IN (SELECT value FROM json_each(json(?)))
                  AND NOT EXISTS (
                      SELECT 1 FROM missing_counters mc
                      WHERE mc.entity_type = 'playlist' AND mc.id = playlists.id
                  );
                "#,
            )
            .bind(now)
            .bind(&remote_json)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if new_missing_count > 0 {
                deleted_playlists = true;
            }
        }

        _ => {}
    }

    // reset missing counters for items that appeared again remotely
    sqlx::query(
        r#"
        DELETE FROM missing_counters
        WHERE entity_type = ?
          AND id IN (SELECT value FROM json_each(json(?)));
        "#,
    )
    .bind(entity_type)
    .bind(&remote_json)
    .execute(&mut *tx)
    .await?;

    // -------
    let stale: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT id FROM missing_counters
        WHERE entity_type = ?
          AND missing_seen_count >= ?;
        "#,
    )
    .bind(entity_type)
    .bind(threshold)
    .fetch_all(&mut *tx)
    .await?;

    // delete stale data
    for (id,) in stale {
        match entity_type {
            "album" => {
                // delete playlist + artist memberships that refer to tracks on this album
                sqlx::query(
                    r#"DELETE FROM playlist_membership
                       WHERE track_id IN (SELECT id FROM tracks WHERE album_id = ?)"#,
                )
                .bind(&id)
                .execute(&mut *tx)
                .await?;

                sqlx::query(
                    r#"DELETE FROM artist_membership
                       WHERE track_id IN (SELECT id FROM tracks WHERE album_id = ?)"#,
                )
                .bind(&id)
                .execute(&mut *tx)
                .await?;

                // delete tracks and album relations
                sqlx::query("DELETE FROM tracks WHERE album_id = ?")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;

                sqlx::query("DELETE FROM album_artist WHERE album_id = ?")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;

                let rows_affected = sqlx::query("DELETE FROM albums WHERE id = ?")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();

                if rows_affected > 0 {
                    sqlx::query("DELETE FROM missing_counters WHERE entity_type = ? AND id = ?")
                        .bind(entity_type)
                        .bind(&id)
                        .execute(&mut *tx)
                        .await?;
                    deleted_albums = true;
                }

                album_paths_to_delete.push(data_dir.join(&id));
            }

            "artist" => {
                let rows_affected = sqlx::query(
                    r#"
                        DELETE FROM artists
                        WHERE id = ?
                          AND id NOT IN (SELECT artist_id FROM album_artist);
                        "#,
                )
                .bind(&id)
                .execute(&mut *tx)
                .await?
                .rows_affected();

                if rows_affected > 0 {
                    sqlx::query("DELETE FROM artist_membership WHERE artist_id = ?")
                        .bind(&id)
                        .execute(&mut *tx)
                        .await?;
                    deleted_artists = true;

                    // remove from missing_counters only if delete succeeded
                    sqlx::query("DELETE FROM missing_counters WHERE entity_type = ? AND id = ?")
                        .bind(entity_type)
                        .bind(&id)
                        .execute(&mut *tx)
                        .await?;
                }
            }

            "playlist" => {
                sqlx::query("DELETE FROM playlist_membership WHERE playlist_id = ?")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("DELETE FROM playlists WHERE id = ?")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("DELETE FROM missing_counters WHERE entity_type = ? AND id = ?")
                    .bind(entity_type)
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;

                deleted_playlists = true;
            }

            _ => {}
        }
    }

    tx.commit().await?;

    for path in album_paths_to_delete {
        let _ = fs::remove_dir_all(&path).await;
        log::info!("deleted local album dir: {:?}", path);
    }

    if let Some(db_thread_tx) = db_thread_tx {
        if deleted_albums {
            db_thread_tx.send(Status::AlbumsUpdated).await.ok();
        }
        if deleted_artists {
            db_thread_tx.send(Status::ArtistsUpdated).await.ok();
        }
        if deleted_playlists {
            db_thread_tx.send(Status::PlaylistsUpdated).await.ok();
        }
    }

    Ok(())
}
