#![allow(unexpected_cfgs)]

/// macOS-specific utilities for Now Playing integration
///
/// The macOS MediaPlayer framework requires the CFRunLoop to be active
/// for the Now Playing controls to work. This module provides a function
/// to pump the runloop periodically.

#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};

/// Pumps the macOS CFRunLoop to allow Now Playing events to be processed.
/// This should be called periodically from the main event loop.
#[cfg(target_os = "macos")]
pub fn pump_runloop() {
    unsafe {
        // Get the current run loop
        let run_loop: *mut objc::runtime::Object = msg_send![class!(NSRunLoop), currentRunLoop];

        // Create a very short date in the past to process pending events without blocking
        let date: *mut objc::runtime::Object = msg_send![class!(NSDate), distantPast];

        // Run the loop until the date (immediately returns after processing pending events)
        let mode: *mut objc::runtime::Object = msg_send![class!(NSString), stringWithUTF8String: b"kCFRunLoopDefaultMode\0".as_ptr()];
        let _: () = msg_send![run_loop, runMode: mode beforeDate: date];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn pump_runloop() {
    // No-op on other platforms
}
