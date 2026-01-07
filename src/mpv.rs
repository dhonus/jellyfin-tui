use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;
use dirs::runtime_dir;
use libmpv2::{Format, Mpv};
use souvlaki::MediaControlEvent;
use tokio::sync::oneshot;
use tokio::time::Instant;
use crate::database::database::UpdateCommand;
use crate::helpers;
use crate::tui::{MpvPlaybackState, Repeat, Song};

const POLL_INTERVAL: Duration = Duration::from_millis(200);

pub struct MpvHandle {
    tx: Sender<MpvCommand>,
    pub dead: AtomicBool,
}

#[derive(Debug)]
pub enum MpvError {
    /// mpv thread crashed, channel closed, or reply dropped
    EngineDied,
    /// mpv rejected a command or is internally broken
    CommandFailed,
    /// mpv failed to initialize
    InitFailed,
}

impl std::fmt::Display for MpvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoadFileFlag {
    Replace,
    Append,
    AppendPlay,
    InsertNext,
    InsertNextPlay,
    InsertAt,
    InsertAtPlay,
}
impl Default for LoadFileFlag {
    fn default() -> Self {
        LoadFileFlag::Replace
    }
}

impl LoadFileFlag {
    pub fn as_str(self) -> &'static str {
        match self {
            LoadFileFlag::Replace => "replace",
            LoadFileFlag::Append => "append",
            LoadFileFlag::AppendPlay => "append-play",
            LoadFileFlag::InsertNext => "insert-next",
            LoadFileFlag::InsertNextPlay => "insert-next-play",
            LoadFileFlag::InsertAt => "insert-at",
            LoadFileFlag::InsertAtPlay => "insert-at-play",
        }
    }
}


pub struct MpvState {
    pub mpris_events: Vec<MediaControlEvent>,
    pub mpv: Mpv,
}

impl MpvState {
    pub fn new(config: &serde_yaml::Value, sender: Sender<MpvPlaybackState>) -> (Arc<Mutex<Self>>, MpvHandle) {
        let mpv = Mpv::with_initializer(|mpv| {
            mpv.set_option("msg-level", "ffmpeg/demuxer=no").unwrap();
            Ok(())
        })
            .expect(" [XX] Failed to initiate mpv context");
        mpv.set_property("vo", "null").unwrap();
        mpv.set_property("volume", 100).unwrap();
        mpv.set_property("prefetch-playlist", "yes").unwrap(); // gapless playback

        // no console output (it shifts the tui around)
        let _ = mpv.set_property("quiet", "yes");
        let _ = mpv.set_property("really-quiet", "yes");

        // optional mpv options (hah...)
        if let Some(mpv_config) = config.get("mpv") {
            if let Some(mpv_config) = mpv_config.as_mapping() {
                for (key, value) in mpv_config {
                    if let (Some(key), Some(value)) = (key.as_str(), value.as_str()) {
                        mpv.set_property(key, value).unwrap_or_else(|e| {
                            panic!("This is not a valid mpv property {key}: {:?}", e)
                        });
                        log::info!("Set mpv property: {} = {}", key, value);
                    }
                }
            } else {
                log::error!("mpv config is not a mapping");
            }
        }

        mpv.disable_deprecated_events().unwrap();
        mpv.observe_property("volume", Format::Int64, 0).unwrap();
        mpv.observe_property("demuxer-cache-state", Format::Node, 0)
            .unwrap();

        let (tx, rx) = std::sync::mpsc::channel::<MpvCommand>();

        let mpv_state = Arc::new(Mutex::new(Self {
            mpris_events: vec![],
            mpv,
        }));
        let copy = mpv_state.clone();
        thread::spawn(move || {
            if let Err(e) = t_mpv_runtime(copy, sender, rx) {
                log::error!("Error in mpv playlist thread: {}", e);
            }
        });

        (mpv_state, MpvHandle { tx, dead: AtomicBool::new(false) })
    }
}

