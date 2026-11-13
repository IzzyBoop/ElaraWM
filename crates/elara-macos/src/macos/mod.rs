//! The real backend: AppKit for displays and applications, the Accessibility API for windows.

mod ax;
mod backend;
mod display;

use objc2_application_services::{
    kAXTrustedCheckOptionPrompt, AXIsProcessTrusted, AXIsProcessTrustedWithOptions,
};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFString, CFType};
use objc2_foundation::MainThreadMarker;

pub use self::backend::MacBackend;
use crate::DisplayInfo;

/// Lists the connected displays, primary first. This needs no permission.
///
/// # Panics
///
/// Panics if called from a thread other than the main thread.
pub fn displays() -> Vec<DisplayInfo> {
    let mtm = MainThreadMarker::new().expect("displays must be listed on the main thread");
    display::displays(mtm)
}

/// Returns whether this process may use the Accessibility API.
///
/// The permission is granted per application in System Settings, under Privacy & Security,
/// Accessibility. For a command-line build it attaches to the terminal that launched it.
pub fn accessibility_trusted() -> bool {
    // SAFETY: takes no arguments and only reads the permission database.
    unsafe { AXIsProcessTrusted() }
}

/// Returns whether this process may use the Accessibility API, and if not, has macOS show its
/// dialog that offers to open the relevant System Settings pane.
///
/// macOS shows the dialog at most once per launch, and granting the permission takes effect
/// without restarting.
pub fn request_accessibility() -> bool {
    // SAFETY: the static is a valid CFString for the lifetime of the process.
    let key: &CFString = unsafe { kAXTrustedCheckOptionPrompt };
    let value: &CFType = CFBoolean::new(true);
    let options = CFDictionary::<CFString, CFType>::from_slices(&[key], &[value]);
    // SAFETY: `options` is a valid dictionary with the one key this function documents.
    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) }
}
