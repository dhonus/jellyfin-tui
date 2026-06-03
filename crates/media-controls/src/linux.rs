// Linux MPRIS backend using mpris-server 0.10.
//
// Architecture:
//   SharedState (Arc<Mutex<…>>) ← polled by MPRIS property readers
//   event_tx → event_rx          returned by events(); app selects on it
//   update_tx → update_rx        update() sends diffs; background task emits
//                                properties_changed() on the Server

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mpris_server::{
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus as MprisStatus, Property, Server, Signal,
    Time, TrackId,
    zbus::{Result as ZbusResult, fdo},
};
use tokio::sync::mpsc;

use crate::{Backend, Config, MediaControlEvent, NowPlaying, PlaybackStatus, SeekDirection};

// ── Shared state (polled by MPRIS property queries) ───────────────────────────

#[derive(Default)]
struct State {
    now_playing: NowPlaying,
    /// Monotonically incremented when the track (title) changes; used as
    /// a trackid path component.
    track_gen: u64,
}

type SharedState = Arc<Mutex<State>>;

// ── MPRIS PlayerInterface implementation ─────────────────────────────────────

struct LinuxPlayer {
    event_tx: mpsc::Sender<MediaControlEvent>,
    state: SharedState,
}

// Helper: best-effort send; never panics, never blocks.
macro_rules! emit_event {
    ($self:expr, $event:expr) => {
        let _ = $self.event_tx.try_send($event);
    };
}

impl mpris_server::RootInterface for LinuxPlayer {
    async fn raise(&self) -> fdo::Result<()> {
        emit_event!(self, MediaControlEvent::Raise);
        Ok(())
    }

    async fn quit(&self) -> fdo::Result<()> {
        emit_event!(self, MediaControlEvent::Quit);
        Ok(())
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_fullscreen(&self, _fullscreen: bool) -> ZbusResult<()> {
        Ok(())
    }

    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn identity(&self) -> fdo::Result<String> {
        Ok("jellyfin-tui".to_string())
    }

    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("jellyfin-tui".to_string())
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec!["file".to_string(), "http".to_string(), "https".to_string()])
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(vec!["audio/mpeg".to_string(), "audio/flac".to_string(), "audio/ogg".to_string()])
    }
}

impl mpris_server::PlayerInterface for LinuxPlayer {
    async fn next(&self) -> fdo::Result<()> {
        emit_event!(self, MediaControlEvent::Next);
        Ok(())
    }

    async fn previous(&self) -> fdo::Result<()> {
        emit_event!(self, MediaControlEvent::Previous);
        Ok(())
    }

    async fn pause(&self) -> fdo::Result<()> {
        emit_event!(self, MediaControlEvent::Pause);
        Ok(())
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        emit_event!(self, MediaControlEvent::Toggle);
        Ok(())
    }

    async fn stop(&self) -> fdo::Result<()> {
        emit_event!(self, MediaControlEvent::Stop);
        Ok(())
    }

