//! What niri draws on top of the windows that is not part of the layout.
//!
//! `hotkey_overlay` is niri's file. `markup` is ElaraWM's own, in place of Pango.

use elara_macos::{InputEvent, MouseEventKind};

pub mod hotkey_overlay;
pub mod markup;

/// Whether an input event takes the hotkey overlay away, if it is showing.
///
/// This is niri's `should_hide_hotkey_overlay()` for the events macOS has: any key press,
/// any mouse button press and any scrolling. Releases and pointer movement leave it up.
pub fn hides_hotkey_overlay(event: &InputEvent) -> bool {
    match event {
        InputEvent::Key(event) => event.pressed,
        InputEvent::Mouse(event) => matches!(event.kind, MouseEventKind::Pressed(_)),
        InputEvent::Scroll(_) => true,
    }
}
