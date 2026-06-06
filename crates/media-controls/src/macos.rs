#![allow(non_upper_case_globals)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, NSObject, Sel};
use objc2::{class, msg_send, sel, ClassType};
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSNumber, NSRunLoop, NSString};
use tokio::sync::mpsc;

use crate::{Backend, Capabilities, Config, MediaControlEvent, NowPlaying, PlaybackStatus};

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities::base() // MPRemoteCommandCenter has no Raise/Quit
    }
}

const MP_STATE_PLAYING: usize = 1;
const MP_STATE_PAUSED: usize = 2;
const MP_STATE_STOPPED: usize = 3;
const MP_CMD_SUCCESS: isize = 0;

#[link(name = "MediaPlayer", kind = "framework")]
extern "C" {
    static MPMediaItemPropertyTitle: *const AnyObject;
    static MPMediaItemPropertyArtist: *const AnyObject;
    static MPMediaItemPropertyAlbumTitle: *const AnyObject;
    static MPMediaItemPropertyPlaybackDuration: *const AnyObject;
    static MPMediaItemPropertyArtwork: *const AnyObject;
    static MPNowPlayingInfoPropertyElapsedPlaybackTime: *const AnyObject;
}

static EVENT_TX: OnceLock<mpsc::Sender<MediaControlEvent>> = OnceLock::new();
static ARTWORK_GEN: AtomicU64 = AtomicU64::new(0);

macro_rules! emit {
    ($event:expr) => {
        if let Some(tx) = EVENT_TX.get() {
            match tx.try_send($event) {
                Ok(()) => {}
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    log::warn!("media-controls: event channel full — dropping event");
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
            }
        }
    };
}

extern "C-unwind" fn cmd_play(_: *mut AnyObject, _: Sel, _: *mut AnyObject) -> isize {
    emit!(MediaControlEvent::Play);
    MP_CMD_SUCCESS
}
extern "C-unwind" fn cmd_pause(_: *mut AnyObject, _: Sel, _: *mut AnyObject) -> isize {
    emit!(MediaControlEvent::Pause);
    MP_CMD_SUCCESS
}
extern "C-unwind" fn cmd_toggle(_: *mut AnyObject, _: Sel, _: *mut AnyObject) -> isize {
    emit!(MediaControlEvent::Toggle);
    MP_CMD_SUCCESS
}
extern "C-unwind" fn cmd_next(_: *mut AnyObject, _: Sel, _: *mut AnyObject) -> isize {
    emit!(MediaControlEvent::Next);
    MP_CMD_SUCCESS
}
extern "C-unwind" fn cmd_prev(_: *mut AnyObject, _: Sel, _: *mut AnyObject) -> isize {
    emit!(MediaControlEvent::Previous);
    MP_CMD_SUCCESS
}
extern "C-unwind" fn cmd_stop(_: *mut AnyObject, _: Sel, _: *mut AnyObject) -> isize {
    emit!(MediaControlEvent::Stop);
    MP_CMD_SUCCESS
}

extern "C-unwind" fn cmd_position(_: *mut AnyObject, _: Sel, evt: *mut AnyObject) -> isize {
    if !evt.is_null() {
        let secs: f64 = unsafe { msg_send![evt, positionTime] };
        if secs.is_finite() && secs >= 0.0 {
            emit!(MediaControlEvent::SetPosition(Duration::from_secs_f64(secs)));
        }
    }
    MP_CMD_SUCCESS
}

