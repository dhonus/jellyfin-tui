use crate::config;
use crate::tui::Song;
use discord_presence::models::{Activity, ActivityAssets, ActivityTimestamps};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;

pub enum DiscordCommand {
    Playing {
        track: Song,
        percentage_played: f64,
        server_url: String,
        paused: bool,
    },
    Stopped,
}

pub fn t_discord(mut rx: Receiver<DiscordCommand>, client_id: u64) {
    let mut drpc = discord_presence::Client::new(client_id);
    let should_reconnect = Arc::new(AtomicBool::new(false));
    let reconnect_flag = should_reconnect.clone();
    let reconnect_flag2 = should_reconnect.clone();

    drpc.on_event(discord_presence::Event::Ready, |ready| {
        log::info!("Discord RPC ready: {:?}", ready);
    })
    .persist();

    drpc.on_error(move |ctx| {
        log::error!("Discord RPC error: {:?}", ctx);
        reconnect_flag2.store(true, Ordering::SeqCst);
    })
    .persist();

    drpc.on_disconnected(move |_| {
        reconnect_flag.store(true, Ordering::SeqCst);
    })
    .persist();

    reconnect_loop(&mut drpc);

    let mut last_update = std::time::Instant::now() - std::time::Duration::from_secs(2);

    while let Some(cmd) = rx.blocking_recv() {
        if should_reconnect.load(Ordering::SeqCst) {
            reconnect_loop(&mut drpc);
            should_reconnect.store(false, Ordering::SeqCst);
        }
        match cmd {
            DiscordCommand::Playing {
                track,
                percentage_played,
                server_url,
                paused
            } => {
                // Hard throttle to 1 update per second
                if last_update.elapsed() < std::time::Duration::from_secs(1) {
                    continue;
                }
                last_update = std::time::Instant::now();

                let duration_secs = track.run_time_ticks as f64 / 10_000_000f64;
                let elapsed_secs = (duration_secs * percentage_played).round() as i64;
                let start_time = chrono::Local::now() - chrono::Duration::seconds(elapsed_secs);
                let end_time = start_time + chrono::Duration::seconds(duration_secs.round() as i64);

                // log::info!(
                //     "Track duration: {:.2} seconds, Elapsed: {} seconds",
                //    duration_secs,
                //    elapsed_secs
                //);

                let mut state = format!("by {}", track.artist);
                state.truncate(128);

                let mut activity = Activity::new()
                    .name(&track.name)
                    .assets(|_| {
                        // Note: Images cover-placeholder, paused and playing need to be registered
                        // on Discord's dev portal to show up in the Rich Presence.
                        let mut assets = ActivityAssets::new();

                        //FIXME: there's got to be a better way to do this
                        let config = config::get_config().unwrap();
                        assets = if config.get("discord_art").and_then(|d| d.as_bool()) == Some(true) {
                            assets.large_image(format!(
                                    "{}/Items/{}/Images/Primary?fillHeight=480&fillWidth=480",
                                    server_url, track.parent_id
                                ))
                        } else {
                            assets.large_image("cover-placeholder")
                        }
                        // This is supposed to only be shown when hovering over the large image in the status.
                        // However, Discord also seems to show it as a third regular line of text now.
                        .large_text(format!("from {}", &track.album));

                        assets = if paused {
                            assets.small_image("paused").small_text("Paused")
                        } else {
                            assets.small_image("playing").small_text("Playing")
                        };

                        assets
                    })
                    .activity_type(discord_presence::models::rich_presence::ActivityType::Listening)
                    .state(state)
                    .details(&track.name);

                // Don't show timestamp if the song is paused, since Discord will continue counting up otherwise
                activity = if paused {
                    activity
                } else {
                    activity.timestamps(|_| {
                        ActivityTimestamps::new()
                            .start(start_time.timestamp() as u64)
                            .end(end_time.timestamp() as u64)
                    })
                };

                if let Err(e) = drpc.set_activity(|_| activity) {
                    match e {
                        discord_presence::error::DiscordError::NotStarted => {
                            log::warn!("Discord RPC not started, starting now");
                            should_reconnect.store(true, Ordering::SeqCst);
                        }
                        _ => {
                            log::error!("Failed to set Discord activity: {}", e);
                        }
                    }
                }
            }
            DiscordCommand::Stopped => {
                if let Err(e) = drpc.clear_activity() {
                    log::error!("Failed to clear Discord activity: {}", e);
                }
            }
        }
    }
    log::info!("Discord command receiver closed, stopping Discord RPC client.");
}

fn reconnect_loop(drpc: &mut discord_presence::Client) {
    log::info!("Reconnecting to Discord RPC...");
    drpc.start();
}
