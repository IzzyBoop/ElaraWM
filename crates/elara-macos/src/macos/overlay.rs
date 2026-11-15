//! Drawing on top of the desktop: one transparent, click-through window per display.
//!
//! macOS draws every application's windows itself, and no application can draw into
//! another's. What a window manager can do is put a window of its own on screen. This one
//! covers a whole display, shows nothing but the outlines it is given, and lets every click
//! through to whatever is beneath it.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSNormalWindowLevel,
    NSView, NSWindow, NSWindowCollectionBehavior, NSWindowNumberListOptions, NSWindowStyleMask,
};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextGetData, CGColor, CGColorSpace, CGContext,
    CGImageAlphaInfo, CGMutablePath,
};
use objc2_quartz_core::{kCAFillRuleEvenOdd, CALayer, CAShapeLayer, CATransaction};

use crate::{DisplayId, DisplayInfo, Outline, Rect};

/// The overlay window of one display.
struct Surface {
    display: DisplayId,
    /// The display's frame the window was last given.
    frame: Rect,
    window: Retained<NSWindow>,
    /// Holds one sublayer per outline. Flipped, so that its sublayers are laid out from the
    /// top left like everything else in this crate.
    container: Retained<CALayer>,
    shapes: Vec<Retained<CAShapeLayer>>,
    /// What is drawn now, in the container's coordinates.
    outlines: Vec<Outline>,
}

/// The overlay windows, one per display.
pub struct Overlay {
    mtm: MainThreadMarker,
    surfaces: Vec<Surface>,
}

