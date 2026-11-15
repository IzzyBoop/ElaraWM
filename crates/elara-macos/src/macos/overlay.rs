//! Drawing on top of the desktop: one transparent, click-through window per display.
//!
//! macOS draws every application's windows itself, and no application can draw into
//! another's. What a window manager can do is put a window of its own on screen. This one
//! covers a whole display, shows nothing but the outlines it is given, and lets every click
//! through to whatever is beneath it.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSNormalWindowLevel,
    NSView, NSWindow, NSWindowCollectionBehavior, NSWindowNumberListOptions, NSWindowStyleMask,
};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    kCGColorSpaceSRGB, CGBitmapContextCreate, CGBitmapContextGetData, CGColor, CGColorSpace,
    CGContext, CGGradient, CGGradientDrawingOptions, CGImageAlphaInfo, CGMutablePath,
};
use objc2_foundation::NSArray;
use objc2_quartz_core::{
    kCAFillRuleEvenOdd, CAGradientLayer, CALayer, CAShapeLayer, CATransaction,
};

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
    shapes: Vec<OutlineLayer>,
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
                .map(|outline| outline.translated(-display.frame.x, -display.frame.y))
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
            self.shapes.pop().unwrap().layer().removeFromSuperlayer();
        }
        while self.shapes.len() < outlines.len() {
            // The container is flipped already.
            let shape = OutlineLayer::new(false);
            self.container.addSublayer(shape.layer());
            self.shapes.push(shape);
        }
        for (idx, (shape, outline)) in self.shapes.iter_mut().zip(&outlines).enumerate() {
            // A shape whose outline did not change stays as it is.
            if self.outlines.get(idx) != Some(outline) {
                shape.set(outline, bounds, display.scale);
            }
        }
        self.outlines = outlines;

        // Whatever was raised since the last change may be covering the outlines.
        self.window.orderFrontRegardless();
    }
}

/// The layers that draw one outline.
///
/// A shape of one color is a shape layer. A gradient is a gradient layer the size of the
/// display, of which a second shape layer lets only the shape show.
pub(super) struct OutlineLayer {
    /// Holds the others, so that the outline keeps its place among its siblings whichever
    /// of them it uses.
    layer: Retained<CALayer>,
    solid: Retained<CAShapeLayer>,
    gradient: Option<(Retained<CAGradientLayer>, Retained<CAShapeLayer>)>,
}

impl OutlineLayer {
    /// `flipped` says whether to lay the shape out from the top left; pass `false` where a
    /// layer above this one does that already.
    pub(super) fn new(flipped: bool) -> Self {
        let layer = CALayer::new();
        layer.setGeometryFlipped(flipped);
        let solid = even_odd_shape();
        layer.addSublayer(&solid);
        Self {
            layer,
            solid,
            gradient: None,
        }
    }

    /// The layer to put in the tree.
    pub(super) fn layer(&self) -> &CALayer {
        &self.layer
    }

    /// Draws `outline`, which is relative to the top left of `bounds`, the area of the
    /// display.
    pub(super) fn set(&mut self, outline: &Outline, bounds: CGRect, scale: f64) {
        self.layer.setFrame(bounds);
        self.solid.setFrame(bounds);
        self.solid.setContentsScale(scale);
        let path = path(outline);

        let Some(gradient) = outline.gradient else {
            self.solid.setPath(Some(&path));
            self.solid.setFillColor(Some(&cg_color(outline.color)));
            if let Some((layer, _)) = &self.gradient {
                layer.setHidden(true);
            }
            return;
        };

        self.solid.setPath(None);
        let (layer, mask) = self.gradient.get_or_insert_with(|| {
            let layer = CAGradientLayer::new();
            let mask = even_odd_shape();
            // What shows of the gradient is what the mask covers, in any opaque color.
            mask.setFillColor(Some(&cg_color([0., 0., 0., 1.])));
            // SAFETY: the mask is a new layer that has no parent, as a mask must be.
            unsafe { layer.setMask(Some(&mask)) };
            self.layer.addSublayer(&layer);
            (layer, mask)
        });
        layer.setHidden(false);
        layer.setFrame(bounds);
        mask.setFrame(bounds);
        mask.setContentsScale(scale);
        mask.setPath(Some(&path));

        let colors: Vec<CFRetained<CGColor>> = gradient
            .stops(outline.color)
            .into_iter()
            .map(cg_color)
            .collect();
        // SAFETY: Core Foundation objects are Objective-C objects, and colors are what a
        // gradient layer takes. The array retains them.
        unsafe {
            let objects: Vec<&AnyObject> = colors
                .iter()
                .map(|color| &*CFRetained::as_ptr(color).as_ptr().cast::<AnyObject>())
                .collect();
            layer.setColors(Some(&NSArray::from_slice(&objects)));
        }
        // With no locations given the colors are spaced evenly, as the stops are. The ends
        // are given as fractions of the layer's size, and may lie outside it.
        let (start, end) = gradient.line();
        let unit = |point: crate::Point| {
            CGPoint::new(
                point.x / bounds.size.width.max(1.),
                point.y / bounds.size.height.max(1.),
            )
        };
        layer.setStartPoint(unit(start));
        layer.setEndPoint(unit(end));
    }
}

fn even_odd_shape() -> Retained<CAShapeLayer> {
    let shape = CAShapeLayer::new();
    // SAFETY: the fill rule is a valid static.
    shape.setFillRule(unsafe { kCAFillRuleEvenOdd });
    shape
}

fn cg_color(color: [f32; 4]) -> CFRetained<CGColor> {
    let [r, g, b, a] = color;
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
        CGContext::add_path(Some(&context), Some(&path(outline)));
        let Some(gradient) = outline.gradient else {
            CGContext::set_fill_color_with_color(Some(&context), Some(&cg_color(outline.color)));
            CGContext::eo_fill_path(Some(&context));
            continue;
        };

        let components: Vec<f64> = gradient
            .stops(outline.color)
            .into_iter()
            .flatten()
            .map(f64::from)
            .collect();
        // SAFETY: the name is a valid static; there are four components per color, and no
        // locations means evenly spaced.
        let fill = unsafe {
            let srgb = CGColorSpace::with_name(Some(kCGColorSpaceSRGB));
            CGGradient::with_color_components(
                srgb.as_deref(),
                components.as_ptr(),
                std::ptr::null(),
                components.len() / 4,
            )
        };
        let (start, end) = gradient.line();
        // Only inside the shape, and all of it: the first and last color go on past the ends.
        CGContext::save_g_state(Some(&context));
        CGContext::eo_clip(Some(&context));
        CGContext::draw_linear_gradient(
            Some(&context),
            fill.as_deref(),
            CGPoint::new(start.x, start.y),
            CGPoint::new(end.x, end.y),
            CGGradientDrawingOptions::DrawsBeforeStartLocation
                | CGGradientDrawingOptions::DrawsAfterEndLocation,
        );
        CGContext::restore_g_state(Some(&context));
    }

    drop(context);
    (width, height, pixels)
}
