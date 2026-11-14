//! Telling which windows are on the macOS Space the user is looking at.
//!
//! The Accessibility API lists an application's windows on every Space and does not say which
//! Space a window is on. The window server does publish which windows are on screen, with the
//! process that owns each and where it is. A window the Accessibility API reports at the same
//! place, owned by the same process, is on screen; one with no such counterpart is on another
//! Space. Neither list needs any permission beyond what ElaraWM already has.

use objc2::rc::Retained;
use objc2_core_graphics::{
    kCGNullWindowID, kCGWindowBounds, kCGWindowLayer, kCGWindowOwnerPID,
    CGWindowListCopyWindowInfo, CGWindowListOption,
};
use objc2_foundation::{ns_string, NSArray, NSDictionary, NSNumber, NSString};

use crate::Rect;

/// How far apart two reports of one window's frame may be. The two sources round differently.
const TOLERANCE: f64 = 2.;

type Dictionary = NSDictionary;

/// The ordinary windows that are on screen: for each, its process and its frame in global
/// coordinates.
#[derive(Debug)]
pub(super) struct OnScreen(Vec<(i32, Rect)>);

impl OnScreen {
    /// Reads the list from the window server. `None` if it would not say.
    pub(super) fn read() -> Option<Self> {
        let options =
            CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
        let list = CGWindowListCopyWindowInfo(options, kCGNullWindowID)?;
        // SAFETY: the function returns an array of dictionaries, and the
        // Core Foundation collections are the same objects as the Foundation ones. The
        // reference count `list` held is handed to the `Retained`.
        let list: Retained<NSArray<Dictionary>> = unsafe {
            let ptr = objc2_core_foundation::CFRetained::into_raw(list);
            Retained::from_raw(ptr.as_ptr().cast())?
        };

        // SAFETY: the keys are valid static strings, bridged like the collections above.
        let key = |key: &'static objc2_core_foundation::CFString| -> &'static NSString {
            unsafe { &*std::ptr::from_ref(key).cast::<NSString>() }
        };
        let (layer_key, pid_key, bounds_key) = unsafe {
            (
                key(kCGWindowLayer),
                key(kCGWindowOwnerPID),
                key(kCGWindowBounds),
            )
        };

        let number = |dictionary: &Dictionary, key: &NSString| -> Option<f64> {
            let value = dictionary.objectForKey(key)?;
            Some(value.downcast::<NSNumber>().ok()?.doubleValue())
        };

        let mut windows = Vec::new();
        for entry in list.iter() {
            // Layer 0 is where ordinary windows live. Menus, the Dock and the like are above.
            if number(&entry, layer_key) != Some(0.) {
                continue;
            }
            let Some(pid) = number(&entry, pid_key) else {
                continue;
            };
            let Some(bounds) = entry
                .objectForKey(bounds_key)
                .and_then(|value| value.downcast::<Dictionary>().ok())
            else {
                continue;
            };
            let field = |name: &NSString| number(&bounds, name);
            let (Some(x), Some(y), Some(w), Some(h)) = (
                field(ns_string!("X")),
                field(ns_string!("Y")),
                field(ns_string!("Width")),
                field(ns_string!("Height")),
            ) else {
                continue;
            };
            windows.push((pid as i32, Rect::new(x, y, w, h)));
        }
        Some(Self(windows))
    }

    /// Whether a window of process `pid` is on screen at `frame`.
    pub(super) fn has(&self, pid: i32, frame: Rect) -> bool {
        let close = |a: f64, b: f64| (a - b).abs() <= TOLERANCE;
        self.0.iter().any(|(owner, bounds)| {
            *owner == pid
                && close(bounds.x, frame.x)
                && close(bounds.y, frame.y)
                && close(bounds.w, frame.w)
                && close(bounds.h, frame.h)
        })
    }

    /// Whether any ordinary window of process `pid` is on screen.
    pub(super) fn has_window_of(&self, pid: i32) -> bool {
        self.0.iter().any(|(owner, _)| *owner == pid)
    }
}
