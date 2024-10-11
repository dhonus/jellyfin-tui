mod client;
mod tui;
mod keyboard;
mod mpris;

use tokio;

use std::{io::stdout, vec};
use std::env;
// use serde_yaml::Value;
// use std::{collections::HashMap};

use libmpv::{*};

use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute
};
use ratatui::prelude::{CrosstermBackend, Terminal};

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
  ⠀⠀⠀⠀⠈⠣⠀⢸⠀⠀⢠⠇⠀⠀⠀⠀This is free software (GPLv3).
  ⠀⠀⠀⠀⠀⠀⢠⠃⠀⠔⠁⠀⠀
  ",
            version, MPV_CLIENT_API_MAJOR, MPV_CLIENT_API_MINOR, MPV_CLIENT_API_VERSION
        )
    );

    let client = client::Client::new().await;
    if client.access_token.is_empty() {
        println!("Failed to authenticate. Exiting...");
        return;
    }

    println!("[OK] Authenticated!");

    let mut artists = match client.artists(String::from("")).await {
        Ok(artists) => artists,
        Err(e) => {
            println!("[!!] Failed to get artists: {:?}", e);
            return;
        }
    };

    let new_artists = match client.new_artists().await {
        Ok(artists) => artists,
        Err(_e) => {
            vec![]
        }
    };

    for artist in &mut artists {
        if new_artists.contains(&artist.id) {
            artist.jellyfintui_recently_added = true;
        }
    }

    enable_raw_mode().unwrap();
    execute!(stdout(), EnterAlternateScreen).unwrap();

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout())).unwrap();
    terminal.clear().unwrap();

    let mut app = tui::App::default();
    app.init(artists).await;

    terminal.clear().unwrap();

    loop {
        app.run(&mut terminal).await;
        if app.exit {
            disable_raw_mode().unwrap();
            execute!(stdout(), LeaveAlternateScreen).unwrap();
            break;
        }
    }
    println!("Exited!");
}

// fn seekable_ranges(demuxer_cache_state: &MpvNode) -> Option<Vec<(f64, f64)>> {
//     let mut res = Vec::new();
//     let props: HashMap<&str, MpvNode> = demuxer_cache_state.to_map()?.collect();
//     let ranges = props.get("seekable-ranges")?.to_array()?;

//     for node in ranges {
//         let range: HashMap<&str, MpvNode> = node.to_map()?.collect();
//         let start = range.get("start")?.to_f64()?;
//         let end = range.get("end")?.to_f64()?;
//         res.push((start, end));
//     }

//     Some(res)
// }
