//! Showing the list of important key bindings: a small window in the middle of each display,
//! on top of everything else.
//!
//! niri draws the list into one image and shows that. So does this: the text is drawn with
//! AppKit into a bitmap, once per display scale, and each display gets a window just large
//! enough to hold it. The window lets every click through.
//!
//! What the list says is niri's. How it looks is not: niri's is a plain box, and here it has
//! the round corners of a window and the focus ring's line around it.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAttributedStringNSStringDrawing, NSBackgroundColorAttributeName, NSBackingStoreType, NSColor,
    NSFont, NSFontAttributeName, NSFontManager, NSFontTraitMask, NSFontWeightBold,
    NSFontWeightRegular, NSForegroundColorAttributeName, NSGraphicsContext, NSPanel, NSView,
    NSWindowCollectionBehavior, NSWindowLevel, NSWindowStyleMask,
};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGBitmapContextGetData, CGColorSpace,
    CGContext, CGImage, CGImageAlphaInfo, CGMutablePath,
};
use objc2_foundation::{NSMutableAttributedString, NSRange, NSString};
use objc2_quartz_core::{CALayer, CATransaction};

use super::overlay::{add_rounded_rect, appkit_rect, cg_color, fill};
use crate::{
    DisplayId, DisplayInfo, Gradient, HotkeyOverlay, HotkeyOverlayFrame, Outline, Rect, TextSpan,
};

// niri draws its list as a plain box. Here it is drawn to go with the windows around it:
// corners as round as theirs, and a line around it like the ring around the focused window.
// The measures are in points.
/// Between the line around the list and what is in it.
const PADDING: f64 = 18.;
const FONT_SIZE: f64 = 14.;
/// Between the title and the first binding.
const TITLE_GAP: f64 = 14.;
/// Between one binding and the next.
const LINE_INTERVAL: f64 = 6.;
/// Between the keys of a binding and what they do.
const COLUMN_GAP: f64 = 14.;
/// Between the keys of a binding and the edge of the cap they are drawn on.
const KEY_PADDING: (f64, f64) = (8., 3.);
const KEY_RADIUS: f64 = 7.;
const OPACITY: f64 = 0.85;
/// #25273A.
const BACKGROUND: [f32; 4] = [37. / 255., 39. / 255., 58. / 255., 1.];
const TEXT_COLOR: [f32; 4] = [1., 1., 1., 1.];
/// The cap behind the keys of a binding.
/// A lighter shade of the background, #363A4F.
const KEY_BACKGROUND: [f32; 4] = [54. / 255., 58. / 255., 79. / 255., 1.];

/// Where the list is among the windows on screen: over everything, the menu bar and the
/// overview included, as niri draws it last.
const LEVEL: NSWindowLevel = 25;

fn ns_color(color: [f32; 4]) -> Retained<NSColor> {
    let [r, g, b, a] = color;
    NSColor::colorWithSRGBRed_green_blue_alpha(r.into(), g.into(), b.into(), a.into())
}

fn font(mtm: MainThreadMarker, span: &TextSpan) -> Retained<NSFont> {
    let font = if span.monospace {
        // SAFETY: the weights are valid statics.
        let weight = unsafe {
            if span.bold {
                NSFontWeightBold
            } else {
                NSFontWeightRegular
            }
        };
        NSFont::monospacedSystemFontOfSize_weight(FONT_SIZE, weight)
    } else if span.bold {
        NSFont::boldSystemFontOfSize(FONT_SIZE)
    } else {
        NSFont::systemFontOfSize(FONT_SIZE)
    };
    if span.italic {
        NSFontManager::sharedFontManager(mtm)
            .convertFont_toHaveTrait(&font, NSFontTraitMask::ItalicFontMask)
    } else {
        font
    }
}

