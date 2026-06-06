use std::time::Duration;
use tokio::sync::mpsc;

/// D-Bus bus-name suffix on Linux; bundle ID on macOS; display label in UIs.
pub struct Config {
    pub dbus_name: &'static str,
    pub display_name: &'static str,
    pub capabilities: Capabilities,
    pub supported_uri_schemes: &'static [&'static str],
    pub supported_mime_types: &'static [&'static str],
    /// Minimum playback rate the player supports (MPRIS MinimumRate).
    pub minimum_rate: f64,
    /// Maximum playback rate the player supports (MPRIS MaximumRate).
    pub maximum_rate: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dbus_name: "media-controls",
            display_name: "Media Controls",
            capabilities: Capabilities::default(),
            supported_uri_schemes: &["file", "http", "https"],
            supported_mime_types: &["audio/mpeg", "audio/flac", "audio/ogg"],
            minimum_rate: 1.0,
            maximum_rate: 1.0,
        }
    }
}

/// Which transport actions and properties the player supports.
#[derive(Clone, Debug)]
pub struct Capabilities {
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_seek: bool,
    pub can_control: bool,
    pub can_raise: bool,
    pub can_quit: bool,
    pub can_set_fullscreen: bool,
    pub has_track_list: bool,
}

impl Capabilities {
    /// Safe baseline: transport controls on, everything else off.
    /// Platform defaults build on top of this.
    pub const fn base() -> Self {
        Capabilities {
            can_go_next: true,
            can_go_previous: true,
            can_play: true,
            can_pause: true,
            can_seek: true,
            can_control: true,
            can_raise: false,
            can_quit: false,
            can_set_fullscreen: false,
            has_track_list: false,
        }
    }
}

/// Partial player state snapshot. `None` fields keep their previous value.
///
/// Build with [`NowPlaying::new()`] and chain setters:
/// ```rust
/// NowPlaying::new().title("Song").artist("Artist").status(PlaybackStatus::Playing);
/// ```
#[derive(Default, Clone, Debug, PartialEq)]
pub struct NowPlaying {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub cover_url: Option<String>,
    pub duration: Option<Duration>,
    pub position: Option<Duration>,
    pub status: Option<PlaybackStatus>,
    /// 0.0–1.0 range.
    pub volume: Option<f64>,
    pub track_number: Option<u32>,
    pub year: Option<u32>,
    pub shuffle: Option<bool>,
    pub loop_status: Option<LoopStatus>,
    pub fullscreen: Option<bool>,
    /// Current playback rate (1.0 = normal speed).
    pub rate: Option<f64>,
}

impl NowPlaying {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, v: impl Into<String>) -> Self {
        self.title = Some(v.into());
        self
    }
    pub fn artist(mut self, v: impl Into<String>) -> Self {
        self.artist = Some(v.into());
        self
    }
    pub fn album(mut self, v: impl Into<String>) -> Self {
        self.album = Some(v.into());
        self
    }
    pub fn cover_url(mut self, v: impl Into<String>) -> Self {
        self.cover_url = Some(v.into());
        self
    }
    pub fn duration(mut self, v: Duration) -> Self {
        self.duration = Some(v);
        self
    }
    pub fn position(mut self, v: Duration) -> Self {
        self.position = Some(v);
        self
    }
    pub fn status(mut self, v: PlaybackStatus) -> Self {
        self.status = Some(v);
        self
    }
    pub fn volume(mut self, v: f64) -> Self {
        self.volume = Some(v);
        self
    }
    pub fn track_number(mut self, v: u32) -> Self {
        self.track_number = Some(v);
        self
    }
    pub fn year(mut self, v: u32) -> Self {
        self.year = Some(v);
        self
    }
    pub fn shuffle(mut self, v: bool) -> Self {
        self.shuffle = Some(v);
        self
    }
    pub fn loop_status(mut self, v: LoopStatus) -> Self {
        self.loop_status = Some(v);
        self
    }
    pub fn fullscreen(mut self, v: bool) -> Self {
        self.fullscreen = Some(v);
        self
    }
    pub fn rate(mut self, v: f64) -> Self {
        self.rate = Some(v);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoopStatus {
    #[default]
    None,
    Track,
    Playlist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekDirection {
    Forward,
    Backward,
}

/// Events from OS media controls (media keys, lock screen, system session).
#[derive(Debug, Clone)]
pub enum MediaControlEvent {
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    Seek(SeekDirection, Duration),
    SetPosition(Duration),
    SetVolume(f64),
    SetShuffle(bool),
    SetLoopStatus(LoopStatus),
    SetFullscreen(bool),
    SetRate(f64),
    Raise,
    Quit,
}

pub(crate) trait Backend: Send + 'static {
    /// Returns `None` on a second call; use [`MediaControls::events`] instead.
    fn take_receiver(&mut self) -> Option<mpsc::Receiver<MediaControlEvent>>;
    fn update(&self, state: NowPlaying);
    /// Pump the platform run-loop. No-op except on macOS.
    fn tick(&self) {}
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(unix, target_os = "macos")))]
mod stub;

/// Handle to OS media controls.
pub struct MediaControls {
    inner: Box<dyn Backend>,
}

impl MediaControls {
    /// **macOS:** must be called on the main thread.
    pub async fn new(config: Config) -> Option<Self> {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let backend = linux::LinuxBackend::new(config).await?;
            return Some(Self { inner: Box::new(backend) });
        }

        #[cfg(target_os = "macos")]
        {
            let backend = macos::MacosBackend::new(config)?;
            return Some(Self { inner: Box::new(backend) });
        }

        #[cfg(not(any(unix, target_os = "macos")))]
        {
            let backend = stub::StubBackend::new(config)?;
            return Some(Self { inner: Box::new(backend) });
        }

        #[allow(unreachable_code)]
        None
    }

    /// Returns the real receiver on first call; returns a closed channel on subsequent calls.
    pub fn events(&mut self) -> mpsc::Receiver<MediaControlEvent> {
        self.inner.take_receiver().unwrap_or_else(|| {
            let (_, rx) = mpsc::channel(1);
            rx
        })
    }

    pub fn update(&self, state: NowPlaying) {
        self.inner.update(state);
    }

    /// Pump the platform run-loop (macOS only; no-op elsewhere).
    pub fn tick(&self) {
        self.inner.tick();
    }
}
