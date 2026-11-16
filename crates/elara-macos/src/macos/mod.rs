//! The real backend: AppKit for displays and applications, the Accessibility API for windows.

mod app;
mod ax;
mod backdrop;
mod backend;
mod display;
mod ghosts;
mod hotkey_overlay;
mod keys;
mod menu_bar;
mod overlay;
mod overview;
mod run_loop;
mod spaces;
mod thumbnails;

use objc2_application_services::{
    kAXTrustedCheckOptionPrompt, AXIsProcessTrusted, AXIsProcessTrustedWithOptions,
};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFPreferencesCopyAppValue, CFString, CFType};
use objc2_foundation::MainThreadMarker;

pub use self::app::{alert, bundle_identifier, set_start_at_login, start_at_login};
pub use self::backend::MacBackend;
pub use self::hotkey_overlay::{render_hotkey_overlay, HotkeyOverlayPanels};
pub use self::keys::{character_for_keycode, KeyTap};
pub use self::menu_bar::{MenuBar, MenuCommand};
pub use self::overlay::{rasterize, Overlay};
pub use self::overview::{render_overview, take_overview_input, OverviewPanels};
pub use self::run_loop::{run, Wake};
pub use self::thumbnails::{request_screen_recording, screen_recording_permitted};
use crate::DisplayInfo;

/// Returns whether "Displays have separate Spaces" is switched on in System Settings, under
/// Desktop & Dock. `None` if the setting could not be read.
///
/// With it on, which is the default, each display has its own menu bar and a window is drawn
/// only on the display that holds most of it. With it off, the displays form one large desktop
/// and a window can straddle them.
pub fn displays_have_separate_spaces() -> Option<bool> {
    let key = CFString::from_static_str("spans-displays");
    let domain = CFString::from_static_str("com.apple.spaces");
    // The key is absent until the setting has been changed once, which means the default.
    let Some(value) = CFPreferencesCopyAppValue(&key, &domain) else {
        return Some(true);
    };
    let spans = value.downcast_ref::<CFBoolean>()?.as_bool();
    Some(!spans)
}

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
