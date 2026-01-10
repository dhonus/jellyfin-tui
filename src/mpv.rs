use crate::tui::{MpvPlaybackState, Repeat};
use libmpv2::{Format, Mpv};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::Instant;

pub struct MpvHandle {
    tx: Sender<MpvCommand>,
    pub dead: AtomicBool,
}

/// The thread that keeps in sync with the mpv thread
fn t_mpv_runtime(
    mpv: Mpv,
    sender: Sender<MpvPlaybackState>,
    command_rx: Receiver<MpvCommand>,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = mpv.command("playlist_clear", &["force"]);

    // this is for resume on launch // filename, target
    let mut pending_resume = None;

    const POLL_INTERVAL: Duration = Duration::from_millis(200);
    let mut last = MpvPlaybackState::default();
    let mut next_poll = Instant::now();

    // This loop polls for commands from the UI, intentionally without immediate latency.
    // the UI conversely polls for MpvPlaybackState
    loop {
        while let Ok(cmd) = command_rx.try_recv() {
            handle_command(&mpv, cmd, &mut pending_resume);
        }

        if let Some(resume) = &mut pending_resume {
            match resume.handle_tick(&mpv) {
                ResumeResult::Pending => {}
                ResumeResult::Abort => {
                    if resume.user_requested_play {
                        let _ = mpv.set_property("pause", false);
                    }
                    pending_resume = None;
                }
                ResumeResult::Done => {
                    pending_resume = None;
                }
            }
        }

        // timed MPV poll
        if Instant::now() >= next_poll {
            let position = mpv.get_property("time-pos").unwrap_or(0.0);
            let duration = mpv.get_property("duration").unwrap_or(0.0);
            let current_index = match mpv.get_property::<i64>("playlist-pos") {
                Ok(i) if i >= 0 => i as usize,
                _ => 0, // or keep previous value
            };
            let volume = mpv.get_property("volume").unwrap_or(0);
            let audio_bitrate = mpv.get_property("audio-bitrate").unwrap_or(0);
            let audio_samplerate = mpv.get_property("audio-params/samplerate").unwrap_or(0);
            let hr_channels: String = mpv
                .get_property("audio-params/hr-channels")
                .unwrap_or_default();
            let file_format: String = mpv.get_property("file-format").unwrap_or_default();

            let paused_for_cache = mpv.get_property("paused-for-cache").unwrap_or(false);
            let seeking = mpv.get_property("seeking").unwrap_or(false);
            let seek_active = pending_resume.is_some();
            let buffering = paused_for_cache || seeking || seek_active;

            if (position - last.position).abs() >= 0.95
                || (duration - last.duration).abs() >= 0.95
                || current_index != last.current_index
                || volume != last.volume
                || seek_active != last.seek_active
                || buffering != last.buffering
            {
                last = MpvPlaybackState {
                    position,
                    duration,
                    current_index,
                    volume,
                    audio_bitrate,
                    audio_samplerate,
                    hr_channels,
                    file_format,
                    buffering,
                    seek_active,
                };

                let _ = sender.send(last.clone());
            }

            next_poll = Instant::now() + POLL_INTERVAL;
        }

        thread::sleep(Duration::from_millis(2));
    }
}

type Reply = oneshot::Sender<bool>; // true = success

enum MpvCommand {
    Play {
        reply: Reply,
    },
    Pause {
        reply: Reply,
    },
    Stop {
        reply: Reply,
    },
    Next {
        reply: Reply,
    },
    Previous {
        current_time: f64,
        reply: Reply,
    },
    Seek {
        target: f64,
        flag: SeekFlag,
        reply: Reply,
    },
    HardSeek {
        target: f64,
        url: String,
        reply: Reply,
    },
    PlayIndex {
        index: usize,
        reply: Reply,
    },
    PlaylistRemove {
        index: usize,
        reply: Reply,
    },
    PlaylistMove {
        from: usize,
        to: usize,
        reply: Reply,
    },
    PlaylistMoveNoReply {
        from: usize,
        to: usize,
    },
    SetVolume {
        volume: i64,
        reply: Reply,
    },
    SetRepeat {
        repeat: Repeat,
        reply: Reply,
    },
    LoadFiles {
        urls: Vec<String>,
        flag: LoadFileFlag,
        index: Option<i64>,
        reply: Reply,
    },
    Await {
        reply: Reply,
    },
}

