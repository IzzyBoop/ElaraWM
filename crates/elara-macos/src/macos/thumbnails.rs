//! Pictures of windows, for the overview to show in place of cards.
//!
//! This is the one thing in ElaraWM that needs the Screen Recording permission, and it is
//! only used when the config asks for it. The pictures come from ScreenCaptureKit, which
//! takes one of a window whether or not it is in view: covered by the overview, or parked
//! off the edge of the display.
//!
//! ScreenCaptureKit knows windows by the window server's numbers and the Accessibility API
//! does not give those out, so the two are paired the way [`super::spaces`] does it: by
//! process and frame, and by title where that leaves a choice.
//!
//! Everything here is asked for on the main thread and answered on a queue of the system's.
//! The answers are put where the main thread finds them, and the main run loop is woken.

use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_core_foundation::{CFRetained, CFRunLoop};
use objc2_core_graphics::{CGImage, CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};
use objc2_foundation::NSError;
use objc2_screen_capture_kit::{
    SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration, SCWindow,
};

use crate::{Rect, WindowId};

/// How often the pictures are taken again while the overview is showing.
const INTERVAL: Duration = Duration::from_secs(1);

/// How long to wait for pictures that were asked for before asking again regardless.
const GIVE_UP_AFTER: Duration = Duration::from_secs(5);

/// How far apart two reports of one window's frame may be, as in [`super::spaces`].
const TOLERANCE: f64 = 2.;

/// Returns whether this process may take pictures of other applications' windows.
///
/// The permission is granted per application in System Settings, under Privacy & Security,
/// Screen & System Audio Recording. This only reads it.
pub fn screen_recording_permitted() -> bool {
    CGPreflightScreenCaptureAccess()
}

/// Returns whether this process may take pictures of other applications' windows, and if
/// not, has macOS show its dialog that offers to open System Settings.
///
/// macOS shows the dialog once per application, ever. A grant takes effect the next time the
/// application starts.
pub fn request_screen_recording() -> bool {
    CGRequestScreenCaptureAccess()
}

/// A window to take a picture of.
#[derive(Debug, Clone)]
pub(super) struct Wanted {
    pub id: WindowId,
    pub pid: i32,
    pub frame: Rect,
    pub title: Option<String>,
}

#[derive(Default)]
struct Shared {
    images: Mutex<HashMap<WindowId, CFRetained<CGImage>>>,
    /// Goes up whenever `images` changes.
    version: AtomicU64,
    /// How many answers are still awaited.
    pending: AtomicUsize,
}

impl Shared {
    /// Notes that one awaited answer has come, and has the main thread look.
    fn answered(&self, changed: bool) {
        if changed {
            self.version.fetch_add(1, Ordering::SeqCst);
        }
        // Not below zero: answers to a request that was given up on can still come in after
        // the count has been started over.
        let mut pending = self.pending.load(Ordering::SeqCst);
        while pending > 0 {
            match self.pending.compare_exchange_weak(
                pending,
                pending - 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(now) => pending = now,
            }
        }
        if let Some(run_loop) = CFRunLoop::main() {
            run_loop.wake_up();
        }
    }
}

/// The pictures taken so far, and the taking of new ones.
pub(super) struct Thumbnails {
    shared: Arc<Shared>,
    /// When pictures were last asked for.
    asked: Option<Instant>,
    /// Whether the user has been told that the permission is missing.
    warned: bool,
}

impl Thumbnails {
    pub(super) fn new() -> Self {
        Self {
            shared: Arc::default(),
            asked: None,
            warned: false,
        }
    }

    /// A number that changes whenever the pictures do.
    pub(super) fn version(&self) -> u64 {
        self.shared.version.load(Ordering::SeqCst)
    }

    pub(super) fn get(&self, id: WindowId) -> Option<CFRetained<CGImage>> {
        self.shared.images.lock().unwrap().get(&id).cloned()
    }

    /// Forgets every picture.
    pub(super) fn clear(&mut self) {
        let mut images = self.shared.images.lock().unwrap();
        if !images.is_empty() {
            images.clear();
            self.shared.version.fetch_add(1, Ordering::SeqCst);
        }
        self.asked = None;
    }

