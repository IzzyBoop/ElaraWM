//! Seeing and intercepting key presses system-wide, through a CoreGraphics event tap.

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_core_foundation::{
    kCFRunLoopDefaultMode, CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType,
};
use objc2_foundation::MainThreadMarker;

use crate::{KeyEvent, KeyModifiers};

/// The event types the system sends to a tap it has switched off, either because the callback
/// took too long or because the user is typing into a secure field.
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

type Handler = Box<dyn FnMut(&KeyEvent) -> bool>;

struct Context {
    handler: Handler,
    /// The tap's own port, needed to switch the tap back on from inside the callback.
    port: Option<CFRetained<CFMachPort>>,
}

/// Returns the character a key produces on the current keyboard layout with no modifiers held.
fn unmodified_character(keycode: u16) -> Option<char> {
    // Asking the system what a synthetic, modifier-free press of this key would type goes
    // through the current input source without needing the Carbon text input APIs.
    let event = CGEvent::new_keyboard_event(None, keycode, true)?;
    CGEvent::set_flags(Some(&event), CGEventFlags(0));

    let mut buffer = [0u16; 4];
    let mut length = 0;
    // SAFETY: `buffer` has room for the four UTF-16 units asked for, and `length` is a valid
    // place for the count.
    unsafe {
        CGEvent::keyboard_get_unicode_string(
            Some(&event),
            buffer.len() as _,
            &mut length,
            buffer.as_mut_ptr(),
        );
    }

    let units = &buffer[..(length as usize).min(buffer.len())];
    char::decode_utf16(units.iter().copied())
        .next()?
        .ok()
        .filter(|c| !c.is_control())
}

unsafe extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    // SAFETY: `user_info` is the `Context` that `KeyTap` owns and keeps alive for as long as
    // the tap exists. Taps are delivered on the main run loop, one at a time.
    let context = unsafe { &mut *user_info.cast::<Context>() };
    // SAFETY: the system passes a valid event that lives for the duration of the callback.
    let event_ref = unsafe { event.as_ref() };

    if event_type.0 == TAP_DISABLED_BY_TIMEOUT || event_type.0 == TAP_DISABLED_BY_USER_INPUT {
        warn!("key event tap was disabled by the system, re-enabling it");
        if let Some(port) = &context.port {
            CGEvent::tap_enable(port, true);
        }
        return event.as_ptr();
    }

    let pressed = match event_type {
        CGEventType::KeyDown => true,
        CGEventType::KeyUp => false,
        _ => return event.as_ptr(),
    };

    let keycode =
        CGEvent::integer_value_field(Some(event_ref), CGEventField::KeyboardEventKeycode) as u16;
    let is_repeat =
        CGEvent::integer_value_field(Some(event_ref), CGEventField::KeyboardEventAutorepeat) != 0;
    let flags = CGEvent::flags(Some(event_ref));

    let key_event = KeyEvent {
        keycode,
        character: unmodified_character(keycode),
        modifiers: KeyModifiers {
            control: flags.contains(CGEventFlags::MaskControl),
            shift: flags.contains(CGEventFlags::MaskShift),
            option: flags.contains(CGEventFlags::MaskAlternate),
            command: flags.contains(CGEventFlags::MaskCommand),
        },
        pressed,
        is_repeat,
    };

    if (context.handler)(&key_event) {
        // Returning null drops the event: no application sees it.
        std::ptr::null_mut()
    } else {
        event.as_ptr()
    }
}

/// A system-wide tap on key presses and releases.
///
/// While it exists, every key event is shown to the handler before any application gets it.
/// The handler returns `true` to swallow the event. It runs on the main thread from the main
/// run loop, and must return quickly: the system switches off a tap that stalls.
pub struct KeyTap {
    port: CFRetained<CFMachPort>,
    source: CFRetained<CFRunLoopSource>,
    context: *mut Context,
}

impl KeyTap {
    /// Installs the tap. Returns `None` if the system refuses, which it does without the
    /// Accessibility permission.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the main thread.
    pub fn new(handler: impl FnMut(&KeyEvent) -> bool + 'static) -> Option<Self> {
        MainThreadMarker::new().expect("the key tap must be created on the main thread");

        let context = Box::into_raw(Box::new(Context {
            handler: Box::new(handler),
            port: None,
        }));

        let mask: CGEventMask = (1 << CGEventType::KeyDown.0) | (1 << CGEventType::KeyUp.0);
        // SAFETY: the callback has the signature the API expects, and `context` stays valid
        // until `Drop`, which disables the tap before freeing it.
        let port = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                mask,
                Some(tap_callback),
                context.cast::<c_void>(),
            )
        };
        let Some(port) = port else {
            // SAFETY: `context` came from `Box::into_raw` above and was not handed to a tap.
            drop(unsafe { Box::from_raw(context) });
            return None;
        };

        let Some(source) = CFMachPort::new_run_loop_source(None, Some(&port), 0) else {
            // SAFETY: as above; the tap exists but is never attached to a run loop, so it never
            // calls back, and dropping `port` releases it.
            drop(unsafe { Box::from_raw(context) });
            return None;
        };

        // SAFETY: nothing else holds a reference to the context yet: the tap is not attached
        // to the run loop until the next statement.
        unsafe { (*context).port = Some(port.clone()) };

        if let Some(run_loop) = CFRunLoop::main() {
            // SAFETY: the mode constant is a valid static.
            run_loop.add_source(Some(&source), unsafe { kCFRunLoopDefaultMode });
        }
        CGEvent::tap_enable(&port, true);

        Some(Self {
            port,
            source,
            context,
        })
    }
}

impl Drop for KeyTap {
    fn drop(&mut self) {
        CGEvent::tap_enable(&self.port, false);
        if let Some(run_loop) = CFRunLoop::main() {
            // SAFETY: the mode constant is a valid static.
            run_loop.remove_source(Some(&self.source), unsafe { kCFRunLoopDefaultMode });
        }
        self.port.invalidate();
        // SAFETY: the tap is disabled, detached and invalidated, so the callback cannot run
        // again, and this is the only place the context is freed.
        drop(unsafe { Box::from_raw(self.context) });
    }
}