/// The thread that keeps in sync with the mpv thread
fn t_mpv_runtime(
    mpv_state: Arc<Mutex<MpvState>>,
    sender: Sender<MpvPlaybackState>,
    command_rx: Receiver<MpvCommand>,
    // state: MpvPlaybackState,
    // repeat: Repeat,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mpv = mpv_state
        .lock()
        .map_err(|e| format!("Failed to lock mpv_state: {:?}", e))?;

    let _ = mpv.mpv.command("playlist_clear", &["force"]);

    // for song in songs {
    //     match helpers::normalize_mpvsafe_url(&song.url) {
    //         Ok(safe_url) => {
    //             let _ = mpv
    //                 .mpv
    //                 .command("loadfile", &[safe_url.as_str(), "append-play"]);
    //         }
    //         Err(e) => log::error!("Failed to normalize URL '{}': {:?}", song.url, e),
    //     }
    // }

    // mpv.mpv.set_property("volume", state.volume)?;
    // mpv.mpv.set_property("playlist-pos", state.current_index)?;
    //
    // match repeat {
    //     Repeat::None => {
    //         let _ = mpv.mpv.set_property("loop-file", "no");
    //         let _ = mpv.mpv.set_property("loop-playlist", "no");
    //     }
    //     Repeat::All => {
    //         let _ = mpv.mpv.set_property("loop-playlist", "inf");
    //     }
    //     Repeat::One => {
    //         let _ = mpv.mpv.set_property("loop-playlist", "no");
    //         let _ = mpv.mpv.set_property("loop-file", "inf");
    //     }
    // }

    drop(mpv);

    let mut last = MpvPlaybackState::default();
    let mut next_poll = Instant::now();

    // This loop polls for commands from the UI, intentionally without immediate latency.
    // the UI conversely polls for MpvPlaybackState
    loop {
        while let Ok(cmd) = command_rx.try_recv() {
            let mpv = mpv_state.lock()
                .map_err(|e| format!("Failed to lock mpv_state: {:?}", e))?;
            handle_command(&mpv.mpv, cmd);
        }

        // timed MPV poll
        if Instant::now() >= next_poll {
            let mpv = mpv_state
                .lock()
                .map_err(|e| format!("Failed to lock mpv_state: {:?}", e))?;

            let position = mpv.mpv.get_property("time-pos").unwrap_or(0.0);
            let duration = mpv.mpv.get_property("duration").unwrap_or(0.0);
            let current_index: i64 =
                mpv.mpv.get_property("playlist-pos").unwrap_or(0);
            let volume = mpv.mpv.get_property("volume").unwrap_or(0);
            let audio_bitrate =
                mpv.mpv.get_property("audio-bitrate").unwrap_or(0);
            let audio_samplerate =
                mpv.mpv.get_property("audio-params/samplerate").unwrap_or(0);
            let hr_channels: String =
                mpv.mpv.get_property("audio-params/hr-channels")
                    .unwrap_or_default();
            let file_format: String =
                mpv.mpv.get_property("file-format")
                    .unwrap_or_default();

            drop(mpv);

            if (position - last.position).abs() >= 0.95
                || (duration - last.duration).abs() >= 0.95
                || current_index != last.current_index
                || volume != last.volume
            {
                last = MpvPlaybackState {
                    position,
                    duration,
                    current_index,
                    last_index: last.last_index,
                    volume,
                    audio_bitrate,
                    audio_samplerate,
                    hr_channels,
                    file_format,
                };

                let _ = sender.send(last.clone());
            }

            next_poll = Instant::now() + POLL_INTERVAL;
        }

        thread::sleep(Duration::from_millis(2));
    }

}

impl std::error::Error for MpvError {}
type Reply = oneshot::Sender<Result<(), MpvError>>;

enum MpvCommand {
    Play {
        reply: Reply,
    },
    Pause {
        reply: Reply,
    },
    Stop {
        keep_playlist: bool,
        reply: Reply,
    },
    Next { reply: Reply },
    Previous { current_time: f64, reply: Reply },
    PlayIndex { index: usize, reply: Reply },
    PlaylistRemove { index: usize, reply: Reply },
    LoadFiles {
        urls: Vec<String>,
        flag: LoadFileFlag,
        index: Option<i64>,
        reply: Reply,
    }
}