/// Makes text that AppKit can measure and draw out of `spans`.
fn attributed(mtm: MainThreadMarker, spans: &[TextSpan]) -> Retained<NSMutableAttributedString> {
    let whole: String = spans.iter().map(|span| span.text.as_str()).collect();
    let text = NSMutableAttributedString::from_nsstring(&NSString::from_str(&whole));

    // Positions in the text count UTF-16 code units.
    let mut location = 0;
    for span in spans {
        let length = span.text.encode_utf16().count();
        let range = NSRange::new(location, length);
        location += length;
        if length == 0 {
            continue;
        }

        let font = font(mtm, span);
        let color = ns_color(span.color.unwrap_or(TEXT_COLOR));
        // SAFETY: the range is within the text, the keys are valid statics, and a font and
        // colors are what these keys take.
        unsafe {
            text.addAttribute_value_range(NSFontAttributeName, &font, range);
            text.addAttribute_value_range(NSForegroundColorAttributeName, &color, range);
            if let Some(background) = span.background {
                let background = ns_color(background);
                text.addAttribute_value_range(NSBackgroundColorAttributeName, &background, range);
            }
        }
    }
    text
}

/// The list with everything measured and given its place.
struct Laid {
    /// How large the whole list is, in points.
    size: CGSize,
    frame: HotkeyOverlayFrame,
    /// The caps the keys are drawn on.
    caps: Vec<Rect>,
    /// Each piece of text and where its top left corner goes.
    texts: Vec<(Retained<NSMutableAttributedString>, CGPoint)>,
}

impl Laid {
    /// Arranges `overlay` as niri does: the title centered at the top, then one line per
    /// binding, with the actions starting where the longest keys end.
    fn new(mtm: MainThreadMarker, overlay: &HotkeyOverlay) -> Self {
        let frame = overlay.frame;
        let inset = frame.width.max(0.) + PADDING;

        let title = attributed(
            mtm,
            &[TextSpan {
                text: overlay.title.clone(),
                bold: true,
                ..Default::default()
            }],
        );
        let title_size = title.size();

        let rows: Vec<_> = overlay
            .rows
            .iter()
            .map(|row| {
                let key = attributed(
                    mtm,
                    &[TextSpan {
                        text: row.key.clone(),
                        monospace: true,
                        ..Default::default()
                    }],
                );
                let action = attributed(mtm, &row.action);
                let (key_size, action_size) = (key.size(), action.size());
                (key, key_size, action, action_size)
            })
            .collect();

        let widest = |sizes: &mut dyn Iterator<Item = f64>| sizes.fold(0., f64::max).ceil();
        let key_width =
            widest(&mut rows.iter().map(|(_, size, _, _)| size.width)) + KEY_PADDING.0 * 2.;
        let action_width = widest(&mut rows.iter().map(|(_, _, _, size)| size.width));
        let width =
            (key_width + COLUMN_GAP + action_width).max(title_size.width.ceil()) + inset * 2.;

        let mut caps = Vec::with_capacity(rows.len());
        let mut texts = Vec::with_capacity(1 + rows.len() * 2);
        texts.push((
            title,
            CGPoint::new(((width - title_size.width) / 2.).floor(), inset),
        ));

        let mut y = inset + title_size.height.ceil() + TITLE_GAP;
        for (idx, (key, key_size, action, action_size)) in rows.into_iter().enumerate() {
            if idx > 0 {
                y += LINE_INTERVAL;
            }
            let height = key_size.height.max(action_size.height).ceil() + KEY_PADDING.1 * 2.;
            // Both pieces of text in the middle of the line, on whole points.
            let middle = |text_height: f64| y + ((height - text_height) / 2.).floor();

            let cap_width = key_size.width.ceil() + KEY_PADDING.0 * 2.;
            caps.push(Rect::new(inset, y, cap_width, height));
            texts.push((
                key,
                CGPoint::new(inset + KEY_PADDING.0, middle(key_size.height)),
            ));
            texts.push((
                action,
                CGPoint::new(inset + key_width + COLUMN_GAP, middle(action_size.height)),
            ));
            y += height;
        }
        let height = y + inset;

        Self {
            size: CGSize::new(width, height),
            frame,
            caps,
            texts,
        }
    }

