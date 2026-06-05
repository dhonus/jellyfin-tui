use std::time::Duration;
use tokio::sync::mpsc;

/// D-Bus bus-name suffix on Linux; bundle ID on macOS; display label in UIs.
pub struct Config {
    pub dbus_name: &'static str,
    pub display_name: &'static str,
}

/// Partial player state snapshot. `None` fields keep their previous value.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct NowPlaying {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// `file://…` URL accepted on both platforms.
    pub cover_url: Option<String>,
    pub duration: Option<Duration>,
    pub position: Option<Duration>,
    pub status: Option<PlaybackStatus>,
    /// 0.0–1.0 range.
    pub volume: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
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
