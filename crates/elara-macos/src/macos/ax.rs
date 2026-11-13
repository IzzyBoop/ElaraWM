//! Thin, safe helpers over the Accessibility C API.
//!
//! Every call here is a synchronous message to another process. It can fail at any time: the
//! application may be busy, may have exited, or the element may be gone. Helpers that read
//! therefore return `Option` or `Result` and callers treat failure as "unknown".

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFArray, CFBoolean, CFRetained, CFString, CFType, CGPoint, CGSize};

use crate::Rect;

pub(super) const WINDOWS: &str = "AXWindows";
pub(super) const FOCUSED_WINDOW: &str = "AXFocusedWindow";
pub(super) const POSITION: &str = "AXPosition";
pub(super) const SIZE: &str = "AXSize";
pub(super) const TITLE: &str = "AXTitle";
pub(super) const ROLE: &str = "AXRole";
pub(super) const SUBROLE: &str = "AXSubrole";
pub(super) const MINIMIZED: &str = "AXMinimized";
pub(super) const FULLSCREEN: &str = "AXFullScreen";
pub(super) const MAIN: &str = "AXMain";
pub(super) const CLOSE_BUTTON: &str = "AXCloseButton";

pub(super) const ACTION_RAISE: &str = "AXRaise";
pub(super) const ACTION_PRESS: &str = "AXPress";

/// Reads an attribute.
pub(super) fn copy(element: &AXUIElement, name: &str) -> Result<CFRetained<CFType>, AXError> {
    let name = CFString::from_str(name);
    let mut out: *const CFType = std::ptr::null();
    // SAFETY: `out` is a valid place for one pointer. On success the function stores a +1
    // retained object there, which `from_raw` takes ownership of.
    let err = unsafe { element.copy_attribute_value(&name, NonNull::from(&mut out)) };
    if err != AXError::Success {
        return Err(err);
    }
    match NonNull::new(out.cast_mut()) {
        // SAFETY: see above.
        Some(ptr) => Ok(unsafe { CFRetained::from_raw(ptr) }),
        None => Err(AXError::NoValue),
    }
}

pub(super) fn string(element: &AXUIElement, name: &str) -> Option<String> {
    let value = copy(element, name).ok()?;
    Some(value.downcast_ref::<CFString>()?.to_string())
}

pub(super) fn boolean(element: &AXUIElement, name: &str) -> Option<bool> {
    let value = copy(element, name).ok()?;
    Some(value.downcast_ref::<CFBoolean>()?.as_bool())
}

pub(super) fn element(element: &AXUIElement, name: &str) -> Option<CFRetained<AXUIElement>> {
    copy(element, name).ok()?.downcast::<AXUIElement>().ok()
}

fn point(element: &AXUIElement, name: &str) -> Option<CGPoint> {
    let value = copy(element, name).ok()?;
    let value = value.downcast_ref::<AXValue>()?;
    let mut out = CGPoint::default();
    // SAFETY: `out` has the layout the requested type names; the function writes it only when
    // the value holds that type and reports whether it did.
    let ok = unsafe {
        value.value(
            AXValueType::CGPoint,
            NonNull::from(&mut out).cast::<c_void>(),
        )
    };
    ok.then_some(out)
}

fn size(element: &AXUIElement, name: &str) -> Option<CGSize> {
    let value = copy(element, name).ok()?;
    let value = value.downcast_ref::<AXValue>()?;
    let mut out = CGSize::default();
    // SAFETY: as in `point()`.
    let ok = unsafe {
        value.value(
            AXValueType::CGSize,
            NonNull::from(&mut out).cast::<c_void>(),
        )
    };
    ok.then_some(out)
}

/// Reads a window's frame.
pub(super) fn frame(window: &AXUIElement) -> Option<Rect> {
    let origin = point(window, POSITION)?;
    let size = size(window, SIZE)?;
    Some(Rect::new(origin.x, origin.y, size.width, size.height))
}

fn set(element: &AXUIElement, name: &str, value: &CFType) -> Result<(), AXError> {
    let name = CFString::from_str(name);
    // SAFETY: both arguments are valid for the duration of the call.
    match unsafe { element.set_attribute_value(&name, value) } {
        AXError::Success => Ok(()),
        err => Err(err),
    }
}

pub(super) fn set_position(window: &AXUIElement, x: f64, y: f64) -> Result<(), AXError> {
    let mut point = CGPoint { x, y };
    // SAFETY: `point` has the layout the type names and outlives the call, which copies it.
    let value = unsafe {
        AXValue::new(
            AXValueType::CGPoint,
            NonNull::from(&mut point).cast::<c_void>(),
        )
    }
    .ok_or(AXError::Failure)?;
    set(window, POSITION, &value)
}

pub(super) fn set_size(window: &AXUIElement, w: f64, h: f64) -> Result<(), AXError> {
    let mut size = CGSize {
        width: w,
        height: h,
    };
    // SAFETY: as in `set_position()`.
    let value = unsafe {
        AXValue::new(
            AXValueType::CGSize,
            NonNull::from(&mut size).cast::<c_void>(),
        )
    }
    .ok_or(AXError::Failure)?;
    set(window, SIZE, &value)
}

pub(super) fn set_boolean(element: &AXUIElement, name: &str, value: bool) -> Result<(), AXError> {
    set(element, name, CFBoolean::new(value))
}

/// Returns whether the application lets an attribute be changed.
pub(super) fn is_settable(element: &AXUIElement, name: &str) -> bool {
    let name = CFString::from_str(name);
    let mut settable: u8 = 0;
    // SAFETY: `settable` is a valid place for one `Boolean`.
    let err = unsafe { element.is_attribute_settable(&name, NonNull::from(&mut settable)) };
    err == AXError::Success && settable != 0
}

pub(super) fn perform(element: &AXUIElement, action: &str) -> Result<(), AXError> {
    let action = CFString::from_str(action);
    // SAFETY: both arguments are valid for the duration of the call.
    match unsafe { element.perform_action(&action) } {
        AXError::Success => Ok(()),
        err => Err(err),
    }
}

/// Lists an application's windows.
pub(super) fn windows(app: &AXUIElement) -> Vec<CFRetained<AXUIElement>> {
    let Ok(value) = copy(app, WINDOWS) else {
        return Vec::new();
    };
    let Some(array) = value.downcast_ref::<CFArray>() else {
        return Vec::new();
    };
    // SAFETY: the AXWindows attribute is documented to be an array of AXUIElement.
    let array: &CFArray<AXUIElement> = unsafe { array.cast_unchecked() };
    array.to_vec()
}
