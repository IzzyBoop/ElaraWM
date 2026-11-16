//! Display enumeration through AppKit.

use std::path::PathBuf;

use objc2_app_kit::{NSScreen, NSWorkspace};
use objc2_foundation::{ns_string, MainThreadMarker, NSNumber};

use crate::{DisplayId, DisplayInfo, Rect};

fn screen_id(screen: &NSScreen) -> Option<u32> {
    screen
        .deviceDescription()
        .objectForKey(ns_string!("NSScreenNumber"))
        .and_then(|value| value.downcast::<NSNumber>().ok())
        .map(|number| number.unsignedIntValue())
}

/// Returns the file of the desktop picture of `display`, as the system reports it. `None`
/// if it reports none, or something that is not a file.
///
/// For a picture that changes through the day this is the file that holds all of its
/// versions, of which an image reader shows the first.
pub(super) fn wallpaper(mtm: MainThreadMarker, display: DisplayId) -> Option<PathBuf> {
    let screens = NSScreen::screens(mtm);
    let screen = screens
        .iter()
        .find(|screen| screen_id(screen) == Some(display.0))?;
    NSWorkspace::sharedWorkspace()
        .desktopImageURLForScreen(&screen)?
        .to_file_path()
}

/// Lists connected displays, primary first.
pub(super) fn displays(mtm: MainThreadMarker) -> Vec<DisplayInfo> {
    let screens = NSScreen::screens(mtm);

    // AppKit's screen space has its origin at the bottom-left of the first screen, which is the
    // one with the menu bar. Its height is what flips every other rectangle.
    let Some(primary_height) = screens.iter().next().map(|s| s.frame().size.height) else {
        return Vec::new();
    };

    screens
        .iter()
        .enumerate()
        .map(|(idx, screen)| {
            let frame = screen.frame();
            let visible = screen.visibleFrame();

            let id = screen_id(&screen).unwrap_or(idx as u32);

            let fps = screen.maximumFramesPerSecond();

            DisplayInfo {
                id: DisplayId(id),
                name: screen.localizedName().to_string(),
                frame: Rect::from_bottom_left(
                    frame.origin.x,
                    frame.origin.y,
                    frame.size.width,
                    frame.size.height,
                    primary_height,
                ),
                visible_frame: Rect::from_bottom_left(
                    visible.origin.x,
                    visible.origin.y,
                    visible.size.width,
                    visible.size.height,
                    primary_height,
                ),
                scale: screen.backingScaleFactor(),
                refresh_mhz: (fps > 0).then(|| fps as u32 * 1000),
                is_primary: idx == 0,
            }
        })
        .collect()
}
