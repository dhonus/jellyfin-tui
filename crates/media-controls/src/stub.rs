use crate::{Backend, Capabilities, Config, MediaControlEvent, NowPlaying};

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities::base()
    }
}
use tokio::sync::mpsc;

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
    fn take_receiver(&mut self) -> Option<mpsc::Receiver<MediaControlEvent>> {
        self.event_rx.take()
    }
    fn update(&self, _state: NowPlaying) {}
}
