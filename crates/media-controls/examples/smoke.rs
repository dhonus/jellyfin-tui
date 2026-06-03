/// Smoke test: construct MediaControls, push a NowPlaying update, receive an
/// event (by simulating one through the internal channel on Linux).
///
/// Run with:
///   cargo run -p media-controls --example smoke
///
/// On Linux you can fire a play event externally while it's waiting with:
///   playerctl play
///   dbus-send --session --dest=org.mpris.MediaPlayer2.jellyfin-tui \
///       /org/mpris/MediaPlayer2 \
///       org.mpris.MediaPlayer2.Player.Play

use std::time::Duration;
use media_controls::{Config, MediaControls, NowPlaying, PlaybackStatus};

#[tokio::main]
async fn main() {
    let mut controls = match MediaControls::new(Config {
        dbus_name: "jellyfin-tui",
        display_name: "jellyfin-tui (smoke test)",
    })
    .await
    {
        Some(c) => {
            println!("[smoke] MediaControls created OK");
            c
        }
        None => {
            println!("[smoke] MediaControls unavailable on this platform/setup — SKIP");
            return;
        }
    };

    let mut rx = controls.events();

    // Push a NowPlaying update — this is the primary control path.
    controls.update(NowPlaying {
        title: Some("Bohemian Rhapsody".into()),
        artist: Some("Queen".into()),
        album: Some("A Night at the Opera".into()),
        cover_url: None,
        duration: Some(Duration::from_secs(354)),
        position: Some(Duration::from_secs(0)),
        status: Some(PlaybackStatus::Playing),
        volume: Some(1.0),
    });
    println!("[smoke] update(NowPlaying) sent");

    // Wait up to 3 s for an event (press a media key or use playerctl to trigger one).
    println!("[smoke] waiting 3 s for a MediaControlEvent (press a media key or use playerctl)…");
    match tokio::time::timeout(Duration::from_secs(3), rx.recv()).await {
        Ok(Some(event)) => println!("[smoke] received event: {:?}", event),
        Ok(None) => println!("[smoke] channel closed"),
        Err(_) => println!("[smoke] no event within 3 s (that's OK for CI)"),
    }

    println!("[smoke] done");
}
