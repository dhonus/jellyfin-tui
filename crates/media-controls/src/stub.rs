// No-op backend for platforms with no media-controls support (Windows, etc.).
use tokio::sync::mpsc;
use crate::{Backend, Config, MediaControlEvent, NowPlaying};

pub struct StubBackend {
    event_rx: Option<mpsc::Receiver<MediaControlEvent>>,
}

impl StubBackend {
    pub fn new(_config: Config) -> Option<Self> {
        let (_tx, rx) = mpsc::channel(1);
        Some(StubBackend { event_rx: Some(rx) })
    }
}

impl Backend for StubBackend {
    fn take_receiver(&mut self) -> mpsc::Receiver<MediaControlEvent> {
        self.event_rx.take().expect("events() called more than once")
    }

    fn update(&self, _state: NowPlaying) {}
}
