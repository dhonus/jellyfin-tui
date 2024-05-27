mod client;
mod tui;

use tokio;

use std::io::stdout;
use std::thread;
use std::time::Duration;
use std::{collections::HashMap, env};

use libmpv::{events::*, *}; // we use mpv as

use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::{CrosstermBackend, Terminal};

const VIDEO_URL: &str = "";

#[tokio::main]
async fn main() {
    let version = env!("CARGO_PKG_VERSION");

    println!(
        "{}",
        format!(
            "
    ⠀⠀⠀⠀⡴⠂⢩⡉⠉⠉⡖⢄⠀
    ⠀⠀⠀⢸⠪⠄⠀⠀⠀⠀⠐⠂⢧⠀⠀⠀\x1b[94mjellyfin-tui\x1b[0m by dhonus
    ⠀⠀⠀⠙⢳⣢⢬⣁⠀⠛⠀⠂⡞
    ⠀⣀⡤⢔⠟⣌⠷⠡⢽⢭⠝⠭⠁⠀⠀⠀⠀-⠀version⠀{}
    ⡸⣡⠴⡫⢺⠏⡇⢰⠸⠘⡄⠀⠀⠀⠀⠀⠀-⠀libmpv {}.{} ({})
    ⡽⠁⢸⠀⢸⡀⢣⠀⢣⠱⡈⢦⠀
    ⡇⠀⠘⣆⠀⢣⡀⣇⠈⡇⢳⠀⢣
    ⠰⠀⠀⠘⢆⠀⠑⢸⢀⠃⠈⡇⢸
    ⠀⠀⠀⠀⠈⠣⠀⢸⠀⠀⢠⠇⠀
    ⠀⠀⠀⠀⠀⠀⢠⠃⠀⠔⠁⠀⠀⠀⠀⠀This program is free software (GPLv3).\n\n
    ",
            version, MPV_CLIENT_API_MAJOR, MPV_CLIENT_API_MINOR, MPV_CLIENT_API_VERSION
        )
    );

    let client = client::Client::new("https://jelly.danielhonus.com").await;
    if client.access_token.is_empty() {
        println!("Failed to authenticate. Exiting...");
        return;
    }
    //client.songs().await;
    let artists = match client.artists().await {
        Ok(artists) => artists,
        Err(e) => {
            println!("Failed to get artists: {:?}", e);
            return;
        }
    };

    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| String::from(VIDEO_URL));

    // Create an `Mpv` and set some properties.
    let mpv = Mpv::new().unwrap();
    mpv.set_property("volume", 50).unwrap();
    mpv.set_property("vo", "null").unwrap();

    let mut ev_ctx = mpv.create_event_context();
    ev_ctx.disable_deprecated_events().unwrap();
    ev_ctx.observe_property("volume", Format::Int64, 0).unwrap();
    ev_ctx
        .observe_property("demuxer-cache-state", Format::Node, 0)
        .unwrap();

    stdout().execute(EnterAlternateScreen).unwrap();
    enable_raw_mode().unwrap();
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout())).unwrap();
    terminal.clear().unwrap();

    let mut app = tui::App::default();
    app.init(artists).await;

    // TODO: use asynchronous green threads from tokio to use Client
    crossbeam::scope(|scope| {
        scope.spawn(|_| {
            mpv.playlist_load_files(&[(&path, FileState::AppendPlay, None)])
                .unwrap();

            thread::sleep(Duration::from_secs(3));

            mpv.set_property("volume", 75).unwrap();
            // let _ = mpv.seek_forward(10.0);

            thread::sleep(Duration::from_secs(40));

            // Trigger `Event::EndFile`.
            mpv.playlist_next_force().unwrap();
        });
        scope.spawn(move |_| loop {
            let ev = ev_ctx.wait_event(16.).unwrap_or(Err(Error::Null));

            // keyboard events, our own events, etc.
            // println!("Event: {:?}", ev);

            match ev {
                Ok(Event::EndFile(r)) => {
                    // println!("Exiting! Reason: {:?}", r);
                    break;
                }

                Ok(Event::PropertyChange {
                    name: "demuxer-cache-state",
                    change: PropertyData::Node(mpv_node),
                    ..
                }) => {
                    let ranges = seekable_ranges(mpv_node).unwrap();
                    // println!("Seekable ranges updated: {:?}", ranges);
                }

                // Ok(e) => println!("Event triggered: {:?}", e),
                // Err(e) => println!("Event errored: {:?}", e),
                _ => {}
            }
        });
        scope.spawn(|_| {
            loop {
                let percentage: f64 = mpv.get_property("percent-pos").unwrap_or(0.0);
                app.percentage = percentage;
                app.run(&mut terminal, &mpv);
                // println!("Percentage: {:?}", percentage);
                if app.exit {
                    println!("Exiting...");
                    disable_raw_mode().unwrap();
                    break;
                }
            }
        });
    })
    .unwrap();
    println!("Exited!");
}

fn seekable_ranges(demuxer_cache_state: &MpvNode) -> Option<Vec<(f64, f64)>> {
    let mut res = Vec::new();
    let props: HashMap<&str, MpvNode> = demuxer_cache_state.to_map()?.collect();
    let ranges = props.get("seekable-ranges")?.to_array()?;

    for node in ranges {
        let range: HashMap<&str, MpvNode> = node.to_map()?.collect();
        let start = range.get("start")?.to_f64()?;
        let end = range.get("end")?.to_f64()?;
        res.push((start, end));
    }

    Some(res)
}