fn handle_command(mpv: &Mpv, cmd: MpvCommand) {
    match cmd {
        MpvCommand::Play { reply } => {
            let res = mpv.set_property("pause", false);
            if let Err(e) = &res {
                log::error!("mpv play failed: {:?}", e);
            }
            let _ = reply.send(
                res.map_err(|_| MpvError::CommandFailed)
            );
        }
        MpvCommand::Pause { reply } => {
            let res = mpv.set_property("pause", true);
            if let Err(e) = &res {
                log::error!("mpv pause failed: {:?}", e);
            }
            let _ = reply.send(
                res.map_err(|_| MpvError::CommandFailed)
            );
        }
        MpvCommand::Stop { reply, keep_playlist } => {
            let res = if keep_playlist {
                mpv.command("stop", &["keep-playlist"])
            } else {
                mpv.command("stop", &[])
            };
            if let Err(e) = &res {
                log::error!("mpv stop failed: {:?}", e);
            }
            let _ = reply.send(
                res.map_err(|_| MpvError::CommandFailed)
            );
        }
        MpvCommand::Next { reply } => {
            let res = mpv.command("playlist-next", &[]);
            let _ = reply.send(
                res.map_err(|_| MpvError::CommandFailed)
            );
        }
        MpvCommand::Previous { current_time , reply } => {
            let res = if current_time > 5.0 {
                mpv.command("seek", &["0.0", "absolute"])
            } else {
                mpv.command("playlist-prev", &["force"])
            };
            let _ = reply.send(
                res.map_err(|_| MpvError::CommandFailed)
            );
        }
        MpvCommand::PlayIndex { index, reply } => {
            let res = mpv.command("playlist-play-index", &[&index.to_string()]);
            if let Err(e) = &res {
                log::error!("mpv playlist-play-index failed: {:?}", e);
            }
            let _ = reply.send(
                res.map_err(|_| MpvError::CommandFailed)
            );
        }
        MpvCommand::PlaylistRemove { index, reply } => {
            let res = mpv.command("playlist_remove", &[&index.to_string()]);
            if let Err(e) = &res {
                log::error!("mpv playlist-remove failed: {:?}", e);
            }
            let _ = reply.send(
                res.map_err(|_| MpvError::CommandFailed)
            );
        }
        MpvCommand::LoadFiles {
            urls,
            flag,
            index,
            reply,
        } => {
            let mut ok = true;
            let flag = flag.as_str();

            for url in urls {
                let res = match index {
                    Some(i) => {
                        mpv.command(
                            "loadfile",
                            &[&url, flag, &i.to_string()],
                        )
                    }
                    None => {
                        mpv.command(
                            "loadfile",
                            &[&url, flag],
                        )
                    }
                };

                if res.is_err() {
                    ok = false;
                    log::error!("mpv loadfile failed for '{}'", url);
                }
            }

            let _ = reply.send(if ok {
                Ok(())
            } else {
                Err(MpvError::CommandFailed)
            });
        }
    }
}

impl MpvHandle {
    pub async fn play(&self)  {
        self.call(|reply| MpvCommand::Play { reply }).await
    }

    pub async fn pause(&self) {
        self.call(|reply| MpvCommand::Pause { reply }).await
    }

    pub async fn stop(&self, keep_playlist: bool) {
        self.call(|reply| MpvCommand::Stop{ keep_playlist, reply }).await
    }

    pub async fn next(&self) {
        self.call(|reply| MpvCommand::Next { reply }).await
    }

    /// If over 5 seconds in, go to the start of current track. If not, go back to previous.
    /// current_time -> current_playback_state.position
    pub async fn previous(&self, current_time: f64) {
        self.call(|reply| MpvCommand::Previous { current_time, reply }).await
    }

    pub async fn play_index(&self, index: usize) {
        self.call(|reply| MpvCommand::PlayIndex { index, reply }).await
    }
    
    pub async fn playlist_remove(&self, index: usize) {
        self.call(|reply| MpvCommand::PlaylistRemove { index, reply }).await
    }

    pub async fn load_files(
        &self,
        urls: Vec<String>,
        flag: LoadFileFlag,
        index: Option<i64>,
    ) {
        self.call(|reply| MpvCommand::LoadFiles {
            urls, flag, index, reply,
        })
            .await
    }

    async fn call(
        &self,
        make_cmd: impl FnOnce(oneshot::Sender<Result<(), MpvError>>) -> MpvCommand,
    ) {
        if self.dead.load(Ordering::Relaxed) {
            return;
        }

        let (tx, rx) = oneshot::channel();

        // mpv thread already dead
        if self.tx.send(make_cmd(tx)).is_err() {
            self.dead.store(true, Ordering::Relaxed);
            return;
        }

        match rx.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                // this is technically half recoverable, if it acts up too often we can be less aggressive
                log::error!("mpv command failed to run correctly: {}", e);
                self.dead.store(true, Ordering::Relaxed);
            }
            Err(e) => {
                // this should hopefully not actually happen very often (mpv is pretty stable)
                // instead of lying about health, we just politely ask the user to restart the app
                log::error!("mpv thread died mid-command: {}", e);
                self.dead.store(true, Ordering::Relaxed);
            }
        }
    }


    // async fn call(
    //     &self,
    //     make_cmd: impl FnOnce(oneshot::Sender<Result<(), MpvError>>) -> Command,
    // ) -> Result<(), MpvError> {
    //     let (tx, rx) = oneshot::channel();
    //
    //     self.tx
    //         .send(make_cmd(tx))
    //         .map_err(|_| MpvError::EngineDied)?;
    //
    //     rx.await.map_err(|_| MpvError::EngineDied)?
    // }
}