    /// Whether it is time to take the pictures again. `at_once` is for when the overview
    /// has just been opened.
    pub(super) fn due(&self, at_once: bool) -> bool {
        let waiting = self.shared.pending.load(Ordering::SeqCst) > 0;
        match self.asked {
            None => true,
            Some(asked) if waiting => asked.elapsed() >= GIVE_UP_AFTER,
            Some(asked) => at_once || asked.elapsed() >= INTERVAL,
        }
    }

    /// Asks for a picture of each of `wanted`, `scale` pixels to the point. The pictures
    /// turn up in [`Self::get()`] when they are ready, and those of windows not in `wanted`
    /// are dropped.
    pub(super) fn refresh(&mut self, wanted: Vec<Wanted>, scale: f64) {
        if !screen_recording_permitted() {
            if !self.warned {
                self.warned = true;
                // The first time ever, this is what makes macOS ask the user.
                request_screen_recording();
                warn!(
                    "window-thumbnails is on, but ElaraWM does not have the Screen Recording \
                     permission; the overview shows cards. Grant it in System Settings, under \
                     Privacy & Security, Screen & System Audio Recording, and start ElaraWM again"
                );
            }
            // No more often than the pictures would be taken.
            self.asked = Some(Instant::now());
            return;
        }

        self.asked = Some(Instant::now());
        let shared = Arc::clone(&self.shared);
        shared.pending.store(1, Ordering::SeqCst);

        let block = RcBlock::new(move |content: *mut SCShareableContent, _: *mut NSError| {
            // SAFETY: the system passes a valid object or null.
            let Some(content) = (unsafe { content.as_ref() }) else {
                debug!("the system did not list the windows that can be captured");
                shared.answered(false);
                return;
            };
            {
                let mut images = shared.images.lock().unwrap();
                let before = images.len();
                images.retain(|id, _| wanted.iter().any(|window| window.id == *id));
                if images.len() != before {
                    shared.version.fetch_add(1, Ordering::SeqCst);
                }
            }

            // SAFETY: `content` is valid, and so are the windows it lists.
            let listed: Vec<Retained<SCWindow>> = unsafe { content.windows() }.to_vec();
            for (window, target) in pair(&wanted, &listed) {
                shared.pending.fetch_add(1, Ordering::SeqCst);
                capture(Arc::clone(&shared), window.id, window.frame, target, scale);
            }
            shared.answered(false);
        });
        // SAFETY: the block takes the two arguments the completion handler is given.
        unsafe {
            SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
                true, false, &block,
            );
        }
    }
}

/// What ScreenCaptureKit says about one window.
struct Listed {
    pid: i32,
    frame: Rect,
    title: Option<String>,
}

/// Finds, for each window in `wanted` that has one, its counterpart in `listed`. No listed
/// window is given to two.
fn pair<'a>(
    wanted: &'a [Wanted],
    listed: &'a [Retained<SCWindow>],
) -> Vec<(&'a Wanted, &'a SCWindow)> {
    // SAFETY: the windows are valid objects, and these only read from them.
    let described: Vec<Option<Listed>> = listed
        .iter()
        .map(|window| unsafe {
            // Layer 0 is where ordinary windows live.
            if window.windowLayer() != 0 {
                return None;
            }
            let frame = window.frame();
            Some(Listed {
                pid: window.owningApplication()?.processID(),
                frame: Rect::new(
                    frame.origin.x,
                    frame.origin.y,
                    frame.size.width,
                    frame.size.height,
                ),
                title: window.title().map(|title| title.to_string()),
            })
        })
        .collect();

    let picks = pair_indices(wanted, &described);
    wanted
        .iter()
        .zip(picks)
        .filter_map(|(window, pick)| Some((window, &*listed[pick?])))
        .collect()
}