fn handle_command(mpv: &Mpv, cmd: MpvCommand, pending_resume: &mut Option<PendingResume>) {
    match cmd {
        MpvCommand::Play { reply } => {
            if let Some(resume) = pending_resume.as_mut() {
                resume.user_requested_play = true;
                let _ = reply.send(true);
                return;
            }

            let res = mpv.set_property("pause", false);
            let _ = reply.send(res.is_ok());
        }
        MpvCommand::Pause { reply } => {
            if let Some(resume) = pending_resume.as_mut() {
                resume.user_requested_play = false;
                let _ = reply.send(true);
                return;
            }
            let res = mpv.set_property("pause", true);
            if let Err(e) = &res {
                log::error!("mpv pause failed: {:?}", e);
            }
            let _ = reply.send(res.is_ok());
        }
        MpvCommand::Stop { reply } => {
            let res = mpv.command("stop", &[]);
            if let Err(e) = &res {
                log::error!("mpv stop failed: {:?}", e);
            }
            let _ = reply.send(res.is_ok());
        }
        MpvCommand::Next { reply } => {
            let res = mpv.command("playlist_next", &["force"]);
            let _ = reply.send(res.is_ok());
        }
        MpvCommand::Previous {
            current_time,
            reply,
        } => {
            let res = if current_time > 5.0 {
                mpv.command("seek", &["0.0", "absolute"])
            } else {
                mpv.command("playlist-prev", &["force"])
            };
            let _ = reply.send(res.is_ok());
        }
        MpvCommand::Seek {
            target,
            flag,
            reply,
        } => {
            let res = match flag {
                SeekFlag::Relative => mpv.command("seek", &[&target.to_string()]),
                SeekFlag::Absolute => mpv.command("seek", &[&target.to_string(), "absolute"]),
            };
            let _ = reply.send(res.is_ok());
        }

        MpvCommand::HardSeek { target, url, reply } => {
            *pending_resume = Some(PendingResume {
                expected_url: url,
                target,
                started_at: Instant::now(),
                last_attempt: Instant::now(),
                user_requested_play: false,
            });
            let _ = reply.send(true);
        }
        MpvCommand::PlayIndex { index, reply } => {
            let res = mpv.command("playlist-play-index", &[&index.to_string()]);
            if let Err(e) = &res {
                log::error!("mpv playlist-play-index failed: {:?}", e);
            }
            let _ = reply.send(res.is_ok());
        }
        MpvCommand::PlaylistRemove { index, reply } => {
            let res = mpv.command("playlist_remove", &[&index.to_string()]);
            if let Err(e) = &res {
                log::error!("mpv playlist-remove failed: {:?}", e);
            }
            let _ = reply.send(res.is_ok());
        }
        MpvCommand::PlaylistMove { from, to, reply } => {
            let res = mpv.command("playlist-move", &[&from.to_string(), &to.to_string()]);
            let _ = reply.send(res.is_ok());
        }
        MpvCommand::PlaylistMoveNoReply { from, to } => {
            let _ = mpv.command("playlist-move", &[&from.to_string(), &to.to_string()]);
        }
        MpvCommand::SetVolume { volume, reply } => {
            let res = mpv.set_property("volume", volume);
            let _ = reply.send(res.is_ok());
        }
        MpvCommand::SetRepeat { repeat, reply } => {
            let mut ok = true;
            match repeat {
                Repeat::None => {
                    ok = ok && mpv.set_property("loop-file", "no").is_ok();
                    ok = ok && mpv.set_property("loop-playlist", "no").is_ok();
                }
                Repeat::All => {
                    ok = ok && mpv.set_property("loop-playlist", "inf").is_ok();
                }
                Repeat::One => {
                    ok = ok && mpv.set_property("loop-playlist", "no").is_ok();
                    ok = ok && mpv.set_property("loop-file", "inf").is_ok()
                }
            }
            let _ = reply.send(ok);
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
                    Some(i) => mpv.command("loadfile", &[&url, flag, &i.to_string()]),
                    None => mpv.command("loadfile", &[&url, flag]),
                };

                if res.is_err() {
                    ok = false;
                    log::error!("mpv loadfile failed for '{}'", url);
                }
            }

            let _ = reply.send(ok);
        }
        MpvCommand::Await { reply } => {
            let _ = reply.send(true);
        }
    }
}

impl MpvHandle {
    pub fn new(config: &serde_yaml::Value, sender: Sender<MpvPlaybackState>) -> MpvHandle {
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

        thread::spawn(move || {
            if let Err(e) = t_mpv_runtime(mpv, sender, rx) {
                log::error!("Error in mpv playlist thread: {}", e);
            }
        });

        Self {
            tx,
            dead: AtomicBool::new(false),
        }
    }

    pub async fn play(&self) {
        self.call(|reply| MpvCommand::Play { reply }).await
    }

    pub async fn pause(&self) {
        self.call(|reply| MpvCommand::Pause { reply }).await
    }

    pub async fn stop(&self) {
        self.call(|reply| MpvCommand::Stop { reply }).await
    }

    pub async fn next(&self) {
        self.call(|reply| MpvCommand::Next { reply }).await
    }

