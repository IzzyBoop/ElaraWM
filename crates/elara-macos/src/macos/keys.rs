//! Seeing and intercepting key presses system-wide, through a CoreGraphics event tap.

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_core_foundation::{
    kCFRunLoopDefaultMode, CFData, CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource, CFType,
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

// The keyboard layout services live in Carbon's HIToolbox and have no Rust bindings.
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    /// Returns the current keyboard layout, retained. Null if there is none.
    fn TISCopyCurrentKeyboardLayoutInputSource() -> *mut c_void;
    /// Returns a property of an input source, not retained. Null if it has no such property.
    fn TISGetInputSourceProperty(source: *mut c_void, key: *const c_void) -> *mut c_void;
    /// Property key for the layout's `UCKeyboardLayout` data, as a `CFData`.
    static kTISPropertyUnicodeKeyLayoutData: *const c_void;
    /// Returns the physical keyboard type, which selects a table within the layout.
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(
        key_layout: *const c_void,
        virtual_key_code: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        key_translate_options: u32,
        dead_key_state: *mut u32,
        max_string_length: usize,
        actual_string_length: *mut usize,
        unicode_string: *mut u16,
    ) -> i32;
}

/// `kUCKeyActionDisplay`: translate for showing the key's label, not for typing.
const KEY_ACTION_DISPLAY: u16 = 3;
/// `kUCKeyTranslateNoDeadKeysMask`: report a dead key as its own character.
const TRANSLATE_NO_DEAD_KEYS: u32 = 1;

/// Returns the character a key produces on the current keyboard layout with no modifiers held.
///
/// The modifier state is passed to the layout explicitly as "none". Asking through a synthetic
/// key event instead is not reliable: the answer follows the modifiers physically held, so
/// with Option down the F key reports as "ƒ" and no bind on Option+F can ever match.
fn unmodified_character(keycode: u16) -> Option<char> {
    let mut buffer = [0u16; 4];
    let mut length = 0usize;
    let mut dead_key_state = 0u32;

    // SAFETY: the input source is checked for null and released below. The layout data
    // belongs to it and is only used while it is alive. `buffer`, `length` and
    // `dead_key_state` are valid for the sizes passed.
    let status = unsafe {
        let source = TISCopyCurrentKeyboardLayoutInputSource();
        let source_ref = NonNull::new(source.cast::<CFType>())?;
        let source = CFRetained::<CFType>::from_raw(source_ref);
        let source_ptr = CFRetained::as_ptr(&source).as_ptr().cast::<c_void>();

        let data = TISGetInputSourceProperty(source_ptr, kTISPropertyUnicodeKeyLayoutData);
        // Input methods such as those for Chinese or Japanese have no key layout of their own.
        let data = NonNull::new(data.cast::<CFData>())?;
        let layout = data.as_ref().byte_ptr();

        UCKeyTranslate(
            layout.cast::<c_void>(),
            keycode,
            KEY_ACTION_DISPLAY,
            0,
            u32::from(LMGetKbdType()),
            TRANSLATE_NO_DEAD_KEYS,
            &mut dead_key_state,
            buffer.len(),
            &mut length,
            buffer.as_mut_ptr(),
        )
    };
    if status != 0 {
        return None;
    }

    let units = &buffer[..length.min(buffer.len())];
    char::decode_utf16(units.iter().copied())
        .next()?
        .ok()
        .filter(|c| !c.is_control())
}

/// Returns what a key types on the current layout with no modifiers. For checking the layout
/// lookup from outside the tap.
pub fn character_for_keycode(keycode: u16) -> Option<char> {
    unmodified_character(keycode)
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