fn setup_command_center() -> Option<*mut AnyObject> {
    let cls: &'static AnyClass = if let Some(existing) = AnyClass::get(c"JellyfinTuiCommandHandler")
    {
        existing
    } else {
        let mut builder = ClassBuilder::new(c"JellyfinTuiCommandHandler", NSObject::class())?;
        unsafe {
            builder.add_method(
                sel!(handlePlay:),
                cmd_play as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> isize,
            );
            builder.add_method(
                sel!(handlePause:),
                cmd_pause as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> isize,
            );
            builder.add_method(
                sel!(handleToggle:),
                cmd_toggle as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> isize,
            );
            builder.add_method(
                sel!(handleNext:),
                cmd_next as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> isize,
            );
            builder.add_method(
                sel!(handlePrev:),
                cmd_prev as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> isize,
            );
            builder.add_method(
                sel!(handleStop:),
                cmd_stop as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> isize,
            );
            builder.add_method(
                sel!(handlePosition:),
                cmd_position as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> isize,
            );
        }
        builder.register()
    };

    let handler: *mut AnyObject = unsafe { msg_send![cls, new] };
    if handler.is_null() {
        return None;
    }

    let cc: *mut AnyObject = unsafe {
        let cls = AnyClass::get(c"MPRemoteCommandCenter")?;
        msg_send![cls, sharedCommandCenter]
    };
    if cc.is_null() {
        unsafe {
            let _: () = msg_send![handler, release];
        }
        return None;
    }

    macro_rules! wire {
        ($getter:ident, $($action:tt)+) => {{
            let cmd: *mut AnyObject = unsafe { msg_send![cc, $getter] };
            if !cmd.is_null() {
                unsafe {
                    let _: () = msg_send![cmd, setEnabled: objc2::runtime::Bool::YES];
                    let _: () = msg_send![cmd, addTarget: handler, action: sel!($($action)+)];
                }
            }
        }};
    }
    wire!(playCommand, handlePlay:);
    wire!(pauseCommand, handlePause:);
    wire!(togglePlayPauseCommand, handleToggle:);
    wire!(nextTrackCommand, handleNext:);
    wire!(previousTrackCommand, handlePrev:);
    wire!(stopCommand, handleStop:);
    wire!(changePlaybackPositionCommand, handlePosition:);

    Some(handler)
}

unsafe fn create_artwork(cover_url: &str) -> Option<*mut AnyObject> {
    let ns_url: *mut AnyObject =
        msg_send![class!(NSURL), URLWithString: &*NSString::from_str(cover_url)];
    if ns_url.is_null() {
        return None;
    }

    let image_alloc: *mut AnyObject = msg_send![class!(NSImage), alloc];
    let image_ptr: *mut AnyObject = msg_send![image_alloc, initWithContentsOfURL: ns_url];
    let image: Retained<AnyObject> = Retained::from_raw(image_ptr)?;

    let bounds: CGSize = msg_send![image_ptr, size];
    let bounds =
        if bounds.width > 0.0 && bounds.height > 0.0 { bounds } else { CGSize::new(600.0, 600.0) };

    let image_for_block = image.clone();
    let block = RcBlock::new(move |_: CGSize| -> *mut AnyObject {
        Retained::as_ptr(&image_for_block) as *mut _
    });

    let artwork_alloc: *mut AnyObject = msg_send![class!(MPMediaItemArtwork), alloc];
    let artwork: *mut AnyObject = msg_send![artwork_alloc,
        initWithBoundsSize: bounds,
        requestHandler: &*block];

    if artwork.is_null() {
        None
    } else {
        Some(artwork)
    }
}

fn push_now_playing(state: NowPlaying, artwork_gen: u64) {
    autoreleasepool(|_| {
        let center_cls = match AnyClass::get(c"MPNowPlayingInfoCenter") {
            Some(c) => c,
            None => return,
        };
        let center: *mut AnyObject = unsafe { msg_send![center_cls, defaultCenter] };
        if center.is_null() {
            return;
        }

        let dict: *mut AnyObject = unsafe { msg_send![class!(NSMutableDictionary), new] };
        if dict.is_null() {
            return;
        }

        unsafe {
            if let Some(ref v) = state.title {
                let _: () = msg_send![dict, setObject: &*NSString::from_str(v), forKey: MPMediaItemPropertyTitle];
            }
            if let Some(ref v) = state.artist {
                let _: () = msg_send![dict, setObject: &*NSString::from_str(v), forKey: MPMediaItemPropertyArtist];
            }
            if let Some(ref v) = state.album {
                let _: () = msg_send![dict, setObject: &*NSString::from_str(v), forKey: MPMediaItemPropertyAlbumTitle];
            }
            if let Some(d) = state.duration {
                let _: () = msg_send![dict, setObject: &*NSNumber::new_f64(d.as_secs_f64()), forKey: MPMediaItemPropertyPlaybackDuration];
            }
            if let Some(p) = state.position {
                let _: () = msg_send![dict, setObject: &*NSNumber::new_f64(p.as_secs_f64()), forKey: MPNowPlayingInfoPropertyElapsedPlaybackTime];
            }
            if let Some(ref url) = state.cover_url {
                if ARTWORK_GEN.load(Ordering::Acquire) == artwork_gen {
                    if let Some(artwork) = create_artwork(url) {
                        let _: () =
                            msg_send![dict, setObject: artwork, forKey: MPMediaItemPropertyArtwork];
                        let _: () = msg_send![artwork, release];
                    }
                }
            }

            let _: () = msg_send![center, setNowPlayingInfo: dict];
            let _: () = msg_send![dict, release];

            let play_state: usize = match state.status {
                Some(PlaybackStatus::Playing) => MP_STATE_PLAYING,
                Some(PlaybackStatus::Paused) => MP_STATE_PAUSED,
                _ => MP_STATE_STOPPED,
            };
            let _: () = msg_send![center, setPlaybackState: play_state];
        }
    });
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_millis() as u64
}

