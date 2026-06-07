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

    controls.update(
        NowPlaying::new()
            .title("Bohemian Rhapsody")
            .artist("Queen")
            .album("A Night at the Opera")
            .track_number(11)
            .year(1975)
            .duration(Duration::from_secs(354))
            .position(Duration::ZERO)
            .status(PlaybackStatus::Playing)
            .shuffle(false)
            .loop_status(LoopStatus::None)
            .volume(1.0),
    );

    while let Some(event) = events.recv().await {
        match event {
            MediaControlEvent::Play => { /* resume */ }
            MediaControlEvent::Pause => { /* pause  */ }
            MediaControlEvent::Next => { /* skip   */ }
            MediaControlEvent::Previous => { /* back   */ }
            MediaControlEvent::SetShuffle(on) => { /* toggle shuffle */ }
            MediaControlEvent::SetLoopStatus(s) => { /* change repeat mode */ }
            MediaControlEvent::Quit => { /* quit   */ }
            _ => {}
        }
    }
}
```

Only set what changed — unset fields keep their previous value:

```rust
controls.update(NowPlaying::new().position(Duration::from_secs(42)).status(PlaybackStatus::Paused));
```

## Capabilities

All capabilities default to `true` on Linux and `false` where the platform has no equivalent (e.g. `can_raise` and
`can_quit` are `false` on macOS). Override as needed:

```rust
Config {
dbus_name: "my-player",
display_name: "My Player",
capabilities: Capabilities {
can_raise: false,
..Default::default ()
},
}
```

## macOS: run-loop ticking

On macOS, `MPRemoteCommandCenter` delivers remote-control events through the Cocoa run-loop. Call `controls.tick()`
regularly from your **main thread** (e.g. each UI frame) so events are dispatched. It is a no-op on all other platforms.

```rust
loop {
controls.tick(); // drive the macOS run-loop
// your frame work
}
```

## Platforms

| Platform | Backend                                            |
|----------|----------------------------------------------------|
| Linux    | MPRIS via D-Bus                                    |
| macOS    | `MPRemoteCommandCenter` + `MPNowPlayingInfoCenter` |
| Other    | no-op (`MediaControls::new` returns `None`)        |

## Smoke test

```bash
cargo run -p media-controls --example smoke
# Linux: playerctl play    macOS: press a media key
```

You can also watch the D-Bus interface with `dbus-monitor`:

```bash
dbus-monitor --session "type='signal',interface='org.freedesktop.DBus.Properties',path='/org/mpris/MediaPlayer2'"
```