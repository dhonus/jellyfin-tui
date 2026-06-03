// macOS media controls skeleton — Phase 2 will fill in the bodies.
//
// Approach (from souvlaki + objc2-media-player docs):
//   • MPRemoteCommandCenter  – registers handlers for transport commands
//   • MPNowPlayingInfoCenter – pushes metadata / playback state
//
// All of these APIs must be used on the main thread.  The frozen interface
// contract:
//   new()  → called on main thread at app startup
//   tick() → called on main thread in the app's event loop tick
//   update() → may be called from any thread; queues work for the main thread
//
// Dependencies for Phase 2:
//   objc2            = "0.5"           (safe Objective-C bindings)
//   objc2-media-player = "0.2"         (MPRemoteCommandCenter, etc.)
//   objc2-foundation   = "0.2"         (NSString, NSMutableDictionary, etc.)
//   dispatch2          = "0.2"         (main-thread dispatch_async)
//
// (Do NOT use the legacy `objc`/`cocoa` crates — souvlaki's macOS code uses
//  them and has a known debug panic, issue #77.)

use std::time::Duration;
use tokio::sync::mpsc;

use crate::{Backend, Config, MediaControlEvent, NowPlaying};

pub struct MacosBackend {
    event_rx: Option<mpsc::Receiver<MediaControlEvent>>,
    // Phase 2: add
    //   command_center: *mut AnyObject,   (MPRemoteCommandCenter ptr, main-thread only)
    //   info_center:    *mut AnyObject,   (MPNowPlayingInfoCenter ptr)
    //   update_tx:      std::sync::mpsc::SyncSender<NowPlaying>,
    //                   (bounded sync channel; main-thread drain in tick())
}

impl MacosBackend {
    /// **Must be called on the main thread.**
    pub fn new(_config: Config) -> Option<Self> {
        let (_event_tx, event_rx) = mpsc::channel::<MediaControlEvent>(64);

        // TODO(Phase 2): initialise MPRemoteCommandCenter and register handlers:
        //
        //   let command_center = unsafe {
        //       MPRemoteCommandCenter::sharedCommandCenter()
        //   };
        //
        //   For each command (play, pause, togglePlayPause, nextTrack,
        //   previousTrack, changePlaybackPosition, changeVolumeSlider):
        //
        //     let cmd = unsafe { command_center.playCommand() };
        //     unsafe { cmd.setEnabled(true) };
        //     unsafe {
        //         cmd.addTargetWithHandler(MethodBlock::new(move |_event| {
        //             let _ = event_tx.try_send(MediaControlEvent::Play);
        //             MPRemoteCommandHandlerStatus::Success
        //         }));
        //     };
        //
        //   (Use objc2_media_player::{MPRemoteCommandCenter, MPRemoteCommandHandlerStatus}
        //    and objc2::rc::Retained for memory-safe Objective-C object handles.)

        Some(MacosBackend { event_rx: Some(event_rx) })
    }
}

impl Backend for MacosBackend {
    fn take_receiver(&mut self) -> mpsc::Receiver<MediaControlEvent> {
        self.event_rx.take().expect("events() called more than once")
    }

    fn update(&self, _state: NowPlaying) {
        // TODO(Phase 2): merge _state into stored NowPlaying, diff, then
        // dispatch_async to main thread to push changes to
        // MPNowPlayingInfoCenter.defaultCenter().  Steps:
        //
        //   1. Lock a Mutex<NowPlaying> and merge non-None fields.
        //   2. If metadata fields changed, build a new NSMutableDictionary:
        //        MPMediaItemPropertyTitle         → NSString
        //        MPMediaItemPropertyArtist        → NSString
        //        MPMediaItemPropertyAlbumTitle    → NSString
        //        MPMediaItemPropertyPlaybackDuration → NSNumber (f64 secs)
        //        MPMediaItemPropertyArtwork       → MPMediaItemArtwork
        //          (built via initWithBoundsSize:requestHandler: with a block
        //           that loads NSImage from the cover_url using
        //           NSImage::initWithContentsOfURL)
        //        MPNowPlayingInfoPropertyElapsedPlaybackTime → NSNumber (f64 secs)
        //      Then: MPNowPlayingInfoCenter.defaultCenter().setNowPlayingInfo(dict)
        //
        //   3. If playback status changed:
        //        MPNowPlayingInfoCenter.defaultCenter().setPlaybackState(state)
        //      where state is MPNowPlayingPlaybackState::{Playing,Paused,Stopped}.
        //
        //   4. Volume: no direct macOS API; nothing to do.
        //
        // Use dispatch2::Queue::main().exec_async(|| { ... }) to ensure all
        // UIKit/AppKit calls happen on the main thread without blocking callers.
        //
        // IMPORTANT: keep a GLOBAL_METADATA_COUNTER (AtomicU64) and check
        // it inside the artwork-loading closure so stale artwork from a
        // previous song doesn't overwrite a newer one (same race souvlaki
        // guarded with its counter).
    }

    /// Pump the main-thread run-loop.  Call from the app's tick/event loop.
    fn tick(&self) {
        // TODO(Phase 2): If using a main-thread-only approach without
        // dispatch_async, drain pending update work here.
        //
        //   while let Ok(update) = self.update_rx.try_recv() {
        //       apply_update_on_main_thread(update);
        //   }
        //
        // If dispatch_async is used in update(), this is a no-op.
    }
}

// Ensure the type is Send even though it will contain raw Obj-C pointers in
// Phase 2.  Safety justification (fill in Phase 2):
//   All Obj-C calls are dispatched to the main thread via dispatch_async, so
//   the pointers are never dereferenced from a non-main thread.
//
// For Phase 1 there are no raw pointers so Send is derived automatically.