/// For each of `wanted`, the index of its counterpart in `listed`, if it has one.
fn pair_indices(wanted: &[Wanted], listed: &[Option<Listed>]) -> Vec<Option<usize>> {
    let close = |a: f64, b: f64| (a - b).abs() <= TOLERANCE;
    let fits = |window: &Wanted, other: &Listed| {
        other.pid == window.pid
            && close(other.frame.x, window.frame.x)
            && close(other.frame.y, window.frame.y)
            && close(other.frame.w, window.frame.w)
            && close(other.frame.h, window.frame.h)
    };

    let mut taken = vec![false; listed.len()];
    let mut picks = vec![None; wanted.len()];
    // Windows of one application that are stacked in one place are told apart by title, so
    // those whose title is found go first and the rest take what is left.
    for by_title in [true, false] {
        for (window, pick) in wanted.iter().zip(&mut picks) {
            if pick.is_some() {
                continue;
            }
            *pick = listed.iter().enumerate().position(|(idx, other)| {
                let Some(other) = other else {
                    return false;
                };
                !taken[idx] && fits(window, other) && (!by_title || other.title == window.title)
            });
            if let Some(idx) = *pick {
                taken[idx] = true;
            }
        }
    }
    picks
}

/// Asks for a picture of `target`, to be kept as that of window `id`.
fn capture(shared: Arc<Shared>, id: WindowId, frame: Rect, target: &SCWindow, scale: f64) {
    let width = (frame.w * scale).round().max(1.) as usize;
    let height = (frame.h * scale).round().max(1.) as usize;

    let block = RcBlock::new(move |image: *mut CGImage, _: *mut NSError| {
        let Some(image) = NonNull::new(image) else {
            trace!(?id, "no picture of the window");
            shared.answered(false);
            return;
        };
        // SAFETY: the system passes a valid image, which is kept past the call by retaining
        // it.
        let image = unsafe { CFRetained::retain(image) };
        shared.images.lock().unwrap().insert(id, image);
        shared.answered(true);
    });

    // SAFETY: `target` is a valid window, the configuration is a new object given plain
    // values, and the block takes the two arguments the completion handler is given.
    unsafe {
        let filter =
            SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), target);
        let config = SCStreamConfiguration::new();
        config.setWidth(width);
        config.setHeight(height);
        config.setShowsCursor(false);
        // The window alone, edge to edge.
        config.setIgnoreShadowsSingleWindow(true);
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &config,
            Some(&block),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wanted(id: u64, pid: i32, x: f64, title: &str) -> Wanted {
        Wanted {
            id: WindowId(id),
            pid,
            frame: Rect::new(x, 50., 800., 600.),
            title: Some(title.to_owned()),
        }
    }

    fn listed(pid: i32, x: f64, title: &str) -> Option<Listed> {
        Some(Listed {
            pid,
            frame: Rect::new(x, 50., 800., 600.),
            title: Some(title.to_owned()),
        })
    }

    #[test]
    fn windows_are_paired_by_process_and_frame() {
        let wanted = [wanted(1, 10, 0., "a"), wanted(2, 20, 0., "b")];
        // The frames are a little off, as the two sources round differently.
        let listed = [listed(20, 1., "b"), None, listed(10, -1., "a")];
        assert_eq!(pair_indices(&wanted, &listed), [Some(2), Some(0)]);
    }

    #[test]
    fn a_window_that_is_not_listed_gets_nothing() {
        let wanted = [wanted(1, 10, 0., "a"), wanted(2, 10, 900., "b")];
        let listed = [listed(10, 0., "a"), listed(11, 900., "b")];
        assert_eq!(pair_indices(&wanted, &listed), [Some(0), None]);
    }

    #[test]
    fn windows_in_one_place_are_told_apart_by_title() {
        let wanted = [wanted(1, 10, 0., "first"), wanted(2, 10, 0., "second")];
        let listed = [listed(10, 0., "second"), listed(10, 0., "first")];
        assert_eq!(pair_indices(&wanted, &listed), [Some(1), Some(0)]);
    }

    #[test]
    fn a_title_that_has_changed_does_not_cost_a_window_its_picture() {
        let wanted = [wanted(1, 10, 0., "old"), wanted(2, 10, 0., "second")];
        let listed = [listed(10, 0., "second"), listed(10, 0., "new")];
        assert_eq!(pair_indices(&wanted, &listed), [Some(1), Some(0)]);
    }
}
