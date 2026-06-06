use std::sync::{Arc, Mutex};
use std::time::Duration;

use mpris_server::{
    zbus::{fdo, Result as ZbusResult},
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus as MprisStatus, Property, Server, Signal,
    Time, TrackId,
};
use tokio::sync::mpsc;

use crate::{Backend, Config, MediaControlEvent, NowPlaying, PlaybackStatus, SeekDirection};

#[derive(Default)]
struct State {
    now_playing: NowPlaying,
    track_gen: u64,
}

type SharedState = Arc<Mutex<State>>;

struct LinuxPlayer {
    event_tx: mpsc::Sender<MediaControlEvent>,
    state: SharedState,
    display_name: String,
    dbus_name: String,
}

macro_rules! emit_event {
    ($self:expr, $event:expr) => {
        match $self.event_tx.try_send($event) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                log::warn!("media-controls: event channel full — dropping event");
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
        }
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
    async fn set_fullscreen(&self, _: bool) -> ZbusResult<()> {
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
        Ok(self.display_name.clone())
    }
    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok(self.dbus_name.clone())
    }
    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec!["file".into(), "http".into(), "https".into()])
    }
    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(vec!["audio/mpeg".into(), "audio/flac".into(), "audio/ogg".into()])
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
                    Duration::from_micros(micros as u64)
                )
            );
        } else {
            emit_event!(
                self,
                MediaControlEvent::Seek(
                    SeekDirection::Backward,
                    Duration::from_micros((-micros) as u64)
                )
            );
        }
        Ok(())
    }

    async fn set_position(&self, _track_id: TrackId, position: Time) -> fdo::Result<()> {
        let micros = position.as_micros();
        if micros >= 0 {
            emit_event!(self, MediaControlEvent::SetPosition(Duration::from_micros(micros as u64)));
        }
        Ok(())
    }

    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Ok(())
    }

    async fn playback_status(&self) -> fdo::Result<MprisStatus> {
        Ok(to_mpris_status(lock_state(&self.state)?.now_playing.status))
    }

    // TODO: wire repeat/shuffle/rate through NowPlaying when jellyfin-tui supports them.
    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(LoopStatus::None)
    }
    async fn set_loop_status(&self, _: LoopStatus) -> ZbusResult<()> {
        Ok(())
    }
    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }
    async fn set_rate(&self, _: PlaybackRate) -> ZbusResult<()> {
        Ok(())
    }
    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn set_shuffle(&self, _: bool) -> ZbusResult<()> {
        Ok(())
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        Ok(build_metadata(&*lock_state(&self.state)?))
    }
    async fn volume(&self) -> fdo::Result<mpris_server::Volume> {
        Ok(lock_state(&self.state)?.now_playing.volume.unwrap_or(1.0))
    }
    async fn set_volume(&self, volume: mpris_server::Volume) -> ZbusResult<()> {
        emit_event!(self, MediaControlEvent::SetVolume(volume));
        Ok(())
    }
    async fn position(&self) -> fdo::Result<Time> {
        Ok(lock_state(&self.state)?
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

fn build_metadata(state: &State) -> Metadata {
    let np = &state.now_playing;
    let trackid = TrackId::try_from(format!("/org/jellyfin_tui/Track/{}", state.track_gen))
        .unwrap_or(TrackId::NO_TRACK);
    let mut b = Metadata::builder().trackid(trackid);
    if let Some(t) = &np.title {
        b = b.title(t.clone());
    }
    if let Some(a) = &np.artist {
        b = b.artist([a.clone()]);
    }
    if let Some(al) = &np.album {
        b = b.album(al.clone());
    }
    if let Some(d) = np.duration {
        b = b.length(Time::from_micros(d.as_micros() as i64));
    }
    if let Some(url) = &np.cover_url {
        b = b.art_url(url.clone());
    }
    if let Some(n) = np.track_number {
        b = b.track_number(n as i32);
    }
    if let Some(y) = np.year {
        b = b.content_created(format!("{y}-01-01T00:00:00"));
    }
    b.build()
}

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

pub struct LinuxBackend {
    state: SharedState,
    update_tx: mpsc::UnboundedSender<Vec<Property>>,
    seek_tx: mpsc::UnboundedSender<Time>,
    event_rx: Option<mpsc::Receiver<MediaControlEvent>>,
}

impl LinuxBackend {
    pub async fn new(config: Config) -> Option<Self> {
        let state: SharedState = Arc::new(Mutex::new(State::default()));
        let (event_tx, event_rx) = mpsc::channel::<MediaControlEvent>(64);

        let player = LinuxPlayer {
            event_tx,
            state: Arc::clone(&state),
            display_name: config.display_name.to_owned(),
            dbus_name: config.dbus_name.to_owned(),
        };

        let server = match Server::new(config.dbus_name, player).await {
            Ok(s) => Arc::new(s),
            Err(e) => {
                log::warn!("MPRIS: failed to start D-Bus server: {e}");
                return None;
            }
        };

        let (update_tx, update_rx) = mpsc::unbounded_channel::<Vec<Property>>();
        let (seek_tx, seek_rx) = mpsc::unbounded_channel::<Time>();
        tokio::spawn(UpdateTask { server, update_rx, seek_rx }.run());

        Some(LinuxBackend { state, update_tx, seek_tx, event_rx: Some(event_rx) })
    }
}

impl Backend for LinuxBackend {
    fn take_receiver(&mut self) -> Option<mpsc::Receiver<MediaControlEvent>> {
        self.event_rx.take()
    }

    fn update(&self, new: NowPlaying) {
        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        let mut props: Vec<Property> = Vec::new();
        let mut seek_pos: Option<Time> = None;

        let meta_dirty = new.title != state.now_playing.title
            || new.artist != state.now_playing.artist
            || new.album != state.now_playing.album
            || new.cover_url != state.now_playing.cover_url
            || new.duration != state.now_playing.duration
            || new.track_number != state.now_playing.track_number
            || new.year != state.now_playing.year;

        if let Some(v) = new.title.clone() {
            if state.now_playing.title.as_deref() != Some(v.as_str()) {
                state.track_gen += 1;
            }
            state.now_playing.title = Some(v);
        }
        if let Some(v) = new.artist {
            state.now_playing.artist = Some(v);
        }
        if let Some(v) = new.album {
            state.now_playing.album = Some(v);
        }
        if let Some(v) = new.cover_url {
            state.now_playing.cover_url = Some(v);
        }
        if let Some(v) = new.duration {
            state.now_playing.duration = Some(v);
        }
        if let Some(v) = new.track_number {
            state.now_playing.track_number = Some(v);
        }
        if let Some(v) = new.year {
            state.now_playing.year = Some(v);
        }
        if meta_dirty {
            props.push(Property::Metadata(build_metadata(&state)));
        }

        if let Some(s) = new.status {
            if state.now_playing.status != Some(s) {
                state.now_playing.status = Some(s);
                props.push(Property::PlaybackStatus(to_mpris_status(Some(s))));
            }
        }

        if let Some(v) = new.volume {
            if state.now_playing.volume != Some(v) {
                state.now_playing.volume = Some(v);
                props.push(Property::Volume(v));
            }
        }

        // TODO: position delta heuristic misfires on <2 s scrubs. Fix requires a
        // dedicated `seeked_to` field in NowPlaying rather than inferring from delta.
        if let Some(pos) = new.position {
            let old_secs = state.now_playing.position.map(|d| d.as_secs()).unwrap_or(u64::MAX);
            state.now_playing.position = Some(pos);
            if old_secs.abs_diff(pos.as_secs()) > 2 {
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
