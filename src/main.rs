mod client;
mod config;
mod database;
mod help;
mod helpers;
mod keyboard;
mod library;
mod mpris;
mod playlists;
mod popup;
mod queue;
mod search;
mod themes;
mod tui;

use std::env;
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::io::stdout;
use std::fs::{File, OpenOptions};
use fs2::FileExt;
use dirs::cache_dir;
use libmpv2::*;

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
// keyboard enhancement flags are used to allow for certain normally blocked key combinations... e.g. ctrl+enter...
use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::prelude::{CrosstermBackend, Terminal};

#[tokio::main]
async fn main() {

    let _lockfile = check_single_instance();

    let version = env!("CARGO_PKG_VERSION");

    let args = env::args().collect::<Vec<String>>();
    if args.len() > 1 {
        if args[1] == "--version" {
            println!(
                "jellyfin-tui {version} (libmpv {major}.{minor} {ver})",
                version = version,
                major = MPV_CLIENT_API_MAJOR,
                minor = MPV_CLIENT_API_MINOR,
                ver = MPV_CLIENT_API_VERSION
            );
            return;
        }
        if args[1] == "--help" {
            print_help();
            return;
        }
    }

    let offline = args.contains(&String::from("--offline"));
    let force_server_select = args.contains(&String::from("--select-server"));

    if !args.contains(&String::from("--no-splash")) {
        println!(
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
        );
    }

    let panicked = std::sync::Arc::new(AtomicBool::new(false));
    let panicked_clone = panicked.clone();

    panic::set_hook(Box::new(move |info| {
        panicked_clone.store(true, Ordering::SeqCst);
        disable_raw_mode().ok();
        execute!(stdout(), PopKeyboardEnhancementFlags).ok();
        execute!(stdout(), LeaveAlternateScreen).ok();
        eprintln!("\n ! (×_×) panik: {}", info);
        eprintln!(" ! If you think this is a bug, please report it at https://github.com/dhonus/jellyfin-tui/issues");
    }));

    config::initialize_config();

    let mut app = tui::App::new(offline, force_server_select).await;
    if let Err(e) = app.load_state().await {
        println!(" ! Error loading state: {}", e);
    }

    enable_raw_mode().unwrap();
    execute!(stdout(), EnterAlternateScreen).unwrap();

    execute!(
        stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )
    .ok();

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout())).unwrap();

    terminal.clear().unwrap();

    loop {
        app.run().await.ok();
        if app.exit || panicked.load(Ordering::SeqCst) {
            disable_raw_mode().unwrap();
            execute!(stdout(), PopKeyboardEnhancementFlags).ok();
            execute!(stdout(), LeaveAlternateScreen).ok();
            break;
        }
        app.draw(&mut terminal).await.ok();
    }
    if panicked.load(Ordering::SeqCst) {
        return;
    }
    println!(" - Exited.");
}

fn check_single_instance() -> File {
    let runtime_dir = match cache_dir() {
        Some(dir) => dir.join("jellyfin-tui.lock"),
        None => {
            println!("Could not find runtime directory");
            std::process::exit(1);
        }
    };

    let file = match OpenOptions::new().read(true).write(true).create(true).open(&runtime_dir) {
        Ok(f) => f,
        Err(e) => {
            println!("Failed to open lock file: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = file.try_lock_exclusive() {
        if e.kind() == std::io::ErrorKind::WouldBlock {
            println!("Another instance of jellyfin-tui is already running.");
            std::process::exit(0);
        }
        println!("Failed to lock the lockfile: {} ", e);
        println!("This should not happen, please report this issue.");
        std::process::exit(1);
    }

    file
}

fn print_help() {
    println!("jellyfin-tui {}", env!("CARGO_PKG_VERSION"));
    println!("Usage: jellyfin-tui [OPTIONS]");
    println!("\nArguments:");
    println!("  --version\tPrint version information");
    println!("  --help\tPrint this help message");
    println!("  --no-splash\tDo not show jellyfish splash screen");
    println!("  --offline\tStart in offline mode");

    println!("\nControls:");
    println!("  For a list of controls, press '?' in the application.");
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
