// Platform support: Linux (MPRIS via D-Bus), macOS (MediaPlayer framework), stub elsewhere.
// Phase 2 fills in macOS bodies; everything else is frozen after Phase 1.

use std::time::Duration;
use tokio::sync::mpsc;

// ── Public types ─────────────────────────────────────────────────────────────

/// Construction config.  `dbus_name` is used as the D-Bus bus-name suffix on
/// Linux and as the bundle identifier on macOS; `display_name` is shown to the
/// user by media-center UIs.
pub struct Config {
    pub dbus_name: &'static str,
    pub display_name: &'static str,
}

/// Declarative snapshot of player state.  Every field is optional: supply only
/// the fields that changed.  The crate merges non-None values into its stored
/// state, diffs, and emits only the changed MPRIS/MediaPlayer properties.
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
    /// 0.0–1.0 (or higher) as on MPRIS Volume; maps to 0–100% internally.
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

/// Events delivered to the app from OS media controls (keyboard media keys,
/// lock screen controls, system media sessions, …).
#[derive(Debug, Clone)]
pub enum MediaControlEvent {
    Play,
    Pause,
    /// Play/Pause toggle (single button).
    Toggle,
    Stop,
    Next,
    Previous,
    /// Relative seek.  The app decides whether to honour it when stopped.
    Seek(SeekDirection, Duration),
    /// Absolute seek to the given position.
    SetPosition(Duration),
    /// Volume in 0.0–1.0 (MPRIS) or 0.0–1.0 (macOS).
    SetVolume(f64),
    Raise,
    Quit,
}

// ── Internal backend interface (platform-complete; frozen after Phase 1) ──────
//
// Every platform implements this trait.  The facade owns a `Box<dyn Backend>`
// and delegates to it.
//
// Invariant for macOS: `new_backend()` must be called on the main thread.
// `tick()` must be called periodically on the main thread to pump the
// CFRunLoop / NSRunLoop.  Linux ignores both constraints.

pub(crate) trait Backend: Send + 'static {
    /// Consume and return the event receiver.  Panics if called more than once
    /// (guarded by Option internally in each impl).
    fn take_receiver(&mut self) -> mpsc::Receiver<MediaControlEvent>;

    /// Apply a partial state update.  The implementation diffs against its
    /// stored state and immediately emits OS notifications for changed fields.
    fn update(&self, state: NowPlaying);

    /// Optional: pump the platform run-loop.  Called by the app on the main
    /// thread if desired.  Linux no-ops this; macOS will use it for CFRunLoop.
    fn tick(&self) {}
}

// ── Platform selection ────────────────────────────────────────────────────────

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(unix, target_os = "macos")))]
mod stub;

// ── Facade ────────────────────────────────────────────────────────────────────

/// Handle to OS media controls.
///
/// - Construct with [`MediaControls::new`]; returns `None` when OS controls
///   are unavailable (D-Bus unreachable, name already taken, unsupported
///   platform).  Never panics.
/// - Call [`events`] once to obtain the async receiver for control events.
/// - Call [`update`] whenever player state changes.
pub struct MediaControls {
    inner: Box<dyn Backend>,
}

impl MediaControls {
    /// Create OS media controls.  Returns `None` on any non-fatal setup
    /// failure so the caller can degrade gracefully.
    ///
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

    /// Return the channel receiver for incoming control events.
    ///
    /// Call this exactly once after construction; the receiver is moved out of
    /// an internal `Option` and a second call will panic.
    pub fn events(&mut self) -> mpsc::Receiver<MediaControlEvent> {
        self.inner.take_receiver()
    }

    /// Push a (possibly partial) state snapshot.
    ///
    /// Fields set to `None` keep their previous value.  Changed fields are
    /// propagated to the OS immediately (sub-100 ms on Linux).
    pub fn update(&self, state: NowPlaying) {
        self.inner.update(state);
    }

    /// Pump the platform run-loop (macOS only; no-op on other platforms).
    pub fn tick(&self) {
        self.inner.tick();
    }
}
