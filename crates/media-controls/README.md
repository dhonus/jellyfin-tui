# media-controls

Cross-platform OS media controls for Rust — MPRIS on Linux, `MPRemoteCommandCenter` on macOS.

## Usage

```rust
use media_controls::{Capabilities, Config, LoopStatus, MediaControlEvent, MediaControls, NowPlaying, PlaybackStatus};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let mut controls = MediaControls::new(Config {
        dbus_name: "my-player",
        display_name: "My Player",
        ..Default::default()
    })
    .await
    .expect("media controls unavailable");

    let mut events = controls.events();

    controls.update(NowPlaying {
        title: Some("Bohemian Rhapsody".into()),
        artist: Some("Queen".into()),
        album: Some("A Night at the Opera".into()),
        track_number: Some(11),
        year: Some(1975),
        duration: Some(Duration::from_secs(354)),
        position: Some(Duration::ZERO),
        status: Some(PlaybackStatus::Playing),
        shuffle: Some(false),
        loop_status: Some(LoopStatus::None),
        volume: Some(1.0),
        cover_url: None,
    });

    while let Some(event) = events.recv().await {
        match event {
            MediaControlEvent::Play              => { /* resume */ }
            MediaControlEvent::Pause             => { /* pause  */ }
            MediaControlEvent::Next              => { /* skip   */ }
            MediaControlEvent::Previous          => { /* back   */ }
            MediaControlEvent::SetShuffle(on)    => { /* toggle shuffle */ }
            MediaControlEvent::SetLoopStatus(s)  => { /* change repeat mode */ }
            MediaControlEvent::Quit              => { /* quit   */ }
            _ => {}
        }
    }
}
```

All `NowPlaying` fields are `Option` — `None` keeps the previous value, so only send what changed:

```rust
controls.update(NowPlaying {
    position: Some(Duration::from_secs(42)),
    status: Some(PlaybackStatus::Paused),
    ..Default::default()
});
```

## Capabilities

All capabilities default to `true` on Linux and `false` where the platform has no equivalent (e.g. `can_raise` and `can_quit` are `false` on macOS). Override as needed:

```rust
Config {
    dbus_name: "my-player",
    display_name: "My Player",
    capabilities: Capabilities {
        can_raise: false,
        ..Default::default()
    },
}
```

## Platforms

| Platform | Backend |
|---|---|
| Linux | MPRIS via D-Bus |
| macOS | `MPRemoteCommandCenter` + `MPNowPlayingInfoCenter` |
| Other | no-op (`MediaControls::new` returns `None`) |

## Smoke test

```bash
cargo run -p media-controls --example smoke
# Linux: playerctl play    macOS: press a media key
```
