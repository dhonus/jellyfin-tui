mod client;
mod tui;

use tokio;

use std::io::stdout;
use std::{collections::HashMap, env};

use libmpv::{events::*, *}; // we use mpv as

use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::{CrosstermBackend, Terminal};

const VIDEO_URL: &str = "https://jelly.danielhonus.com/Audio/f1c67214e4379e07aea5e73078e9a770/universal?UserId=f9784d6dce9645d48e2b00a160a24015&DeviceId=TW96aWxsYS81LjAgKFgxMTsgTGludXggeDg2XzY0KSBBcHBsZVdlYktpdC81MzcuMzYgKEtIVE1MLCBsaWtlIEdlY2tvKSBDaHJvbWUvMTIzLjAuMC4wIFNhZmFyaS81MzcuMzZ8MTcxNDI0ODI4MTUzOQ11&MaxStreamingBitrate=12547063&Container=opus%2Cwebm%7Copus%2Cmp3%2Caac%2Cm4a%7Caac%2Cm4b%7Caac%2Cflac%2Cwebma%2Cwebm%7Cwebma%2Cwav%2Cogg&TranscodingContainer=mp4&TranscodingProtocol=hls&AudioCodec=aac&api_key=bcbf698769b24b4d9879ab315f9659a4&PlaySessionId=1716728074797&StartTimeTicks=0&EnableRedirection=true&EnableRemoteMedia=false";

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

    // 
    let client = client::Client::new("https://jelly.danielhonus.com").await;
    if client.access_token.is_empty() {
        println!("Failed to authenticate. Exiting...");
        return;
    }

    println!("[OK] Authenticated!");
    //client.songs().await;
    let artists = match client.artists().await {
        Ok(artists) => artists,
        Err(e) => {
            println!("Failed to get artists: {:?}", e);
            return;
        }
    };

    stdout().execute(EnterAlternateScreen).unwrap();
    enable_raw_mode().unwrap();
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout())).unwrap();
    terminal.clear().unwrap();

    let mut app = tui::App::default();
    app.init(artists).await;

    loop {
        app.run(&mut terminal).await;
        if app.exit {
            println!("Exiting...");
            disable_raw_mode().unwrap();
            break;
        }
    }
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