#[allow(dead_code)]
struct ObjcPtr(*mut AnyObject);
unsafe impl Send for ObjcPtr {}

pub struct MacosBackend {
    event_rx: Option<mpsc::Receiver<MediaControlEvent>>,
    state: Arc<Mutex<NowPlaying>>,
    _handler: ObjcPtr,
    last_tick_ms: AtomicU64,
}

impl MacosBackend {
    pub fn new(_config: Config) -> Option<Self> {
        let (tx, rx) = mpsc::channel::<MediaControlEvent>(64);
        if EVENT_TX.set(tx).is_err() {
            return None;
        }
        let handler = setup_command_center()?;
        Some(MacosBackend {
            event_rx: Some(rx),
            state: Arc::new(Mutex::new(NowPlaying::default())),
            _handler: ObjcPtr(handler),
            last_tick_ms: AtomicU64::new(0),
        })
    }
}

impl Backend for MacosBackend {
    fn take_receiver(&mut self) -> Option<mpsc::Receiver<MediaControlEvent>> {
        self.event_rx.take()
    }

    fn update(&self, new: NowPlaying) {
        let (snapshot, gen) = {
            let mut g = match self.state.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            if new.title.is_some() {
                g.title = new.title;
            }
            if new.artist.is_some() {
                g.artist = new.artist;
            }
            if new.album.is_some() {
                g.album = new.album;
            }
            if new.cover_url.is_some() {
                if g.cover_url != new.cover_url {
                    ARTWORK_GEN.fetch_add(1, Ordering::AcqRel);
                }
                g.cover_url = new.cover_url;
            }
            if new.duration.is_some() {
                g.duration = new.duration;
            }
            if new.position.is_some() {
                g.position = new.position;
            }
            if new.status.is_some() {
                g.status = new.status;
            }
            if new.volume.is_some() {
                g.volume = new.volume;
            }
            if new.track_number.is_some() {
                g.track_number = new.track_number;
            }
            if new.year.is_some() {
                g.year = new.year;
            }
            if new.shuffle.is_some() {
                g.shuffle = new.shuffle;
            }
            if new.loop_status.is_some() {
                g.loop_status = new.loop_status;
            }
            if new.fullscreen.is_some() {
                g.fullscreen = new.fullscreen;
            }
            if new.rate.is_some() {
                g.rate = new.rate;
            }
            (g.clone(), ARTWORK_GEN.load(Ordering::Acquire))
        };
        DispatchQueue::main().exec_async(move || push_now_playing(snapshot, gen));
    }

    fn tick(&self) {
        const INTERVAL_MS: u64 = 50;
        let now = now_ms();
        if now.saturating_sub(self.last_tick_ms.load(Ordering::Relaxed)) < INTERVAL_MS {
            return;
        }
        self.last_tick_ms.store(now, Ordering::Relaxed);
        autoreleasepool(|_| unsafe {
            NSRunLoop::currentRunLoop()
                .runMode_beforeDate(NSDefaultRunLoopMode, &NSDate::distantPast());
        });
    }
}

unsafe impl Send for MacosBackend {}
