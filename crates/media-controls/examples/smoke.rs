// cargo run -p media-controls --example smoke
// Linux: playerctl play   macOS: press a media key
use media_controls::{Config, LoopStatus, MediaControls, NowPlaying, PlaybackStatus};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let mut controls = match MediaControls::new(Config {
        dbus_name: "jellyfin-tui",
        display_name: "jellyfin-tui (smoke test)",
        ..Default::default()
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

    controls.update(NowPlaying {
        title: Some("Bohemian Rhapsody".into()),
        artist: Some("Queen".into()),
        album: Some("A Night at the Opera".into()),
        cover_url: None,
        duration: Some(Duration::from_secs(354)),
        position: Some(Duration::from_secs(0)),
        status: Some(PlaybackStatus::Playing),
        volume: Some(1.0),
        track_number: Some(11),
        year: Some(1975),
        shuffle: Some(false),
        loop_status: Some(LoopStatus::None),
    });
    println!("[smoke] update(NowPlaying) sent");

    // Let macOS dispatch_async drain before waiting for events.
    tokio::time::sleep(Duration::from_millis(200)).await;

    println!("[smoke] waiting 10 s for a MediaControlEvent (press play/pause key)…");
    match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
        Ok(Some(event)) => println!("[smoke] received event: {:?}", event),
        Ok(None) => println!("[smoke] channel closed"),
        Err(_) => println!("[smoke] no event within 10 s (that's OK for CI)"),
    }

    println!("[smoke] done");
}
