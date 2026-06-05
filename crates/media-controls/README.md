# media-controls

Tiny cross-platform media controls for Rust.

This crate lets your app:

* Show "Now Playing" info to the operating system
* Receive media key events (play, pause, next, previous, etc.)
* Integrate with lock screen / media controls on supported platforms

## Installation

```toml
[dependencies]
media-controls = { path = "crates/media-controls" }
tokio = { version = "1", features = ["full"] }
```

## Example

```rust
use std::time::Duration;

use media_controls::{
    Config,
    MediaControls,
    MediaControlEvent,
    NowPlaying,
    PlaybackStatus,
};

#[tokio::main]
async fn main() {
    let mut controls = MediaControls::new(Config {
        dbus_name: "my-player",
        display_name: "My Player",
    })
    .await
    .expect("media controls unavailable");

    let mut events = controls.events();

    controls.update(NowPlaying {
        title: Some("Bohemian Rhapsody".into()),
        artist: Some("Queen".into()),
        album: Some("A Night at the Opera".into()),
        duration: Some(Duration::from_secs(354)),
        position: Some(Duration::from_secs(0)),
        status: Some(PlaybackStatus::Playing),
        volume: Some(1.0),
        cover_url: None,
    });

    while let Some(event) = events.recv().await {
        match event {
            MediaControlEvent::Play => println!("play"),
            MediaControlEvent::Pause => println!("pause"),
            MediaControlEvent::Next => println!("next"),
            MediaControlEvent::Previous => println!("previous"),
            _ => {}
        }
    }
}
```

## Updating metadata

Call `update()` whenever the currently playing track changes or playback state changes.

```rust
controls.update(NowPlaying {
    title: Some("Track Name".into()),
    artist: Some("Artist".into()),
    status: Some(PlaybackStatus::Paused),
    ..Default::default()
});
```

Fields set to `None` are left unchanged, so you only need to send the values that changed.

## Events

The crate can receive media control events from the operating system:

* Play
* Pause
* Toggle
* Stop
* Next
* Previous
* Seek
* SetPosition
* SetVolume
* Raise
* Quit

Use `events()` to get a Tokio channel receiver and handle them in your player.

## Platforms

* Linux (MPRIS / D-Bus)
* macOS (Media Center / lock screen controls)

Unsupported platforms return `None` from `MediaControls::new()`.

## Smoke test

```bash
cargo run -p media-controls --example smoke
```

On Linux, try:

```bash
playerctl play
```

while the example is running to trigger an event.