    /// Draws the list at `scale` pixels per point. Returns the bitmap it was drawn into,
    /// which has the top row first, and its width and height in pixels.
    fn draw(&self, scale: f64) -> (CFRetained<CGContext>, usize, usize) {
        let width = (self.size.width * scale).round().max(1.) as usize;
        let height = (self.size.height * scale).round().max(1.) as usize;

        let space = CGColorSpace::new_device_rgb().expect("the device RGB color space exists");
        // SAFETY: with no buffer given, the context allocates one of the size described.
        let context: CFRetained<CGContext> = unsafe {
            CGBitmapContextCreate(
                std::ptr::null_mut::<c_void>(),
                width,
                height,
                8,
                width * 4,
                Some(&space),
                CGImageAlphaInfo::PremultipliedLast.0,
            )
        }
        .expect("a plain RGBA bitmap context can be created");

        // Core Graphics counts rows from the bottom; the list is laid out from the top.
        CGContext::translate_ctm(Some(&context), 0., height as f64);
        CGContext::scale_ctm(Some(&context), scale, -scale);

        let bounds = Rect::new(0., 0., self.size.width, self.size.height);
        let filled = |rect: Rect, radius: f64, color: [f32; 4]| {
            let path = CGMutablePath::new();
            add_rounded_rect(&path, rect, [radius; 4]);
            CGContext::add_path(Some(&context), Some(&path));
            CGContext::set_fill_color_with_color(Some(&context), Some(&cg_color(color)));
            CGContext::fill_path(Some(&context));
        };
        let radius = self.frame.corner_radius.max(0.);
        filled(bounds, radius, BACKGROUND);
        for cap in &self.caps {
            filled(*cap, KEY_RADIUS, KEY_BACKGROUND);
        }

        // Text is drawn through AppKit, which draws into whichever context is current. It
        // is told the context is flipped, so that the text comes out the right way up.
        let graphics = NSGraphicsContext::graphicsContextWithCGContext_flipped(&context, true);
        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&graphics));
        for (text, at) in &self.texts {
            text.drawAtPoint(*at);
        }
        NSGraphicsContext::restoreGraphicsState_class();

        // The line around it all, drawn as the ring around a window is.
        if self.frame.width > 0. {
            let line = Outline {
                rect: bounds,
                line_width: self.frame.width,
                corner_radius: [radius; 4],
                color: self.frame.color,
                gradient: self.frame.gradient.map(|gradient| Gradient {
                    area: bounds,
                    ..gradient
                }),
            };
            fill(&context, &line);
        }

        (context, width, height)
    }
}

/// The window that shows the list on one display.
struct Surface {
    display: DisplayId,
    window: Retained<NSPanel>,
    layer: Retained<CALayer>,
}

impl Surface {
    fn new(mtm: MainThreadMarker, display: DisplayId) -> Self {
        let rect = CGRect::new(CGPoint::new(0., 0.), CGSize::new(1., 1.));
        let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
        // SAFETY: the arguments are a valid rectangle and valid constants. The panel is told
        // below not to release itself when closed, so the reference held here stays the
        // owning one.
        let window = unsafe {
            let window = NSPanel::initWithContentRect_styleMask_backing_defer(
                NSPanel::alloc(mtm),
                rect,
                style,
                NSBackingStoreType::Buffered,
                false,
            );
            window.setReleasedWhenClosed(false);
            window
        };
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setHasShadow(false);
        window.setAlphaValue(OPACITY);
        // Something to read, not something to click.
        window.setIgnoresMouseEvents(true);
        // Panels go away when their application is not the active one, which this one
        // never is.
        window.setHidesOnDeactivate(false);
        window.setLevel(LEVEL);
        // As for the window that shows outlines: hidden by Mission Control, never switched to.
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::Transient | NSWindowCollectionBehavior::IgnoresCycle,
        );

        let layer = CALayer::new();
        layer.setFrame(rect);
        // Giving the view its layer before asking it to use one makes the layer ours to
        // arrange, instead of AppKit's.
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect);
        view.setLayer(Some(&layer));
        view.setWantsLayer(true);
        window.setContentView(Some(&view));

        Self {
            display,
            window,
            layer,
        }
    }
}

/// Where the list goes on a display: in the middle, or against the top left if it is larger
/// than the display.
fn centered(display: &DisplayInfo, size: CGSize) -> Rect {
    let frame = display.frame;
    // On whole pixels, so that the text stays sharp.
    let snap = |offset: f64| (offset.max(0.) * display.scale).round() / display.scale;
    Rect::new(
        frame.x + snap((frame.w - size.width) / 2.),
        frame.y + snap((frame.h - size.height) / 2.),
        size.width,
        size.height,
    )
}

