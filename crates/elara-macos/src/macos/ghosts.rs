//! Pictures of windows, drawn where the windows themselves cannot be.
//!
//! macOS keeps every window below the menu bar. A workspace that slides in from above, or
//! out at the top, has windows whose place is partly above it for most of the way. Those are
//! kept out of sight, and a picture of each is drawn here in its place, cut off at the menu
//! bar as the window would be if it could go under it.
//!
//! Like the outlines, the pictures go in a transparent, click-through window that covers a
//! display. It is on screen only while there is a picture to show.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSNormalWindowLevel,
    NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGImage;
use objc2_quartz_core::{CALayer, CATransaction};

use super::overlay::appkit_rect;
use crate::{DisplayId, DisplayInfo, Ghost, Rect, WindowId};

/// One picture: a layer that cuts off what is outside it, and the picture inside.
struct Picture {
    clip: Retained<CALayer>,
    image: Retained<CALayer>,
    /// What the image layer shows.
    shown: Option<CFRetained<CGImage>>,
}

/// The window of one display.
struct Surface {
    display: DisplayId,
    /// The display's frame the window was last given.
    frame: Rect,
    window: Retained<NSWindow>,
    root: Retained<CALayer>,
    pictures: Vec<Picture>,
    on_screen: bool,
}

/// The windows the pictures are drawn in, one per display.
pub struct GhostPanels {
    mtm: MainThreadMarker,
    surfaces: Vec<Surface>,
}

impl GhostPanels {
    /// Prepares to draw. No window is made until there is something to show.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the main thread.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mtm = MainThreadMarker::new().expect("pictures must be drawn on the main thread");
        // As for the outlines: a process needs an application object to own windows.
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        Self {
            mtm,
            surfaces: Vec::new(),
        }
    }

    /// Shows exactly `ghosts`, which are in global coordinates, on `displays`. `picture`
    /// gives the picture of a window; a window it has none of is not shown.
    pub fn set(
        &mut self,
        displays: &[DisplayInfo],
        ghosts: &[Ghost],
        picture: &dyn Fn(WindowId) -> Option<CFRetained<CGImage>>,
    ) {
        self.surfaces.retain(|surface| {
            let keep = displays.iter().any(|d| d.id == surface.display);
            if !keep {
                surface.window.close();
            }
            keep
        });

        // AppKit measures from the bottom of the primary display.
        let Some(primary_height) = displays.first().map(|d| d.frame.h) else {
            return;
        };

        CATransaction::begin();
        // The pictures follow the layout frame by frame, and do not ease into place.
        CATransaction::setDisableActions(true);

        for display in displays {
            let local: Vec<(Ghost, CFRetained<CGImage>)> = ghosts
                .iter()
                .filter(|ghost| ghost.clip.intersects(&display.frame))
                .filter_map(|ghost| Some((*ghost, picture(ghost.window)?)))
                .collect();

            let idx = self
                .surfaces
                .iter()
                .position(|surface| surface.display == display.id);
            let surface = match idx {
                Some(idx) => &mut self.surfaces[idx],
                None if local.is_empty() => continue,
                None => {
                    self.surfaces
                        .push(Surface::new(self.mtm, display, primary_height));
                    self.surfaces.last_mut().unwrap()
                }
            };
            surface.update(display, primary_height, local);
        }

        CATransaction::commit();
        // The process runs a bare run loop, not AppKit's, so do not wait for one to notice.
        CATransaction::flush();
    }
}

impl Drop for GhostPanels {
    fn drop(&mut self) {
        for surface in &self.surfaces {
            surface.window.close();
        }
    }
}

impl Surface {
    fn new(mtm: MainThreadMarker, display: &DisplayInfo, primary_height: f64) -> Self {
        let rect = appkit_rect(display.frame, primary_height);
        // SAFETY: the arguments are a valid rectangle and valid constants. The window is
        // told below not to release itself when closed, so the reference held here stays
        // the owning one.
        let window = unsafe {
            let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                rect,
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            );
            window.setReleasedWhenClosed(false);
            window
        };
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setHasShadow(false);
        window.setIgnoresMouseEvents(true);
        // Among ordinary windows, as the windows the pictures stand for are.
        window.setLevel(NSNormalWindowLevel);
        // As for the window that shows outlines: hidden by Mission Control, never switched to.
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::Transient | NSWindowCollectionBehavior::IgnoresCycle,
        );

        let bounds = CGRect::new(CGPoint::new(0., 0.), rect.size);
        let root = CALayer::new();
        root.setFrame(bounds);
        // Giving the view its layer before asking it to use one makes the layer ours to
        // arrange, instead of AppKit's.
        let view = NSView::initWithFrame(NSView::alloc(mtm), bounds);
        view.setLayer(Some(&root));
        view.setWantsLayer(true);
        window.setContentView(Some(&view));

        Self {
            display: display.id,
            frame: display.frame,
            window,
            root,
            pictures: Vec::new(),
            on_screen: false,
        }
    }

    fn update(
        &mut self,
        display: &DisplayInfo,
        primary_height: f64,
        ghosts: Vec<(Ghost, CFRetained<CGImage>)>,
    ) {
        if self.frame != display.frame {
            self.frame = display.frame;
            let rect = appkit_rect(display.frame, primary_height);
            self.window.setFrame_display(rect, false);
            self.root
                .setFrame(CGRect::new(CGPoint::new(0., 0.), rect.size));
        }

        while self.pictures.len() > ghosts.len() {
            self.pictures.pop().unwrap().clip.removeFromSuperlayer();
        }
        while self.pictures.len() < ghosts.len() {
            let clip = CALayer::new();
            clip.setMasksToBounds(true);
            let image = CALayer::new();
            clip.addSublayer(&image);
            self.root.addSublayer(&clip);
            self.pictures.push(Picture {
                clip,
                image,
                shown: None,
            });
        }

        for (picture, (ghost, image)) in self.pictures.iter_mut().zip(ghosts) {
            // Layers are laid out from the bottom left of their parent; everything else in
            // this crate counts from the top left.
            let clip = ghost.clip;
            picture.clip.setFrame(CGRect::new(
                CGPoint::new(
                    clip.x - display.frame.x,
                    display.frame.max_y() - clip.max_y(),
                ),
                CGSize::new(clip.w, clip.h),
            ));
            let rect = ghost.rect;
            picture.image.setFrame(CGRect::new(
                CGPoint::new(rect.x - clip.x, clip.max_y() - rect.max_y()),
                CGSize::new(rect.w, rect.h),
            ));
            if picture.shown.as_ref() != Some(&image) {
                // SAFETY: an image is one of the kinds of object a layer accepts as its
                // contents, and Core Foundation objects are Objective-C objects.
                unsafe {
                    let object = CFRetained::as_ptr(&image).as_ptr().cast::<AnyObject>();
                    picture.image.setContents(Some(&*object));
                }
                picture.shown = Some(image);
            }
        }

        let wanted = !self.pictures.is_empty();
        if wanted && !self.on_screen {
            self.window.orderFrontRegardless();
        } else if !wanted && self.on_screen {
            self.window.orderOut(None);
        }
        self.on_screen = wanted;
    }
}
