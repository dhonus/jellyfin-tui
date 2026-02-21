#![cfg_attr(target_os = "macos", allow(unexpected_cfgs))]
mod client;
mod config;
mod database;
mod discord;
mod help;
mod helpers;
mod keyboard;
mod library;
mod macos;
mod mpris;
mod mpv;
mod player;
mod playlists;
mod popup;
mod queue;
mod search;
mod sort;
mod themes;
mod tui;

use dirs::data_dir;
use flexi_logger::{FileSpec, Logger};
use fs2::FileExt;
use std::backtrace::Backtrace;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::stdout;
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
// keyboard enhancement flags are used to allow for certain normally blocked key combinations... e.g. ctrl+enter...
use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use libmpv2::{MPV_CLIENT_API_MAJOR, MPV_CLIENT_API_MINOR, MPV_CLIENT_API_VERSION};
use ratatui::prelude::{CrosstermBackend, Terminal};

#[tokio::main]
async fn main() {
    let version = env!("CARGO_PKG_VERSION");

    let args = env::args().collect::<Vec<String>>();
    if args.len() > 1 {
        if args[1] == "--version" || args[1] == "-v" {
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

    let _lockfile = check_single_instance();

    let offline = args.contains(&String::from("--offline"));
    let force_server_select = args.contains(&String::from("--select-server"));

    if !args.contains(&String::from("--no-splash")) {
        println!(
            "
  ⠀⠀⠀⠀⡴⠂⢩⡉⠉⠉⡖⢄⠀
  ⠀⠀⠀⢸⠪⠄⠀⠀⠀⠀⠐⠂⢧⠀⠀⠀\x1b[94mjellyfin-tui\x1b[0m
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
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
        let _ = execute!(stdout(), LeaveAlternateScreen);
        let bt = Backtrace::force_capture();
        log::error!("Panic occurred: {}", info);
        log::error!("Backtrace:\n{}", bt);
        eprintln!("\n ! (×_×) panik: {}", info);
        eprintln!(" ! If you think this is a bug, please report it at https://github.com/dhonus/jellyfin-tui/issues");
    }));

    match config::prepare_directories() {
        Ok(_) => {}
        Err(e) => {
            println!(" ! Creating directories failed. This is a system error, please report your environment and the following error {}:", e);
            std::process::exit(1);
        }
    }

    let data_dir = dirs::data_dir().expect("! Could not find data directory").join("jellyfin-tui");

    let _logger = Logger::try_with_env_or_str("info,zbus=error")
        .expect(" ! Failed to initialize logger")
        .log_to_file(
            FileSpec::default()
                .directory(data_dir.join("log"))
                .basename("jellyfin-tui")
                .suffix("log"),
        )
        .rotate(
            flexi_logger::Criterion::Age(flexi_logger::Age::Day),
            flexi_logger::Naming::Timestamps,
            flexi_logger::Cleanup::KeepLogFiles(3),
        )
        .format(flexi_logger::detailed_format)
        .start();

    log::info!("jellyfin-tui {} started", version);

    config::initialize_config();

    let mut app = tui::App::new(offline, force_server_select).await;
    if let Err(e) = app.load_state().await {
        println!(" ! Error loading state: {}", e);
    }

    enable_raw_mode().unwrap();
    execute!(stdout(), EnterAlternateScreen).unwrap();

    let _ = execute!(
        stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    app.combiner.enable_combining().ok();

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout())).unwrap();

    terminal.clear().unwrap();

    loop {
        // Pump the macOS runloop to allow Now Playing events to be processed
        #[cfg(target_os = "macos")]
        macos::pump_runloop();

        // main event loop
        // run() polls events and updates the app state
        if let Err(e) = app.run().await {
            log::error!("Runtime error: {}", e);
        }
        if app.exit || panicked.load(Ordering::SeqCst) {
            let _ = disable_raw_mode();
            let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
            let _ = execute!(stdout(), LeaveAlternateScreen);
            break;
        }
        // draw() renders the app state to the terminal
        if let Err(e) = app.draw(&mut terminal).await {
            log::error!("Draw error: {}", e);
        }
    }
    if panicked.load(Ordering::SeqCst) {
        return;
    }
    println!(" - Exiting...");
}

fn check_single_instance() -> File {
    let runtime_dir = match data_dir() {
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
    println!("  --version\t\tPrint version information");
    println!("  --help\t\tPrint this help message");
    println!("  --no-splash\t\tDo not show jellyfish splash screen");
    println!("  --select-server\tForce server selection on startup");
    println!("  --offline\t\tStart in offline mode");

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
