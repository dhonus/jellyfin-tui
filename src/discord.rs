use crate::tui::Song;
use discord_rich_presence::activity::StatusDisplayType;
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;

pub enum DiscordCommand {
    Playing {
        track: Song,
        percentage_played: f64,
        server_url: String,
        paused: bool,
        show_art: bool,
        status_display_type: StatusDisplayType,
    },
    Stopped,
}

pub fn t_discord(mut rx: Receiver<DiscordCommand>, client_id: u64) {
    let mut drpc: Option<DiscordIpcClient> = None;
    let should_reconnect = Arc::new(AtomicBool::new(false));
    let reconnect_flag = should_reconnect.clone();
    let reconnect_flag2 = should_reconnect.clone();

    reconnect_loop(&mut drpc, client_id);

    while let Some(cmd) = rx.blocking_recv() {
        if should_reconnect.load(Ordering::SeqCst) {
            reconnect_loop(&mut drpc, client_id);
            should_reconnect.store(false, Ordering::SeqCst);
        }
        match cmd {
            DiscordCommand::Playing {
                track,
                percentage_played,
                server_url,
                paused,
                show_art,
                status_display_type,
            } => {
                let duration_secs = track.run_time_ticks as f64 / 10_000_000f64;
                let elapsed_secs = (duration_secs * percentage_played).round() as i64;
                let start_time = chrono::Local::now() - chrono::Duration::seconds(elapsed_secs);
                let end_time = start_time + chrono::Duration::seconds(duration_secs.round() as i64);

                // log::info!(
                //     "Track duration: {:.2} seconds, Elapsed: {} seconds",
                //    duration_secs,
                //    elapsed_secs
                //);

                let state = track.artist.chars().take(128).collect::<String>();

                // Note: Images cover-placeholder, paused and playing need to be registered
                // on Discord's dev portal to show up in the Rich Presence.
                let mut assets = activity::Assets::new();

                let url = format!(
                    "{}/Items/{}/Images/Primary?fillHeight=480&fillWidth=480",
                    server_url, track.album_id
                );
                assets = if show_art {
                    assets.large_image(url.as_str())
                } else {
                    assets.large_image("cover-placeholder")
                }
                // This is supposed to only be shown when hovering over the large image in the status.
                // However, Discord also seems to show it as a third regular line of text now.
                .large_text(track.album);

                assets = if paused {
                    assets.small_image("paused").small_text("Paused")
                } else {
                    assets.small_image("playing").small_text("Playing")
                };

                let mut activity = activity::Activity::new()
                    .activity_type(activity::ActivityType::Listening)
                    .status_display_type(status_display_type)
                    .state(state.as_str())
                    .details(track.name)
                    .assets(assets);

                // Don't show timestamp if the song is paused, since Discord will continue counting up otherwise
                activity = if paused {
                    activity
                } else {
                    let ts = activity::Timestamps::new()
                        .start(start_time.timestamp())
                        .end(end_time.timestamp());
                    activity.timestamps(ts)
                };

                let send_result = drpc
                    .as_mut()
                    .ok_or_else(|| "Discord IPC not connected".to_string())
                    .and_then(|c| c.set_activity(activity).map_err(|e| e.to_string()));

                if let Err(e) = send_result {
                    log::debug!("Failed to set Discord activity: {}", e);
                    reconnect_flag.store(true, Ordering::SeqCst);
                    reconnect_flag2.store(true, Ordering::SeqCst);
                }
            }
            DiscordCommand::Stopped => {
                let cleared = drpc.as_mut().map(|c| {
                    c.clear_activity().or_else(|_| c.set_activity(activity::Activity::new()))
                });
                if let Some(Err(e)) = cleared {
                    log::error!("Failed to clear Discord activity: {}", e);
                    should_reconnect.store(true, Ordering::SeqCst);
                }
            }
        }
    }
    log::info!("Discord command receiver closed, stopping Discord RPC client.");
    if let Some(mut c) = drpc.take() {
        let _ = c.close();
    }
}

fn reconnect_loop(drpc: &mut Option<DiscordIpcClient>, client_id: u64) {
    log::debug!("Reconnecting to Discord RPC...");
    if let Some(mut c) = drpc.take() {
        let _ = c.close();
    }
    let app_id = client_id.to_string();
    let mut client = DiscordIpcClient::new(&app_id);
    match client.connect() {
        Ok(()) => {
            *drpc = Some(client);
            log::info!("Discord RPC connected.");
        }
        Err(e) => {
            *drpc = None;
            log::debug!("Discord RPC connect failed: {e}, retrying in 5 seconds...");
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    }
}