/// The windows that show the list of key bindings, one per display. They exist from the
/// first time the list is shown, and are on screen only while it is.
pub struct HotkeyOverlayPanels {
    mtm: MainThreadMarker,
    surfaces: Vec<Surface>,
    shown: bool,
}

impl HotkeyOverlayPanels {
    /// Prepares to show the list. No window is made until then.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the main thread.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mtm = MainThreadMarker::new().expect("the hotkey overlay is shown on the main thread");
        Self {
            mtm,
            surfaces: Vec::new(),
            shown: false,
        }
    }

    /// Shows `overlay` in the middle of each of `displays`, or nothing.
    pub fn set(&mut self, displays: &[DisplayInfo], overlay: Option<&HotkeyOverlay>) {
        // The window of a display that is gone goes with it.
        self.surfaces.retain(|surface| {
            let keep = displays.iter().any(|d| d.id == surface.display);
            if !keep {
                surface.window.close();
            }
            keep
        });

        let Some(overlay) = overlay else {
            if self.shown {
                self.shown = false;
                for surface in &self.surfaces {
                    surface.window.orderOut(None);
                    // SAFETY: taking the contents away.
                    unsafe { surface.layer.setContents(None) };
                }
                CATransaction::flush();
            }
            return;
        };

        // AppKit measures from the bottom of the primary display.
        let Some(primary_height) = displays.first().map(|d| d.frame.h) else {
            return;
        };

        let laid = Laid::new(self.mtm, overlay);
        // One image per scale, shared by the displays that have it.
        let mut images: Vec<(f64, Option<CFRetained<CGImage>>)> = Vec::new();

        CATransaction::begin();
        CATransaction::setDisableActions(true);
        for display in displays {
            let image = match images.iter().find(|(scale, _)| *scale == display.scale) {
                Some((_, image)) => image.clone(),
                None => {
                    let (context, _, _) = laid.draw(display.scale);
                    let image = CGBitmapContextCreateImage(Some(&context));
                    images.push((display.scale, image.clone()));
                    image
                }
            };

            let idx = self
                .surfaces
                .iter()
                .position(|surface| surface.display == display.id);
            let surface = match idx {
                Some(idx) => &self.surfaces[idx],
                None => {
                    self.surfaces.push(Surface::new(self.mtm, display.id));
                    self.surfaces.last().unwrap()
                }
            };

            let rect = appkit_rect(centered(display, laid.size), primary_height);
            surface.window.setFrame_display(rect, false);
            surface
                .layer
                .setFrame(CGRect::new(CGPoint::new(0., 0.), rect.size));
            surface.layer.setContentsScale(display.scale);
            // SAFETY: an image is one of the kinds of object a layer accepts as its
            // contents, and Core Foundation objects are Objective-C objects.
            unsafe {
                let image = image
                    .as_ref()
                    .map(|image| &*CFRetained::as_ptr(image).as_ptr().cast::<AnyObject>());
                surface.layer.setContents(image);
            }
            surface.window.orderFrontRegardless();
        }
        self.shown = true;
        CATransaction::commit();
        // The process runs a bare run loop, not AppKit's, so do not wait for one to notice.
        CATransaction::flush();
    }
}

impl Drop for HotkeyOverlayPanels {
    fn drop(&mut self) {
        for surface in &self.surfaces {
            surface.window.close();
        }
    }
}

/// Draws `overlay` into an image at `scale` pixels per point, without putting anything on
/// screen.
///
/// Returns the image's width and height in pixels and its pixels, top row first, four bytes
/// each: red, green, blue and alpha. For checking what the hotkey overlay would show.
///
/// # Panics
///
/// Panics if called from a thread other than the main thread.
pub fn render_hotkey_overlay(overlay: &HotkeyOverlay, scale: f64) -> (usize, usize, Vec<u8>) {
    let mtm = MainThreadMarker::new().expect("the hotkey overlay is drawn on the main thread");
    let (context, width, height) = Laid::new(mtm, overlay).draw(scale);

    let data = CGBitmapContextGetData(Some(&context)).cast::<u8>();
    assert!(!data.is_null(), "a bitmap context has pixels");
    // SAFETY: the context was made with `height` rows of `width * 4` bytes, and it is alive
    // until the end of this function.
    let pixels = unsafe { std::slice::from_raw_parts(data, width * height * 4) }.to_vec();
    (width, height, pixels)
}
