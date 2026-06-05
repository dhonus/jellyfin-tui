use media_controls::{Config, MediaControls, NowPlaying, PlaybackStatus};
/// Smoke test: construct MediaControls, push a NowPlaying update, receive an
/// event (by simulating one through the internal channel on Linux).
///
/// Run with:
///   cargo run -p media-controls --example smoke
///
/// On Linux you can fire a play event externally while it's waiting with:
///   playerctl play
///
/// On macOS press a media key (play/pause on keyboard or Touch Bar) or use
/// the lock screen / Control Center transport controls within 10 s.
///
/// The macOS backend registers with MPRemoteCommandCenter on the main thread.
/// After construction the example spins a 10-second tick loop so the run-loop
/// can receive remote-control events, then exits.
use std::time::Duration;

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
    println!("[smoke] metadata should appear in Control Center / lock screen now");

    // On macOS the dispatch_async from update() lands on the main thread, but
    // tokio's #[main] uses a background runtime.  Give the main run-loop a
    // moment to drain by yielding briefly.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Wait up to 10 s for an event (press a media key to trigger one).
    println!("[smoke] waiting 10 s for a MediaControlEvent (press play/pause key)…");
    match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
        Ok(Some(event)) => println!("[smoke] received event: {:?}", event),
        Ok(None) => println!("[smoke] channel closed"),
        Err(_) => println!("[smoke] no event within 10 s (that's OK for CI)"),
    }

    println!("[smoke] done");
}