impl Overlay {
    /// Prepares to draw. No window is made until there is something to show.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the main thread.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mtm = MainThreadMarker::new().expect("the overlay must be created on the main thread");
        // A process needs an application object to own windows. As an accessory it gets no
        // Dock icon and no menu bar, and never becomes the active application.
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        Self {
            mtm,
            surfaces: Vec::new(),
        }
    }

    /// Shows exactly `outlines`, which are in global coordinates, on `displays`.
    pub fn set(&mut self, displays: &[DisplayInfo], outlines: &[Outline]) {
        // The overlay of a display that is gone goes with it.
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
        // Outlines follow windows, which do not ease into place.
        CATransaction::setDisableActions(true);

        for display in displays {
            let local: Vec<Outline> = outlines
                .iter()
                .filter(|outline| outline.rect.intersects(&display.frame))
                .map(|outline| Outline {
                    rect: Rect::new(
                        outline.rect.x - display.frame.x,
                        outline.rect.y - display.frame.y,
                        outline.rect.w,
                        outline.rect.h,
                    ),
                    ..*outline
                })
                .collect();

            let idx = self
                .surfaces
                .iter()
                .position(|surface| surface.display == display.id);
            let surface = match idx {
                Some(idx) => &mut self.surfaces[idx],
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

impl Overlay {
    /// Returns the frame of every overlay window that the window server lists as on screen.
    /// For checking that drawing works.
    pub fn on_screen_frames(&self) -> Vec<Rect> {
        // The window server's list of this application's windows that are on screen, on the
        // active Space. AppKit's own idea of whether a window is showing is only brought up
        // to date by its event loop, which this process does not run.
        let listed: Vec<isize> =
            NSWindow::windowNumbersWithOptions(NSWindowNumberListOptions(0), self.mtm)
                .map(|numbers| numbers.iter().map(|n| n.integerValue()).collect())
                .unwrap_or_default();

        self.surfaces
            .iter()
            .filter(|surface| listed.contains(&surface.window.windowNumber()))
            .map(|surface| surface.frame)
            .collect()
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        for surface in &self.surfaces {
            surface.window.close();
        }
    }
}

pub(super) fn appkit_rect(frame: Rect, primary_height: f64) -> CGRect {
    CGRect::new(
        CGPoint::new(frame.x, primary_height - frame.y - frame.h),
        CGSize::new(frame.w, frame.h),
    )
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
        // Among ordinary windows, and brought to the front whenever what it shows changes.
        // Above them all the time, it would also cover windows that are not managed.
        window.setLevel(NSNormalWindowLevel);
        // Not a window to switch to. And transient, which is what panels are: Mission Control
        // hides such windows while it is open. The outlines belong to windows at their full
        // size, and Mission Control shrinks those and moves them about. A stationary window
        // would stay behind, showing rings around windows that are no longer there; an
        // ordinary one would be shown as a tile of its own.
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::Transient | NSWindowCollectionBehavior::IgnoresCycle,
        );

        let bounds = CGRect::new(CGPoint::new(0., 0.), rect.size);
        let root = CALayer::new();
        root.setFrame(bounds);
        let container = CALayer::new();
        container.setFrame(bounds);
        container.setGeometryFlipped(true);
        root.addSublayer(&container);

        // Giving the view its layer before asking it to use one makes the layer ours to
        // arrange, instead of AppKit's.
        let view = NSView::initWithFrame(NSView::alloc(mtm), bounds);
        view.setLayer(Some(&root));
        view.setWantsLayer(true);
        window.setContentView(Some(&view));
        // The window stays on screen even with nothing to show. It is on the Space that was
        // active when it was made and stays there, so whether it is on screen says whether
        // the user is looking at that Space.
        window.orderFrontRegardless();

        Self {
            display: display.id,
            frame: display.frame,
            window,
            container,
            shapes: Vec::new(),
            outlines: Vec::new(),
        }
    }

    fn update(&mut self, display: &DisplayInfo, primary_height: f64, outlines: Vec<Outline>) {
        if self.frame != display.frame {
            self.frame = display.frame;
            let rect = appkit_rect(display.frame, primary_height);
            self.window.setFrame_display(rect, false);
            let bounds = CGRect::new(CGPoint::new(0., 0.), rect.size);
            if let Some(root) = self.container.superlayer() {
                root.setFrame(bounds);
            }
            self.container.setFrame(bounds);
            // Shapes are sized to the container; make them all again.
            self.outlines.clear();
        }

        if self.outlines == outlines {
            return;
        }

        let bounds = CGRect::new(
            CGPoint::new(0., 0.),
            CGSize::new(display.frame.w, display.frame.h),
        );
        while self.shapes.len() > outlines.len() {
            self.shapes.pop().unwrap().removeFromSuperlayer();
        }
        while self.shapes.len() < outlines.len() {
            let shape = CAShapeLayer::new();
            // SAFETY: the fill rule is a valid static.
            shape.setFillRule(unsafe { kCAFillRuleEvenOdd });
            self.container.addSublayer(&shape);
            self.shapes.push(shape);
        }
        for (idx, (shape, outline)) in self.shapes.iter().zip(&outlines).enumerate() {
            shape.setFrame(bounds);
            shape.setContentsScale(display.scale);
            // A shape whose outline did not change keeps its path.
            if self.outlines.get(idx) != Some(outline) {
                shape.setPath(Some(&path(outline)));
                shape.setFillColor(Some(&color(outline)));
            }
        }
        self.outlines = outlines;

        // Whatever was raised since the last change may be covering the outlines.
        self.window.orderFrontRegardless();
    }
}

pub(super) fn color(outline: &Outline) -> CFRetained<CGColor> {
    let [r, g, b, a] = outline.color;
    CGColor::new_srgb(r.into(), g.into(), b.into(), a.into())
}

/// Adds a rectangle with rounded corners to `path`. `radii` are for the top-left, top-right,
/// bottom-right and bottom-left corners.
fn add_rounded_rect(path: &CGMutablePath, rect: Rect, radii: [f64; 4]) {
    if rect.w <= 0. || rect.h <= 0. {
        return;
    }
    // A corner cannot be rounder than half the shorter side.
    let limit = rect.w.min(rect.h) / 2.;
    let [tl, tr, br, bl] = radii.map(|radius| radius.clamp(0., limit));
    let (x0, y0, x1, y1) = (rect.x, rect.y, rect.max_x(), rect.max_y());
    let none = std::ptr::null();

    // SAFETY: `path` is a valid mutable path and a null transform means none.
    unsafe {
        CGMutablePath::move_to_point(Some(path), none, x0 + tl, y0);
        CGMutablePath::add_arc_to_point(Some(path), none, x1, y0, x1, y1, tr);
        CGMutablePath::add_arc_to_point(Some(path), none, x1, y1, x0, y1, br);
        CGMutablePath::add_arc_to_point(Some(path), none, x0, y1, x0, y0, bl);
        CGMutablePath::add_arc_to_point(Some(path), none, x0, y0, x1, y0, tl);
    }
    CGMutablePath::close_subpath(Some(path));
}

/// The area an outline covers, to be filled with the even-odd rule: a rounded rectangle, with
/// a smaller one cut out of it if the outline is a line and not a filled shape.
pub(super) fn path(outline: &Outline) -> CFRetained<CGMutablePath> {
    let path = CGMutablePath::new();
    add_rounded_rect(&path, outline.rect, outline.corner_radius);

    let width = outline.line_width;
    if width > 0. {
        let inner = Rect::new(
            outline.rect.x + width,
            outline.rect.y + width,
            outline.rect.w - 2. * width,
            outline.rect.h - 2. * width,
        );
        // The inside of a line follows its outside, one line width in.
        let radii = outline.corner_radius.map(|radius| (radius - width).max(0.));
        add_rounded_rect(&path, inner, radii);
    }
    path
}

/// Draws `outlines` into an image of `size` points at `scale` pixels per point, without
/// putting anything on screen. `outlines` are relative to the top left of the image.
///
/// Returns the image's width and height in pixels and its pixels, top row first, four bytes
/// each: red, green, blue and alpha, with the colors multiplied by the alpha. For checking
/// what the overlay would show.
pub fn rasterize(outlines: &[Outline], size: crate::Size, scale: f64) -> (usize, usize, Vec<u8>) {
    let width = (size.w * scale).round().max(1.) as usize;
    let height = (size.h * scale).round().max(1.) as usize;
    let mut pixels = vec![0u8; width * height * 4];

    let space = CGColorSpace::new_device_rgb().expect("the device RGB color space exists");
    // SAFETY: the buffer is `height` rows of `width * 4` bytes, as described to the context,
    // and outlives it: the context is dropped at the end of this function, before `pixels`
    // is returned.
    let context: CFRetained<CGContext> = unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast::<c_void>(),
            width,
            height,
            8,
            width * 4,
            Some(&space),
            CGImageAlphaInfo::PremultipliedLast.0,
        )
    }
    .expect("a plain RGBA bitmap context can be created");
    debug_assert!(!CGBitmapContextGetData(Some(&context)).is_null());

    // Core Graphics counts rows from the bottom; outlines count from the top.
    CGContext::translate_ctm(Some(&context), 0., height as f64);
    CGContext::scale_ctm(Some(&context), scale, -scale);

    for outline in outlines {
        CGContext::set_fill_color_with_color(Some(&context), Some(&color(outline)));
        CGContext::add_path(Some(&context), Some(&path(outline)));
        CGContext::eo_fill_path(Some(&context));
    }

    drop(context);
    (width, height, pixels)
}