    /// If over 5 seconds in, go to the start of current track. If not, go back to previous.
    ///
    /// current_time -> current_playback_state.position
    pub async fn previous(&self, current_time: f64) {
        self.call(|reply| MpvCommand::Previous {
            current_time,
            reply,
        })
        .await
    }

    /// Change the playback position. By default, seeks by a relative amount of seconds.
    /// The second argument consists of flags controlling the seek mode:
    ///
    /// `relative` (default)
    ///     Seek relative to current position (a negative value seeks backwards).
    ///
    /// `absolute`
    ///     Seek to a given time (a negative value starts from the end of the file).
    pub async fn seek(&self, target: f64, flag: SeekFlag) {
        self.call(|reply| MpvCommand::Seek {
            target,
            flag,
            reply,
        })
        .await
    }

    pub async fn hard_seek(&self, target: f64, url: String) {
        self.call(|reply| MpvCommand::HardSeek { target, url, reply })
            .await
    }

    pub async fn play_index(&self, index: usize) {
        self.call(|reply| MpvCommand::PlayIndex { index, reply })
            .await
    }

    pub async fn playlist_remove(&self, index: usize) {
        self.call(|reply| MpvCommand::PlaylistRemove { index, reply })
            .await
    }

    pub async fn playlist_move(&self, from: usize, to: usize) {
        self.call(|reply| MpvCommand::PlaylistMove { from, to, reply })
            .await
    }

    pub fn playlist_move_nowait(&self, from: usize, to: usize) {
        if self.dead.load(Ordering::Relaxed) {
            return;
        }
        let _ = self.tx.send(MpvCommand::PlaylistMoveNoReply { from, to });
    }

    pub async fn set_volume(&self, volume: i64) {
        self.call(|reply| MpvCommand::SetVolume { volume, reply })
            .await
    }

    pub async fn set_repeat(&self, repeat: Repeat) {
        self.call(|reply| MpvCommand::SetRepeat { repeat, reply })
            .await
    }
    pub async fn load_files(&self, urls: Vec<String>, flag: LoadFileFlag, index: Option<i64>) {
        self.call(|reply| MpvCommand::LoadFiles {
            urls,
            flag,
            index,
            reply,
        })
        .await
    }

    pub async fn await_reply(&self) {
        self.call(|reply| MpvCommand::Await { reply }).await
    }

    async fn call(&self, make_cmd: impl FnOnce(oneshot::Sender<bool>) -> MpvCommand) {
        if self.dead.load(Ordering::Relaxed) {
            return;
        }

        let (tx, rx) = oneshot::channel();

        if self.tx.send(make_cmd(tx)).is_err() {
            self.dead.store(true, Ordering::Relaxed);
            log::error!("mpv thread is dead");
            return;
        }

        match rx.await {
            Ok(true) => {}
            Ok(false) => {
                // this is not so bad usually, mpv refuses to run certain commands pretty often
                log::error!("mpv command failed to run command");
            }
            Err(e) => {
                // this should hopefully not actually happen very often (mpv is pretty stable)
                // instead of lying about health, we just politely ask the user to restart the app
                log::error!("mpv thread died mid-command: {}", e);
                self.dead.store(true, Ordering::Relaxed);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
// #[serde(rename_all = "kebab-case")]
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

pub enum SeekFlag {
    Relative,
    Absolute,
}

/// This implements pending resume which is a feature that will seek to the location in the song
/// the app last at when closed, after you launch the app again. Created after receiving HardSeek
/// from the UI, it will try its best to seek while we're in the same file.
struct PendingResume {
    expected_url: String,
    target: f64,
    started_at: Instant,
    last_attempt: Instant,
    user_requested_play: bool,
}

impl PendingResume {
    fn handle_tick(&mut self, mpv: &Mpv) -> ResumeResult {
        let current_url = mpv.get_property::<String>("path").unwrap_or_default();
        let pos = mpv.get_property::<f64>("time-pos").unwrap_or(0.0);

        let elapsed = self.started_at.elapsed();

        // wrong file - abort
        if elapsed > Duration::from_millis(200) && current_url != self.expected_url {
            return ResumeResult::Abort;
        }

        // success OR timeout
        if elapsed > Duration::from_secs(3) || (pos - self.target).abs() <= 0.5 {
            if self.user_requested_play {
                let _ = mpv.set_property("pause", false);
            }
            return ResumeResult::Done;
        }

        if elapsed >= Duration::from_millis(100) {
            let _ = mpv.command("seek", &[&self.target.to_string(), "absolute"]);
            self.last_attempt = Instant::now();
        }

        ResumeResult::Pending
    }
}

enum ResumeResult {
    Pending,
    Done,
    Abort,
}