    async fn play(&self) -> fdo::Result<()> {
        emit_event!(self, MediaControlEvent::Play);
        Ok(())
    }

    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        let micros = offset.as_micros();
        if micros >= 0 {
            emit_event!(
                self,
                MediaControlEvent::Seek(
                    SeekDirection::Forward,
                    Duration::from_micros(micros as u64),
                )
            );
        } else {
            emit_event!(
                self,
                MediaControlEvent::Seek(
                    SeekDirection::Backward,
                    Duration::from_micros((-micros) as u64),
                )
            );
        }
        Ok(())
    }

    async fn set_position(&self, _track_id: TrackId, position: Time) -> fdo::Result<()> {
        let micros = position.as_micros();
        if micros >= 0 {
            emit_event!(
                self,
                MediaControlEvent::SetPosition(Duration::from_micros(micros as u64))
            );
        }
        Ok(())
    }

    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Ok(())
    }

    async fn playback_status(&self) -> fdo::Result<MprisStatus> {
        let state = lock_state(&self.state)?;
        Ok(to_mpris_status(state.now_playing.status))
    }

    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(LoopStatus::None)
    }

    async fn set_loop_status(&self, _loop_status: LoopStatus) -> ZbusResult<()> {
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn set_rate(&self, _rate: PlaybackRate) -> ZbusResult<()> {
        Ok(())
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_shuffle(&self, _shuffle: bool) -> ZbusResult<()> {
        Ok(())
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        let state = lock_state(&self.state)?;
        Ok(build_metadata(&state))
    }

    async fn volume(&self) -> fdo::Result<mpris_server::Volume> {
        let state = lock_state(&self.state)?;
        Ok(state.now_playing.volume.unwrap_or(1.0))
    }

    async fn set_volume(&self, volume: mpris_server::Volume) -> ZbusResult<()> {
        emit_event!(self, MediaControlEvent::SetVolume(volume));
        Ok(())
    }

    async fn position(&self) -> fdo::Result<Time> {
        let state = lock_state(&self.state)?;
        Ok(state
            .now_playing
            .position
            .map(|d| Time::from_micros(d.as_micros() as i64))
            .unwrap_or(Time::ZERO))
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn lock_state(shared: &SharedState) -> fdo::Result<std::sync::MutexGuard<'_, State>> {
    shared.lock().map_err(|_| fdo::Error::Failed("state lock poisoned".into()))
}

fn to_mpris_status(s: Option<PlaybackStatus>) -> MprisStatus {
    match s {
        Some(PlaybackStatus::Playing) => MprisStatus::Playing,
        Some(PlaybackStatus::Paused) => MprisStatus::Paused,
        _ => MprisStatus::Stopped,
    }
}

/// Build a `Metadata` value from the stored state.
/// Uses `/org/jellyfin_tui/Track/<gen>` as trackid so each new song gets a
/// unique object path (fully spec-compliant, no fixed "/").
fn build_metadata(state: &State) -> Metadata {
    let np = &state.now_playing;

    let trackid = TrackId::try_from(format!("/org/jellyfin_tui/Track/{}", state.track_gen))
        .unwrap_or(TrackId::NO_TRACK);

    let mut builder = Metadata::builder().trackid(trackid);

    if let Some(t) = &np.title {
        builder = builder.title(t.clone());
    }
    if let Some(a) = &np.artist {
        builder = builder.artist([a.clone()]);
    }
    if let Some(al) = &np.album {
        builder = builder.album(al.clone());
    }
    if let Some(d) = np.duration {
        builder = builder.length(Time::from_micros(d.as_micros() as i64));
    }
    if let Some(url) = &np.cover_url {
        builder = builder.art_url(url.clone());
    }

    builder.build()
}

// ── Update channel handler: owns the Server and pushes properties_changed ────

struct UpdateTask {
    server: Arc<Server<LinuxPlayer>>,
    update_rx: mpsc::UnboundedReceiver<Vec<Property>>,
    seek_rx: mpsc::UnboundedReceiver<Time>,
}

impl UpdateTask {
    async fn run(mut self) {
        loop {
            tokio::select! {
                maybe_props = self.update_rx.recv() => {
                    let Some(props) = maybe_props else { break };
                    if let Err(e) = self.server.properties_changed(props).await {
                        log::warn!("MPRIS properties_changed failed: {e}");
                    }
                }
                maybe_pos = self.seek_rx.recv() => {
                    let Some(pos) = maybe_pos else { break };
                    if let Err(e) = self.server.emit(Signal::Seeked { position: pos }).await {
                        log::warn!("MPRIS Seeked signal failed: {e}");
                    }
                }
            }
        }
    }
}

// ── Public backend ────────────────────────────────────────────────────────────

pub struct LinuxBackend {
    state: SharedState,
    update_tx: mpsc::UnboundedSender<Vec<Property>>,
    seek_tx: mpsc::UnboundedSender<Time>,
    event_rx: Option<mpsc::Receiver<MediaControlEvent>>,
}

impl LinuxBackend {
    pub async fn new(config: Config) -> Option<Self> {
        let state: SharedState = Arc::new(Mutex::new(State::default()));

        // Channel capacity: 64 is plenty; events are rare.
        let (event_tx, event_rx) = mpsc::channel::<MediaControlEvent>(64);

        let player = LinuxPlayer { event_tx, state: Arc::clone(&state) };

        let server = match Server::new(config.dbus_name, player).await {
            Ok(s) => Arc::new(s),
            Err(e) => {
                log::warn!("MPRIS: failed to start D-Bus server: {e}");
                return None;
            }
        };

        let (update_tx, update_rx) = mpsc::unbounded_channel::<Vec<Property>>();
        let (seek_tx, seek_rx) = mpsc::unbounded_channel::<Time>();

        let task = UpdateTask { server, update_rx, seek_rx };
        tokio::spawn(task.run());

        Some(LinuxBackend { state, update_tx, seek_tx, event_rx: Some(event_rx) })
    }
}

impl Backend for LinuxBackend {
    fn take_receiver(&mut self) -> mpsc::Receiver<MediaControlEvent> {
        self.event_rx.take().expect("events() called more than once")
    }

    fn update(&self, new: NowPlaying) {
        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        let mut props: Vec<Property> = Vec::new();
        let mut seek_pos: Option<Time> = None;

        // ── Metadata fields ───────────────────────────────────────────────
        let meta_dirty = new.title != state.now_playing.title
            || new.artist != state.now_playing.artist
            || new.album != state.now_playing.album
            || new.cover_url != state.now_playing.cover_url
            || new.duration != state.now_playing.duration;

        // Merge non-None fields into stored state.
        if let Some(v) = new.title.clone() {
            if state.now_playing.title.as_deref() != Some(v.as_str()) {
                state.track_gen += 1;
            }
            state.now_playing.title = Some(v);
        }
        if let Some(v) = new.artist { state.now_playing.artist = Some(v); }
        if let Some(v) = new.album  { state.now_playing.album  = Some(v); }
        if let Some(v) = new.cover_url { state.now_playing.cover_url = Some(v); }
        if let Some(v) = new.duration  { state.now_playing.duration  = Some(v); }

        if meta_dirty {
            props.push(Property::Metadata(build_metadata(&state)));
        }

        // ── Playback status ───────────────────────────────────────────────
        if let Some(s) = new.status {
            if state.now_playing.status != Some(s) {
                state.now_playing.status = Some(s);
                props.push(Property::PlaybackStatus(to_mpris_status(Some(s))));
            }
        }

        // ── Volume ────────────────────────────────────────────────────────
        if let Some(v) = new.volume {
            if state.now_playing.volume != Some(v) {
                state.now_playing.volume = Some(v);
                props.push(Property::Volume(v));
            }
        }

        // ── Position (discontinuous seek → Seeked signal) ─────────────────
        // We always update stored position for polling, but only emit the
        // Seeked signal when position is explicitly set via update().
        if let Some(pos) = new.position {
            let old_secs = state
                .now_playing
                .position
                .map(|d| d.as_secs())
                .unwrap_or(u64::MAX);
            state.now_playing.position = Some(pos);
            // Emit Seeked only when there's a meaningful jump (> 2 s delta).
            let new_secs = pos.as_secs();
            if old_secs.abs_diff(new_secs) > 2 {
                seek_pos = Some(Time::from_micros(pos.as_micros() as i64));
            }
        }

        drop(state);

        if !props.is_empty() {
            let _ = self.update_tx.send(props);
        }
        if let Some(pos) = seek_pos {
            let _ = self.seek_tx.send(pos);
        }
    }
}
