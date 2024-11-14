mod config;
mod client;
mod tui;
mod keyboard;
mod mpris;
mod library;
mod search;
use tokio;

use std::{io::stdout, vec};
use std::env;
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
// use serde_yaml::Value;
// use std::{collections::HashMap};

use libmpv2::{*};

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

    let client = client::Client::new(false).await;
    if client.access_token.is_empty() {
        println!("[XX] Failed to authenticate. Exiting...");
        return;
    }

    println!("[OK] Authenticated as {}.", client.user_name);

    let mut artists = match client.artists(String::from("")).await {
        Ok(artists) => artists,
        Err(e) => {
            println!("[XX] Failed to get artists: {:?}", e);
            return;
        }
    };

    let new_artists = client.new_artists().await.unwrap_or(vec![]);

    for artist in &mut artists {
        if new_artists.contains(&artist.id) {
            artist.jellyfintui_recently_added = true;
        }
    }

    let panicked = std::sync::Arc::new(AtomicBool::new(false));
    let panicked_clone = panicked.clone();

    panic::set_hook(Box::new(move |info| {
        panicked_clone.store(true, Ordering::SeqCst);
        eprintln!("\n[XX] (×_×) panik: {}", info);
        eprintln!("[!!] If you think this is a bug, please report it at https://github.com/dhonus/jellyfin-tui/issues");
    }));
    
    let mut app = tui::App::default();
    app.init(artists).await;

    enable_raw_mode().unwrap();
    execute!(stdout(), EnterAlternateScreen).unwrap();

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout())).unwrap();

    terminal.clear().unwrap();

    loop {
        app.run().await.ok();
        if app.exit || panicked.load(Ordering::SeqCst) {
            disable_raw_mode().unwrap();
            execute!(stdout(), LeaveAlternateScreen).unwrap();
            break;
        }
        app.draw(&mut terminal).await.ok();
    }
    if panicked.load(Ordering::SeqCst) {
        return;
    }
    println!("[OK] Exited.");
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
